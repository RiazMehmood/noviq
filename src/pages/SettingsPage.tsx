/**
 * SettingsPage — user-facing settings.
 *
 * Client-server model: there is no hardware/GPU probe here any more (that was a
 * desktop-only concern). This page now carries:
 *   - "My Documents" — every signed-in WEB user's private uploads.
 *   - "Indexed Folders" — the desktop corpus manager (Tauri only); on the web,
 *     corpus + folder management lives in the Admin console, not here.
 *   - Cloud Inference + Query Trace Logs — DESKTOP only (the web build manages
 *     inference and traces from the Admin console, so they are not duplicated).
 *
 * Admins on the web do NOT use this page; their controls live under /admin.
 */
import { Component, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import {
  IndexedFolder,
  Source,
  listIndexedFolders,
  listFolderSources,
  retryFolderSource,
  openQueryLogsDir,
  readLatestQueryTrace,
  downloadQueryTraceTo,
  downloadAllQueryTracesTo,
  downloadRecentQueryTracesTo,
  CloudSettings,
  getCloudSettings,
  setCloudSettings,
  getUserPrefs,
  setUserPrefs,
  myUsage,
  MyUsage,
  setVisibility,
  deleteMyDocument,
} from "../api/index";
import { save } from "@tauri-apps/plugin-dialog";
import { isTauri, onIndexProgress, uploadDocuments, downloadAuthed, UploadRejected } from "../api/transport";
import { currentUser, isAdmin } from "../stores/auth";
import IndexedFolders from "../components/IndexedFolders";

const SettingsPage: Component = () => {
  // Query trace logs (desktop only)
  const [traceFilename, setTraceFilename] = createSignal<string>("");
  const [traceContent, setTraceContent] = createSignal<string>("");
  const [traceModalOpen, setTraceModalOpen] = createSignal(false);
  const [traceError, setTraceError] = createSignal<string | null>(null);
  // Answer preferences (per-user, web + desktop). Starts `true` so the toggle
  // renders in its default state before the fetch resolves rather than flicking
  // from off to on.
  const [refinementOffer, setRefinementOffer] = createSignal(true);
  const [prefsSaving, setPrefsSaving] = createSignal(false);
  const [prefsError, setPrefsError] = createSignal<string | null>(null);

  const loadPrefs = async () => {
    try { setRefinementOffer((await getUserPrefs()).refinementOffer); }
    catch (e) { console.warn("User preferences unavailable:", e); }
  };

  const toggleRefinementOffer = async () => {
    const next = !refinementOffer();
    setRefinementOffer(next);          // optimistic — the toggle should feel instant
    setPrefsSaving(true);
    setPrefsError(null);
    try {
      const saved = await setUserPrefs({ refinementOffer: next });
      setRefinementOffer(saved.refinementOffer);   // trust the server's answer
    } catch (e) {
      setRefinementOffer(!next);       // roll back so the UI never lies about what was saved
      setPrefsError(e instanceof Error ? e.message : String(e));
    } finally {
      setPrefsSaving(false);
    }
  };

  // Cloud inference (desktop only)
  const [cloudSettings, setCloudSettingsSig] = createSignal<CloudSettings | null>(null);
  const [cloudSaving, setCloudSaving] = createSignal(false);
  const [cloudMsg, setCloudMsg] = createSignal<string | null>(null);

  const loadCloud = async () => {
    try { setCloudSettingsSig(await getCloudSettings()); }
    catch (e) { console.warn("Cloud settings unavailable:", e); }
  };

  const saveCloud = async () => {
    const s = cloudSettings();
    if (!s) return;
    setCloudSaving(true);
    setCloudMsg(null);
    try {
      await setCloudSettings(s);
      setCloudMsg("Saved.");
    } catch (e) { setCloudMsg(`Save failed: ${e}`); }
    setCloudSaving(false);
  };

  const handleOpenLogsFolder = async () => {
    setTraceError(null);
    try { await openQueryLogsDir(); }
    catch (e) { setTraceError(`Open folder failed: ${e}`); }
  };

  const handleViewLatestTrace = async () => {
    setTraceError(null);
    try {
      const [name, body] = await readLatestQueryTrace();
      if (!name) { setTraceError("No trace yet — run a query first."); return; }
      setTraceFilename(name);
      setTraceContent(body);
      setTraceModalOpen(true);
    } catch (e) { setTraceError(`Read failed: ${e}`); }
  };

  const handleDownloadAllTraces = async () => {
    setTraceError(null);
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    if (!isTauri()) {
      try { await downloadAuthed(`/traces/all`, `noviq-traces-${stamp}.md`); }
      catch (e) { setTraceError(`Download all failed: ${e}`); }
      return;
    }
    try {
      const dest = await save({
        defaultPath: `noviq-traces-${stamp}.md`,
        filters: [{ name: "Markdown / Text", extensions: ["md", "txt", "log"] }],
      });
      if (!dest) return;
      const [count] = await downloadAllQueryTracesTo(dest);
      setTraceError(`Wrote ${count} trace${count === 1 ? "" : "s"} to ${dest}`);
    } catch (e) { setTraceError(`Download all failed: ${e}`); }
  };

  const handleDownloadRecentTraces = async () => {
    setTraceError(null);
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    if (!isTauri()) {
      try { await downloadAuthed(`/traces/recent?limit=30`, `noviq-traces-last30-${stamp}.md`); }
      catch (e) { setTraceError(`Download recent failed: ${e}`); }
      return;
    }
    try {
      const dest = await save({
        defaultPath: `noviq-traces-last30-${stamp}.md`,
        filters: [{ name: "Markdown / Text", extensions: ["md", "txt", "log"] }],
      });
      if (!dest) return;
      const [count] = await downloadRecentQueryTracesTo(dest, 30);
      setTraceError(`Wrote ${count} trace${count === 1 ? "" : "s"} to ${dest}`);
    } catch (e) { setTraceError(`Download recent failed: ${e}`); }
  };

  const handleDownloadLatestTrace = async () => {
    setTraceError(null);
    if (!isTauri()) {
      try { await downloadAuthed(`/traces/latest`, `noviq-trace-latest.log`); }
      catch (e) { setTraceError(`Download failed: ${e}`); }
      return;
    }
    try {
      const [name] = await readLatestQueryTrace();
      if (!name) { setTraceError("No trace yet — run a query first."); return; }
      const dest = await save({
        defaultPath: name,
        filters: [{ name: "Log file", extensions: ["log", "txt"] }],
      });
      if (!dest) return;
      await downloadQueryTraceTo(dest);
    } catch (e) { setTraceError(`Download failed: ${e}`); }
  };

  // ── F021: "My documents" (every web user) ────────────────────────────────
  const [usage, setUsage] = createSignal<MyUsage | null>(null);
  const [myDocs, setMyDocs] = createSignal<{ folder: IndexedFolder; source: Source }[]>([]);
  const [myDocsError, setMyDocsError] = createSignal<string | null>(null);
  const [myDocsRejected, setMyDocsRejected] = createSignal<UploadRejected[]>([]);
  const [myDocsBusy, setMyDocsBusy] = createSignal(false);
  const [sourceProgress, setSourceProgress] = createSignal<Record<string, number>>({});

  const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));
  let myFileInputRef: HTMLInputElement | undefined;

  // The folders this user *owns* — an admin's uploads go to the shared corpus
  // (owner_id NULL), so they are deliberately excluded here.
  const myFolders = (all: IndexedFolder[]) =>
    all.filter((f) => f.owner_username && f.owner_username === currentUser()?.username);

  const loadMyDocs = async () => {
    try {
      const all = await listIndexedFolders();
      const rows: { folder: IndexedFolder; source: Source }[] = [];
      for (const folder of myFolders(all)) {
        const sources = await listFolderSources(folder.folder_path);
        for (const source of sources) rows.push({ folder, source });
      }
      setMyDocs(rows);
      setUsage(await myUsage());
    } catch (e) {
      setMyDocsError(String(e));
    }
  };

  onMount(async () => {
    await loadPrefs();
    await loadCloud();
    if (!isTauri()) await loadMyDocs();

    // Live per-file indexing progress for the user's own documents.
    const unlistenSource = onIndexProgress((p) => {
      const { source_id, percent } = p;
      setSourceProgress((prev) => ({ ...prev, [source_id]: percent }));
      if (percent >= 100) {
        setMyDocs((prev) =>
          prev.map((r) =>
            r.source.id === source_id
              ? { ...r, source: { ...r.source, indexing_status: "indexed" as const } }
              : r
          )
        );
        void myUsage().then(setUsage).catch(() => {});
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
  });

  // SSE-tunnel fallback: poll while any of my documents is still indexing.
  const indexingActive = (s: string) => s === "pending" || s === "processing";
  const indexPollNeeded = createMemo(
    () => !isTauri() && myDocs().some((r) => indexingActive(r.source.indexing_status))
  );
  createEffect(() => {
    if (!indexPollNeeded()) return;
    const id = setInterval(() => { void loadMyDocs(); }, 5000);
    onCleanup(() => clearInterval(id));
  });

  const handleMyFilesSelected = async (e: Event) => {
    const input = e.currentTarget as HTMLInputElement;
    const picked = Array.from(input.files ?? []);
    if (picked.length === 0) return;
    setMyDocsBusy(true);
    setMyDocsError(null);
    setMyDocsRejected([]);
    try {
      const outcome = await uploadDocuments(picked);
      setMyDocsRejected(outcome.rejected);
      await loadMyDocs();
    } catch (err) {
      setMyDocsError(errMsg(err));
    } finally {
      setMyDocsBusy(false);
      input.value = "";
    }
  };

  // Admin-gated sharing (2026-07-28): "share" files a review request — the
  // document stays private until the admin approves. Track requested ids so
  // the button reads "Requested" for the rest of the session (the backend
  // dedupes on reload, so a repeat click is harmless).
  const [shareRequested, setShareRequested] = createSignal<Set<string>>(new Set());

  const handleToggleDocVisibility = async (source: Source) => {
    const next = source.visibility === "shared" ? "private" : "shared";
    try {
      const res = await setVisibility({ sourceId: source.id }, next);
      if (res.requested) {
        setShareRequested((prev) => new Set(prev).add(source.id));
        return; // visibility unchanged — awaiting admin review
      }
      setMyDocs((prev) =>
        prev.map((r) =>
          r.source.id === source.id ? { ...r, source: { ...r.source, visibility: next } } : r,
        ),
      );
    } catch (err) {
      setMyDocsError(errMsg(err));
    }
  };

  const handleDeleteMyDoc = async (source: Source) => {
    if (!confirm(`Permanently delete "${source.file_name}"? It will stop appearing in answers.`)) return;
    try {
      await deleteMyDocument(source.id);
      setMyDocs((prev) => prev.filter((r) => r.source.id !== source.id));
      setUsage(await myUsage());
    } catch (err) {
      setMyDocsError(errMsg(err));
    }
  };

  const handleRetryMyDoc = async (source: Source, folderId: string) => {
    try {
      await retryFolderSource(source.id, folderId);
      setMyDocs((prev) =>
        prev.map((r) =>
          r.source.id === source.id
            ? { ...r, source: { ...r.source, indexing_status: "processing" as const } }
            : r,
        ),
      );
    } catch (err) {
      setMyDocsError(errMsg(err));
    }
  };

  const fmtMb = (bytes: number) => `${(bytes / (1024 * 1024)).toFixed(1)} MB`;

  return (
    <div class="p-4 sm:p-8 overflow-y-auto h-full bg-noviq-paper text-noviq-ink">
      <h1 class="text-xl font-semibold text-noviq-ink mb-6">Settings</h1>

      {/* ── Answer preferences (per-user; web AND desktop) ─────────────────── */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-noviq-600 uppercase tracking-wider mb-3">
          Answers
        </h2>
        <div class="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-3 sm:gap-4 max-w-2xl">
          <div>
            <p class="text-sm text-noviq-ink">Suggest next steps</p>
            <p class="text-xs text-noviq-500 mt-0.5">
              End each answer with a short “Next steps” line offering what else
              could be covered — for example the approval authority or a related
              provision. Turn this off for answers only.
            </p>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={refinementOffer()}
            aria-label="Suggest next steps after each answer"
            disabled={prefsSaving()}
            onClick={toggleRefinementOffer}
            class={`relative shrink-0 w-11 h-6 rounded-full transition-colors disabled:opacity-50 ${
              refinementOffer() ? "bg-noviq-700" : "bg-noviq-300"
            }`}
          >
            <span
              class={`absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white transition-transform ${
                refinementOffer() ? "translate-x-5" : "translate-x-0"
              }`}
            />
          </button>
        </div>
        <Show when={prefsError()}>
          <p class="mt-2 text-xs text-red-400">Could not save: {prefsError()}</p>
        </Show>
      </section>

      {/* ── F021: My documents — every signed-in web user ──────────────────── */}
      <Show when={!isTauri()}>
      <section>
        <div class="flex items-center justify-between mb-3">
          <h2 class="text-sm font-semibold text-noviq-600 uppercase tracking-wider">
            My Documents
          </h2>
          <div class="flex items-center gap-3">
            <Show when={usage() && !isAdmin()}>
              <span class="text-xs text-noviq-500">
                {fmtMb(usage()!.usedBytes)} / {fmtMb(usage()!.limitBytes)} ·{" "}
                {usage()!.usedFiles}/{usage()!.limitFiles} files
              </span>
            </Show>
            <input
              ref={myFileInputRef}
              type="file"
              multiple
              class="hidden"
              onChange={handleMyFilesSelected}
            />
            <button
              onClick={() => myFileInputRef?.click()}
              disabled={myDocsBusy()}
              class="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium
                     bg-noviq-700 hover:bg-noviq-800 text-white rounded-lg
                     disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                  d="M4 16v2a2 2 0 002 2h12a2 2 0 002-2v-2M12 4v12m0-12l-4 4m4-4l4 4" />
              </svg>
              {myDocsBusy() ? "Uploading…" : "Upload files"}
            </button>
          </div>
        </div>
        <p class="text-xs text-noviq-500 mb-3">
          Your documents are <span class="font-medium">private</span> by default — only your
          own answers can use them. Share one to make it available to everyone.
        </p>
        <Show when={myDocsError()}>
          <div class="text-sm text-red-700 bg-red-100 border border-red-300 rounded-lg p-3 mb-3">{myDocsError()}</div>
        </Show>
        <Show when={myDocsRejected().length > 0}>
          <div class="bg-amber-50 border border-amber-300 rounded-lg p-3 mb-3">
            <div class="flex items-start justify-between gap-2">
              <p class="text-sm font-medium text-amber-800">
                {myDocsRejected().length === 1
                  ? "1 file couldn't be added:"
                  : `${myDocsRejected().length} files couldn't be added:`}
              </p>
              <button
                class="text-xs text-amber-700 hover:text-amber-900"
                onClick={() => setMyDocsRejected([])}
                aria-label="Dismiss"
              >
                ✕
              </button>
            </div>
            <ul class="mt-1.5 space-y-1">
              <For each={myDocsRejected()}>
                {(r) => (
                  <li class="text-sm text-amber-800">
                    <span class="font-medium">{r.file_name}</span>
                    <span class="text-amber-700"> — {r.reason}</span>
                  </li>
                )}
              </For>
            </ul>
          </div>
        </Show>
        <Show when={isAdmin()}>
          <p class="text-xs text-noviq-600 bg-noviq-50 border border-noviq-line rounded-lg p-2.5 mb-3">
            You're signed in as <b>admin</b>: anything you upload is indexed into the{" "}
            <b>shared corpus</b> (every user can cite it in answers), not into a private
            "My Documents" area. Manage corpus files from the <b>Admin console</b>.
          </p>
        </Show>
        <Show
          when={myDocs().length > 0}
          fallback={
            <p class="text-sm text-noviq-500 italic">
              {isAdmin()
                ? "No private documents — your uploads live in the shared corpus (Admin console)."
                : "No documents yet — upload files to ask questions about them alongside the corpus."}
            </p>
          }
        >
          <div class="space-y-1.5">
            <For each={myDocs()}>
              {(row) => (
                <div class="flex items-center gap-3 px-3 py-2 bg-noviq-panel border border-noviq-line rounded-lg">
                  <span class="flex-1 text-sm text-noviq-ink truncate" title={row.source.file_name}>
                    {row.source.file_name}
                  </span>
                  <Show when={sourceProgress()[row.source.id] !== undefined}>
                    <span class="text-xs text-noviq-500">
                      {sourceProgress()[row.source.id]}%
                    </span>
                  </Show>
                  <span
                    class={`text-xs px-2 py-0.5 rounded-full ${
                      row.source.indexing_status === "indexed"
                        ? "bg-green-100 text-green-800 border border-green-300"
                        : row.source.indexing_status === "failed"
                          ? "bg-red-100 text-red-700 border border-red-300"
                          : "bg-noviq-line/40 text-noviq-600"
                    }`}
                    title={row.source.error_reason ?? undefined}
                  >
                    {row.source.indexing_status}
                  </span>
                  <Show when={row.source.indexing_status === "failed"}>
                    <button
                      class="text-xs text-noviq-500 hover:text-noviq-ink"
                      onClick={() => handleRetryMyDoc(row.source, row.folder.id)}
                    >
                      Retry
                    </button>
                  </Show>
                  <button
                    class={`text-xs px-2 py-0.5 rounded-full border transition-colors ${
                      row.source.visibility === "shared"
                        ? "border-noviq-500 text-noviq-500 hover:bg-noviq-line/30"
                        : shareRequested().has(row.source.id)
                          ? "border-amber-500 text-amber-600 cursor-default"
                          : "border-noviq-line text-noviq-500 hover:text-noviq-ink"
                    }`}
                    title={
                      row.source.visibility === "shared"
                        ? "Shared with everyone — click to make private"
                        : shareRequested().has(row.source.id)
                          ? "Sent to your administrator for review"
                          : "Private to you — click to request sharing (admin reviews it)"
                    }
                    disabled={
                      row.source.visibility !== "shared" && shareRequested().has(row.source.id)
                    }
                    onClick={() => handleToggleDocVisibility(row.source)}
                  >
                    {row.source.visibility === "shared"
                      ? "Shared"
                      : shareRequested().has(row.source.id)
                        ? "Requested"
                        : "Private"}
                  </button>
                  <button
                    class="text-xs text-red-500 hover:text-red-400"
                    onClick={() => handleDeleteMyDoc(row.source)}
                  >
                    Delete
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
      </section>
      </Show>

      {/* ── Indexed Folders — desktop corpus manager only. On the web, folder /
             corpus management lives in the Admin console. ───────────────────── */}
      <Show when={isTauri()}>
        <div class="mt-8">
          <IndexedFolders />
        </div>
      </Show>

      {/* ── Cloud Inference + Query Trace Logs — DESKTOP only ─────────────────
             (the web build manages inference + traces from the Admin console). */}
      <Show when={isTauri()}>
      {/* ── Cloud Inference (multi-provider) ─────────────────────────────── */}
      <section class="mt-8">
        <h2 class="text-sm font-semibold text-noviq-600 uppercase tracking-wider mb-3">
          Cloud Inference
        </h2>
        <p class="text-xs text-noviq-500 mb-3">
          Route chat queries through a cloud LLM instead of the local model.
          Any OpenAI-compatible provider works. Trades privacy (data leaves
          device) for speed and capability. Default OFF.
        </p>
        <Show when={cloudSettings()} fallback={<p class="text-xs text-noviq-400">Loading…</p>}>
          {(s) => {
            const PROVIDERS: Record<string, { label: string; keys: string; model: string; prefix: string }> = {
              groq:       { label: "Groq",       keys: "console.groq.com/keys",          model: "llama-3.3-70b-versatile",         prefix: "gsk_…" },
              cerebras:   { label: "Cerebras",   keys: "cloud.cerebras.ai",              model: "qwen-3-32b",                       prefix: "csk-…" },
              openrouter: { label: "OpenRouter", keys: "openrouter.ai/keys",             model: "meta-llama/llama-3.3-70b-instruct:free", prefix: "sk-or-…" },
              together:   { label: "Together",   keys: "api.together.ai/settings/api-keys", model: "meta-llama/Llama-3.3-70B-Instruct-Turbo", prefix: "…" },
              deepseek:   { label: "DeepSeek",   keys: "platform.deepseek.com/api_keys", model: "deepseek-chat",                    prefix: "sk-…" },
              openai:     { label: "OpenAI",     keys: "platform.openai.com/api-keys",   model: "gpt-4o-mini",                      prefix: "sk-…" },
              fireworks:  { label: "Fireworks",  keys: "fireworks.ai/account/api-keys",  model: "accounts/fireworks/models/llama-v3p3-70b-instruct", prefix: "fw_…" },
              custom:     { label: "Custom (OpenAI-compatible)", keys: "—", model: "", prefix: "…" },
            };
            const p = () => PROVIDERS[s().provider] ?? PROVIDERS.groq;
            return (
              <div class="space-y-3 max-w-xl">
                <label class="flex items-center gap-2 text-sm text-noviq-700">
                  <input
                    type="checkbox"
                    checked={s().enabled}
                    onChange={(e) => setCloudSettingsSig({ ...s(), enabled: e.currentTarget.checked })}
                  />
                  Enable cloud inference
                </label>
                <div>
                  <label class="block text-xs text-noviq-600 mb-1">Provider</label>
                  <select
                    value={s().provider}
                    onChange={(e) => {
                      const next = e.currentTarget.value;
                      const suggested = PROVIDERS[next]?.model ?? "";
                      const cur = s().model;
                      const knownSuggestions = Object.values(PROVIDERS).map((x) => x.model);
                      const model = !cur || knownSuggestions.includes(cur) ? suggested : cur;
                      setCloudSettingsSig({ ...s(), provider: next, model });
                    }}
                    class="w-full px-2 py-1 text-sm bg-white border border-noviq-line rounded text-white [&>option]:bg-white [&>option]:text-white"
                    style="color-scheme: dark"
                  >
                    <For each={Object.entries(PROVIDERS)}>
                      {([key, info]) => (
                        <option value={key} style="background:#1f2937;color:#fff">{info.label}</option>
                      )}
                    </For>
                  </select>
                </div>
                <Show when={s().provider === "custom"}>
                  <div>
                    <label class="block text-xs text-noviq-600 mb-1">Endpoint URL</label>
                    <input
                      type="text"
                      value={s().endpoint_url ?? ""}
                      placeholder="https://your-host/v1/chat/completions"
                      onInput={(e) => setCloudSettingsSig({ ...s(), endpoint_url: e.currentTarget.value })}
                      class="w-full px-2 py-1 text-sm bg-white border border-noviq-line rounded text-white font-mono"
                    />
                    <p class="text-xs text-noviq-400 mt-1">
                      Must accept the OpenAI <span class="font-mono">/v1/chat/completions</span> shape.
                    </p>
                  </div>
                </Show>
                <div>
                  <label class="block text-xs text-noviq-600 mb-1">API key</label>
                  <input
                    type="password"
                    value={s().api_key}
                    placeholder={p().prefix}
                    onInput={(e) => setCloudSettingsSig({ ...s(), api_key: e.currentTarget.value })}
                    class="w-full px-2 py-1 text-sm bg-white border border-noviq-line rounded text-white"
                  />
                  <Show when={p().keys !== "—"}>
                    <p class="text-xs text-noviq-400 mt-1">
                      Get a key at <span class="font-mono">{p().keys}</span>
                    </p>
                  </Show>
                </div>
                <div>
                  <label class="block text-xs text-noviq-600 mb-1">Model</label>
                  <input
                    type="text"
                    value={s().model}
                    placeholder={p().model || "model-id"}
                    onInput={(e) => setCloudSettingsSig({ ...s(), model: e.currentTarget.value })}
                    class="w-full px-2 py-1 text-sm bg-white border border-noviq-line rounded text-white font-mono"
                  />
                  <p class="text-xs text-noviq-400 mt-1">
                    Paste any model id supported by the selected provider.
                    Suggested for {p().label}: <span class="font-mono">{p().model || "—"}</span>
                  </p>
                </div>
                <div class="flex items-center gap-3">
                  <button
                    onClick={saveCloud}
                    disabled={cloudSaving()}
                    class="px-3 py-1.5 text-xs bg-noviq-700 hover:bg-noviq-800 disabled:opacity-50 rounded text-white"
                  >
                    {cloudSaving() ? "Saving…" : "Save"}
                  </button>
                  <Show when={cloudMsg()}>
                    <span class="text-xs text-noviq-600">{cloudMsg()}</span>
                  </Show>
                </div>
              </div>
            );
          }}
        </Show>
      </section>

      {/* ── Query Trace Logs ─────────────────────────────────────────────── */}
      <section class="mt-8">
        <h2 class="text-sm font-semibold text-noviq-600 uppercase tracking-wider mb-3">
          Query Trace Logs
        </h2>
        <p class="text-xs text-noviq-500 mb-3">
          Step-by-step record of what the app does for each query — useful for
          debugging slow or wrong answers.
        </p>
        <div class="flex flex-wrap gap-2">
          <button onClick={handleOpenLogsFolder} class="px-3 py-1.5 text-xs bg-noviq-100 hover:bg-noviq-300 rounded text-white">
            Open logs folder
          </button>
          <button onClick={handleViewLatestTrace} class="px-3 py-1.5 text-xs bg-noviq-100 hover:bg-noviq-300 rounded text-white">
            View latest trace
          </button>
          <button onClick={handleDownloadLatestTrace} class="px-3 py-1.5 text-xs bg-noviq-100 hover:bg-noviq-300 rounded text-white">
            Download latest trace
          </button>
          <button onClick={handleDownloadRecentTraces} class="px-3 py-1.5 text-xs bg-noviq-700 hover:bg-noviq-700 rounded text-white"
            title="Concatenate the most recent 30 traces into one file">
            Download last 30 traces
          </button>
          <button onClick={handleDownloadAllTraces} class="px-3 py-1.5 text-xs bg-noviq-700 hover:bg-noviq-700 rounded text-white"
            title="Concatenate every trace in the queries folder into one file">
            Download all traces (combined)
          </button>
        </div>
        <Show when={traceError()}>
          <p class="mt-2 text-xs text-red-400">{traceError()}</p>
        </Show>
      </section>
      </Show>

      {/* Trace viewer modal */}
      <Show when={traceModalOpen()}>
        <div
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
          onClick={() => setTraceModalOpen(false)}
        >
          <div
            class="bg-noviq-panel border border-noviq-line rounded-lg max-w-4xl w-full max-h-[85vh] flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div class="flex items-center justify-between px-4 py-2 border-b border-noviq-line">
              <span class="text-sm text-noviq-700 font-mono truncate">{traceFilename()}</span>
              <button
                onClick={() => setTraceModalOpen(false)}
                class="text-noviq-600 hover:text-white text-lg leading-none"
                aria-label="Close trace viewer"
              >
                ×
              </button>
            </div>
            <pre class="flex-1 overflow-auto p-4 text-xs text-noviq-700 font-mono whitespace-pre">
{traceContent()}
            </pre>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default SettingsPage;
