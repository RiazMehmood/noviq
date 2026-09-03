# Quickstart: Agent Task Executor + Desktop Overlay (App 2)

**Branch**: `002-agent-task-overlay` | **Date**: 2026-04-19

---

## Prerequisites

All prerequisites from App 1 apply (see `specs/001-notebook-knowledge-engine/quickstart.md`). Additional requirements for App 2:

| Requirement | Check command | Notes |
|-------------|--------------|-------|
| System Chromium (for web scraping, P4) | `chromium --version` | Optional — web scraping disabled gracefully if absent |
| Python 3 (for code execution, P3) | `python3 --version` | Optional — code exec disabled gracefully if absent |
| Microphone (for voice, P2) | `arecord -l` | Optional — voice input disabled gracefully if no mic |
| Disk space: +150 MB for new models | `df -h /media/riaz/New\ Volume/` | Silero VAD (2 MB) + Kokoro TTS (83 MB) |

---

## Download New Models

Before running App 2 with voice features, download the required ONNX models to `models/`:

```bash
cd "/media/riaz/New Volume/echo-AI"

# Silero VAD (speech detection, 1.8 MB)
curl -L "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx" \
     -o models/silero_vad.onnx

# Kokoro TTS (text-to-speech, ~83 MB)
curl -L "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files/kokoro-v0_19.onnx" \
     -o models/kokoro-v0_19.onnx
```

> Voice features will not crash if models are absent — they degrade gracefully with a Settings notification.

---

## Running the App

Use the same `run-dev.sh` script from App 1 — App 2 overlay window is created automatically in the same process:

```bash
cd "/media/riaz/New Volume/echo-AI"
./run-dev.sh
```

The overlay window starts **hidden** by default (FR-027). Look for the EchoAI tray icon in your system tray.

---

## Activating the Overlay

**Option A — Global shortcut**: Press `Ctrl+Shift+Space` (default; configurable in Settings)

**Option B — System tray**: Left-click the EchoAI tray icon → "Show Overlay"

**Option C — Wake word**: Say "EchoAI" (requires voice mode enabled and microphone present)

---

## P1 Test Flow: Task Assignment + Live Progress

1. Open overlay (Ctrl+Shift+Space)
2. Type: `List all PDF files in ~/Documents`
3. Agent shows decomposed plan (e.g., Step 1: Scan ~/Documents, Step 2: Filter .pdf files)
4. Click **Run**
5. Watch steps execute with live status indicators
6. View result in overlay

**Expected**: First step begins within 3 seconds of clicking Run (SC-001).

---

## P2 Test Flow: File Automation

1. Create a test folder: `mkdir /tmp/test-files && touch /tmp/test-files/a.txt /tmp/test-files/b.pdf`
2. Submit task: `Move all PDF files from /tmp/test-files to /tmp/pdf-archive`
3. Agent shows plan → Click Run
4. Agent requests confirmation for the file move (PAQ — CAUTIOUS action)
5. Click **Approve**
6. Verify `/tmp/pdf-archive/b.pdf` exists; `/tmp/test-files/b.pdf` gone

**Cancel test**: Click Cancel mid-execution → verify the file move is reversed.

---

## P2 Test Flow: Voice Interaction

1. Ensure microphone is connected and voice mode is set to "Both" in Settings
2. Say: `EchoAI`
3. Overlay activates within 1 second (SC-009)
4. Say: `List files in my Desktop`
5. Transcribed text appears in input
6. Agent executes task; result is read aloud via TTS (< 2s after completion, SC-011)

---

## P3 Test Flow: Code Execution

1. Create test CSV: `echo -e "name,revenue\nAlpha,500\nBeta,300\nGamma,700" > /tmp/test.csv`
2. Submit task: `Sum the revenue column in /tmp/test.csv and show the total`
3. Agent writes a Python script (visible in overlay before execution)
4. Click **Run** → PAQ approval requested for code execution (DANGEROUS)
5. Click **Approve**
6. Verify result shows `1500`

---

## P4 Test Flow: Web Scraping

> Requires Chromium installed.

1. Submit task: `Get the page title and first paragraph from https://en.wikipedia.org/wiki/Rust_(programming_language)`
2. Agent fetches via local headless Chromium (zero external proxy)
3. Extracted data shown in overlay
4. Verify no outbound calls except to the Wikipedia URL provided (use `ss -tp` or `tcpdump` to confirm)

---

## Checking History

1. Open overlay → click **History** tab
2. All past tasks listed with step drill-down
3. Type a keyword to search (FTS5, < 1s response, SC-006)
4. Click **Replay** on any past task → instruction pre-filled in input

---

## Settings Reference

Open Settings from the overlay (gear icon or tray menu):

| Setting | Default | Description |
|---------|---------|-------------|
| Wake phrase | EchoAI | Configurable wake word for voice activation |
| Input mode | Both | voice_only / chat_only / both |
| TTS enabled | true | Read results aloud when voice input used |
| History limit | 500 | Tasks kept; oldest purged when exceeded. Set 0 for unlimited |
| Allowed paths | None (unrestricted) | Optional restriction: only these folders accessible to agent |
| Overlay start visible | false | Show overlay on app launch instead of tray-only |

---

## Troubleshooting

| Problem | Likely cause | Fix |
|---------|-------------|-----|
| Overlay not visible | Hidden by default | Ctrl+Shift+Space or tray icon |
| Global shortcut not working | Wayland focus issue | Use tray icon or try X11 session |
| Voice not activating | No mic / voice mode off | Check `arecord -l`; enable in Settings |
| Kokoro TTS error | Model not downloaded | Run `curl` command above for kokoro model |
| Code execution disabled | Python3 not found | `sudo apt install python3` or task shows graceful error |
| Web scraping disabled | Chromium not found | `sudo apt install chromium-browser` |
| Model OOM | 7B loaded + TTS active | Both use same model slot — TTS runs after LLM eviction (90s idle) |
