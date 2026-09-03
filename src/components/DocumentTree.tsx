/**
 * DocumentTree — a 2-level "category → documents" tree of the indexed corpus,
 * with checkboxes for scoping a query/draft to specific documents.
 *
 * Used by both the Chat and Drafting sidebars (Noviq paper theme). All
 * documents are selected by default; the user may deselect to narrow scope.
 *
 * Selection is reported via `onSelectionChange(selectedSourceIds, allSelected)`:
 *   - `allSelected` true  → no restriction (treat as "every document").
 *   - `allSelected` false → restrict the query to `selectedSourceIds`.
 *
 * **Grouped by CATEGORY, not by folder (migration 021).** It used to show the
 * upload folders — which meant the whole shared corpus sat under one node called
 * "admin", an implementation detail of where the files were written, and the
 * admin's categories were invisible here. Folders still carry the F021 ownership
 * distinction, so that is not thrown away: it moves to a per-document badge, and
 * the viewer's own private uploads get their own group instead of being mixed
 * into Uncategorised with the shared corpus.
 */
import { Component, For, Show, createEffect, createMemo, createSignal, onMount } from "solid-js";
import {
  IndexedFolder,
  Source,
  listIndexedFolders,
  listSourcesInFolder,
} from "../api";
import { categories, loadCategories } from "../stores/categories";

interface Props {
  /** Called whenever the selection changes (and once after initial load). */
  onSelectionChange?: (selectedSourceIds: string[], allSelected: boolean) => void;
  /** Called once after load with the flat catalog (id + name) of every document,
   *  so callers (e.g. the chat composer) can offer @-mention scoping without
   *  re-fetching the corpus. */
  onCatalog?: (docs: { id: string; name: string }[]) => void;
  /** Optional refresh trigger — bump to reload folders/sources. */
  refreshKey?: number;
  /**
   * Corpus size, for a header rendered OUTSIDE this component (the chat rail's
   * "Repository" block). Reported from here rather than counted again by the
   * caller so the number always describes the list directly beneath it —
   * `files` excludes withdrawn documents, exactly like `allIds()`.
   */
  onStats?: (stats: { files: number; groups: number }) => void;
}

interface FolderNode {
  folder: IndexedFolder;
  sources: Source[];
}

/** One document plus the ownership context its folder used to convey. */
interface DocRow {
  src: Source;
  /** `null` for the shared corpus (owner_id NULL) — the ordinary case, no badge. */
  badge: string | null;
  badgeTitle: string;
}

/** A rendered group: a category, Uncategorised, or the viewer's own uploads. */
interface DocGroup {
  id: string;
  name: string;
  docs: DocRow[];
}

/**
 * Migration 021 — a document the admin has withdrawn from service.
 *
 * It is still LISTED (an officer who used it yesterday needs to see where it
 * went) but must never be selectable: `visible_source_ids` excludes it server
 * side, so a question scoped to it would retrieve nothing and refuse, with the
 * picker having shown it as available. The scope this component offers and the
 * scope retrieval honours have to be the same set.
 */
const isWithdrawn = (s: Source) => !!s.disabled_at;

const DocumentTree: Component<Props> = (props) => {
  const [nodes, setNodes] = createSignal<FolderNode[]>([]);
  const [loading, setLoading] = createSignal(true);
  // Selected source ids (the set of *checked* documents).
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  // EXPANDED rather than collapsed, so the default (empty set) is "everything
  // closed" — inverted 2026-08-14. With one category per Sindh department the
  // corpus reaches ~50 groups and several thousand documents; opening them all
  // by default buries the group names themselves, which are the only way to
  // navigate that many files. A group appearing later now starts closed too,
  // which is the wanted behaviour here (it was deliberately the opposite when
  // the list was nineteen documents).
  //
  // Expansion is presentation only — it does NOT change the retrieval scope.
  // Every available document stays SELECTED whether or not its group is open,
  // so collapsing a group never silently narrows what a question can reach.
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
  /**
   * Free-text filter over department (group) names AND document names. At ~50
   * departments and several thousand files, scrolling was the only way to reach
   * a known department — the closed-by-default groups made the list navigable
   * but not searchable.
   */
  const [filter, setFilter] = createSignal("");
  const filterActive = () => filter().trim().length > 0;
  // While filtering, every surviving group is open: a hit inside a closed group
  // would otherwise render as a group row with nothing visible under it.
  const isOpen = (id: string) => expanded().has(id) || filterActive();

  /**
   * The documents, grouped for display: categories in the admin's display order,
   * then Uncategorised, then the viewer's own uploads last.
   *
   * Empty groups are dropped — a category with nothing in it is managed in the
   * admin console, not stared at in a narrow sidebar. When no categories exist
   * at all this collapses to a single unlabelled group, i.e. the flat list this
   * component showed before 021.
   */
  const groups = createMemo<DocGroup[]>(() => {
    const cats = categories();
    const shared: DocRow[] = [];
    const mine: DocRow[] = [];

    for (const node of nodes()) {
      // A PRIVATE folder returned to this viewer is their own: the folder list
      // is already scoped to "mine + shared", so no ownership test is needed
      // beyond visibility.
      const isMine = node.folder.visibility === "private";
      const owner = node.folder.owner_username;
      for (const src of node.sources) {
        if (isMine) {
          mine.push({
            src,
            badge: "Private",
            badgeTitle: "Private — only you can use this in answers",
          });
        } else {
          shared.push({
            src,
            // Someone else's shared upload keeps its attribution; the global
            // corpus (owner_id NULL) carries no badge, which is the common case.
            badge: owner ? `Shared · ${owner}` : null,
            badgeTitle: owner ? `Shared by ${owner}` : "",
          });
        }
      }
    }

    const out: DocGroup[] = [];
    for (const c of cats) {
      const docs = shared.filter((d) => d.src.category_id === c.id);
      if (docs.length) out.push({ id: `cat:${c.id}`, name: c.name, docs });
    }
    // A document filed into a category that has since been deleted reads as
    // Uncategorised — which is also what the database says, since
    // `sources.category_id` is ON DELETE SET NULL.
    const loose = shared.filter(
      (d) => !d.src.category_id || !cats.some((c) => c.id === d.src.category_id),
    );
    if (loose.length) {
      out.push({
        id: "cat:none",
        // Unlabelled when there is nothing to contrast it with, so a corpus with
        // no categories looks exactly as it did before.
        name: cats.length > 0 ? "Uncategorised" : "",
        docs: loose,
      });
    }
    if (mine.length) out.push({ id: "mine", name: "My documents", docs: mine });
    return out;
  });

  /** Every SELECTABLE document id — withdrawn ones are not offerable as scope. */
  const allIds = () =>
    nodes().flatMap((n) => n.sources.filter((s) => !isWithdrawn(s)).map((s) => s.id));

  /** Thousands separators — at ~4,000 documents "3976" is hard to read at 10px. */
  const fmt = (n: number) => n.toLocaleString();

  /**
   * Header counts. `totalDocs` counts what is USABLE, matching `allIds()` and so
   * the "All" button — counting withdrawn documents here would advertise a
   * corpus larger than any question can reach.
   */
  const totalDocs = createMemo(() => allIds().length);
  const selectedCount = createMemo(() => {
    const sel = selected();
    return allIds().filter((id) => sel.has(id)).length;
  });

  /**
   * What the list RENDERS: groups whose name matches the filter (whole group
   * kept), plus groups holding a matching document (only those documents).
   *
   * `group` is carried through unchanged so the group checkbox keeps speaking
   * for the whole department — filtering is a view, and a tick applied to a
   * filtered row must not silently narrow the scope to the visible subset.
   */
  const visibleGroups = createMemo<Array<{ group: DocGroup; docs: DocRow[] }>>(() => {
    const q = filter().trim().toLowerCase();
    const all = groups();
    if (!q) return all.map((group) => ({ group, docs: group.docs }));
    const out: Array<{ group: DocGroup; docs: DocRow[] }> = [];
    for (const group of all) {
      if (group.name && group.name.toLowerCase().includes(q)) {
        out.push({ group, docs: group.docs });
        continue;
      }
      const docs = group.docs.filter((d) => d.src.file_name.toLowerCase().includes(q));
      if (docs.length > 0) out.push({ group, docs });
    }
    return out;
  });

  // Corpus size up to the caller (the "Repository" rail header). An effect, not
  // a call inside `load()`, so it also fires when a reload changes the numbers.
  createEffect(() => {
    if (loading()) return;
    props.onStats?.({ files: totalDocs(), groups: groups().filter((g) => g.name).length });
  });

  const emit = () => {
    const sel = selected();
    const all = allIds();
    const allSelected = all.length > 0 && all.every((id) => sel.has(id));
    props.onSelectionChange?.([...sel], allSelected);
  };

  const load = async () => {
    setLoading(true);
    try {
      // Category names drive the group headers. Non-blocking and non-fatal: if
      // it fails the documents still list, as one unlabelled group.
      void loadCategories();
      const folders = await listIndexedFolders();
      const built: FolderNode[] = await Promise.all(
        folders.map(async (folder) => {
          let sources: Source[] = [];
          try {
            sources = await listSourcesInFolder(folder.id);
          } catch {
            /* folder may have no sources yet */
          }
          return { folder, sources };
        }),
      );
      setNodes(built);
      // Default: every AVAILABLE document selected + every folder expanded.
      setSelected(
        new Set<string>(
          built.flatMap((n) => n.sources.filter((s) => !isWithdrawn(s)).map((s) => s.id)),
        ),
      );
      setExpanded(new Set<string>()); // every group CLOSED on load — see the signal
      // The @-mention catalog omits withdrawn documents too — mentioning one
      // would scope the question to a document retrieval will not return.
      props.onCatalog?.(
        built.flatMap((n) =>
          n.sources.filter((s) => !isWithdrawn(s)).map((s) => ({ id: s.id, name: s.file_name })),
        ),
      );
      emit();
    } catch {
      setNodes([]);
    } finally {
      setLoading(false);
    }
  };

  onMount(load);
  // Reload when the parent bumps refreshKey.
  let lastKey = props.refreshKey;
  const maybeReload = () => {
    if (props.refreshKey !== lastKey) {
      lastKey = props.refreshKey;
      load();
    }
  };

  const toggleExpand = (groupId: string) => {
    setExpanded((prev) => {
      const next = new Set<string>(prev);
      next.has(groupId) ? next.delete(groupId) : next.add(groupId);
      return next;
    });
  };

  const toggleSource = (id: string) => {
    // Guard as well as disable the control: `selectAll`/`toggleFolder` are not
    // the only ways in, and a withdrawn id must never enter the selection.
    if (nodes().some((n) => n.sources.some((s) => s.id === id && isWithdrawn(s)))) return;
    setSelected((prev) => {
      const next = new Set<string>(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
    emit();
  };

  /** A group's selectable documents — the only ones its checkbox speaks for. */
  const selectableIn = (group: DocGroup) =>
    group.docs.filter((d) => !isWithdrawn(d.src)).map((d) => d.src);

  const groupState = (group: DocGroup): "all" | "none" | "some" => {
    const usable = selectableIn(group);
    // A group holding nothing BUT withdrawn documents reads as unselected
    // rather than fully selected — there is nothing in it to ask against.
    if (usable.length === 0) return "none";
    const sel = selected();
    const checked = usable.filter((s) => sel.has(s.id)).length;
    if (checked === 0) return "none";
    if (checked === usable.length) return "all";
    return "some";
  };

  const toggleGroup = (group: DocGroup) => {
    const state = groupState(group);
    const usable = selectableIn(group);
    setSelected((prev) => {
      const next = new Set<string>(prev);
      if (state === "all") {
        usable.forEach((s) => next.delete(s.id));
      } else {
        usable.forEach((s) => next.add(s.id));
      }
      return next;
    });
    emit();
  };

  const selectAll = (on: boolean) => {
    setSelected(on ? new Set<string>(allIds()) : new Set<string>());
    emit();
  };

  /**
   * The tick. `size` is not decoration: a document row and its parent group row
   * used to render the SAME 4×4 box, so a document read with the weight of a
   * folder and the two levels were told apart only by indentation. The group
   * keeps the larger box (it speaks for many documents); a leaf gets the smaller
   * one. See the matching icon pair below — together they carry the hierarchy.
   *
   * Both were taken down a step (12px/10px, was 14px/12px) at the architect's
   * request: the tick is a control the reader reaches for, not a thing to be
   * read, so it should sit under the folder glyph in weight rather than match
   * it. The ratio between the two levels is preserved — shrinking only the leaf
   * would have collapsed the size cue that tells a document from a department.
   */
  const Box: Component<{ state: "all" | "none" | "some"; size?: "sm" | "md" }> = (p) => (
    <span
      class="flex flex-shrink-0 items-center justify-center border bg-white transition-colors"
      classList={{
        "h-3 w-3 rounded-[3px]": (p.size ?? "md") === "md",
        "h-2.5 w-2.5 rounded-[3px]": p.size === "sm",
        // ALWAYS white, with the mark itself in green (architect, 2026-08-25).
        // The boxes used to FILL forest green when ticked, and since every
        // department is selected by default that made a column of ~51 solid
        // green blocks down the left edge — heavy, and carrying no information
        // at rest since they were all identical.
        //
        // The two levels are now told apart by size and by the weight of the
        // green: a group is the larger box with the darker border and mark, a
        // document the smaller and lighter one.
        "border-noviq-500 text-noviq-700": p.state === "all" && (p.size ?? "md") === "md",
        "border-noviq-300 text-noviq-500": p.state === "all" && p.size === "sm",
        "border-noviq-400 text-noviq-400": p.state === "some" && (p.size ?? "md") === "md",
        "border-noviq-300 text-noviq-300": p.state === "some" && p.size === "sm",
        "border-noviq-300 text-transparent": p.state === "none",
      }}
    >
      <Show when={p.state !== "none"}>
        <Show
          when={p.state === "all"}
          fallback={<span class="h-[1.5px] w-1 rounded-full bg-current" />}
        >
          <svg
            class="w-full"
            classList={{ "h-2": (p.size ?? "md") === "md", "h-[7px]": p.size === "sm" }}
            viewBox="0 0 12 12"
            fill="none"
            stroke="currentColor"
            stroke-width="2.4"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M2.5 6.3 4.8 8.6 9.5 3.6" />
          </svg>
        </Show>
      </Show>
    </span>
  );

  /** Folder glyph for a group row — filled when open, outline when closed. */
  const FolderIcon: Component<{ open: boolean }> = (p) => (
    <svg
      class="h-3.5 w-3.5 flex-shrink-0 text-noviq-400"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.4"
      stroke-linejoin="round"
    >
      <Show
        when={p.open}
        fallback={<path d="M1.8 4.2a1 1 0 0 1 1-1h3.1l1.4 1.6h5.9a1 1 0 0 1 1 1v6.2a1 1 0 0 1-1 1H2.8a1 1 0 0 1-1-1Z" />}
      >
        <path
          d="M1.8 4.2a1 1 0 0 1 1-1h3.1l1.4 1.6h5.9a1 1 0 0 1 1 1V7H1.8Z"
          fill="currentColor"
          stroke="none"
        />
        <path d="M1.8 7h12.4l-1.1 5.2a1 1 0 0 1-1 .8H2.8a1 1 0 0 1-1-1Z" />
      </Show>
    </svg>
  );

  /** Page glyph for a document row — replaces the 📄 emoji, which rendered at a
   *  different size on every platform and read as a sticker next to the tick. */
  const DocIcon: Component = () => (
    <svg
      class="h-3.5 w-3.5 flex-shrink-0 text-noviq-300"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      stroke-width="1.3"
      stroke-linejoin="round"
    >
      <path d="M4 2.2h4.6L12 5.6v8.2a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3.2a1 1 0 0 1 1-1Z" />
      <path d="M8.5 2.4v3.3h3.3" />
    </svg>
  );

  maybeReload();

  return (
    <div class="flex flex-col min-h-0">
      {/* Department search. The corpus size itself is stated by the rail header
          above (`onStats`), so it is not repeated here. */}
      <Show when={!loading() && nodes().length > 0}>
        <div class="relative px-1 pb-2">
          <svg
            class="pointer-events-none absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-noviq-400"
            viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
            stroke-linecap="round" stroke-linejoin="round"
          >
            <path d="M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16zm10 2-4.35-4.35" />
          </svg>
          <input
            type="search"
            value={filter()}
            onInput={(e) => setFilter(e.currentTarget.value)}
            placeholder="Search departments..."
            aria-label="Search departments and documents"
            class="w-full rounded-xl border border-noviq-line bg-white py-2 pl-9 pr-3 text-[12px]
                   text-noviq-ink placeholder:text-noviq-400 focus:border-noviq-400 focus:outline-none"
          />
        </div>
      </Show>

      <div class="flex items-center justify-between gap-2 px-1 pb-1.5">
        <div class="flex min-w-0 items-baseline gap-1.5">
          <span class="text-[10px] font-semibold uppercase tracking-wide text-noviq-500">
            {filterActive() ? "Matching departments" : "Top departments"}
          </span>
          {/* SCOPE, not size. The corpus total is stated once by the rail header
              above (`onStats`); repeating it here was redundant when unfiltered
              and actively WRONG while filtering — the row read "MATCHING
              DEPARTMENTS 3,931" when three departments matched. It now appears
              only when the selection is narrowed, where it says what the next
              question can actually reach and the header cannot. */}
          <Show when={!loading() && totalDocs() > 0 && selectedCount() !== totalDocs()}>
            <span
              class="flex-shrink-0 rounded-full bg-noviq-100 px-1.5 py-px text-[10px]
                     font-medium tabular-nums text-noviq-600"
              title={`${fmt(selectedCount())} of ${fmt(totalDocs())} documents selected`}
            >
              {fmt(selectedCount())} / {fmt(totalDocs())}
            </span>
          </Show>
        </div>
        <Show when={nodes().length > 0}>
          <div class="flex flex-shrink-0 gap-2 text-[10px] text-noviq-500">
            <button class="hover:text-noviq-700" onClick={() => selectAll(true)}>All</button>
            <span class="text-noviq-300">|</span>
            <button class="hover:text-noviq-700" onClick={() => selectAll(false)}>None</button>
          </div>
        </Show>
      </div>

      <div class="flex-1 overflow-y-auto pr-1 space-y-0.5">
        <Show
          when={!loading()}
          fallback={<p class="px-1 py-2 text-xs text-noviq-400">Loading documents…</p>}
        >
          <Show
            when={nodes().length > 0}
            fallback={<p class="px-1 py-2 text-xs text-noviq-400">No documents indexed yet.</p>}
          >
            <Show
              when={visibleGroups().length > 0}
              fallback={
                <p class="px-1 py-2 text-xs text-noviq-400">
                  Nothing matches "{filter().trim()}".
                </p>
              }
            >
            <For each={visibleGroups()}>
              {({ group, docs }) => (
                <div>
                  {/* Group row. Suppressed when unlabelled (no categories and no
                      private uploads) so a corpus without categories renders as
                      the plain flat list it was before. */}
                  <Show when={group.name}>
                    <div
                      class="group/row flex items-center gap-1.5 rounded-md px-1.5 py-[5px] hover:bg-noviq-100"
                      classList={{ "bg-noviq-100/60": isOpen(group.id) }}
                    >
                      {/* Reading order follows the design: folder, then its
                          name, then the count, and the expand arrow LAST at the
                          right edge. The tick stays ahead of the folder because
                          it is what scopes the next question — the design has no
                          such control to place. */}
                      <button
                        class="flex items-center gap-1.5 min-w-0 flex-1 text-left"
                        onClick={() => toggleGroup(group)}
                      >
                        <FolderIcon open={isOpen(group.id)} />
                        <span class="truncate text-xs font-semibold text-noviq-ink" title={group.name}>
                          {group.name}
                        </span>
                        <span
                          class="ml-auto flex-shrink-0 rounded-full bg-noviq-100 px-1.5 py-px
                                 text-[10px] font-medium tabular-nums text-noviq-500"
                        >
                          {group.docs.length}
                        </span>
                      </button>
                      <button
                        class="flex h-4 w-4 flex-shrink-0 items-center justify-center text-noviq-400 hover:text-noviq-700"
                        onClick={() => toggleExpand(group.id)}
                        title={isOpen(group.id) ? "Collapse" : "Expand"}
                        aria-expanded={isOpen(group.id)}
                      >
                        <svg
                          class="h-3.5 w-3.5 transition-transform duration-150"
                          classList={{ "rotate-90": isOpen(group.id) }}
                          viewBox="0 0 12 12"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="1.8"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <path d="M4.5 2.5 8 6l-3.5 3.5" />
                        </svg>
                      </button>
                      {/* Scoping tick LAST, after the arrow (architect,
                          2026-08-25). The row now reads folder -> name -> count
                          -> arrow -> tick, so the eye meets the department
                          before the control that acts on it.
                          `-ml-1` pulls it back against the row's `gap-1.5` so
                          the two sit ~2px apart and read as ONE cluster at the
                          right edge; at the full gap the tick floated free and
                          looked like it belonged to the next row down. */}
                      {/* ⚠️ Hover-reveal, with TWO exceptions that are not
                          optional:
                          1. A department that is EXCLUDED or PARTIAL stays
                             visible. Hiding it would make "this is out of
                             scope" invisible — the one state the reader most
                             needs to see. Only the default (fully included)
                             hides.
                          2. Below `lg` it is always visible: hover does not
                             exist on touch, and this rail is a drawer there, so
                             a hover-only control would be unreachable. */}
                      <button
                        class={`flex h-4 w-3 -ml-1 flex-shrink-0 items-center justify-center transition-opacity ${
                          groupState(group) === "all"
                            ? "lg:opacity-0 lg:group-hover/row:opacity-100 lg:focus-visible:opacity-100"
                            : ""
                        }`}
                        onClick={() => toggleGroup(group)}
                        title="Include this department in answers"
                        aria-label="Include this department in answers"
                      >
                        <Box state={groupState(group)} />
                      </button>
                    </div>
                  </Show>

                  {/* Document rows */}
                  <Show when={!group.name || isOpen(group.id)}>
                    {/* Children hang off a hairline rail rather than floating at
                        an indent, so at ~50 groups the eye can follow a document
                        back to the department it belongs to without counting
                        pixels. The rail is drawn only when there IS a parent. */}
                    <div
                      class="space-y-px"
                      classList={{ "ml-2 border-l border-noviq-line pl-2 pt-0.5": !!group.name }}
                    >
                      <For
                        each={docs}
                        fallback={<p class="px-1.5 py-1 text-[11px] text-noviq-400">No documents.</p>}
                      >
                        {(row) => (
                          <button
                            class="group/row flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left"
                            classList={{
                              "hover:bg-noviq-100": !isWithdrawn(row.src),
                              // Still shown, plainly inert: an officer who used
                              // this document yesterday must be able to see that
                              // it was withdrawn rather than think it vanished.
                              "opacity-50 cursor-not-allowed": isWithdrawn(row.src),
                            }}
                            disabled={isWithdrawn(row.src)}
                            title={
                              isWithdrawn(row.src)
                                ? `${row.src.file_name} — withdrawn from service by an administrator; it cannot be used in answers`
                                : row.src.file_name
                            }
                            onClick={() => toggleSource(row.src.id)}
                          >
                            <DocIcon />
                            <span
                              class="truncate text-[11px] leading-tight text-noviq-700"
                              classList={{ "line-through": isWithdrawn(row.src) }}
                            >
                              {row.src.file_name}
                            </span>
                            {/* F021 ownership, moved down from the folder row it
                                used to live on — the grouping is by category now,
                                so private/shared has to travel with the document
                                or it would be lost entirely. */}
                            <Show when={row.badge && !isWithdrawn(row.src)}>
                              <span
                                class="ml-auto flex-shrink-0 rounded-full border border-noviq-line
                                       px-1 text-[9px] text-noviq-400"
                                title={row.badgeTitle}
                              >
                                {row.badge}
                              </span>
                            </Show>
                            <Show when={isWithdrawn(row.src)}>
                              <span class="ml-auto flex-shrink-0 rounded-full border border-noviq-line
                                           px-1 text-[9px] text-noviq-400">
                                Unavailable
                              </span>
                            </Show>
                            {/* Tick last here too — a document row that ticked on
                                the left beside a department that ticked on the
                                right would read as two different controls. */}
                            <span
                              class={`ml-auto flex-shrink-0 pl-1 transition-opacity ${
                                selected().has(row.src.id)
                                  ? "lg:opacity-0 lg:group-hover/row:opacity-100"
                                  : ""
                              }`}
                            >
                              <Box state={selected().has(row.src.id) ? "all" : "none"} size="sm" />
                            </span>
                          </button>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              )}
            </For>
            </Show>
          </Show>
        </Show>
      </div>
    </div>
  );
};

export default DocumentTree;
