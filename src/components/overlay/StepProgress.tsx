import { Component, createSignal, onCleanup, onMount, For, Show } from "solid-js";
import { listen } from "@tauri-apps/api/event";

type StepStatus = "pending" | "running" | "completed" | "failed" | "skipped";

type Step = {
  id: string;
  step_number: number;
  tool_name: string | null;
  output_text: string | null;
  status: StepStatus;
  duration_ms: number | null;
  expanded: boolean;
};

type Props = {
  taskId: string | null;
};

const statusIcon = (s: StepStatus) => {
  if (s === "pending")   return "⏳";
  if (s === "running")   return "🔄";
  if (s === "completed") return "✅";
  if (s === "failed")    return "❌";
  return "⏭";
};

const StepProgress: Component<Props> = (props) => {
  const [steps, setSteps] = createSignal<Step[]>([]);
  const [resultText, setResultText] = createSignal<string | null>(null);
  const [errorText, setErrorText] = createSignal<string | null>(null);

  const upsertStep = (patch: Partial<Step> & { id: string }) => {
    setSteps(prev => {
      const idx = prev.findIndex(s => s.id === patch.id);
      if (idx === -1) {
        return [...prev, { step_number: 0, tool_name: null, output_text: null, status: "pending", duration_ms: null, expanded: false, ...patch }];
      }
      const next = [...prev];
      next[idx] = { ...next[idx], ...patch };
      return next;
    });
  };

  const toggleExpand = (id: string) => {
    setSteps(prev => prev.map(s => s.id === id ? { ...s, expanded: !s.expanded } : s));
  };

  onMount(() => {
    const unlisteners: Array<() => void> = [];

    listen<{ task_id: string; step: { id: string; step_number: number; tool_name: string } }>(
      "agent:step-started", ev => {
        if (ev.payload.task_id !== props.taskId) return;
        const { id, step_number, tool_name } = ev.payload.step;
        upsertStep({ id, step_number, tool_name, status: "running" });
      }
    ).then(u => unlisteners.push(u));

    listen<{ task_id: string; step: { id: string; output_text: string; status: StepStatus; duration_ms: number } }>(
      "agent:step-completed", ev => {
        if (ev.payload.task_id !== props.taskId) return;
        const { id, output_text, status, duration_ms } = ev.payload.step;
        upsertStep({ id, output_text, status, duration_ms });
      }
    ).then(u => unlisteners.push(u));

    listen<{ task_id: string; result_text: string }>(
      "agent:task-completed", ev => {
        if (ev.payload.task_id !== props.taskId) return;
        setResultText(ev.payload.result_text);
      }
    ).then(u => unlisteners.push(u));

    listen<{ task_id: string; error_text: string }>(
      "agent:task-failed", ev => {
        if (ev.payload.task_id !== props.taskId) return;
        setErrorText(ev.payload.error_text);
      }
    ).then(u => unlisteners.push(u));

    onCleanup(() => unlisteners.forEach(u => u()));
  });

  return (
    <Show when={props.taskId !== null}>
      <div class="p-3 flex flex-col gap-2">
        <For each={steps()}>
          {step => (
            <div class="bg-gray-800/60 rounded-lg px-3 py-2 text-xs">
              <div
                class="flex items-center gap-2 cursor-pointer select-none"
                onClick={() => toggleExpand(step.id)}
              >
                <span class={step.status === "running" ? "animate-spin" : ""}>
                  {statusIcon(step.status)}
                </span>
                <span class="text-gray-300 font-mono">{step.tool_name ?? `step ${step.step_number}`}</span>
                <Show when={step.duration_ms !== null}>
                  <span class="text-gray-500 ml-auto">{step.duration_ms}ms</span>
                </Show>
              </div>
              <Show when={step.expanded && step.output_text}>
                <pre class="text-xs text-gray-300 bg-gray-900 rounded p-2 mt-2 whitespace-pre-wrap break-words max-h-32 overflow-y-auto">
                  {step.output_text}
                </pre>
              </Show>
            </div>
          )}
        </For>

        <Show when={resultText() !== null}>
          <div class="rounded-lg px-3 py-2 bg-green-900/30 border border-green-700/50 text-green-300 text-xs whitespace-pre-wrap">
            {resultText()}
          </div>
        </Show>

        <Show when={errorText() !== null}>
          <div class="rounded-lg px-3 py-2 bg-red-900/30 border border-red-700/50 text-red-300 text-xs">
            {errorText()}
          </div>
        </Show>
      </div>
    </Show>
  );
};

export default StepProgress;
