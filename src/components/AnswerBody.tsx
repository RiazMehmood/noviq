/**
 * AnswerBody — renders an assistant answer as Markdown AND turns each inline
 * citation marker `[n]` into a hoverable chip. Hovering shows a NotebookLM-style
 * popover with the source (instrument · provision · page) and the chunk excerpt
 * the citation came from. Works for BOTH normal and deep answers — they share the
 * same enriched citation shape ({ n, instrument, path, verbatim, page }).
 *
 * Pipeline mirrors <Markdown>: linkify [n] → marked → sanitizeAnswerHtml →
 * innerHTML. Because innerHTML content can't carry Solid handlers, the popover
 * uses event delegation on the container.
 *
 * ⚠️ Sanitising goes through `lib/sanitizeHtml`, never `DOMPurify.sanitize`
 * directly — the default profile permits <img> and remote src, which is the
 * zero-click exfiltration hole S1 closed. See that module's header.
 */
import { Component, createMemo, createSignal, Show } from "solid-js";
import { marked } from "marked";
import { sanitizeAnswerHtml } from "../lib/sanitizeHtml";
import type { Citation, WebResult } from "../api/assistant";
import ChunkViewer from "./ChunkViewer";
import { openAuthedInNewTab } from "../api/transport";

marked.setOptions({ gfm: true, breaks: true });

const AnswerBody: Component<{
  text: string;
  citations: Citation[];
  /** Web sources for this turn, so `[W1]` markers become links to the page. */
  webResults?: WebResult[];
  /** The project this answer belongs to — required for a citation to open a
   *  project document (see the `projectId` note on `ChunkViewer`). */
  projectId?: string | null;
}> = (props) => {
  const byN = createMemo(() => {
    const m = new Map<number, Citation>();
    for (const c of props.citations ?? []) m.set(c.n, c);
    return m;
  });

  // Replace bare `[n]` markers with a hoverable <sup> carrying data-n. Only when a
  // citation with that number exists (so stray brackets stay plain text).
  const html = createMemo(() => {
    const linked = (props.text ?? "").replace(/\[(\d{1,3})\]/g, (whole, num) => {
      const c = byN().get(Number(num));
      if (!c) return whole;
      // Openable citations get the pointer cursor; hover-only ones keep `help`,
      // so the affordance matches what actually happens on click.
      const cls = c.chunk_id && c.source_id ? "nq-cite nq-cite-open" : "nq-cite";
      return `<sup class="${cls}" data-n="${num}">[${num}]</sup>`;
    });
    // `[W1]` markers become links to the page itself. An indexed citation opens
    // the document in the viewer; a web source has no chunk to show, so the
    // honest equivalent is the source. Amber, matching the "From the web" block,
    // so a reader never mistakes one for an indexed instrument.
    const web = props.webResults ?? [];
    const withWeb = linked.replace(/\[W(\d{1,3})\]/g, (whole, num) => {
      const r = web[Number(num) - 1];
      if (!r?.url) return whole;
      const safeTitle = (r.title ?? r.url).replace(/"/g, "&quot;");
      return (
        `<sup class="nq-web-cite"><a href="${r.url}" target="_blank" ` +
        `rel="noopener noreferrer nofollow" title="${safeTitle}">[W${num}]</a></sup>`
      );
    });
    const raw = marked.parse(withWeb, { async: false }) as string;
    return sanitizeAnswerHtml(raw, ["data-n", "target", "rel"]);
  });

  const [pop, setPop] = createSignal<{ x: number; y: number; c: Citation } | null>(null);
  // The citation whose source document is open in the viewer.
  const [open, setOpen] = createSignal<Citation | null>(null);
  // Failure text for the "open original" link — never swallowed silently.
  const [fileErr, setFileErr] = createSignal<string | null>(null);

  /**
   * Open the ORIGINAL document (the indexed PDF) in a new tab, jumped to the
   * cited page (2026-07-28 tester request — they verify page numbers against
   * the printed book). Access is checked server-side against the same
   * visibility rules as retrieval.
   */
  const openOriginal = async (c: Citation, e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setFileErr(null);
    try {
      // `source_file` scopes through `scope_for(project_id)` exactly as
      // `source_chunks` does, and takes the project as a QUERY param because it
      // is a GET with a path param. Without it a project document is outside the
      // caller's scope and the route answers 404 — the same failure the viewer
      // had. `ProjectQuery` reads `project_id` (snake), not camelCase.
      const pid = props.projectId;
      const qs = pid ? `?project_id=${encodeURIComponent(pid)}` : "";
      const opened = await openAuthedInNewTab(
        `/sources/${encodeURIComponent(c.source_id!)}/file${qs}`,
        c.page != null ? `#page=${c.page}` : "",
      );
      if (!opened) setFileErr("Your browser blocked the new tab — allow pop-ups for this site.");
    } catch (err) {
      setFileErr(err instanceof Error ? err.message : "Could not open the document.");
    }
  };

  // Click opens the source AT the cited passage — the other half of the
  // NotebookLM behaviour. Only citations that carry a chunk can be opened;
  // a whole-document summary has no single passage to jump to, so it stays
  // hover-only rather than presenting a link that goes nowhere.
  const onClick = (e: MouseEvent) => {
    const el = (e.target as HTMLElement)?.closest?.(".nq-cite") as HTMLElement | null;
    if (!el) return;
    const c = byN().get(Number(el.dataset.n));
    if (!c?.chunk_id || !c?.source_id) return;
    e.preventDefault();
    setPop(null);
    setOpen(c);
  };

  const onOver = (e: MouseEvent) => {
    const el = (e.target as HTMLElement)?.closest?.(".nq-cite") as HTMLElement | null;
    if (!el) return;
    const n = Number(el.dataset.n);
    const c = byN().get(n);
    if (!c) return;
    const r = el.getBoundingClientRect();
    setPop({ x: r.left + r.width / 2, y: r.top, c });
  };
  const onOut = (e: MouseEvent) => {
    const to = e.relatedTarget as HTMLElement | null;
    if (to?.closest?.(".nq-cite") || to?.closest?.(".nq-pop")) return;
    setPop(null);
  };

  return (
    <>
      <style>{`
        .nq-cite {
          cursor: help; color: #0c96ff; font-weight: 600; font-size: .72em;
          padding: 0 .1em; border-radius: 3px; transition: background .12s;
        }
        .nq-cite:hover { background: rgba(12,150,255,.14); }
        .nq-cite-open { cursor: pointer; }
        /* Web markers are AMBER, never the citation blue: a reader must be able
           to tell an indexed instrument from an unverified web page at a glance,
           without reading the marker. */
        .nq-web-cite a {
          cursor: pointer; color: #b45309; font-weight: 600; font-size: .72em;
          padding: 0 .1em; border-radius: 3px; text-decoration: none;
          transition: background .12s;
        }
        .nq-web-cite a:hover { background: rgba(180,83,9,.14); text-decoration: underline; }
      `}</style>
      <div class="prose prose-sm max-w-none text-[15px] text-noviq-ink
                  prose-headings:text-noviq-ink prose-headings:font-semibold
                  prose-h1:text-lg prose-h2:text-base prose-h3:text-[15px]
                  prose-p:my-2 prose-li:my-0.5 prose-ul:my-2 prose-ol:my-2
                  prose-strong:text-noviq-ink prose-a:text-noviq-600
                  prose-table:text-sm prose-th:text-noviq-600
                  prose-code:text-noviq-700 prose-code:bg-noviq-panel prose-code:px-1 prose-code:rounded"
        onMouseOver={onOver}
        onMouseOut={onOut}
        onClick={onClick}
        innerHTML={html()}
      />
      <Show when={pop()}>
        {(p) => (
          <div
            class="nq-pop fixed z-50 w-[320px] max-w-[85vw] -translate-x-1/2 -translate-y-full
                   rounded-xl border border-noviq-line bg-white p-3 shadow-xl text-left"
            style={{ left: `${p().x}px`, top: `${p().y - 8}px` }}
          >
            <div class="flex items-baseline gap-1.5 mb-1.5">
              <span class="font-mono text-[11px] font-semibold text-[#0c96ff]">[{p().c.n}]</span>
              <span class="text-[12px] font-semibold text-noviq-ink truncate">{p().c.instrument}</span>
              {/* F027 (FR-016) — in a project answer that also drew on the law,
                  a reader must be able to tell at a glance which is which. An
                  uploaded draft read as enacted law is the exact failure
                  Citation Discipline exists to prevent. */}
              <Show when={p().c.provenance === "corpus"}>
                <span class="shrink-0 rounded bg-noviq-50 px-1.5 py-0.5 text-[10px] font-medium text-noviq-600">
                  law
                </span>
              </Show>
              <Show when={p().c.provenance === "project"}>
                <span class="shrink-0 rounded bg-amber-50 px-1.5 py-0.5 text-[10px] font-medium text-amber-800">
                  your document
                </span>
              </Show>
            </div>
            <Show when={p().c.path || p().c.page != null}>
              <div class="text-[11px] text-noviq-500 mb-1.5">
                <Show when={p().c.path}>{p().c.path}</Show>
                <Show when={p().c.page != null}> · p.{p().c.page}</Show>
              </div>
            </Show>
            <div class="max-h-40 overflow-auto text-[12px] leading-snug text-noviq-700 whitespace-pre-wrap">
              {p().c.verbatim || "(source excerpt unavailable)"}
            </div>
            <Show when={p().c.chunk_id && p().c.source_id}>
              <div class="mt-2 flex items-center justify-between gap-2 border-t border-noviq-line pt-1.5 text-[11px]">
                <span class="text-[#0c96ff]">Click to open the source</span>
                <button
                  type="button"
                  class="shrink-0 rounded border border-noviq-line px-1.5 py-0.5 text-noviq-600
                         hover:border-[#0c96ff] hover:text-[#0c96ff]"
                  title={
                    p().c.page != null
                      ? `Open the original document at page ${p().c.page} in a new tab`
                      : "Open the original document in a new tab"
                  }
                  onClick={(e) => void openOriginal(p().c, e)}
                >
                  Open document ↗
                </button>
              </div>
              <Show when={fileErr()}>
                <div class="mt-1 text-[11px] text-red-500">{fileErr()}</div>
              </Show>
            </Show>
          </div>
        )}
      </Show>
      <Show when={open()}>
        {(c) => (
          <ChunkViewer
            sourceId={c().source_id!}
            fileName={c().instrument}
            focusChunkId={c().chunk_id!}
            audience="user"
            projectId={props.projectId}
            onClose={() => setOpen(null)}
          />
        )}
      </Show>
    </>
  );
};

export default AnswerBody;
