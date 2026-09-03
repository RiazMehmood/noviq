// Feature 027 — "My Projects": the list of a user's workspaces.
//
// ⚠️ Every route this page calls is refused server-side when the deployment gate
// is closed. Not rendering the nav entry is presentation, never the enforcement
// (FR-038) — so this page failing to load is a normal, expected state, not a bug
// to work around with client-side checks.

import { createSignal, For, onMount, Show } from "solid-js";
import { useNavigate } from "@solidjs/router";
import {
  createProject,
  deleteProject,
  enterProject,
  loadProjects,
  projects,
  renameProject,
  type Project,
} from "../stores/projects";

export default function ProjectsPage() {
  const navigate = useNavigate();
  const [name, setName] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      await loadProjects();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load projects.");
    }
  });

  async function onCreate(e: Event) {
    e.preventDefault();
    const trimmed = name().trim();
    if (!trimmed || busy()) return;
    setBusy(true);
    setError(null);
    try {
      const p = await createProject(trimmed);
      setName("");
      // 2026-08-29 — straight into the project's chat. Naming a project used to
      // land on an overview page whose three sections were all empty on a brand
      // new project; the officer's next act is always to ask something or add a
      // document, and both live in the chat screen now.
      await openInChat(p.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not create the project.");
    } finally {
      setBusy(false);
    }
  }

  /** The one way into a project: its most recent chat, or a first one. */
  async function openInChat(projectId: string) {
    const conversationId = await enterProject(projectId);
    navigate(`/projects/${projectId}/chat/${conversationId}`);
  }

  async function onOpen(p: Project) {
    if (busy()) return;
    setBusy(true);
    setError(null);
    try {
      await openInChat(p.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not open the project.");
    } finally {
      setBusy(false);
    }
  }

  async function onRename(p: Project) {
    const next = window.prompt("Rename project", p.name);
    if (!next || next.trim() === p.name) return;
    try {
      await renameProject(p.id, next);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not rename the project.");
    }
  }

  async function onDelete(p: Project) {
    // Deleting takes the chats with it, so say so with real numbers rather than
    // a generic "are you sure".
    const chats = p.chat_count === 1 ? "1 chat" : `${p.chat_count} chats`;
    const ok = window.confirm(
      `Delete "${p.name}"? Its ${chats} will be deleted too.\n\n` +
        `Documents you shared into someone else's project stay with them.`,
    );
    if (!ok) return;
    try {
      await deleteProject(p.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not delete the project.");
    }
  }

  return (
    // ⚠️ 2026-08-07: this page shipped with DEFAULT Tailwind (`bg-blue-600`,
    // `bg-gray-100`, bare `border`) and no Noviq tokens. Theming only the centred
    // container left the surround dark, because `AppShell` sets no background —
    // each page owns its own. Hence the FULL-BLEED shell here.
    <div class="min-h-full w-full overflow-y-auto bg-noviq-paper text-noviq-ink">
     <div class="mx-auto w-full max-w-4xl px-4 py-8">
      {/* Empty state: creating the first project is the ONLY thing to do, so it
          is a centred card rather than a strip floating above nothing. */}
      <Show when={projects().length === 0}>
        <div class="flex min-h-[60vh] flex-col items-center justify-center">
          <div class="w-full max-w-md rounded-xl border border-noviq-line bg-white p-8 text-center shadow-sm">
            <h1 class="text-xl font-semibold text-noviq-ink">Create your first project</h1>
            <p class="mt-2 text-sm text-noviq-600">
              A project holds its own documents and its own chats. Questions asked inside it
              are answered from those documents only.
            </p>
            <form onSubmit={onCreate} class="mt-6 flex flex-col gap-3">
              <input
                class="w-full rounded-lg border border-noviq-line bg-white px-3 py-2 text-center text-noviq-ink
                       placeholder:text-noviq-400 focus:border-noviq-500 focus:outline-none"
                placeholder="Project name…"
                value={name()}
                maxLength={200}
                onInput={(e) => setName(e.currentTarget.value)}
              />
              <button
                type="submit"
                class="w-full rounded-lg bg-noviq-700 px-4 py-2 text-white hover:bg-noviq-600 disabled:opacity-50"
                disabled={busy() || !name().trim()}
              >
                Create project
              </button>
            </form>
          </div>
        </div>
      </Show>

      {/* Once projects exist the creator collapses to a compact row — the list
          is what the officer came for. */}
      <Show when={projects().length > 0}>
        <header class="mb-6">
          <h1 class="text-2xl font-semibold text-noviq-ink">My Projects</h1>
          <p class="mt-1 text-sm text-noviq-600">
            A project holds its own documents and its own chats. Questions asked inside it are
            answered from those documents only.
          </p>
        </header>

        <form onSubmit={onCreate} class="mb-8 flex gap-2">
          <input
            class="flex-1 rounded-lg border border-noviq-line bg-white px-3 py-2 text-noviq-ink
                   placeholder:text-noviq-400 focus:border-noviq-500 focus:outline-none"
            placeholder="New project name…"
            value={name()}
            maxLength={200}
            onInput={(e) => setName(e.currentTarget.value)}
          />
          <button
            type="submit"
            class="rounded-lg bg-noviq-700 px-4 py-2 text-white hover:bg-noviq-600 disabled:opacity-50"
            disabled={busy() || !name().trim()}
          >
            Create
          </button>
        </form>
      </Show>

      <Show when={error()}>
        <p class="mb-4 rounded border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-800">
          {error()}
        </p>
      </Show>

      {/* No `fallback` — the centred create card above IS the empty state. Two
          empty states for one condition is how a page ends up saying "no projects
          yet" underneath a card already asking you to make one. */}
      <Show when={projects().length > 0}>
        <ul class="divide-y divide-noviq-line rounded-lg border border-noviq-line bg-white">
          <For each={projects()}>
            {(p) => (
              <li class="flex flex-col gap-2 px-4 py-3 hover:bg-noviq-50 sm:flex-row sm:items-center sm:gap-4">
                <button
                  class="min-w-0 flex-1 text-left"
                  onClick={() => void onOpen(p)}
                >
                  <span class="block break-words font-medium text-noviq-ink">{p.name}</span>
                  <Show when={p.is_copy}>
                    <span class="ml-2 rounded bg-noviq-100 px-2 py-0.5 text-xs text-noviq-700">
                      shared with you
                    </span>
                  </Show>
                  <span class="mt-0.5 block text-xs text-noviq-500">
                    {p.document_count} document{p.document_count === 1 ? "" : "s"} ·{" "}
                    {p.chat_count} chat{p.chat_count === 1 ? "" : "s"}
                  </span>
                </button>
                <div class="flex flex-shrink-0 gap-1">
                  <button class="rounded-md px-3 py-2 text-sm text-noviq-600 hover:bg-noviq-100 hover:text-noviq-800" onClick={() => onRename(p)}>
                    Rename
                  </button>
                  <button class="rounded-md px-3 py-2 text-sm text-red-700 hover:bg-red-50" onClick={() => onDelete(p)}>
                    Delete
                  </button>
                </div>
              </li>
            )}
          </For>
        </ul>
      </Show>
     </div>
    </div>
  );
}
