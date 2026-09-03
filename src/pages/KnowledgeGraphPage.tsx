/**
 * KnowledgeGraphPage — T076 (US4).
 * Shows a full-screen progress overlay while NER extraction is running for any
 * source in the active notebook. Once ALL sources are done, the graph is revealed.
 */
import { Component, createSignal, onMount, onCleanup, Show, For } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import GraphCanvas from "../components/knowledge-graph/GraphCanvas";
import RelationTypesPanel from "../components/knowledge-graph/RelationTypesPanel";
import { getKgStatus, KgSourceStatus } from "../api";

// Format seconds as "Xm Ys" or "Xs".
function fmtEta(secs: number): string {
  if (secs <= 0) return "almost done";
  if (secs < 60) return `${secs}s`;
  return `${Math.floor(secs / 60)}m ${secs % 60}s`;
}

const KnowledgeGraphPage: Component = () => {
  const [showRelationTypes, setShowRelationTypes] = createSignal(false);
  const [statuses, setStatuses] = createSignal<KgSourceStatus[]>([]);
  const [ready, setReady] = createSignal(false);
  const [selectedSourceIds, setSelectedSourceIds] = createSignal<string[]>([]);
  const [showSourceFilter, setShowSourceFilter] = createSignal(false);

  const refresh = async () => {
    try {
      const rows = await getKgStatus();
      setStatuses(rows);
      // Empty = no indexed sources yet; treat as ready (nothing to extract).
      setReady(rows.length === 0 || rows.every((r) => r.ner_status === "done"));
    } catch {
      setReady(true); // on error just show graph
    }
  };

  onMount(async () => {
    await refresh();

    const unlisten = await listen<{
      source_id: string;
      file_name: string;
      processed: number;
      total: number;
      eta_seconds: number | null;
    }>("kg://progress", (ev) => {
      setStatuses((prev) => {
        const exists = prev.some((s) => s.source_id === ev.payload.source_id);
        const status: KgSourceStatus = {
          source_id: ev.payload.source_id,
          file_name: ev.payload.file_name,
          ner_status: (ev.payload.processed >= ev.payload.total ? "done" : "running") as KgSourceStatus["ner_status"],
          ner_progress: ev.payload.total > 0
            ? Math.round((ev.payload.processed / ev.payload.total) * 100)
            : 0,
          ner_total: ev.payload.total,
        };
        const updated = exists
          ? prev.map((s) => s.source_id === ev.payload.source_id ? status : s)
          : [...prev, status];
        setReady(updated.every((r) => r.ner_status === "done"));
        return updated;
      });
    });

    onCleanup(unlisten);
  });

  // Compute aggregate progress across all non-done sources.
  const activeSource = () =>
    statuses().find((s) => s.ner_status === "running") ??
    statuses().find((s) => s.ner_status === "pending");

  const overallPct = () => {
    const all = statuses();
    if (all.length === 0) return 100;
    const done = all.filter((s) => s.ner_status === "done").length;
    const running = all.find((s) => s.ner_status === "running");
    const runningFraction = running ? running.ner_progress / 100 : 0;
    return Math.round(((done + runningFraction) / all.length) * 100);
  };

  const etaSecs = () => {
    const src = activeSource();
    if (!src || src.ner_progress >= 100) return 0;
    // Rough ETA from progress rate — precise ETA comes from kg://progress events
    // but we don't store it, so derive from progress percentage.
    return 0;
  };

  return (
    <div class="flex flex-col h-full bg-gray-950">
      <div class="px-4 py-3 border-b border-gray-800 flex-shrink-0 flex items-start justify-between gap-4">
        <div>
          <h1 class="text-sm font-semibold text-gray-200">Knowledge Graph</h1>
          <p class="text-xs text-gray-500 mt-0.5">
            Entities and relationships extracted from your indexed documents.
          </p>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          {/* Source filter dropdown */}
          <Show when={statuses().length > 0}>
            <div class="relative">
              <button
                type="button"
                onClick={() => setShowSourceFilter((v) => !v)}
                class={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg transition-colors ${
                  selectedSourceIds().length > 0
                    ? "bg-blue-600 hover:bg-blue-500 text-white"
                    : "bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-700"
                }`}
                title="Filter by document"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                    d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2a1 1 0 01-.293.707L13 13.414V19a1 1 0 01-.553.894l-4 2A1 1 0 017 21v-7.586L3.293 6.707A1 1 0 013 6V4z" />
                </svg>
                {selectedSourceIds().length > 0
                  ? `${selectedSourceIds().length} doc${selectedSourceIds().length > 1 ? "s" : ""}`
                  : "All docs"}
              </button>

              <Show when={showSourceFilter()}>
                <div class="absolute right-0 top-full mt-1 w-64 bg-gray-900 border border-gray-700 rounded-lg shadow-xl z-20 p-2">
                  <div class="flex items-center justify-between px-2 py-1 mb-1">
                    <span class="text-xs font-medium text-gray-400">Filter by document</span>
                    <button
                      type="button"
                      onClick={() => setSelectedSourceIds([])}
                      class="text-xs text-blue-400 hover:text-blue-300"
                    >
                      Clear
                    </button>
                  </div>
                  <For each={statuses().filter((s) => s.ner_status === "done")}>
                    {(src) => {
                      const checked = () => selectedSourceIds().includes(src.source_id);
                      return (
                        <label class="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-gray-800 cursor-pointer">
                          <input
                            type="checkbox"
                            checked={checked()}
                            onChange={() => {
                              setSelectedSourceIds((prev) =>
                                checked()
                                  ? prev.filter((id) => id !== src.source_id)
                                  : [...prev, src.source_id],
                              );
                            }}
                            class="accent-blue-500 flex-shrink-0"
                          />
                          <span class="text-xs text-gray-300 truncate" title={src.file_name}>
                            {src.file_name}
                          </span>
                        </label>
                      );
                    }}
                  </For>
                </div>
              </Show>
            </div>
          </Show>

          <button
            type="button"
            onClick={() => setShowRelationTypes((v) => !v)}
            class={`flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg transition-colors ${
              showRelationTypes()
                ? "bg-blue-600 hover:bg-blue-500 text-white"
                : "bg-gray-800 hover:bg-gray-700 text-gray-200 border border-gray-700"
            }`}
            title="Configure relation types"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
            Relation Types
          </button>
        </div>
      </div>

      <div class="flex-1 flex overflow-hidden relative">
        {/* Graph (always rendered but invisible behind overlay) */}
        <div
          class="flex-1 overflow-hidden"
          onClick={() => { if (showSourceFilter()) setShowSourceFilter(false); }}
        >
          <GraphCanvas sourceIds={selectedSourceIds} />
        </div>
        <Show when={showRelationTypes()}>
          <div class="w-80 flex-shrink-0">
            <RelationTypesPanel onClose={() => setShowRelationTypes(false)} />
          </div>
        </Show>

        {/* Progress overlay — shown while any source is still extracting */}
        <Show when={!ready()}>
          <div class="absolute inset-0 bg-gray-950/95 flex flex-col items-center justify-center z-10 gap-6">
            {/* Spinner */}
            <div class="w-12 h-12 rounded-full border-4 border-gray-700 border-t-blue-500 animate-spin" />

            <div class="text-center max-w-sm w-full px-8">
              <p class="text-sm font-medium text-gray-200 mb-1">
                Building Knowledge Graph
              </p>
              <Show when={activeSource()}>
                {(src) => (
                  <p class="text-xs text-gray-500 mb-4 truncate">
                    Extracting from {src().file_name}…
                  </p>
                )}
              </Show>

              {/* Overall progress bar */}
              <div class="w-full bg-gray-800 rounded-full h-2 mb-2">
                <div
                  class="bg-blue-500 h-2 rounded-full transition-all duration-500"
                  style={{ width: `${overallPct()}%` }}
                />
              </div>
              <div class="flex justify-between text-xs text-gray-500">
                <span>{overallPct()}%</span>
                <span>
                  {statuses().filter((s) => s.ner_status === "done").length}/
                  {statuses().length} files done
                </span>
              </div>

              {/* Per-source running bar */}
              <Show when={activeSource()?.ner_status === "running"}>
                <div class="mt-4">
                  <div class="w-full bg-gray-800 rounded-full h-1.5">
                    <div
                      class="bg-blue-400 h-1.5 rounded-full transition-all duration-300"
                      style={{ width: `${activeSource()?.ner_progress ?? 0}%` }}
                    />
                  </div>
                  <p class="text-xs text-gray-600 mt-1 text-right">
                    {activeSource()?.ner_progress ?? 0}% of current file
                  </p>
                </div>
              </Show>

              <p class="text-xs text-gray-600 mt-4">
                The graph will appear when extraction is complete.
              </p>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
};

export default KnowledgeGraphPage;
