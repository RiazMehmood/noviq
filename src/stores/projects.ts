// Feature 027 — "My Projects" client store.
//
// Shapes mirror `specs/027-my-projects/contracts/api.md` exactly. Two rules the
// UI must not quietly break:
//
//   1. `includeCorpus` is PER QUESTION. It is deliberately not held in this
//      store and not persisted anywhere — it is passed on a single `query` call
//      and defaults back to false on the next turn (FR-017). If you find
//      yourself adding it to state, that is the requirement being lost.
//   2. Hiding the projects nav entry when the feature is off is presentation
//      only. The server refuses every project route regardless (FR-038), so
//      never treat a UI check as the gate.

import { createSignal } from "solid-js";
import { call } from "../api/transport";

// ── Deployment gate, shared (2026-08-07) ────────────────────────────────────
// ⚠️ App.tsx and the admin console each held their OWN copy of this flag, read
// once at mount. Toggling it in Admin updated the server and Admin's copy, and
// nothing told the nav — so "My Projects" only appeared after a full browser
// reload. One signal, both readers, no refresh.
//
// Still PRESENTATION only: the server refuses every project route on its own
// (FR-038). Never treat this as the gate.
const [projectsEnabled, setProjectsEnabledSignal] = createSignal(false);
export { projectsEnabled };

/** Read the gate from the server into the shared signal. Fails CLOSED. */
export async function refreshProjectsEnabled(): Promise<boolean> {
  try {
    const on = await call<boolean>("get_my_projects_enabled", {});
    setProjectsEnabledSignal(on);
    return on;
  } catch {
    setProjectsEnabledSignal(false);
    return false;
  }
}

/**
 * Flip the gate on the server AND in this signal, so every reader updates at
 * once. Throws on failure so the caller can revert its checkbox.
 */
export async function setProjectsEnabled(enabled: boolean): Promise<void> {
  await call("set_my_projects_enabled", { enabled });
  setProjectsEnabledSignal(enabled);
}

export type ProjectDocumentStatus = "pending" | "ready" | "failed";

export interface ProjectDocument {
  source_id: string;
  file_name: string;
  display_name: string | null;
  /** Shown verbatim — a pending document must look pending, never absent (FR-008). */
  status: ProjectDocumentStatus;
  added_at: number;
  /** True when this document arrived by copy rather than by the user's upload. */
  added_by_copy: boolean;
}

export interface Project {
  id: string;
  name: string;
  document_count: number;
  chat_count: number;
  created_at: number;
  updated_at: number;
  /** This project was received from another user (ADR-0028). */
  is_copy: boolean;
}

export interface ProjectChatSummary {
  conversation_id: string;
  title: string;
  updated_at: number;
  is_shared: boolean;
}

const [projects, setProjects] = createSignal<Project[]>([]);
const [activeProject, setActiveProject] = createSignal<Project | null>(null);
const [documents, setDocuments] = createSignal<ProjectDocument[]>([]);
const [chats, setChats] = createSignal<ProjectChatSummary[]>([]);
const [enabled, setEnabled] = createSignal(false);

export {
  projects,
  activeProject,
  documents,
  chats,
  enabled,
  setEnabled,
  setActiveProject,
};

export async function loadProjects(): Promise<void> {
  setProjects(await call<Project[]>("list_projects", {}));
}

export async function createProject(name: string): Promise<Project> {
  const project = await call<Project>("create_project", { name: name.trim() });
  await loadProjects();
  return project;
}

export async function renameProject(projectId: string, name: string): Promise<void> {
  await call("rename_project", { project_id: projectId, name: name.trim() });
  await loadProjects();
}

export async function deleteProject(projectId: string): Promise<number> {
  // The server reports how many chats went with it so the confirmation can be
  // honest rather than a guess.
  const res = await call<{ deleted: boolean; chats_deleted: number }>(
    "delete_project",
    { project_id: projectId },
  );
  await loadProjects();
  return res.chats_deleted;
}

export async function openProject(projectId: string): Promise<void> {
  const res = await call<Project & { documents: ProjectDocument[] }>("get_project", {
    project_id: projectId,
  });
  setActiveProject(res);
  setDocuments(res.documents);
  setChats(await call<ProjectChatSummary[]>("list_project_chats", { project_id: projectId }));
}

export async function addDocuments(
  projectId: string,
  sourceIds: string[],
): Promise<{ added: number; already_present: number }> {
  const res = await call<{ added: number; already_present: number }>(
    "add_project_documents",
    { project_id: projectId, source_ids: sourceIds },
  );
  await openProject(projectId);
  return res;
}

export async function removeDocument(
  projectId: string,
  sourceId: string,
): Promise<{ removed: boolean; content_retained: boolean }> {
  const res = await call<{ removed: boolean; content_retained: boolean }>(
    "remove_project_document",
    { project_id: projectId, source_id: sourceId },
  );
  await openProject(projectId);
  return res;
}

/**
 * Drop a chat's project membership.
 *
 * ⚠️ Only the membership. The caller must still do the normal conversation
 * cleanup (`deleteConversationAttachments` + `hideConversation`) exactly as the
 * global rail does — this route deliberately owns one thing, so the two paths
 * cannot drift on what "delete a chat" means.
 */
export async function deleteProjectChat(conversationId: string): Promise<boolean> {
  const res = await call<{ removed: boolean }>("delete_project_chat", {
    conversation_id: conversationId,
  });
  return res.removed;
}

export async function startProjectChat(projectId: string): Promise<string> {
  const res = await call<{ conversation_id: string }>("start_project_chat", {
    project_id: projectId,
  });
  return res.conversation_id;
}

/**
 * Enter a project's workspace and return the chat to land on.
 *
 * 2026-08-29 — the project overview page was retired, so a project's ONLY
 * destination is one of its chats, and one has to exist to navigate to. Opens
 * the most recent chat, or starts the first one.
 *
 * ⚠️ Creating a chat here is a side effect of the officer ASKING to enter a
 * project, which is a thing they did on purpose. It must never become a side
 * effect of deleting one — see `deleteProjectChatEntry` in `ChatPage`, where a
 * `newChat()` call made a deleted chat look like it had come back.
 */
export async function enterProject(projectId: string): Promise<string> {
  await openProject(projectId);
  const existing = chats();
  // `list_project_chats` returns newest first — the same ordering the chat rail
  // relies on when it moves to "the most recent survivor" after a delete.
  if (existing.length > 0) return existing[0].conversation_id;
  const conversationId = await startProjectChat(projectId);
  await openProject(projectId); // the new chat must already be in the rail on arrival
  return conversationId;
}

/**
 * Share a project. The recipient receives their OWN copy — this does not grant
 * access to the caller's project (ADR-0028).
 *
 * The caller MUST be shown, before confirming, that documents shared this way
 * cannot afterwards be recalled from the copy (FR-024).
 */
export async function shareProject(
  projectId: string,
  recipient: string,
): Promise<{ shared_with: string; new_project_id: string }> {
  return call("share_project", { project_id: projectId, recipient });
}
