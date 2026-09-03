/**
 * MeetingPanel — Phase 2 smoke-test UI for the Meeting Agent capture pipeline.
 * Allows manual Start/Stop and live segment polling (2 s interval).
 */
import { Component, createSignal, For, onCleanup, Show } from "solid-js";
import {
  startMeeting,
  stopMeeting,
  listMeetingSegments,
  MeetingSegment,
} from "../../api/index";

const DEFAULT_MODEL =
  "/media/riaz/New Volume/echo-AI/models/whisper/ggml-base.en.bin";

const MeetingPanel: Component = () => {
  const [title, setTitle] = createSignal("Smoke test meeting");
  const [language, setLanguage] = createSignal("en");
  const [modelPath, setModelPath] = createSignal(DEFAULT_MODEL);
  const [activeId, setActiveId] = createSignal<number | null>(null);
  const [status, setStatus] = createSignal<"idle" | "active" | "stopped" | "error">("idle");
  const [segments, setSegments] = createSignal<MeetingSegment[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  const stopPolling = () => {
    if (pollInterval !== null) {
      clearInterval(pollInterval);
      pollInterval = null;
    }
  };

  const refreshSegments = async (id: number) => {
    try {
      const segs = await listMeetingSegments(id);
      setSegments(segs);
    } catch (e) {
      console.error("listMeetingSegments error:", e);
      setError(String(e));
    }
  };

  const startPolling = (id: number) => {
    stopPolling();
    pollInterval = setInterval(() => refreshSegments(id), 2000);
  };

  onCleanup(() => stopPolling());

  const onStart = async () => {
    setError(null);
    try {
      const id = await startMeeting(title(), language(), modelPath());
      setActiveId(id);
      setStatus("active");
      setSegments([]);
      startPolling(id);
    } catch (e) {
      console.error("start_meeting error:", e);
      setError(String(e));
      setStatus("error");
    }
  };

  const onStop = async () => {
    const id = activeId();
    if (id === null) return;
    setError(null);
    try {
      await stopMeeting(id);
      setStatus("stopped");
      stopPolling();
      await refreshSegments(id);
    } catch (e) {
      console.error("stop_meeting error:", e);
      setError(String(e));
      setStatus("error");
      stopPolling();
    }
  };

  const onClear = () => {
    setActiveId(null);
    setStatus("idle");
    setSegments([]);
    setError(null);
    stopPolling();
  };

  const statusColor = () => {
    switch (status()) {
      case "active":  return "text-green-400";
      case "stopped": return "text-gray-400";
      case "error":   return "text-red-400";
      default:        return "text-gray-500";
    }
  };

  return (
    <div class="bg-white rounded-lg shadow p-4 space-y-4">
      {/* Input row */}
      <div class="flex flex-wrap gap-3 items-end">
        <div class="flex flex-col gap-1 flex-1 min-w-[140px]">
          <label class="text-xs text-gray-500 font-medium">Title</label>
          <input
            class="border border-gray-300 rounded px-2 py-1 text-sm text-gray-900 bg-white placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-blue-400"
            value={title()}
            onInput={(e) => setTitle(e.currentTarget.value)}
            disabled={status() === "active"}
          />
        </div>
        <div class="flex flex-col gap-1 w-16">
          <label class="text-xs text-gray-500 font-medium">Lang</label>
          <input
            class="border border-gray-300 rounded px-2 py-1 text-sm text-gray-900 bg-white placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-blue-400"
            value={language()}
            onInput={(e) => setLanguage(e.currentTarget.value)}
            disabled={status() === "active"}
          />
        </div>
        <div class="flex flex-col gap-1 flex-[3] min-w-[200px]">
          <label class="text-xs text-gray-500 font-medium">Model Path</label>
          <input
            class="border border-gray-300 rounded px-2 py-1 text-sm font-mono text-gray-900 bg-white placeholder-gray-400 focus:outline-none focus:ring-1 focus:ring-blue-400"
            value={modelPath()}
            onInput={(e) => setModelPath(e.currentTarget.value)}
            disabled={status() === "active"}
          />
        </div>
      </div>

      {/* Action buttons + status */}
      <div class="flex items-center gap-3 flex-wrap">
        <button
          class="px-3 py-1.5 bg-blue-600 text-white rounded text-sm font-medium hover:bg-blue-700 disabled:opacity-40 disabled:cursor-not-allowed"
          onClick={onStart}
          disabled={status() === "active"}
        >
          Start Meeting
        </button>
        <button
          class="px-3 py-1.5 bg-red-600 text-white rounded text-sm font-medium hover:bg-red-700 disabled:opacity-40 disabled:cursor-not-allowed"
          onClick={onStop}
          disabled={status() !== "active"}
        >
          Stop Meeting
        </button>
        <Show when={status() !== "idle"}>
          <button
            class="px-3 py-1.5 bg-gray-200 text-gray-700 rounded text-sm font-medium hover:bg-gray-300"
            onClick={onClear}
          >
            Clear
          </button>
        </Show>
        <Show when={status() === "stopped"}>
          <button
            class="px-3 py-1.5 bg-gray-200 text-gray-700 rounded text-sm font-medium hover:bg-gray-300"
            onClick={() => { const id = activeId(); if (id !== null) refreshSegments(id); }}
          >
            Refresh
          </button>
        </Show>
        <Show when={activeId() !== null}>
          <span class="text-xs text-gray-500">
            Meeting ID: <span class="font-mono font-semibold">{activeId()}</span>
            {" — "}
            <span class={`font-semibold ${statusColor()}`}>{status()}</span>
          </span>
        </Show>
      </div>

      {/* Error banner */}
      <Show when={error()}>
        <div class="bg-red-50 border border-red-200 rounded px-3 py-2 text-sm text-red-700">
          {error()}
        </div>
      </Show>

      {/* Segments list */}
      <Show when={segments().length > 0}>
        <div>
          <p class="text-xs text-gray-500 font-medium mb-1">
            Transcript segments ({segments().length})
          </p>
          <div class="max-h-64 overflow-y-auto border border-gray-200 rounded divide-y divide-gray-100 bg-gray-50">
            <For each={segments()}>
              {(seg) => (
                <div class="px-3 py-1.5 text-sm flex gap-3">
                  <span class="text-gray-400 font-mono text-xs flex-shrink-0 pt-0.5">
                    [{seg.t0_ms}–{seg.t1_ms} ms]
                  </span>
                  <span class="text-gray-800">{seg.text}</span>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>

      <Show when={status() === "active" && segments().length === 0}>
        <p class="text-xs text-gray-400 italic">Waiting for transcript segments…</p>
      </Show>
    </div>
  );
};

export default MeetingPanel;
