# Tauri Event Contracts: Agent Task Executor + Desktop Overlay

**Branch**: `002-agent-task-overlay` | **Date**: 2026-04-19

All events are emitted from Rust backend via `app_handle.emit(event_name, payload)` and listened to in SolidJS frontend via `listen(event_name, handler)`.

---

## Task Lifecycle Events

### `agent:task-updated`

Fired whenever a task's status or queue position changes.

```ts
{
  task_id: string;
  status: TaskStatus;
  queue_position: number | null;
}
```

**When fired**: On every state transition (queued → planning → awaiting_approval → running → paused → completed/failed/cancelled/interrupted).

---

### `agent:plan-ready`

Fired when the agent has decomposed the task into steps and is waiting for user to click Run.

```ts
{
  task_id: string;
  steps: {
    step_index: number;
    step_name: string;
    tool: string;
    reversibility: 'reversible' | 'irreversible' | 'read_only';
  }[];
}
```

**Frontend action**: Show plan preview UI with "Run" button.

---

### `agent:step-started`

Fired when execution of a step begins.

```ts
{
  task_id: string;
  step: {
    id: string;
    step_index: number;
    step_name: string;
    tool: string;
    reversibility: 'reversible' | 'irreversible' | 'read_only';
    started_at: number;
  };
}
```

---

### `agent:step-completed`

Fired when a step finishes (done, failed, or skipped).

```ts
{
  task_id: string;
  step: {
    id: string;
    step_index: number;
    status: 'done' | 'failed' | 'skipped';
    output_text: string | null;
    error_text: string | null;
    duration_ms: number;
  };
}
```

---

### `agent:task-completed`

Fired when all steps finish successfully.

```ts
{
  task_id: string;
  result_summary: string;
  input_mode: 'chat' | 'voice';   // if 'voice', TTS will also fire agent:tts-started
}
```

---

### `agent:task-failed`

Fired when the agent cannot complete the task.

```ts
{
  task_id: string;
  error: string;              // human-readable explanation
  last_step_index: number;
}
```

---

## Approval Events

### `agent:approval-needed`

Fired when a DANGEROUS or CAUTIOUS tool call is queued in the PAQ awaiting user decision.

```ts
{
  task_id: string;
  action: {
    id: string;
    action_class: 'DANGEROUS' | 'CAUTIOUS';
    description: string;     // e.g. "Delete file ~/Downloads/report.pdf"
    tool_call_json: string;
  };
}
```

**Frontend action**: Render `PaqApproval` dialog; block Run button until decided.

---

## Voice Events

### `agent:voice-wake`

Fired when wake phrase is detected and the system enters full-utterance listening mode.

```ts
{}   // no payload
```

**Frontend action**: Show overlay if hidden; display `VoiceIndicator` in listening state.

---

### `agent:voice-transcript`

Fired when whisper-rs produces a transcription of the user's utterance.

```ts
{
  text: string;               // transcribed text
  confidence: number;         // 0.0–1.0
}
```

**Frontend action**: Populate task input field; trigger task submission flow.

---

### `agent:tts-started`

Fired when Kokoro TTS begins synthesizing/playing a response.

```ts
{
  text: string;               // the text being spoken
}
```

**Frontend action**: Show TTS speaking indicator; enable stop button.

---

### `agent:tts-completed`

Fired when TTS playback finishes or is stopped by user.

```ts
{
  stopped_by_user: boolean;
}
```

---

### `agent:voice-error`

Fired on any voice subsystem error (mic permission denied, model load failure, etc.).

```ts
{
  error_code: 'mic_unavailable' | 'model_load_failed' | 'transcription_failed' | 'tts_failed';
  message: string;
}
```

**Frontend action**: Show error in overlay; disable affected voice controls gracefully.

---

## Overlay Events

### `agent:overlay-shortcut`

Fired when the global keyboard shortcut is pressed (registered in `tauri-plugin-global-shortcut`).

```ts
{}   // no payload
```

**Frontend action**: Toggle overlay visibility.

---

## Event Listener Registration (SolidJS pattern)

```ts
import { listen } from '@tauri-apps/api/event';
import { onMount, onCleanup } from 'solid-js';

// Example: listen for step updates
onMount(() => {
  const unlisten = listen<StepCompletedPayload>('agent:step-completed', (event) => {
    // update step in local signal
  });
  // Register cleanup synchronously (never after await)
  onCleanup(() => { unlisten.then(f => f()); });
});
```

> Per project feedback: register `onCleanup` synchronously, never after `await` in `onMount`.
