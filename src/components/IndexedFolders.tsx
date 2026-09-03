/**
 * IndexedFolders — the document pane, shared by the desktop Settings page and the
 * web Admin console (Indexing & Data tab).
 *
 * Presents ONE flat, always-visible list of documents with a search box, newest
 * first. It replaced a folder tree whose documents were hidden behind a
 * disclosure and whose rows each carried re-index / deep-index / remove / delete
 * buttons: a freshly uploaded file was hard to find, and destructive controls sat
 * one stray click away on every row. Now the row carries only status and
 * "Chunks"; re-index and delete are bulk actions that require an explicit
 * selection first.
 *
 * Owns its own document list and live progress (Tauri `folder:progress` + the web
 * `/api/index/events` SSE bridge, with a poll fallback because quick tunnels
 * buffer SSE). Admin uploads land in the SHARED corpus (owner_id NULL); a regular
 * desktop user's land in their own area — the backend decides.
 */
import { Component, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import {
  IndexedFolder,
  Source,
  addIndexedFolder,
  listIndexedFolders,
  listFolderSources,
  retryFolderSource,
} from "../api/index";
import { open } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  MultiInstrumentNotice,
  SimilarNotice,
  isTauri,
  onIndexProgress,
  request,
  uploadDocuments,
} from "../api/transport";
import ChunkViewer from "./ChunkViewer";
import { isAdmin } from "../stores/auth";
import { UPLOAD_ACCEPT } from "../lib/uploadFormats";
import {
  categories,
  categorizeSources,
  createCategory,
  loadCategories,
  setSourceDisabled,
} from "../stores/categories";

interface FolderProgressEvent {
  folder_id: string;
  indexed: number;
  total: number;
  status: string;
}

const IndexedFolders: Component = () => {
  const [folders, setFolders] = createSignal<IndexedFolder[]>([]);
  const [folderError, setFolderError] = createSignal<string | null>(null);
  // Uploads that turned out to hold several instruments (admin uploads only).
  const [multiInstrument, setMultiInstrument] = createSignal<MultiInstrumentNotice[]>([]);
  const [similarExisting, setSimilarExisting] = createSignal<SimilarNotice[]>([]);
  const [isAddingFolder, setIsAddingFolder] = createSignal(false);
  const [folderSources, setFolderSources] = createSignal<Record<string, Source[]>>({});
  const [sourceProgress, setSourceProgress] = createSignal<Record<string, number>>({});
  const [isUploading, setIsUploading] = createSignal(false);
  const [pickDirectory, setPickDirectory] = createSignal(false);
  // Bulk selection + the document whose chunks are open in the viewer. Both live
  // here so this panel is the SINGLE place documents are inspected and acted on.
  const [selectedSources, setSelectedSources] = createSignal<Set<string>>(new Set<string>());
  const [viewerSource, setViewerSource] = createSignal<Source | null>(null);
  const [isBulkDeleting, setIsBulkDeleting] = createSignal(false);
  const [isBulkReindexing, setIsBulkReindexing] = createSignal(false);
  const [docFilter, setDocFilter] = createSignal("");
  // Migration 021 — the document whose withdraw/restore is in flight, so only
  // that row's button goes busy rather than the whole list.
  const [disablingId, setDisablingId] = createSignal<string | null>(null);
  const [isFiling, setIsFiling] = createSignal(false);
  // A folder picked but NOT yet uploaded. Selecting and committing are separate
  // steps: the admin sees how many files were found, sets a category, and only
  // then starts indexing. Empty category = leave Uncategorised.
  // Consecutive failures of the BACKGROUND poll only. Automatic GPU indexing
  // restarts the server mid-batch, so a miss or two is expected, not an error.
  const [pollFailures, setPollFailures] = createSignal(0);
  const [reconnecting, setReconnecting] = createSignal(false);
  const [stagedFiles, setStagedFiles] = createSignal<File[]>([]);
  const [pendingCategoryId, setPendingCategoryId] = createSignal("");
  const [newCategoryName, setNewCategoryName] = createSignal("");
  const [isCreatingCategory, setIsCreatingCategory] = createSignal(false);
  let fileInputRef: HTMLInputElement | undefined;

  /** Corpus organisation is an admin action on the web; desktop has neither. */
  const canOrganise = () => isAdmin() && !isTauri();

  /** Every indexed document, flattened across folders, newest first. */
  const allSources = createMemo(() => {
    const seen = new Set<string>();
    const out: Source[] = [];
    for (const list of Object.values(folderSources())) {
      for (const s of list) {
        // A document can be reachable through more than one folder record; show
        // it once.
        if (!seen.has(s.id)) {
          seen.add(s.id);
          out.push(s);
        }
      }
    }
    return out;
  });

  /** `allSources` narrowed by the search box. */
  const visibleSources = createMemo(() => {
    const q = docFilter().trim().toLowerCase();
    if (!q) return allSources();
    return allSources().filter((s) => s.file_name.toLowerCase().includes(q));
  });

  /**
   * `visibleSources` split into category groups (migration 021).
   *
   * Uncategorised comes FIRST, and that is deliberate rather than cosmetic: a
   * new upload is always Uncategorised, and this list's whole design rule is
   * that the file you just added must be the easiest one to find. Categories
   * then follow in the admin's display order, and empty ones are omitted — a
   * category with no documents is managed in the manager, not stared at here.
   *
   * With no categories defined at all this returns a single unlabelled group, so
   * the list renders exactly as it did before 021.
   */
  const groupedSources = createMemo(() => {
    const docs = visibleSources();
    const cats = categories();
    if (cats.length === 0) return [{ id: null as string | null, name: "", docs }];

    const byCat = new Map<string | null, Source[]>();
    for (const s of docs) {
      // A document filed into a category that has since been deleted reads as
      // Uncategorised — `ON DELETE SET NULL` means that is also true in the DB.
      const key = s.category_id && cats.some((c) => c.id === s.category_id) ? s.category_id : null;
      const list = byCat.get(key);
      list ? list.push(s) : byCat.set(key, [s]);
    }
    const out: { id: string | null; name: string; docs: Source[] }[] = [];
    const loose = byCat.get(null);
    if (loose?.length) out.push({ id: null, name: "Uncategorised", docs: loose });
    for (const c of cats) {
      const list = byCat.get(c.id);
      if (list?.length) out.push({ id: c.id, name: c.name, docs: list });
    }
    return out;
  });

  const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

  const toggleSourceSelected = (id: string) => {
    // Replace the Set rather than mutate — Solid compares by reference.
    const next = new Set(selectedSources());
    next.has(id) ? next.delete(id) : next.add(id);
    setSelectedSources(next);
  };

  /**
   * Patch one document in place across every folder list holding it.
   *
   * Used by the withdraw toggle so the row updates immediately instead of after
   * a full `reloadAll()`. A document can appear under more than one folder
   * record, so every list is mapped — patching only the first would leave a
   * second copy of the row showing the old state.
   */
  const patchSource = (id: string, patch: Partial<Source>) => {
    setFolderSources((prev) => {
      const next: Record<string, Source[]> = {};
      for (const [fid, list] of Object.entries(prev)) {
        next[fid] = list.map((s) => (s.id === id ? { ...s, ...patch } : s));
      }
      return next;
    });
  };

  /**
   * Withdraw a document from service, or return it (migration 021).
   *
   * Label-only and immediate: `visible_source_ids` is recomputed per request, so
   * the document stops being retrievable and citable on the NEXT turn with no
   * re-index — and restoring it is just as immediate. It stays listed here,
   * greyed, because an admin has to be able to find what they withdrew.
   */
  const handleToggleDisabled = async (source: Source) => {
    const disable = !source.disabled_at;
    if (
      disable &&
      !confirm(
        `Withdraw “${source.file_name}” from service?\n\n` +
          `It stops being retrievable and citable from the next question onward. ` +
          `Nothing is deleted or re-indexed, and you can restore it at any time.`,
      )
    ) {
      return;
    }
    setDisablingId(source.id);
    try {
      await setSourceDisabled(source.id, disable);
      patchSource(source.id, { disabled_at: disable ? Date.now() : null });
      setFolderError(null);
    } catch (e) {
      setFolderError(errMsg(e));
    } finally {
      setDisablingId(null);
    }
  };

  /** File the current selection into a category, or back to Uncategorised. */
  const handleFileInto = async (categoryId: string | null) => {
    const ids = [...selectedSources()];
    if (ids.length === 0) return;
    setIsFiling(true);
    try {
      await categorizeSources(ids, categoryId);
      for (const id of ids) patchSource(id, { category_id: categoryId });
      setSelectedSources(new Set<string>());
      setFolderError(null);
    } catch (e) {
      setFolderError(errMsg(e));
    } finally {
      setIsFiling(false);
    }
  };

  /**
   * Delete every selected document, after showing exactly what will go.
   *
   * The counts in the confirm dialog come from the server's preview, never from
   * anything re-derived here: the folder-delete bug was a silent no-op that
   * reported success, so the number the user approves must be the number the
   * server actually computed.
   */
  const handleBulkDelete = async () => {
    const ids = [...selectedSources()];
    if (ids.length === 0) return;
    try {
      const preview = await request<{
        targets: { source_id: string; file_name: string; chunk_count: number }[];
      }>("POST", "/admin/sources/delete_bulk", { sourceIds: ids, preview: true });
      const lines = preview.targets
        .map((t) => `  • ${t.file_name} (${t.chunk_count} chunks)`)
        .join("\n");
      const totalChunks = preview.targets.reduce((n, t) => n + t.chunk_count, 0);
      if (
        !confirm(
          `Permanently delete ${preview.targets.length} document(s) and ${totalChunks} chunks?\n\n` +
            `${lines}\n\nThis cannot be undone.`,
        )
      ) {
        return;
      }
      setIsBulkDeleting(true);
      await request("POST", "/admin/sources/delete_bulk", { sourceIds: ids, preview: false });
      setSelectedSources(new Set<string>());
      setViewerSource(null);
      await reloadAll();
      setFolderError(null);
    } catch (e) {
      setFolderError(errMsg(e));
    } finally {
      setIsBulkDeleting(false);
    }
  };

  /**
   * Re-parse and re-embed the selected documents.
   *
   * Kept as a BULK action rather than a per-row button: re-indexing is
   * occasional and expensive, and a row-level trigger sitting beside every
   * document is easy to hit by accident.
   */
  const handleBulkReindex = async () => {
    const ids = [...selectedSources()];
    if (ids.length === 0) return;
    if (!confirm(`Re-index ${ids.length} document(s)? Each is re-parsed and re-embedded.`)) {
      return;
    }
    setIsBulkReindexing(true);
    try {
      for (const id of ids) {
        const src = allSources().find((s) => s.id === id);
        const folder = folders().find((f) =>
          (folderSources()[f.id] ?? []).some((s) => s.id === id),
        );
        // Argument order is (sourceId, folderId) — both are strings, so getting
        // it backwards typechecks fine and fails only at runtime.
        if (src && folder) await retryFolderSource(src.id, folder.id);
      }
      setSelectedSources(new Set<string>());
      await reloadAll();
      setFolderError(null);
    } catch (e) {
      setFolderError(errMsg(e));
    } finally {
      setIsBulkReindexing(false);
    }
  };

  const SUPPORTED_EXTS = [
    "pdf", "docx", "xlsx", "pptx", "txt", "md",
    "png", "jpg", "jpeg", "mp3", "mp4",
    "rs", "py", "js", "ts", "go",
  ];

  /**
   * Load the folder list.
   *
   * `silent` is for the background poll, and it matters: automatic GPU indexing
   * RESTARTS the server mid-batch (the ONNX runtime switch is read at exec), so
   * a poll landing in that window fails through no fault of the user's. Painting
   * a raw ApiError under a progress bar that is still advancing is alarming and
   * wrong — the indexing is fine, the socket was briefly gone.
   *
   * A sustained outage is still worth saying, so after a few consecutive misses
   * it shows a calm "reconnecting" note rather than staying mute. User-initiated
   * loads keep reporting errors immediately, as before.
   */
  const loadFolders = async (silent = false) => {
    try {
      setFolders(await listIndexedFolders());
      setPollFailures(0);
      setReconnecting(false);
    } catch (e) {
      if (!silent) {
        setFolderError(String(e));
        return;
      }
      const n = pollFailures() + 1;
      setPollFailures(n);
      // ~4 polls ≈ 12s. A server restart is back well inside that.
      if (n >= 4) setReconnecting(true);
    }
  };

  // The list is flat and always visible, so sources are loaded for EVERY folder.
  // While this only ran for an expanded folder, a freshly uploaded document did
  // not appear at all until the page was reloaded by hand.
  const refreshFolderSources = async (folderId: string, folderPath: string) => {
    try {
      const sources = await listFolderSources(folderPath);
      setFolderSources((prev) => ({ ...prev, [folderId]: sources }));
    } catch (_) {}
  };

  /** Reload the folder list and every folder's documents. */
  const reloadAll = async (silent = false) => {
    await loadFolders(silent);
    await Promise.all(folders().map((f) => refreshFolderSources(f.id, f.folder_path)));
  };

  onMount(async () => {
    await reloadAll();
    // Categories drive the group headers and the "File into…" menu. Admin-only
    // and web-only: the route is admin-gated and `request` throws in Tauri.
    if (canOrganise()) void loadCategories();

    // Per-file indexing progress (Tauri event on desktop, SSE bridge on web).
    const unlistenSource = onIndexProgress((p) => {
      const { source_id, percent } = p;
      setSourceProgress((prev) => ({ ...prev, [source_id]: percent }));
      if (percent >= 100) {
        let found = false;
        setFolderSources((prev) => {
          const updated: Record<string, Source[]> = {};
          for (const [fid, sources] of Object.entries(prev)) {
            const mapped = sources.map((s) =>
              s.id === source_id ? { ...s, indexing_status: "indexed" as const } : s
            );
            if (mapped.some((s) => s.id === source_id)) found = true;
            updated[fid] = mapped;
          }
          return updated;
        });
        if (!found) {
          for (const folder of folders()) {
            refreshFolderSources(folder.id, folder.folder_path);
          }
        }
        setTimeout(() => {
          setSourceProgress((prev) => {
            const next = { ...prev };
            delete next[source_id];
            return next;
          });
        }, 1500);
      }
    });
    onCleanup(() => unlistenSource());

    // Real-time folder scan progress from the backend (Tauri only).
    const unlisten = await listen<FolderProgressEvent>("folder:progress", (evt) => {
      const { folder_id, indexed, total, status } = evt.payload;
      setFolders((prev) =>
        prev.map((f) =>
          f.id === folder_id ? { ...f, indexing_status: status, file_count: indexed } : f
        )
      );
      if (status === "indexed") {
        const f = folders().find((x) => x.id === folder_id);
        if (f) refreshFolderSources(folder_id, f.folder_path);
      }
    });
    onCleanup(() => unlisten());
  });

  // SSE-tunnel fallback: quick tunnels buffer the SSE stream, so poll while
  // anything is still pending/processing so statuses converge on the web.
  const indexingActive = (s: string) => s === "pending" || s === "processing";
  // Keyed off the DOCUMENTS, not the folders. Re-indexing a single file leaves
  // its folder `indexed`, so a folder-only test never started the poll: the row
  // sat at "processing" with no percentage and the list did not refresh until the
  // page was reloaded by hand. Both symptoms, one cause.
  const indexPollNeeded = createMemo(
    () =>
      !isTauri() &&
      (folders().some((f) => indexingActive(f.indexing_status)) ||
        // Migration 026 — a batch in flight, which is the authoritative signal:
        // it is written to the folder row, so it survives a refresh and is the
        // one thing that keeps ticking while the tunnel swallows the SSE stream.
        folders().some((f) => f.indexing_total > 0) ||
        allSources().some((s) => indexingActive(s.indexing_status))),
  );
  createEffect(() => {
    if (!indexPollNeeded()) return;
    const id = setInterval(() => {
      void reloadAll(true);
    }, 3000);
    onCleanup(() => clearInterval(id));
  });

  /**
   * The batch currently indexing, if any (migration 026).
   *
   * Read from the folder row rather than from a stream, which is the whole
   * point: the admin who reported this refreshed the tab mid-run and only then
   * saw a number. Committed state is the only progress that survives a refresh,
   * a tab switch, or the tunnel buffering the SSE stream.
   */
  const activeBatch = createMemo(() => folders().find((f) => f.indexing_total > 0));

  /**
   * One batch at a time.
   *
   * Every admin upload lands in the SAME `uploads/admin` folder row, so a second
   * folder started mid-run joins the first batch rather than queueing behind it:
   * one merged bar, and no way to see which folder is where. The documents are
   * filed correctly either way — the category is per file — but the progress is
   * uninterpretable, so the controls are closed until the run finishes.
   *
   * Lifting this needs a real job queue (one row per folder, progress derived
   * per job), not a looser guard here.
   */
  const indexingBusy = createMemo(() => !!activeBatch());
  const busyReason = "Indexing in progress — wait for it to finish before adding more";

  const indexFolderPath = async (path: string) => {
    await addIndexedFolder(path);
    await loadFolders();
  };

  const handleAddFolder = async () => {
    setIsAddingFolder(true);
    setFolderError(null);
    try {
      const selected = await open({ directory: true, multiple: false });
      if (!selected || typeof selected !== "string") return;
      await indexFolderPath(selected);
    } catch (e) {
      setFolderError(String(e));
    } finally {
      setIsAddingFolder(false);
    }
  };

  createEffect(() => {
    if (!fileInputRef) return;
    if (pickDirectory()) fileInputRef.setAttribute("webkitdirectory", "");
    else fileInputRef.removeAttribute("webkitdirectory");
  });

  /**
   * Folder chosen — STAGE it, do not upload.
   *
   * Picking a folder used to start the upload on the spot, which gave no chance
   * to see what was selected or to change the category, and no way out short of
   * letting hundreds of files index. Selection and commitment are now separate:
   * this only records the batch, and `startIndexing` is the deliberate act.
   */
  const handleFilesSelected = (e: Event) => {
    const input = e.currentTarget as HTMLInputElement;
    const picked = Array.from(input.files ?? []);
    const files = picked.filter((f) =>
      SUPPORTED_EXTS.includes((f.name.split(".").pop() ?? "").toLowerCase())
    );
    // Cleared immediately so re-picking the SAME folder still fires `change`;
    // the File objects in `files` stay valid after the input is reset.
    input.value = "";
    if (files.length === 0) {
      setFolderError("No supported files found in that folder.");
      return;
    }
    setFolderError(null);
    setStagedFiles(files);
    setPendingCategoryId("");
    setNewCategoryName("");
  };

  /** Folder name the batch came from, for the confirmation panel. */
  const stagedFolderName = createMemo(() => {
    const first = stagedFiles()[0] as (File & { webkitRelativePath?: string }) | undefined;
    const rel = first?.webkitRelativePath ?? "";
    return rel.includes("/") ? rel.split("/")[0] : "";
  });

  const cancelStaged = () => {
    setStagedFiles([]);
    setPendingCategoryId("");
    setNewCategoryName("");
  };

  /** The commit step: create the category if asked, then upload and index. */
  const startIndexing = async () => {
    const files = stagedFiles();
    if (files.length === 0) return;

    // Creating the category is part of THIS click, not of picking the folder,
    // so a batch is never uploaded against a category that failed to be created.
    let categoryId = pendingCategoryId();
    const name = newCategoryName().trim();
    if (name) {
      setIsCreatingCategory(true);
      try {
        await createCategory(name, categories().length);
        const created = categories().find((c) => c.name === name);
        if (!created) {
          setFolderError("Category was created but could not be selected — pick it from the list.");
          return;
        }
        categoryId = created.id;
        setPendingCategoryId(created.id);
      } catch (err) {
        setFolderError(errMsg(err));
        return;
      } finally {
        setIsCreatingCategory(false);
      }
    }

    setIsUploading(true);
    setFolderError(null);
    try {
      const outcome = await uploadDocuments(files, undefined, categoryId || undefined);
      if (outcome.rejected.length > 0) {
        setFolderError(
          `${outcome.rejected.length} file(s) skipped: ` +
            outcome.rejected.map((r) => `${r.file_name} (${r.reason})`).join("; ")
        );
      }
      // A compendium indexed as ONE document makes every "Rule 4" inside it
      // ambiguous. Say so at upload, while the person still has the document in
      // mind, rather than leaving it to be noticed in a wrong answer months on.
      setMultiInstrument(outcome.multi_instrument ?? []);
      // Same reasoning, different failure: a near-identical title is how the
      // same document ends up indexed twice under two names. Only the uploader
      // can tell that from an amended instrument, so this is shown, not enforced.
      setSimilarExisting(outcome.similar_existing ?? []);
      await loadFolders();
    } catch (err) {
      setFolderError(errMsg(err));
    } finally {
      setIsUploading(false);
      // One batch, one choice. Leaving these set would silently carry the same
      // category into the NEXT folder — the stickiness that misfiled documents
      // through the web-search toggle before.
      setStagedFiles([]);
      setPendingCategoryId("");
      setNewCategoryName("");
    }
  };

  /** "Add folder": open the picker straight away. Nothing uploads until Index. */
  const startFolderUpload = () => {
    setPickDirectory(true);
    setFolderError(null);
    if (canOrganise()) void loadCategories();
    queueMicrotask(() => fileInputRef?.click());
  };








  // Deleting a folder destroys every chunk and embedding derived from it, and
  // that is only recoverable by re-indexing. So: ask first, with real numbers
  // from the server rather than a generic "are you sure".

  // 700-level hues: the 400s are dark-theme colors, unreadable on light paper.
  function statusColor(status: string): string {
    switch (status) {
      case "indexed":
        return "text-green-700";
      case "processing":
        return "text-amber-700";
      case "failed":
        return "text-red-700";
      default:
        return "text-noviq-600";
    }
  }

  return (
    <section>
      <div class="flex items-center justify-between mb-3">
        <h2 class="text-sm font-semibold text-noviq-600 uppercase tracking-wider">
          Documents
        </h2>
        <Show
          when={isTauri()}
          fallback={
            <div class="flex items-center gap-2">
              <input
                ref={fileInputRef}
                type="file"
                multiple
                accept={UPLOAD_ACCEPT}
                class="hidden"
                onChange={handleFilesSelected}
              />
              <button
                onClick={() => {
                  setPickDirectory(false);
                  queueMicrotask(() => fileInputRef?.click());
                }}
                disabled={isUploading() || indexingBusy()}
                title={
                  indexingBusy()
                    ? busyReason
                    : isAdmin()
                      ? "Index files into the shared corpus (everyone can use them in answers)"
                      : "Index files into your own documents"
                }
                class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium
                       bg-noviq-700 hover:bg-noviq-800 text-white rounded-lg
                       disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                    d="M4 16v2a2 2 0 002 2h12a2 2 0 002-2v-2M12 4v12m0-12l-4 4m4-4l4 4" />
                </svg>
                {isUploading() ? "Uploading…" : isAdmin() ? "Add files (shared)" : "Add files"}
              </button>
              <button
                onClick={startFolderUpload}
                disabled={isUploading() || indexingBusy()}
                title={indexingBusy() ? busyReason : "Index a whole folder (recursively) from your device"}
                class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium
                       border border-noviq-300 text-noviq-700 hover:bg-noviq-100 rounded-lg
                       disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                📁 {isUploading() ? "Uploading…" : "Add folder"}
              </button>
              {/* Appears only with a selection, so the destructive control is not
                  sitting armed next to the everyday ones. */}
              <Show when={isAdmin() && selectedSources().size > 0}>
                {/* Filing is reversible and touches nothing but a label, so it
                    sits with the other selection actions rather than behind a
                    confirm. Resets to the placeholder after each use: it is a
                    menu of actions, not a field showing current state — the
                    selection can span several categories at once. */}
                <Show when={canOrganise()}>
                  <select
                    class="px-2 py-1.5 text-xs border border-noviq-300 rounded-lg bg-white
                           text-noviq-700 disabled:opacity-50"
                    disabled={isFiling()}
                    value=""
                    onChange={(e) => {
                      const v = e.currentTarget.value;
                      e.currentTarget.value = "";
                      if (v === "") return;
                      void handleFileInto(v === "__none__" ? null : v);
                    }}
                    title="File the selected documents into a category (label only — nothing is re-indexed)"
                  >
                    <option value="">
                      {isFiling() ? "Filing…" : `File ${selectedSources().size} into…`}
                    </option>
                    <For each={categories()}>
                      {(c) => <option value={c.id}>{c.name}</option>}
                    </For>
                    <option value="__none__">— Uncategorised —</option>
                  </select>
                </Show>
                <button
                  onClick={handleBulkReindex}
                  disabled={isBulkReindexing()}
                  class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium
                         border border-noviq-300 text-noviq-700 hover:bg-noviq-100 rounded-lg
                         disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {isBulkReindexing()
                    ? "Starting…"
                    : `Re-index ${selectedSources().size} selected`}
                </button>
                <button
                  onClick={handleBulkDelete}
                  disabled={isBulkDeleting()}
                  class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium
                         bg-red-600 hover:bg-red-700 text-white rounded-lg
                         disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {isBulkDeleting()
                    ? "Deleting…"
                    : `Delete ${selectedSources().size} selected`}
                </button>
              </Show>
            </div>
          }
        >
          <button
            onClick={handleAddFolder}
            disabled={isAddingFolder()}
            class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium
                   bg-noviq-700 hover:bg-noviq-800 text-white rounded-lg
                   disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
            </svg>
            {isAddingFolder() ? "Selecting…" : "Add Folder"}
          </button>
        </Show>
      </div>

      {/* Staged batch — chosen, counted, and NOT yet uploaded. Nothing leaves
          the browser until "Start indexing" is clicked. */}
      <Show when={stagedFiles().length > 0}>
        <div class="border border-noviq-300 bg-noviq-50 rounded-lg p-3 mb-3">
          <p class="text-sm font-medium text-noviq-800 mb-2">
            {stagedFiles().length} document{stagedFiles().length === 1 ? "" : "s"} ready to index
            <Show when={stagedFolderName()}>
              {" "}
              from <span class="font-mono text-xs">{stagedFolderName()}</span>
            </Show>
          </p>
          <Show when={canOrganise()}>
            <div class="flex flex-wrap items-center gap-2 mb-2">
              <label class="text-xs text-noviq-600">Category</label>
              <select
                class="px-2 py-1.5 text-xs border border-noviq-300 rounded-lg bg-white
                       text-noviq-700 disabled:opacity-50"
                value={pendingCategoryId()}
                disabled={!!newCategoryName().trim() || isUploading()}
                onChange={(e) => setPendingCategoryId(e.currentTarget.value)}
              >
                <option value="">— Uncategorised —</option>
                <For each={categories()}>{(c) => <option value={c.id}>{c.name}</option>}</For>
              </select>
              <span class="text-xs text-noviq-500">or</span>
              <input
                type="text"
                placeholder="Create new category…"
                value={newCategoryName()}
                disabled={isUploading()}
                onInput={(e) => setNewCategoryName(e.currentTarget.value)}
                class="px-2 py-1.5 text-xs border border-noviq-300 rounded-lg bg-white
                       text-noviq-700 disabled:opacity-50"
              />
            </div>
          </Show>
          <div class="flex items-center gap-2">
            <button
              onClick={() => void startIndexing()}
              disabled={isUploading() || isCreatingCategory() || indexingBusy()}
              title={indexingBusy() ? busyReason : undefined}
              class="px-3 py-1.5 text-xs font-medium bg-noviq-700 hover:bg-noviq-800 text-white
                     rounded-lg disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {isCreatingCategory()
                ? "Creating category…"
                : isUploading()
                  ? "Uploading…"
                  : `Index ${stagedFiles().length} document${stagedFiles().length === 1 ? "" : "s"}`}
            </button>
            <button
              onClick={cancelStaged}
              disabled={isUploading() || isCreatingCategory()}
              class="px-3 py-1.5 text-xs font-medium border border-noviq-300 text-noviq-700
                     hover:bg-noviq-100 rounded-lg disabled:opacity-50 transition-colors"
            >
              Cancel
            </button>
          </div>
          <p class="text-xs text-noviq-500 mt-2">
            Uploading starts when you click Index. Filing is a label only — no file is moved and
            nothing is re-indexed, so the category can be changed later.
          </p>
        </div>
      </Show>

      <Show when={activeBatch()}>
        {(batch) => (
          <div class="border border-amber-300 bg-amber-50 rounded-lg p-3 mb-3">
            <div class="flex items-center justify-between mb-1.5">
              <span class="text-sm font-medium text-amber-900">
                Indexing {batch().indexing_done} of {batch().indexing_total} documents
              </span>
              <span class="text-xs text-amber-800">
                {Math.floor((batch().indexing_done / Math.max(1, batch().indexing_total)) * 100)}%
              </span>
            </div>
            <div class="h-2 w-full bg-amber-200 rounded-full overflow-hidden">
              <div
                class="h-full bg-amber-600 transition-all duration-500"
                style={{
                  width: `${Math.min(
                    100,
                    (batch().indexing_done / Math.max(1, batch().indexing_total)) * 100,
                  )}%`,
                }}
              />
            </div>
            <p class="text-xs text-amber-800 mt-1.5">
              Adding more documents is paused until this finishes. You can leave this page —
              progress is saved and will still be here when you come back.
              <Show when={reconnecting()}>
                {" "}
                <span class="font-medium">
                  Reconnecting to the server… indexing continues in the background.
                </span>
              </Show>
            </p>
          </div>
        )}
      </Show>

      <Show when={folderError()}>
        <div class="text-sm text-red-700 bg-red-100 border border-red-300 rounded-lg p-3 mb-3">
          {folderError()}
        </div>
      </Show>

      {/* Not an error — the file indexed. But a compendium indexed as ONE
          document makes every provision number inside it ambiguous, and that is
          far cheaper to fix now than to diagnose later from a wrong citation. */}
      <Show when={multiInstrument().length > 0}>
        <div class="text-sm text-amber-800 bg-amber-50 border border-amber-300 rounded-lg p-3 mb-3">
          <For each={multiInstrument()}>
            {(m) => (
              <p class="mb-1">
                <b>{m.file_name}</b> appears to contain{" "}
                <b>{m.instrument_count} separate laws</b>
                <Show when={m.sample_titles.length > 0}>
                  {" "}
                  — {m.sample_titles.join("; ")}
                  {m.instrument_count > m.sample_titles.length ? "; …" : ""}
                </Show>
                .
              </p>
            )}
          </For>
          <p class="text-xs mt-2">
            It has been indexed as a single document, so a reference like "Rule 4"
            cannot tell those laws apart. Split it under{" "}
            <b>Admin → Indexing &amp; Data → Document structure</b>, where you can
            check the boundaries first.
          </p>
        </div>
      </Show>

      {/* Also not an error. Identical BYTES are deduped silently and never get
          here; what reaches this notice is a similar NAME, which is genuinely
          ambiguous — an amended instrument must sit beside its base text, while
          a re-download must not. Only the person who uploaded it knows which. */}
      <Show when={similarExisting().length > 0}>
        <div class="text-sm text-sky-900 bg-sky-50 border border-sky-300 rounded-lg p-3 mb-3">
          <For each={similarExisting()}>
            {(s) => (
              <p class="mb-1">
                <b>{s.file_name}</b> resembles a document already indexed
                {": "}
                {s.matches.join("; ")}.
              </p>
            )}
          </For>
          <p class="text-xs mt-2">
            It has been indexed. If this is the same document under a different
            name, delete the copy you do not want — two copies of one instrument
            split its citations across both. If it is a different version (an
            amendment, a later year), keep both.
          </p>
        </div>
      </Show>

      {/* Flat, always-visible document list. Previously this was a folder tree
          you had to expand: the file you just uploaded was hidden behind a
          disclosure, and per-file actions were crowded onto every row. Documents
          are the unit people work with, so they are listed directly, actions that
          destroy or rebuild are bulk-only (top bar, after an explicit selection),
          and the row itself carries only status and inspection. */}
      <Show
        when={allSources().length > 0}
        fallback={
          <p class="text-sm text-noviq-500">
            No documents indexed yet. Add files to index them into the corpus.
          </p>
        }
      >
        <div class="rounded-lg border border-noviq-line bg-white">
          <div class="flex items-center gap-2 border-b border-noviq-line p-2">
            <input
              class="flex-1 rounded border border-noviq-line px-2.5 py-1.5 text-xs
                     outline-none focus:border-noviq-400"
              placeholder={`Search ${allSources().length} documents…`}
              value={docFilter()}
              onInput={(e) => setDocFilter(e.currentTarget.value)}
            />
            <Show when={isAdmin() && allSources().length > 0}>
              <label class="flex items-center gap-1.5 px-1 text-xs text-noviq-500">
                <input
                  type="checkbox"
                  aria-label="Select all shown"
                  checked={
                    visibleSources().length > 0 &&
                    visibleSources().every((s) => selectedSources().has(s.id))
                  }
                  onChange={(e) =>
                    setSelectedSources(
                      e.currentTarget.checked
                        ? new Set<string>(visibleSources().map((s) => s.id))
                        : new Set<string>(),
                    )
                  }
                />
                All
              </label>
            </Show>
          </div>

          <div class="max-h-[26rem] overflow-y-auto">
            <For each={groupedSources()}>
              {(group) => (
                <div>
                  {/* Sticky so the group a row belongs to stays readable while
                      scrolling a long corpus. Suppressed entirely when no
                      categories exist, so the pre-021 list is unchanged. */}
                  <Show when={group.name}>
                    <div
                      class="sticky top-0 z-10 flex items-center gap-2 border-y border-noviq-line
                             bg-noviq-50 px-3 py-1 text-[0.65rem] font-semibold uppercase
                             tracking-wider text-noviq-500"
                    >
                      <Show when={canOrganise()}>
                        <input
                          type="checkbox"
                          class="flex-shrink-0"
                          aria-label={`Select all in ${group.name}`}
                          title={`Select every document in ${group.name}`}
                          checked={
                            group.docs.length > 0 &&
                            group.docs.every((s) => selectedSources().has(s.id))
                          }
                          onChange={(e) => {
                            const next = new Set(selectedSources());
                            for (const s of group.docs) {
                              e.currentTarget.checked ? next.add(s.id) : next.delete(s.id);
                            }
                            setSelectedSources(next);
                          }}
                        />
                      </Show>
                      <span class="flex-1 truncate">{group.name}</span>
                      <span class="tabular-nums font-normal normal-case tracking-normal">
                        {group.docs.length}
                      </span>
                    </div>
                  </Show>

                  <div class="divide-y divide-noviq-line">
                    <For each={group.docs}>
                      {(source) => (
                        <div
                          class="flex items-center gap-2 px-3 py-2 text-xs hover:bg-noviq-50"
                          // A withdrawn document is still listed — an admin has to be
                          // able to find what they took out of service — but must never
                          // read as active. Dimmed, and labelled below.
                          classList={{ "opacity-60": !!source.disabled_at }}
                        >
                          <Show when={isAdmin()}>
                            <input
                              type="checkbox"
                              class="flex-shrink-0"
                              aria-label={`Select ${source.file_name}`}
                              checked={selectedSources().has(source.id)}
                              onChange={() => toggleSourceSelected(source.id)}
                            />
                          </Show>

                          <span
                            class={`w-2 h-2 rounded-full flex-shrink-0 ${
                              source.disabled_at
                                ? "bg-noviq-400"
                                : source.indexing_status === "indexed"
                                ? "bg-green-500"
                                : source.indexing_status === "processing" ||
                                  source.indexing_status === "pending"
                                ? "bg-noviq-500 animate-pulse"
                                : source.indexing_status === "failed"
                                ? "bg-red-500"
                                : "bg-noviq-300"
                            }`}
                          />

                          <span
                            class="flex-1 truncate text-noviq-700"
                            classList={{ "line-through": !!source.disabled_at }}
                            title={source.file_path}
                          >
                            {source.file_name}
                          </span>

                          {/* Percent when the backend reports one, otherwise the status
                              word — never a blank cell, which reads as "nothing is
                              happening" during the long CPU embedding pass. Withdrawn
                              beats the indexing status: it is the reason the document
                              will not appear in an answer. */}
                          <span class="flex-shrink-0 w-24 text-right font-mono text-noviq-600">
                            <Show
                              when={!source.disabled_at}
                              fallback={
                                <span
                                  class="font-sans text-noviq-500"
                                  title="Withdrawn from service — not retrievable or citable"
                                >
                                  withdrawn
                                </span>
                              }
                            >
                              <Show
                                when={sourceProgress()[source.id] !== undefined}
                                fallback={
                                  <span
                                    class={
                                      source.indexing_status === "indexed"
                                        ? "text-green-700 font-sans"
                                        : source.indexing_status === "failed"
                                        ? "text-red-700 font-sans"
                                        : "text-noviq-500 font-sans"
                                    }
                                  >
                                    {source.indexing_status}
                                  </span>
                                }
                              >
                                {sourceProgress()[source.id]}%
                              </Show>
                            </Show>
                          </span>

                          <Show when={canOrganise() && source.indexing_status === "indexed"}>
                            <button
                              onClick={() => void handleToggleDisabled(source)}
                              disabled={disablingId() === source.id}
                              class="flex-shrink-0 rounded px-2 py-0.5 transition-colors
                                     disabled:opacity-50"
                              classList={{
                                "bg-noviq-100 text-noviq-700 hover:bg-noviq-800 hover:text-white":
                                  !source.disabled_at,
                                "bg-green-100 text-green-800 hover:bg-green-700 hover:text-white":
                                  !!source.disabled_at,
                              }}
                              title={
                                source.disabled_at
                                  ? "Return this document to service — retrievable again from the next question"
                                  : "Withdraw from service — stays in the corpus but is never retrieved or cited"
                              }
                            >
                              {disablingId() === source.id
                                ? "…"
                                : source.disabled_at
                                ? "Restore"
                                : "Withdraw"}
                            </button>
                          </Show>

                          <Show when={isAdmin() && source.indexing_status === "indexed"}>
                            <button
                              onClick={() => setViewerSource(source)}
                              class="flex-shrink-0 rounded bg-noviq-100 px-2 py-0.5 text-noviq-700
                                     transition-colors hover:bg-noviq-800 hover:text-white"
                              title="View the chunks this document was indexed into"
                            >
                              Chunks
                            </button>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              )}
            </For>
            <Show when={visibleSources().length === 0}>
              <p class="py-4 text-center text-xs text-noviq-500">
                No document matches “{docFilter()}”.
              </p>
            </Show>
          </div>
        </div>
      </Show>

      {/* One viewer, opened from the document row that owns it. */}
      <Show when={viewerSource()}>
        {(s) => (
          <ChunkViewer
            sourceId={s().id}
            fileName={s().file_name}
            onClose={() => setViewerSource(null)}
          />
        )}
      </Show>
    </section>
  );
};

export default IndexedFolders;
