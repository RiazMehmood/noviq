// Feature 027 — the right rail INSIDE a project chat.
//
// Why not `DocumentTree`: that component loads the whole indexed corpus (folders,
// categories, per-folder fetches) and has no scoping prop. Rendering it inside a
// project is what made a project chat look like it already held every document.
// A project's documents are a flat list the store already has, so this is a small
// dedicated panel rather than a scoping bolt-on to a much larger component.
//
// 2026-08-29 — the project overview page was retired, so this rail is now the
// ONLY place a project's documents are managed: search, tick to scope, add, and
// remove all live here.
//
// ⚠️ Presentation only. The server scopes retrieval from `conversation_id` via
// `project_chat` (FR-038, ADR-0028); nothing here is the access boundary.

import { createMemo, createSignal, For, Show } from "solid-js";
import type { ProjectDocument } from "../stores/projects";

interface Props {
  documents: ProjectDocument[];
  /** Selected source ids, or `null` for "all of them" (the default). */
  selected: string[] | null;
  onSelectionChange: (selected: string[] | null) => void;
  /** Open the add-documents dialog. Owned by the page, not by this rail. */
  onAdd: () => void;
  /** Remove one document from the project. The caller confirms and reports. */
  onRemove: (doc: ProjectDocument) => void;
}

// ⚠️ The per-question "also search the shared corpus" tick is NOT here. It already
// exists as `CorpusToggle` in the composer, driven by `corpusThisTurn` and cleared
// on every send (FR-017). A second control for the same flag would be two sources
// of truth for a setting whose whole contract is that it does not persist.

/** A document still indexing must LOOK like it (FR-008), never silently absent. */
function statusPill(status: ProjectDocument["status"]) {
  if (status === "ready") return null;
  const label = status === "pending" ? "indexing…" : "failed";
  const tone =
    status === "pending"
      ? "bg-noviq-100 text-noviq-700"
      : "bg-red-100 text-red-800";
  return <span class={`ml-2 rounded px-1.5 py-0.5 text-[10px] ${tone}`}>{label}</span>;
}

const label = (d: ProjectDocument) => d.display_name || d.file_name;

export default function ProjectDocuments(props: Props) {
  // Name filter. Local to the rail: it narrows what you can SEE and tick, and
  // never what a question searches — that is the tick-boxes' job alone.
  const [filter, setFilter] = createSignal("");

  const isOn = (id: string) => props.selected === null || props.selected.includes(id);
  const ready = () => props.documents.filter((d) => d.status === "ready");

  const shown = createMemo(() => {
    const q = filter().trim().toLowerCase();
    if (!q) return props.documents;
    return props.documents.filter((d) => label(d).toLowerCase().includes(q));
  });

  function toggle(id: string, on: boolean) {
    // `null` means "all"; materialise it before removing one, so unchecking the
    // first document does not read as "select nothing".
    const base = props.selected === null ? ready().map((d) => d.source_id) : props.selected;
    const next = on ? [...new Set([...base, id])] : base.filter((x) => x !== id);
    props.onSelectionChange(next.length === ready().length ? null : next);
  }

  return (
    <div class="flex h-full flex-col bg-noviq-paper text-noviq-ink">
      {/* The document COUNT is not repeated here — the rail header above states
          it, the same way the corpus rail states its size once. */}
      <Show when={props.documents.length > 0}>
        <div class="relative px-3 pb-2 pt-3">
          <svg
            class="pointer-events-none absolute left-5 top-1/2 h-4 w-4 -translate-y-1/2 text-noviq-400"
            viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
            stroke-linecap="round" stroke-linejoin="round"
          >
            <path d="M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16zm10 2-4.35-4.35" />
          </svg>
          <input
            type="search"
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
            placeholder="Search documents..."
            aria-label="Search this project's documents"
            class="w-full rounded-xl border border-noviq-line bg-white py-2 pl-9 pr-3 text-[12px]
                   text-noviq-ink placeholder:text-noviq-400 focus:border-noviq-400 focus:outline-none"
          />
        </div>

        <div class="flex items-center justify-between gap-2 px-3 pb-1.5">
          <span class="text-[10px] font-semibold uppercase tracking-wide text-noviq-500">
            {filter().trim() ? "Matching documents" : "In scope"}
          </span>
          <Show when={props.selected !== null}>
            <button
              class="text-[11px] text-noviq-600 hover:text-noviq-800"
              onClick={() => props.onSelectionChange(null)}
            >
              Select all
            </button>
          </Show>
        </div>
      </Show>

      <div class="flex-1 overflow-y-auto px-2 pb-2">
        <Show
          when={props.documents.length > 0}
          fallback={
            <p class="px-3 py-8 text-center text-xs leading-relaxed text-noviq-600">
              Documents you add appear here. Tick them to narrow a question to a few.
            </p>
          }
        >
          <Show
            when={shown().length > 0}
            fallback={
              <p class="px-3 py-6 text-center text-xs text-noviq-500">
                No document here matches “{filter().trim()}”.
              </p>
            }
          >
            <For each={shown()}>
              {(d) => (
                // ⚠️ The row is NOT one big `<label>`. A remove button inside a
                // label is still part of the label, so clicking it would toggle
                // the checkbox on the way out — removing a document and silently
                // changing the scope of the next question at the same time.
                <div class="group flex items-start gap-1 rounded px-1 hover:bg-noviq-50">
                  <label
                    class="flex min-w-0 flex-1 cursor-pointer items-start gap-2 py-1.5 pl-1 text-sm"
                    title={label(d)}
                  >
                    <input
                      type="checkbox"
                      class="mt-0.5"
                      checked={isOn(d.source_id)}
                      disabled={d.status !== "ready"}
                      onChange={(e) => toggle(d.source_id, e.currentTarget.checked)}
                    />
                    <span class="min-w-0 flex-1 break-words">
                      {label(d)}
                      {statusPill(d.status)}
                    </span>
                  </label>
                  {/* Revealed on hover on a pointer device; always visible on a
                      touch screen, where there is no hover to reveal it with. */}
                  <button
                    type="button"
                    onClick={() => props.onRemove(d)}
                    title={`Remove "${label(d)}" from this project`}
                    aria-label={`Remove ${label(d)} from this project`}
                    class="mt-1 flex-shrink-0 rounded p-1 text-noviq-400 opacity-100 transition-opacity
                           hover:bg-red-50 hover:text-red-600 focus:opacity-100
                           lg:opacity-0 lg:group-hover:opacity-100 lg:group-focus-within:opacity-100"
                  >
                    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2m2 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"
                        stroke-linecap="round" stroke-linejoin="round" />
                    </svg>
                  </button>
                </div>
              )}
            </For>
          </Show>
        </Show>
      </div>

      <div class="flex flex-col gap-2 border-t border-noviq-line px-3 py-3">
        {/* Pinned rather than tucked into the header: adding documents is the
            commonest thing to do in a young project, and it should not be a
            16-pixel target beside the collapse chevron. */}
        <button
          type="button"
          onClick={props.onAdd}
          class="flex items-center justify-center gap-2 rounded-lg bg-noviq-700 py-2 text-sm
                 font-medium text-white transition-colors hover:bg-noviq-800"
        >
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12 5v14M5 12h14" stroke-linecap="round" />
          </svg>
          Add documents
        </button>
        <p class="text-center text-[10px] text-noviq-500">
          Answers here are drawn from these documents only.
        </p>
      </div>
    </div>
  );
}
