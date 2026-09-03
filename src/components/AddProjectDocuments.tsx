// Feature 027 — getting documents INTO a project.
//
// ⚠️ This closed a real dead end (2026-08-07). `ProjectPage` told the officer
// "No documents yet. Until you add some, questions in this project have nothing
// to search" — and offered no way to add any. The only route in was a shared
// copy from another user. The backend half (`add_project_documents`) existed the
// whole time; only the UI was missing.
//
// Two ways in, because the backend attaches ALREADY-INDEXED sources by id:
//   • Upload new  — the normal upload+index pipeline, then attach by id.
//   • My documents — attach something already indexed, storing nothing twice.

import { createSignal, For, Show, onMount } from "solid-js";
import { listIndexedFolders, listSourcesInFolder, type Source } from "../api";
import { uploadDocuments } from "../api/transport";
import { addDocuments, openProject } from "../stores/projects";
import { UPLOAD_ACCEPT, UPLOAD_HINT } from "../lib/uploadFormats";

interface Props {
  projectId: string;
  /** Source ids already in the project — never offer these twice. */
  present: string[];
  onClose: () => void;
  onAdded: (n: number) => void;
}

type Tab = "upload" | "existing";

export default function AddProjectDocuments(props: Props) {
  const [tab, setTab] = createSignal<Tab>("upload");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [candidates, setCandidates] = createSignal<Source[]>([]);
  const [picked, setPicked] = createSignal<string[]>([]);

  onMount(() => void loadExisting());

  /** Everything this officer has indexed that is not already in the project. */
  async function loadExisting() {
    try {
      const folders = await listIndexedFolders();
      const lists = await Promise.all(folders.map((f) => listSourcesInFolder(f.id)));
      const flat = lists.flat().filter((s) => !props.present.includes(s.id));
      // De-duplicate: one document can sit in more than one folder listing.
      const seen = new Set<string>();
      setCandidates(flat.filter((s) => (seen.has(s.id) ? false : (seen.add(s.id), true))));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not list your documents.");
    }
  }

  async function onUpload(files: File[]) {
    if (!files.length || busy()) return;
    setBusy(true);
    setError(null);
    try {
      // ⚠️ Send the project id (2026-08-11). The server then indexes into a
      // per-project folder owned by this officer — private by construction —
      // and records the membership itself. Uploading without it took the
      // ordinary corpus path, which for an ADMIN meant the file became a
      // shared document visible to every user.
      const outcome = await uploadDocuments(files, props.projectId);
      if (outcome.rejected?.length) {
        setError(
          outcome.rejected.map((r) => `${r.file_name}: ${r.reason}`).join(" · "),
        );
      }
      // ⚠️ Trust the server's own list (2026-08-11). This used to re-derive the
      // ids by listing the uploads folder and matching file names — but since
      // migration 024 a document uploaded INTO a project is deliberately absent
      // from folder listings, so the match found nothing and EVERY successful
      // project upload reported "could not match the files to indexed
      // documents". The attach had already happened server-side; only the
      // confirmation was looking in a place the document is designed not to be.
      const ids = outcome.attached ?? [];
      if (!ids.length) {
        // No rejection reason and nothing attached means indexing dropped the
        // files — still worth surfacing, just no longer the normal path.
        if (!outcome.rejected?.length) setError("Uploaded, but no document was indexed.");
        return;
      }
      // ⚠️ 2026-08-29 — REFRESH THE STORE. `addDocuments` (the other tab) calls
      // `openProject` for you; this path does not go through it, because the
      // server does the attaching itself during upload. So nothing ever put the
      // new rows into `documents()` and the rail stayed empty until the officer
      // reloaded the page — the document was in the project the whole time.
      await openProject(props.projectId);
      props.onAdded(ids.length);
      props.onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Upload failed.");
    } finally {
      setBusy(false);
    }
  }

  async function onAttach() {
    if (!picked().length || busy()) return;
    setBusy(true);
    setError(null);
    try {
      const res = await addDocuments(props.projectId, picked());
      props.onAdded(res.added);
      props.onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not add those documents.");
    } finally {
      setBusy(false);
    }
  }

  const tabClass = (t: Tab) =>
    `px-3 py-1.5 text-sm rounded-md ${
      tab() === t ? "bg-noviq-100 text-noviq-ink font-medium" : "text-noviq-600 hover:bg-noviq-50"
    }`;

  return (
    // ⚠️ `max-h` + `flex-col` on the panel, and scrolling on the BODY only.
    // Without it, a viewport shorter than the panel (a phone in landscape) clips
    // the footer and takes the Add button off-screen with no way to reach it.
    // `items-end sm:items-center` makes it a bottom sheet on a phone, which is
    // both easier to reach one-handed and immune to the same clipping.
    <div class="fixed inset-0 z-50 flex items-end justify-center bg-black/40 p-0 sm:items-center sm:p-4">
      <div class="flex max-h-[90vh] w-full max-w-lg flex-col rounded-t-xl border border-noviq-line bg-white shadow-lg sm:rounded-xl">
        <div class="flex flex-shrink-0 items-center justify-between border-b border-noviq-line px-4 py-3">
          <h3 class="text-sm font-semibold text-noviq-ink">Add documents</h3>
          {/* p-2 -m-2 keeps the visual size but gives a 40px touch target. */}
          <button class="-m-2 p-2 text-noviq-500 hover:text-noviq-ink" onClick={props.onClose}>
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 6l12 12M18 6L6 18" stroke-linecap="round" /></svg>
          </button>
        </div>

        <div class="flex flex-shrink-0 gap-2 border-b border-noviq-line px-4 py-2">
          <button class={tabClass("upload")} onClick={() => setTab("upload")}>Upload new</button>
          <button class={tabClass("existing")} onClick={() => setTab("existing")}>
            My documents
          </button>
        </div>

        <Show when={error()}>
          <p class="mx-4 mt-3 rounded border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-800">
            {error()}
          </p>
        </Show>

        <Show when={tab() === "upload"}>
          <div class="min-h-0 flex-1 overflow-y-auto p-4">
            <label class="flex cursor-pointer flex-col items-center justify-center rounded-lg border border-dashed border-noviq-line bg-noviq-50 px-4 py-10 text-center hover:bg-noviq-100">
              <span class="text-sm font-medium text-noviq-ink">
                {busy() ? "Uploading…" : "Choose files to upload"}
              </span>
              <span class="mt-1 text-xs text-noviq-600">{UPLOAD_HINT}</span>
              <input
                type="file"
                multiple
                accept={UPLOAD_ACCEPT}
                class="hidden"
                disabled={busy()}
                onChange={(e) => {
                  const fs = Array.from(e.currentTarget.files ?? []);
                  e.currentTarget.value = "";
                  void onUpload(fs);
                }}
              />
            </label>
            <p class="mt-3 text-xs text-noviq-600">
              Uploaded files are indexed first, then added here. They stay private to you.
            </p>
          </div>
        </Show>

        <Show when={tab() === "existing"}>
          <div class="min-h-0 flex-1 overflow-y-auto p-2">
            <Show
              when={candidates().length > 0}
              fallback={
                <p class="px-2 py-8 text-center text-xs text-noviq-600">
                  Nothing left to add — every document you have indexed is already in this
                  project.
                </p>
              }
            >
              <For each={candidates()}>
                {(s) => (
                  <label class="flex cursor-pointer items-center gap-3 rounded px-2 py-2.5 text-sm hover:bg-noviq-50">
                    <input
                      type="checkbox"
                      checked={picked().includes(s.id)}
                      onChange={(e) =>
                        setPicked((p) =>
                          e.currentTarget.checked ? [...p, s.id] : p.filter((x) => x !== s.id),
                        )
                      }
                    />
                    <span class="min-w-0 flex-1 truncate text-noviq-ink">{s.file_name}</span>
                    <Show when={s.indexing_status !== "indexed"}>
                      <span class="rounded bg-noviq-100 px-1.5 py-0.5 text-[10px] text-noviq-700">
                        {s.indexing_status}
                      </span>
                    </Show>
                  </label>
                )}
              </For>
            </Show>
          </div>
          <div class="flex flex-shrink-0 justify-end gap-2 border-t border-noviq-line px-4 py-3">
            <button class="rounded-lg px-3 py-1.5 text-sm text-noviq-600 hover:bg-noviq-50" onClick={props.onClose}>
              Cancel
            </button>
            <button
              class="rounded-lg bg-noviq-700 px-4 py-1.5 text-sm text-white hover:bg-noviq-600 disabled:opacity-50"
              disabled={busy() || picked().length === 0}
              onClick={() => void onAttach()}
            >
              Add {picked().length || ""}
            </button>
          </div>
        </Show>
      </div>
    </div>
  );
}
