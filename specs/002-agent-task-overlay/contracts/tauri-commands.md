# Tauri Command Contracts: Agent Task Executor + Desktop Overlay

**Branch**: `002-agent-task-overlay` | **Date**: 2026-04-19

All commands are invoked from SolidJS frontend via `invoke('<command>', payload)`. Rust handlers are decorated with `#[tauri::command]` in `src-tauri/src/agent/commands.rs`.

---

## Task Management

### `agent_submit_task`

Submit a new natural-language instruction to the agent queue.

**Input**:
```ts
{
  instruction: string;    // max 2000 chars (FR-006)
  input_mode: 'chat' | 'voice';
}
```

**Output**:
```ts
{ task_id: string }       // UUID of created task
```

**Errors**: `"InstructionTooLong"` | `"QueueFull"`

**Side effects**: Inserts row in `agent_tasks` (status: `queued`); emits `agent:task-updated`; triggers ReAct loop on next available slot.

---

### `agent_get_task`

Fetch full task record including all steps.

**Input**: `{ task_id: string }`

**Output**:
```ts
{
  id: string;
  instruction: string;
  input_mode: 'chat' | 'voice';
  status: TaskStatus;
  queue_position: number | null;
  created_at: number;
  started_at: number | null;
  completed_at: number | null;
  result_summary: string | null;
  steps: Step[];
}
```

**Errors**: `"TaskNotFound"`

---

### `agent_list_tasks`

List tasks with optional status filter and pagination.

**Input**:
```ts
{
  status?: TaskStatus;    // filter by status; omit for all
  limit?: number;         // default 50, max 200
  offset?: number;        // default 0
}
```

**Output**: `Task[]` (same shape as `agent_get_task` output, steps omitted)

---

### `agent_pause_task`

Pause a running task after the current step completes.

**Input**: `{ task_id: string }`

**Output**: `{ ok: true }`

**Errors**: `"TaskNotFound"` | `"TaskNotPauseable"` (task not in `running` state)

**Side effects**: Sets status to `paused`; emits `agent:task-updated`.

---

### `agent_resume_task`

Resume a paused task from the next pending step.

**Input**: `{ task_id: string }`

**Output**: `{ ok: true }`

**Errors**: `"TaskNotFound"` | `"TaskNotResumeable"`

---

### `agent_cancel_task`

Cancel a running or paused task and trigger rollback of reversible steps.

**Input**: `{ task_id: string }`

**Output**:
```ts
{
  ok: true;
  rolled_back_steps: number;      // count of reversed file ops
  irreversible_steps: number;     // count of steps flagged irreversible
}
```

**Errors**: `"TaskNotFound"` | `"TaskNotCancellable"`

**Side effects**: Status → `cancelled`; rollback engine reverses reversible steps in reverse order; emits `agent:task-updated`.

---

### `agent_search_history`

Full-text search over task instructions and outputs.

**Input**:
```ts
{
  query: string;
  limit?: number;    // default 50
}
```

**Output**: `Task[]` (sorted by relevance then recency)

**Performance target**: < 1s for 500+ task history (SC-006, FTS5-backed)

---

### `agent_replay_task`

Create a new task pre-filled with the instruction from a historical task.

**Input**: `{ task_id: string }`

**Output**: `{ new_task_id: string }`

**Errors**: `"TaskNotFound"`

---

## Pending Actions Queue

### `agent_get_pending_actions`

Fetch all actions awaiting user decision for a task.

**Input**: `{ task_id: string }`

**Output**:
```ts
{
  id: string;
  task_id: string;
  step_id: string;
  action_class: 'DANGEROUS' | 'CAUTIOUS';
  description: string;        // human-readable e.g. "Delete file ~/old.txt"
  tool_call_json: string;     // exact params for audit trail
  status: 'awaiting' | 'approved' | 'rejected';
  created_at: number;
}[]
```

---

### `agent_decide_action`

Approve or reject a pending action in the PAQ.

**Input**:
```ts
{
  action_id: string;
  decision: 'approve' | 'reject';
}
```

**Output**: `{ ok: true }`

**Errors**: `"ActionNotFound"` | `"ActionAlreadyDecided"`

**Side effects**: Updates `pending_actions.status`; unblocks ReAct loop; emits `agent:task-updated`.

---

## Voice

### `agent_start_listening`

Begin full-utterance transcription (called after wake word detected, or by user clicking mic button).

**Input**: `{}`

**Output**: `{ ok: true }`

**Errors**: `"MicrophoneUnavailable"` | `"VoiceModeDisabled"`

---

### `agent_stop_listening`

Cancel active microphone capture.

**Input**: `{}`

**Output**: `{ ok: true }`

---

### `agent_get_voice_status`

**Input**: `{}`

**Output**:
```ts
{
  microphone_available: boolean;
  voice_mode: 'voice_only' | 'chat_only' | 'both';
  is_wake_listening: boolean;     // continuously monitoring for wake word
  is_utterance_listening: boolean; // capturing post-wake speech
  is_tts_speaking: boolean;
}
```

---

## Overlay State

### `agent_show_overlay`

Make the overlay window visible and focused.

**Input**: `{}`

**Output**: `{ ok: true }`

---

### `agent_hide_overlay`

Hide the overlay window (tray icon remains).

**Input**: `{}`

**Output**: `{ ok: true }`

---

### `agent_get_overlay_state`

**Output**:
```ts
{
  visible: boolean;
  position: { x: number; y: number };
  size: { width: number; height: number };
  active_task_id: string | null;
}
```

---

### `agent_save_overlay_state`

Persist overlay position/size on user resize/move.

**Input**:
```ts
{
  position: { x: number; y: number };
  size: { width: number; height: number };
}
```

**Output**: `{ ok: true }`

---

## Settings

### `agent_get_settings`

**Output**:
```ts
{
  wake_phrase: string;                         // default "EchoAI"
  voice_mode: 'voice_only' | 'chat_only' | 'both';
  tts_enabled: boolean;
  history_limit: number | null;                // null = unlimited
  allowed_paths: string[] | null;              // null = no restriction
  overlay_start_visible: boolean;              // default false (FR-027)
}
```

---

### `agent_update_settings`

**Input**: Partial of `AgentSettings` (same shape as above, all fields optional)

**Output**: `{ ok: true }`

**Side effects**: Persists to `echo.db` settings table; notifies voice subsystem of mode change.

---

## Shared Types

```ts
type TaskStatus =
  | 'queued' | 'planning' | 'awaiting_approval'
  | 'running' | 'paused' | 'completed'
  | 'failed' | 'cancelled' | 'interrupted';

type Step = {
  id: string;
  step_index: number;
  step_name: string;
  tool: string;
  reversibility: 'reversible' | 'irreversible' | 'read_only';
  status: 'pending' | 'running' | 'done' | 'failed' | 'skipped';
  input_json: string | null;
  output_text: string | null;
  error_text: string | null;
  generated_code: string | null;
  started_at: number | null;
  completed_at: number | null;
  duration_ms: number | null;
};
```
