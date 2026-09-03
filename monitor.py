#!/usr/bin/env python3
"""
EchoAI Crash-Survivable Monitor
================================
Logs system RAM, swap, and app process stats at ~20ms intervals (50 Hz).
Every line is line-flushed to disk immediately — so even if the system OOM-kills
the app, the log file is complete up to the crash instant.

Usage:
    python3 monitor.py [app_log_file]

The optional app_log_file argument tails it in real-time and interleaves
[APP] lines into the monitor log for a single unified view.
"""

import os
import sys
import time
import threading
import subprocess
from datetime import datetime

# ── Config ──────────────────────────────────────────────────────────────────
INTERVAL_S   = 0.020          # 20 ms = 50 readings/second
PID_RESCAN_S = 2.0            # re-scan /proc for app PIDs every 2 s
LOG_DIR      = os.path.join(os.path.dirname(__file__), "logs")
APP_KEYWORDS = ["echo-ai", "tauri-dev", "npm run tauri"]

# ── Setup ────────────────────────────────────────────────────────────────────
os.makedirs(LOG_DIR, exist_ok=True)
session_ts = datetime.now().strftime("%Y%m%d_%H%M%S")
LOG_FILE   = os.path.join(LOG_DIR, f"monitor_{session_ts}.log")

# ── Helpers ──────────────────────────────────────────────────────────────────

def read_meminfo() -> dict:
    """Read /proc/meminfo — no subprocess, sub-millisecond."""
    d = {}
    with open("/proc/meminfo") as f:
        for line in f:
            if ":" in line:
                k, v = line.split(":", 1)
                d[k.strip()] = int(v.strip().split()[0])
    return d


def read_loadavg() -> str:
    with open("/proc/loadavg") as f:
        return f.read().split()[:3]


_cached_pids: list[int] = []
_last_pid_scan: float = 0.0

def find_app_pids() -> list[int]:
    """Scan /proc for app PIDs; cached so we don't scan 50×/second."""
    global _cached_pids, _last_pid_scan
    now = time.monotonic()
    if now - _last_pid_scan < PID_RESCAN_S and _cached_pids:
        # Validate cached PIDs still exist
        _cached_pids = [p for p in _cached_pids if os.path.exists(f"/proc/{p}")]
        return _cached_pids

    _last_pid_scan = now
    found = []
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                cmd = f.read().replace(b"\x00", b" ").decode(errors="replace")
            if any(kw in cmd for kw in APP_KEYWORDS):
                found.append(int(pid))
        except (PermissionError, FileNotFoundError):
            pass
    _cached_pids = found
    return found


def get_proc_rss_kb(pid: int) -> int:
    """Read RSS in kB from /proc/<pid>/status — no subprocess."""
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmRSS"):
                    return int(line.split()[1])
    except (FileNotFoundError, PermissionError, ValueError):
        pass
    return 0


def get_proc_threads(pid: int) -> int:
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("Threads"):
                    return int(line.split()[1])
    except Exception:
        pass
    return 0


# ── Main monitor loop ────────────────────────────────────────────────────────

def monitor_loop(log):
    tick = 0
    while True:
        t0 = time.monotonic()
        now = datetime.now()
        ts  = now.strftime("%H:%M:%S.") + f"{now.microsecond // 1000:03d}"

        try:
            mem   = read_meminfo()
            total = mem.get("MemTotal",     1)
            avail = mem.get("MemAvailable", 0)
            free  = mem.get("MemFree",      0)
            bufs  = mem.get("Buffers",      0)
            cache = mem.get("Cached",       0)
            swap_total = mem.get("SwapTotal", 0)
            swap_free  = mem.get("SwapFree",  0)
            swap_used  = swap_total - swap_free

            used     = total - avail
            used_pct = used / total * 100.0

            load = read_loadavg()

            pids     = find_app_pids()
            rss_list = [(p, get_proc_rss_kb(p)) for p in pids]
            thr_list = [(p, get_proc_threads(p)) for p in pids]
            app_rss  = sum(r for _, r in rss_list)
            app_thr  = sum(t for _, t in thr_list)

            # WARN threshold — highlight when free RAM < 500 MB
            warn = " ⚠️  LOW_RAM" if avail < 512_000 else ""
            if swap_used > 100_000:
                warn += f" ⚠️  SWAP_ACTIVE={swap_used//1024}MB"

            line = (
                f"[SYS] {ts} | "
                f"RAM {used//1024:>5}MB/{total//1024}MB ({used_pct:4.1f}%) "
                f"avail={avail//1024}MB free={free//1024}MB | "
                f"SWAP {swap_used//1024}MB/{swap_total//1024}MB | "
                f"load={' '.join(load)} | "
                f"APP pids={pids} rss={app_rss//1024}MB threads={app_thr}"
                f"{warn}"
            )

            log.write(line + "\n")
            # Print a summary line to stdout every 1 second (50 ticks)
            if tick % 50 == 0:
                print(line, flush=True)

        except Exception as e:
            err = f"[MONITOR_ERR] {ts} {e}"
            log.write(err + "\n")
            print(err, flush=True)

        tick += 1
        elapsed = time.monotonic() - t0
        sleep_for = max(0.0, INTERVAL_S - elapsed)
        time.sleep(sleep_for)


def tail_app_log(app_log_path: str, log):
    """Background thread: tail app log and interleave into monitor log."""
    # Wait until file exists
    for _ in range(60):
        if os.path.exists(app_log_path):
            break
        time.sleep(1)
    if not os.path.exists(app_log_path):
        log.write(f"[MONITOR] App log not found: {app_log_path}\n")
        return

    with open(app_log_path) as f:
        f.seek(0, 2)  # seek to end — only new lines
        while True:
            line = f.readline()
            if line:
                ts = datetime.now().strftime("%H:%M:%S.") + \
                     f"{datetime.now().microsecond // 1000:03d}"
                entry = f"[APP] {ts} {line.rstrip()}"
                log.write(entry + "\n")
                print(entry, flush=True)
            else:
                time.sleep(0.005)


# ── Entry point ──────────────────────────────────────────────────────────────

if __name__ == "__main__":
    app_log = sys.argv[1] if len(sys.argv) > 1 else None

    # Open log with line buffering (buffering=1) — every write is flushed.
    with open(LOG_FILE, "w", buffering=1) as log:
        header = (
            f"{'='*70}\n"
            f"EchoAI Session Monitor\n"
            f"Started : {datetime.now().isoformat()}\n"
            f"Interval: {INTERVAL_S*1000:.0f} ms\n"
            f"Log file: {LOG_FILE}\n"
            f"App log : {app_log or 'none (start separately)'}\n"
            f"{'='*70}\n"
        )
        log.write(header)
        print(header, end="", flush=True)

        if app_log:
            t = threading.Thread(target=tail_app_log, args=(app_log, log), daemon=True)
            t.start()

        monitor_loop(log)
