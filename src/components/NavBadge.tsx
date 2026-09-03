import { For, Show } from "solid-js";
import { useWorkloads } from "../stores/scheduler";

export default function NavBadge() {
  const workloads = useWorkloads();
  const background = () =>
    Object.values(workloads()).filter(
      (w) => w.progress.paused || w.kind !== "chat",
    );
  return (
    <Show when={background().length > 0}>
      <div class="nav-badge flex gap-2 text-xs text-gray-400">
        <For each={background()}>
          {(w) => (
            <span>
              {w.kind} {w.progress.paused ? "paused" : ""} • {w.progress.processed}/{w.progress.total}
            </span>
          )}
        </For>
      </div>
    </Show>
  );
}
