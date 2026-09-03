# App 2 — EchoAI Agent Task Executor + Desktop Overlay

A persistent always-on-top floating overlay that turns plain-language instructions — typed or spoken — into executed actions on your machine: file operations, shell commands, sandboxed code, and local web scraping. Fully offline, non-focus-stealing, and shares one process + one LLM slot with App 1.

> **Status:** Specified and planned on branch `002-agent-task-overlay`. Currently in the **tasks** phase (implementation in progress).

---

## 1. Technologies Used

### Backend (Rust)
| Layer | Library | Purpose |
|---|---|---|
| Shell | **Tauri 2.0** (multi-window) | Second overlay window in the same process as App 1 |
| Global hotkey | **tauri-plugin-global-shortcut 2** | Activate overlay from any app |
| Tray | **tauri-plugin-tray 2** | Tray-resident process; show/hide per window |
| LLM inference | **llama-cpp-2** (shared with App 1) | ReAct loop reasoning, task decomposition |
| STT | **whisper-rs** (reused) | Offline transcription + wake-word pipeline |
| TTS | **Kokoro** via **ort** (ONNX Runtime) | Local neural voice response |
| Mic capture | **cpal** | Audio in |
| Audio out | **rodio** | TTS playback |
| Code sandbox | **Wasmtime 20** (WASI preview 1) + **QuickJS.wasm** | Safe JS execution; Python/Bash via subprocess |
| Web scraping | **chromiumoxide 0.7** | Local headless Chromium over CDP — zero cloud calls |
| Database | **SQLite** (shared `echo.db`) | 6 new additive tables; reads App 1 tables read-only |
| Embeddings | **fastembed-rs** (reused) | Semantic context for agent |
| Config | **TOML** personas | Agent prompt, tool schema, safety tier — no hardcoded behaviour |

### Frontend (TypeScript)
- **SolidJS** + **Tailwind CSS** overlay window
- Real-time step view, Pending Actions Queue, history panel

### New tables added to `echo.db`
`tasks`, `steps`, `execution_logs`, `pending_actions`, `task_results`, plus history index.

---

## 2. Working Flow

### 2.1 Process / window layout

```
 ┌───────────────── One Tauri process ─────────────────┐
 │                                                     │
 │   ┌──────────────────┐      ┌────────────────────┐  │
 │   │ App 1 Notebook   │      │ App 2 Overlay      │  │
 │   │ window (opt-in)  │      │ (auto-start, tray) │  │
 │   └────────┬─────────┘      └─────────┬──────────┘  │
 │            │                          │             │
 │            ▼                          ▼             │
 │   ┌────────────────────────────────────────────┐    │
 │   │           echo-ai-core (shared)            │    │
 │   │  ModelSlot  |  echo.db  |  embedder        │    │
 │   │  query router (tiers)  |  fastembed        │    │
 │   └────────────────────────────────────────────┘    │
 │                                                     │
 └─────────────────── System Tray ─────────────────────┘
```

Single LLM slot: 3B orchestrator stays warm; 7B worker is demand-loaded and evicted after 90 s of idle. At most one LLM in RAM at any time.

### 2.2 Agent ReAct loop (per task)

```
 [Voice "EchoAI …"  OR  Ctrl+Space → type]
               │
               ▼
    ┌────────────────────┐
    │ Wake word / hotkey │  whisper-rs (STT)  /  global shortcut
    └─────────┬──────────┘
              ▼
    ┌────────────────────┐
    │ Plan (Tier 3, 3B)  │  decompose → ordered step list
    └─────────┬──────────┘
              ▼
    ┌────────────────────┐
    │ Show plan to user  │  ← explicit "Run" required
    └─────────┬──────────┘
              │ Run clicked
              ▼
    ┌────────────────────────────────────────────┐
    │  ReAct loop (per step)                     │
    │  ┌──────────┐  ┌──────────────┐            │
    │  │ Reason   │─▶│ Pick tool    │            │
    │  └──────────┘  └──────┬───────┘            │
    │                       ▼                    │
    │        ┌──────────────────────────────┐    │
    │        │ Safety classifier (TOML)     │    │
    │        │  READ-ONLY → run             │    │
    │        │  CAUTIOUS  → brief confirm   │    │
    │        │  DANGEROUS → PAQ + approve   │    │
    │        └───────────┬──────────────────┘    │
    │                    ▼                       │
    │        ┌──────────────────────────────┐    │
    │        │ Execute tool                 │    │
    │        │  ├─ file ops (reversible)    │    │
    │        │  ├─ shell (irreversible)     │    │
    │        │  ├─ wasmtime code sandbox    │    │
    │        │  └─ chromiumoxide browser    │    │
    │        └───────────┬──────────────────┘    │
    │                    ▼                       │
    │        ┌──────────────────────────────┐    │
    │        │ Observe output → next step   │    │
    │        └──────────────────────────────┘    │
    └────────────────────────────────────────────┘
              │
              ▼
    ┌────────────────────┐
    │ Save to history    │  500 tasks default, configurable
    └─────────┬──────────┘
              ▼
    ┌────────────────────┐
    │ TTS read result    │  only when input was voice
    └────────────────────┘
```

### 2.3 Pending Actions Queue (safety gate — non-bypassable)

```
   Tool call emitted by agent
             │
             ▼
   ┌───────────────────────┐
   │ Classify (TOML)       │
   └──┬────────┬────────┬──┘
      │        │        │
  READ-ONLY  CAUTIOUS DANGEROUS
      │        │        │
      │        │        ▼
      │        │   ┌───────────────┐
      │        │   │ PAQ row +     │
      │        │   │ overlay modal │
      │        │   └──────┬────────┘
      │        │          │ user Approve / Deny
      │        ▼          ▼
      │  ┌──────────┐  ┌──────────────────┐
      │  │ confirm  │  │ execute OR block │
      │  │ toast    │  │ & log reason     │
      │  └──────────┘  └──────────────────┘
      ▼
   execute immediately
```

### 2.4 Cancel / rollback policy

```
User clicks Cancel
       │
       ▼
 ┌──────────────────────┐
 │ For each done step,  │
 │ in reverse order:    │
 │  - file op → undo    │
 │  - shell   → log as  │
 │             IRREVERS │
 │  - code    → log     │
 │  - fetch   → log     │
 └──────────────────────┘
```

Irreversible side-effects are **never silently discarded** — every command, args, stdout, stderr, exit code is written to history so the user can remediate manually.

### 2.5 What App 2 does for the user

- **Global shortcut / wake-word** → floating overlay on top of every app, never steals focus.
- **Plan-first execution** — agent shows the decomposed plan, user must click **Run**.
- **Four capabilities** — file ops · shell · sandboxed code (Python / Bash / JS in Wasmtime) · local headless-browser scraping.
- **Voice equivalence** — wake word "EchoAI", local STT, local Kokoro TTS; voice and chat produce identical results.
- **Pause / Resume / Cancel** any running task.
- **Task queue** — submit more tasks while one runs; sequential execution.
- **History** — default 500 tasks, searchable, replayable, exportable.
- **Path restriction** (optional) — limit the agent to specified folders.
- **Read-only access to App 1's knowledge** — the agent can use your notebooks as context.

---

## 3. Pros & Cons

### Pros
- **Hands-free automation** — voice + overlay means you never break focus.
- **Offline & private** — zero outbound calls except to URLs you explicitly supply.
- **Safety by construction** — Pending Actions Queue is non-bypassable; destructive ops always confirmed.
- **Transparent** — you see the plan and generated code *before* execution; every step is logged.
- **Reuses App 1** — no extra LLM, STT, or DB processes; one install, one RAM footprint.
- **TOML personas** — add a new capability or a domain persona (Legal, Finance…) by editing a config file, no rebuild.
- **Rollback-aware** — file ops reverse on Cancel; irreversible steps are loudly flagged, not hidden.

### Cons
- **Linux-only in this phase** — Windows/macOS are future work.
- **Sequential tasks** — no parallel execution (by design, to avoid file-write races).
- **Shell side-effects can't be auto-undone** — reversibility is bounded by what the OS allows.
- **Code sandbox is Wasmtime-first** — native Python / Bash only available if already installed on the host.
- **Headless Chromium adds disk footprint** (~150 MB) if not system-installed.
- **Wake-word false triggers** possible in noisy rooms — mitigated by a grace-period cancel indicator.
- **Agent still LLM-bounded** — complex multi-hop tasks may need clarifying questions; the agent surfaces them rather than guessing.

---

## 4. Usage Benefits by Role

### For the CEO
- "EchoAI, pull every contract signed this quarter into `/Review` and summarise each in one paragraph" — done while you stay in email.
- Confidential by default: no cloud agent sees the contents.

### For the CTO
- "Run this migration script in a sandbox against the dev DB dump and show me the row counts."
- "Scrape the last 10 entries from our internal status page and save as CSV" — via local headless browser, no API keys required.
- Automate repetitive ops (log rotation, build artefact cleanup) without writing a bash script.

### For the CFO
- "Extract all payment deadlines from every PDF in `~/Invoices/Q2` and write them to a dated CSV."
- "Flag any contract in `/Legal` that mentions a penalty over $50,000" — agent runs code + file ops together.
- Full audit log of what the agent did, when, and with what output — compliance-friendly.

### For Directors / Department Heads
- "Organise my Downloads folder by project, move anything older than 30 days to Archive, and email me the summary" (email step human-approved via PAQ).
- Replay favourite workflows with one click from history.

### For Secretaries / Executive Assistants
- Voice-driven: "EchoAI, rename every file in today's meeting folder to include the date and speaker" — no keyboard, overlay reads back the result.
- Batch rename, format conversion, folder tidy-up — tasks that used to eat an hour now take 30 seconds.

### For Employees / Engineers / Analysts
- "Parse this CSV, give me the top-10 rows by revenue" — agent writes the script, runs it in a Wasmtime sandbox, shows output.
- "Extract article titles and dates from `<url>`" — local scraper, no cloud API, no rate limits.
- Auto-retry on script errors (max 3) — you only see the hard failures.

### For Personal / Home Use
- "EchoAI, find every photo in Pictures taken in 2023 and copy them to an album folder."
- "Summarise every PDF bill in Downloads and write a month-by-month total to `bills.txt`."
- Voice-first on a home laptop — hands-free while cooking, working on something else, or away from the keyboard.

### Office-wide Benefits
- A **local "digital intern"** on every workstation — same install as App 1, same footprint.
- **No SaaS agent fees**, no data-processing agreements, no DLP leaks.
- PAQ + path-restriction + full audit log → passes internal security review more easily than cloud agent tooling.
- Works on air-gapped / classified networks where cloud agents are forbidden.
- Shared knowledge layer with App 1 — the agent can act on the same documents you already indexed for chat.
