/**
 * F014 — server-status poller (web transport only).
 *
 * While the GPU server is rebuilding (a `make deploy`), the still-running old
 * binary reports `{status:"deploying"}` from /api/health; during the brief
 * restart it is unreachable. Either way the browser shows a friendly "updating"
 * banner (see ServerStatusBanner) instead of a broken page, and auto-reloads
 * once the new version is live so the user picks up the new frontend assets.
 *
 * In the Tauri desktop shell there is no remote server, so polling is a no-op.
 */
import { createSignal } from "solid-js";
import { isTauri } from "../api/transport";

export type ServerStatus = "ok" | "deploying" | "maintenance" | "unreachable";

const [serverStatus, setServerStatus] = createSignal<ServerStatus>("ok");
export { serverStatus };

// Operator-supplied reason for planned maintenance, straight from /api/health.
// Rendered verbatim so the banner states the real cause instead of guessing —
// "chat is paused for GPU indexing" is not "updating to a new version".
const [serverMessage, setServerMessage] = createSignal<string>("");
export { serverMessage };

let pollTimer: ReturnType<typeof setInterval> | undefined;
// Set once we observe an in-progress deploy (the `deploying` sentinel). When the
// server next reports `ok`, we reload exactly once so the browser swaps in the
// freshly-built frontend bundle. Keying off the deploy itself — not the static
// `version` field (which never changes) — means this catches BOTH frontend-only
// and backend deploys. (Requires the VM deploy script to write the sentinel; see
// vm_enable_deploy_banner.txt.)
let sawDeploying = false;

async function poll(): Promise<void> {
  try {
    const res = await fetch("/api/health", { cache: "no-store" });
    if (!res.ok) {
      // Non-2xx → we could NOT confirm a deploy, so do not claim one (2026-07-24).
      // A 502 here is ambiguous: it is emitted both during the post-build restart AND
      // by Cloudflare for a tunnel whose hostname has rotated away, and the latter
      // never resolves on its own. Reporting "unreachable" keeps the banner honest;
      // `sawDeploying` is unaffected because a real deploy sets it from the sentinel
      // (status:"deploying") BEFORE deploy.sh clears it and restarts, so the
      // auto-reload on recovery still fires.
      setServerStatus("unreachable");
      return;
    }
    const body = (await res.json()) as { status?: string; message?: string | null };
    if (body?.status === "deploying") {
      sawDeploying = true;
      setServerStatus("deploying");
      return;
    }
    // Planned maintenance (e.g. GPU-embedding mode stops vLLM to free VRAM).
    // Deliberately does NOT set `sawDeploying`: nothing was rebuilt, so there are
    // no new assets to pick up and forcing a reload would just interrupt the user.
    if (body?.status === "maintenance") {
      setServerMessage(body.message ?? "");
      setServerStatus("maintenance");
      return;
    }
    // Back to ok: if we lived through a deploy, reload once to pick up new assets.
    if (sawDeploying) {
      sawDeploying = false;
      window.location.reload();
      return;
    }
    setServerStatus("ok");
  } catch {
    // fetch threw → server down / restarting → treat as updating.
    setServerStatus("unreachable");
  }
}

/** Begin polling /api/health. No-op in the Tauri desktop shell. */
export function startServerStatusPolling(intervalMs = 5000): void {
  if (isTauri() || pollTimer) return;
  void poll();
  pollTimer = setInterval(() => void poll(), intervalMs);
}

export function stopServerStatusPolling(): void {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = undefined;
  }
}
