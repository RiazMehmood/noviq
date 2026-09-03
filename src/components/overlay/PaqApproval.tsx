import { Component, createSignal, onCleanup, onMount, Show } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

type PendingAction = {
  id: string;
  task_id: string;
  description: string;
  safety_class: "READ_ONLY" | "CAUTIOUS" | "DANGEROUS";
};

const badgeClass: Record<PendingAction["safety_class"], string> = {
  READ_ONLY: "text-xs px-2 py-0.5 rounded bg-green-900/50 text-green-300 border border-green-700",
  CAUTIOUS:  "text-xs px-2 py-0.5 rounded bg-yellow-900/50 text-yellow-300 border border-yellow-700",
  DANGEROUS: "text-xs px-2 py-0.5 rounded bg-red-900/50 text-red-300 border border-red-700",
};

const badgeLabel: Record<PendingAction["safety_class"], string> = {
  READ_ONLY: "READ ONLY",
  CAUTIOUS:  "CAUTIOUS",
  DANGEROUS: "DANGEROUS",
};

const PaqApproval: Component = () => {
  const [action, setAction] = createSignal<PendingAction | null>(null);

  onMount(() => {
    const unlistenPromise = listen<PendingAction>("agent:approval-needed", ev => {
      setAction(ev.payload);
    });

    onCleanup(() => { unlistenPromise.then(u => u()); });
  });

  const decide = async (decision: "Approve" | "Reject") => {
    const a = action();
    if (!a) return;
    setAction(null);
    await invoke("agent_decide_action", { id: a.id, decision });
  };

  return (
    <Show when={action() !== null}>
      <div class="absolute inset-0 bg-black/60 flex items-center justify-center z-50 rounded-xl">
        <div class="bg-gray-800 border border-gray-600 rounded-xl p-4 mx-4 max-w-sm w-full shadow-xl">
          <p class="text-sm font-semibold text-gray-100 mb-2">Action Required</p>
          <span class={badgeClass[action()!.safety_class]}>
            {badgeLabel[action()!.safety_class]}
          </span>
          <p class="text-sm text-gray-300 my-3">{action()!.description}</p>
          <div class="flex gap-2">
            <button
              class="flex-1 py-2 rounded-lg bg-gray-700 hover:bg-gray-600 text-gray-200 text-sm"
              onClick={() => decide("Reject")}
            >
              Reject
            </button>
            <button
              class="flex-1 py-2 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium"
              onClick={() => decide("Approve")}
            >
              Approve
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

export default PaqApproval;
