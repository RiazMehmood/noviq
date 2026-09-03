/**
 * ChunkViewer — a slide-over drawer showing one document's chunks as they were
 * actually indexed.
 *
 * Serves two callers deliberately, because they want the same thing:
 *   - the admin Indexing console ("show me how this document was chunked");
 *   - an inline citation ("show me the passage this claim came from").
 *
 * The second is the NotebookLM behaviour we were missing: hovering a citation
 * previewed the excerpt, but clicking it did nothing, because a citation carried
 * no chunk id to navigate to. Passing `focusChunkId` scrolls that chunk into
 * view and highlights it, with its neighbours around it for context.
 *
 * Chunks are paged (a document can hold thousands), so the drawer fetches a
 * window at a time rather than the whole document.
 */
import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { downloadAuthed, openAuthedInNewTab, request } from "../api/transport";

export interface ChunkRow {
  id: string;
  chunk_index: number;
  unit_type: string;
  unit_label: string | null;
  unit_path: string | null;
  page_number: number | null;
  token_count: number;
  label_status: string | null;
  content: string;
}

interface BrowseResp {
  total: number;
  offset: number;
  chunks: ChunkRow[];
}

const PAGE_SIZE = 50;

const ChunkViewer: Component<{
  sourceId: string;
  fileName: string;
  /** Scroll to and highlight this chunk when the drawer opens. */
  focusChunkId?: string;
  /**
   * Which endpoint to read through. "admin" is the corpus-wide console view;
   * "user" is scoped to documents the caller may already see, and is what a
   * clicked citation must use — an officer is not an admin.
   */
  audience?: "admin" | "user";
  /**
   * The project this citation was opened from, when it was opened inside one.
   *
   * ⚠️ Not optional in practice for project documents. `source_chunks` resolves
   * visibility through `scope_for(project_id)`, and project scope is EXCLUSIVE —
   * without the project the caller's scope is the global corpus, which a project
   * document is deliberately not part of. The route then answers 404 "document
   * not found", so every citation opened inside a project failed, from the
   * inline `[n]` markers as well as the Sources list. The server has taken
   * `projectId` since F027; the client simply never sent it.
   */
  projectId?: string | null;
  onClose: () => void;
}> = (props) => {
  const [rows, setRows] = createSignal<ChunkRow[]>([]);
  const [total, setTotal] = createSignal(0);
  const [offset, setOffset] = createSignal(0);
  const [query, setQuery] = createSignal("");
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  // Separate from `error` (which is worded for chunk loading): failures of the
  // "Open document" button. Surfaced, never swallowed.
  const [fileErr, setFileErr] = createSignal<string | null>(null);

  const load = async (nextOffset: number, q: string, aroundChunkId?: string) => {
    setLoading(true);
    setError(null);
    try {
      let resp: BrowseResp;
      if ((props.audience ?? "admin") === "admin") {
        const params = new URLSearchParams({
          source_id: props.sourceId,
          offset: String(nextOffset),
          limit: String(PAGE_SIZE),
        });
        if (q.trim()) params.set("q", q.trim());
        resp = await request<BrowseResp>("GET", `/admin/chunks?${params}`);
      } else {
        resp = await request<BrowseResp>("POST", "/source_chunks", {
          sourceId: props.sourceId,
          aroundChunkId: aroundChunkId,
          offset: nextOffset,
          limit: PAGE_SIZE,
          // camelCase deliberately — the server struct is `rename_all =
          // "camelCase"` and reads `projectId` (it aliases `project_id` too).
          projectId: props.projectId ?? undefined,
        });
      }
      setRows(resp.chunks);
      setTotal(resp.total);
      setOffset(resp.offset);
    } catch (e) {
      // Surface the failure — a silently empty drawer reads as "no chunks",
      // which is a very different and much worse message than "load failed".
      setError(e instanceof Error ? e.message : String(e));
      setRows([]);
    } finally {
      setLoading(false);
    }
  };

  createEffect(() => {
    // Re-fetch whenever the drawer is pointed at a different document. When a
    // citation supplied a chunk, ask the server to centre the window on it —
    // the cited passage may be chunk 900 of 2,000, and page 1 would not contain it.
    const id = props.sourceId;
    if (id) void load(0, "", props.focusChunkId);
  });

  // Once rows land, bring the cited chunk into view.
  createEffect(() => {
    const focus = props.focusChunkId;
    if (!focus || !rows().length) return;
    requestAnimationFrame(() => {
      document
        .querySelector(`[data-chunk-id="${CSS.escape(focus)}"]`)
        ?.scrollIntoView({ block: "center", behavior: "smooth" });
    });
  });

  const pageStart = () => (total() === 0 ? 0 : offset() + 1);
  const pageEnd = () => Math.min(offset() + rows().length, total());

  return (
    <div class="fixed inset-0 z-50 flex justify-end bg-black/30" onClick={props.onClose}>
      <div
        class="flex h-full w-full max-w-3xl flex-col bg-white shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header class="flex items-start justify-between gap-3 border-b border-noviq-line px-5 py-4">
          <div class="min-w-0">
            <h2 class="truncate text-[15px] font-semibold text-noviq-ink">{props.fileName}</h2>
            <p class="mt-0.5 text-[12px] text-noviq-500">
              <Show when={total() > 0} fallback="no chunks">
                showing {pageStart()}–{pageEnd()} of {total()} chunks
              </Show>
            </p>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            {/* Open the ORIGINAL file (2026-07-28): the chunk list answers
                "what did it index?", this answers "what does the page say?".
                Jumps to the focused chunk's page when we know it. */}
            <button
              class="rounded-lg border border-noviq-line px-2.5 py-1.5 text-[12px] hover:bg-noviq-50"
              title="Open the original document in a new tab"
              onClick={async () => {
                setFileErr(null);
                try {
                  const page = rows().find((c) => c.id === props.focusChunkId)?.page_number;
                  // `source_file` is scoped exactly like `source_chunks` above,
                  // so it needs the project too or a project document is outside
                  // the caller's scope and the route answers 404. It is a GET,
                  // so the project goes in the QUERY STRING as `project_id`
                  // (snake — `ProjectQuery`), not the camelCase used in bodies.
                  const qs = props.projectId
                    ? `?project_id=${encodeURIComponent(props.projectId)}`
                    : "";
                  const ok = await openAuthedInNewTab(
                    `/sources/${encodeURIComponent(props.sourceId)}/file${qs}`,
                    page != null ? `#page=${page}` : "",
                  );
                  if (!ok) setFileErr("Pop-up blocked — allow pop-ups for this site.");
                } catch (e) {
                  setFileErr(e instanceof Error ? e.message : "Could not open the document.");
                }
              }}
            >
              Open document ↗
            </button>
            {/* Export and search are admin-only endpoints — don't render controls
                for an officer that would only come back 403. */}
            <Show when={(props.audience ?? "admin") === "admin"}>
              <button
                class="rounded-lg border border-noviq-line px-2.5 py-1.5 text-[12px] hover:bg-noviq-50"
                onClick={() =>
                  downloadAuthed(
                    `/admin/chunks/export?source_id=${encodeURIComponent(props.sourceId)}`,
                    `chunks-${props.fileName}.csv`,
                  )
                }
              >
                Download CSV
              </button>
            </Show>
            <button
              class="rounded-lg px-2 py-1.5 text-[16px] leading-none text-noviq-500 hover:bg-noviq-50"
              onClick={props.onClose}
              aria-label="Close"
            >
              ×
            </button>
          </div>
        </header>

        <Show when={(props.audience ?? "admin") === "admin"}>
        <div class="border-b border-noviq-line px-5 py-2.5">
          <input
            class="w-full rounded-lg border border-noviq-line px-3 py-1.5 text-[13px] outline-none focus:border-noviq-400"
            placeholder="Filter by provision label or text…"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void load(0, query());
            }}
          />
        </div>
        </Show>

        <div class="flex-1 overflow-y-auto px-5 py-3">
          <Show when={error()}>
            <p class="rounded-lg bg-red-50 px-3 py-2 text-[12px] text-red-700">
              Could not load chunks: {error()}
            </p>
          </Show>
          <Show when={fileErr()}>
            <p class="mb-2 rounded-lg bg-red-50 px-3 py-2 text-[12px] text-red-700">{fileErr()}</p>
          </Show>
          <Show when={loading()}>
            <p class="py-6 text-center text-[12px] text-noviq-500">Loading…</p>
          </Show>
          <Show when={!loading() && !error() && rows().length === 0}>
            <p class="py-6 text-center text-[12px] text-noviq-500">No chunks match.</p>
          </Show>
          <For each={rows()}>
            {(c) => (
              <article
                data-chunk-id={c.id}
                class={`mb-2 rounded-xl border px-3.5 py-2.5 ${
                  c.id === props.focusChunkId
                    ? "border-[#0c96ff] bg-[#0c96ff]/[.06]"
                    : "border-noviq-line"
                }`}
              >
                <div class="mb-1 flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                  <span class="font-mono text-[11px] text-noviq-500">#{c.chunk_index}</span>
                  <span class="text-[12px] font-semibold text-noviq-ink">
                    {/* Unresolved labels drop the (guessed) unit word — show the
                        bare parent-child ordinal; the amber badge says why. */}
                    {c.label_status === "unresolved"
                      ? (c.unit_label || c.unit_type).replace(
                          /^(article|rule|section|regulation|schedule|chapter|part|clause|annexure?|appendix)\s+/i,
                          "",
                        )
                      : c.unit_label || c.unit_type}
                  </span>
                  <Show when={c.page_number != null}>
                    <span class="text-[11px] text-noviq-500">p.{c.page_number}</span>
                  </Show>
                  <span class="text-[11px] text-noviq-500">{c.token_count} tok</span>
                  {/* The importer has always written this flag and nothing ever
                      showed it — it is the parse-health signal. */}
                  <Show when={c.label_status && c.label_status !== "resolved"}>
                    <span class="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium text-amber-800">
                      {c.label_status}
                    </span>
                  </Show>
                </div>
                <Show when={c.unit_path}>
                  <div class="mb-1 truncate text-[11px] text-noviq-500">{c.unit_path}</div>
                </Show>
                <p class="whitespace-pre-wrap text-[12.5px] leading-snug text-noviq-700">
                  {c.content}
                </p>
              </article>
            )}
          </For>
        </div>

        <footer class="flex items-center justify-between border-t border-noviq-line px-5 py-3">
          <button
            class="rounded-lg border border-noviq-line px-3 py-1.5 text-[12px] disabled:opacity-40 hover:enabled:bg-noviq-50"
            disabled={offset() === 0 || loading()}
            onClick={() => void load(Math.max(0, offset() - PAGE_SIZE), query())}
          >
            ← Previous
          </button>
          <button
            class="rounded-lg border border-noviq-line px-3 py-1.5 text-[12px] disabled:opacity-40 hover:enabled:bg-noviq-50"
            disabled={pageEnd() >= total() || loading()}
            onClick={() => void load(offset() + PAGE_SIZE, query())}
          >
            Next →
          </button>
        </footer>
      </div>
    </div>
  );
};

export default ChunkViewer;
