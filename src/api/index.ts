/**
 * Typed invoke() wrappers for all Tauri backend commands.
 * T035: All P1 wrappers; extended in later phases.
 */
// T010: route all command wrappers through the transport seam — Tauri `invoke`
// on the desktop, HTTP `POST /api/<cmd>` in the browser (same call signature).
import { call as invoke, request } from "./transport";

// ─── Types (mirror of Rust DTO structs) ──────────────────────────────────────

export interface Notebook {
  id: string;
  name: string;
  created_at: number;
  updated_at: number;
}

export interface Source {
  id: string;
  file_path: string;
  file_name: string;
  format: string;
  file_size_bytes: number;
  indexing_status: "pending" | "processing" | "indexed" | "failed" | "unindexed";
  error_reason: string | null;
  last_modified: number;
  indexed_at: number | null;
  created_at: number;
  index_tier: "fast" | "deep";
  // F021 — per-document visibility for the share toggle.
  visibility: "private" | "shared";
  /** F021 — owner of the document. `null` = the global/shared corpus (admin uploads). */
  owner_id?: string | null;
  // Migration 021 — corpus organisation, admin-controlled.
  /** `null`/absent = Uncategorised. */
  category_id?: string | null;
  /** When the document was withdrawn from service. `null`/absent = in service. */
  disabled_at?: number | null;
}

export interface QuerySession {
  id: string;
  notebook_id: string;
  created_at: number;
  updated_at: number;
}

export interface Citation {
  source_id: string;
  chunk_id: string;
  excerpt: string;
  page: number | null;
}

export interface QualityNotice {
  constrained: boolean;
  message: string;
}

export interface Message {
  id: string;
  session_id: string;
  role: "user" | "assistant";
  content: string;
  citations: Citation[];
  tier_used: string | null;
  is_incomplete: boolean;
  created_at: number;
}

/// Emitted by `chat_query_legal` at the start of a query so the frontend
/// can capture the chat_id and cancel mid-flight via `cancelChat`. Other
/// `chat://*` events from the legacy `stream_query_notebook` pipeline
/// (thinking / token / done / cancelled / error / background-paused)
/// were removed 2026-06-03 along with that command.
export interface ChatStartedEvent {
  chat_id: string;
}

export interface HardwareTier {
  max_model_tier: number;
  embedding_model: string;
  has_discrete_gpu: boolean;
  vram_gb: number;
  ram_gb: number;
}

export interface SearchResult {
  source_id: string;
  chunk_id: string;
  file_path: string;
  file_name: string;
  excerpt: string;
  last_modified: number;
  relevance_score: number;
  page_number: number | null;
}

export interface IndexedFolder {
  id: string;
  folder_path: string;
  indexing_status: string;
  file_count: number;
  last_scanned_at: number | null;
  created_at: number;
  // F021 — Private/Shared + owner (FR-012).
  visibility: "private" | "shared";
  owner_username: string | null;
  // Migration 026 — the batch running right now, for the indexing progress bar.
  // `indexing_total === 0` means idle; do NOT infer idle from done === total,
  // which is also true for the moment a batch completes.
  indexing_total: number;
  indexing_done: number;
}

export interface Entity {
  id: string;
  canonical_value: string;
  entity_type: string;
  normalized_value: string | null;
  source_count: number;
  created_at: number;
}

export interface BatchJob {
  id: string;
  folder_path: string;
  operation: string;
  status: string;
  total_files: number;
  processed_files: number;
  failed_files: number;
  created_at: number;
  completed_at: number | null;
}

export interface AuditEvent {
  id: string;
  event_type: string;
  entity_id: string | null;
  details_json: string | null;
  created_at: number;
}

// ─── Source / KG status commands ──────────────────────────────────────────────

export interface KgSourceStatus {
  source_id: string;
  file_name: string;
  ner_status: "pending" | "running" | "done";
  ner_progress: number; // 0-100
  ner_total: number;
}

export const getKgStatus = () =>
  invoke<KgSourceStatus[]>("get_kg_status");

export interface ChatScope {
  kind: "notebook" | "folder" | "both";
  notebookIds?: string[];
  folderId?: string | null;
  sourceIds?: string[];
}

// ─── Feature 007 — Pakistan legal RAG (single chat surface) ───────────────────

export type Intent =
  | "lookup"
  | "list_children"
  | "summarize"
  | "difference"
  | "cross_compare"
  | "action_items"
  | "timeline"
  | "procedure"
  | "fallback";

export type IntentScope =
  | { kind: "both" }
  | { kind: "folder" }
  | { kind: "folder_doc"; doc_id: string }
  | { kind: "notebook" }
  | { kind: "notebook_doc"; doc_id: string };

export interface LegalCitation {
  officer_format: string;
  short_format: string;
  doc_id: string;
  node_id: string | null;
  page: number | null;
}

export interface UpgradePrompt {
  doc_ids: string[];
  reason: string;
}

export interface FlowStep {
  order: number;
  actor: string;
  action: string;
  condition: string | null;
  timeline: string | null;
  citation: LegalCitation;
  branches: FlowStep[];
}

export interface FlowDiagram {
  process_name: string;
  steps: FlowStep[];
}

export interface ChatBubble {
  headline: string;
  body: string;
  citations: LegalCitation[];
  see_also: LegalCitation[];
  flow: FlowDiagram | null;
}

export interface ChatResponse {
  intent: Intent;
  intent_confidence: number;
  bubbles: ChatBubble[];
  upgrade_prompt: UpgradePrompt | null;
}

export const listSourcesInFolder = (folderId: string) =>
  invoke<Source[]>("list_sources_in_folder", { folderId });

// ─── System commands ──────────────────────────────────────────────────────────

export const getHardwareTier = () => invoke<HardwareTier>("get_hardware_tier");

// ─── Search commands (P2) ─────────────────────────────────────────────────────

export const globalSearch = (query: string) =>
  invoke<SearchResult[]>("global_search", { query });

// ─── Folder commands (P2) ─────────────────────────────────────────────────────

export const addIndexedFolder = (folderPath: string) =>
  invoke<IndexedFolder>("add_indexed_folder", { folderPath });

export const listIndexedFolders = () =>
  invoke<IndexedFolder[]>("list_indexed_folders");

/** What a folder delete removed, or (from the preview) would remove. */
export interface FolderDeleteCounts {
  sources: number;
  chunks: number;
  embeddings: number;
  folderFound: boolean;
}

export const revokeIndexedFolder = (folderId: string) =>
  invoke<FolderDeleteCounts>("revoke_indexed_folder", { folderId });

/** Read-only: how much deleting this folder would destroy. Backs the confirm
 *  dialog so the warning quotes real numbers instead of a generic "are you sure". */
export const folderDeletePreview = (folderId: string) =>
  invoke<FolderDeleteCounts>("folder_delete_preview", { folderId });

export const listFolderSources = (folderPath: string) =>
  invoke<Source[]>("list_folder_sources", { folderPath });

export const retryFolderSource = (sourceId: string, folderId: string) =>
  invoke<void>("retry_folder_source", { sourceId, folderId });

export const deepIndexFolderSource = (sourceId: string, folderId: string) =>
  invoke<void>("deep_index_folder_source", { sourceId, folderId });

export const undeepIndexFolderSource = (sourceId: string, folderId: string) =>
  invoke<void>("undeep_index_folder_source", { sourceId, folderId });

export const removeFolderSource = (sourceId: string, folderId: string) =>
  invoke<void>("remove_folder_source", { sourceId, folderId });

/// Permanently delete a file: wipe its index AND remove it from disk + list.
export const deleteFolderSource = (sourceId: string, folderId: string) =>
  invoke<void>("delete_folder_source", { sourceId, folderId });

// ─── F021: user documents (visibility, usage, attachments) ──────────────────

export interface MyUsage {
  usedBytes: number;
  usedFiles: number;
  limitBytes: number;
  limitFiles: number;
}

export const myUsage = () => invoke<MyUsage>("my_usage");

// Admin-gated sharing (2026-07-28): for a non-admin, visibility:"shared" no
// longer flips the flag — it files a review request; the response then carries
// `requested: true` (and `alreadyPending` on a repeat click).
export const setVisibility = (
  target: { folderId: string } | { sourceId: string },
  visibility: "private" | "shared",
) =>
  invoke<{ ok: boolean; visibility: string; requested?: boolean; alreadyPending?: boolean }>(
    "set_visibility",
    { ...target, visibility },
  );

// ─── Admin-gated sharing: admin review queue ────────────────────────────────

export interface ShareRequest {
  id: string;
  sourceId: string;
  requesterUserId: string;
  requesterUsername: string | null;
  fileName: string | null;
  displayName: string | null;
  status: string;
  createdAt: number;
}

export const listShareRequests = () =>
  request<ShareRequest[]>("GET", "/admin/share_requests");

export const decideShareRequest = (id: string, approve: boolean) =>
  request<{ ok: boolean; approved: boolean }>("POST", `/admin/share_requests/${id}/decide`, {
    approve,
  });

export const deleteMyDocument = (sourceId: string) =>
  invoke<void>("delete_my_document", { sourceId });

/**
 * Latest indexing tick per source — the tunnel-safe twin of the `/index/events`
 * SSE stream (which the Cloudflare quick tunnel buffers to nothing). Sources with
 * no tick yet are simply absent from the response.
 */
export const indexProgress = (sourceIds: string[]) =>
  invoke<{ source_id: string; percent: number; stage: string }[]>("index_progress", {
    sourceIds,
  });

export const listConversationAttachments = (conversationId: string) =>
  invoke<import("./transport").ConversationAttachment[]>("list_conversation_attachments", {
    conversationId,
  });

export const promoteAttachment = (sourceId: string) =>
  invoke<void>("promote_attachment", { sourceId });

export const deleteConversationAttachments = (conversationId: string) =>
  invoke<{ ok: boolean; removed: number }>("delete_conversation", { conversationId });

// ─── Account-bound chat history (migration 020) ──────────────────────────────
//
// Chat history used to be written only to localStorage, so it was bound to one
// browser: sign in on a second device and the rail was empty. These read the
// server's per-turn record instead, scoped to the signed-in user, so the
// transcript follows the account. Web transport only — the desktop build has no
// server session to scope by and keeps its local history.

/** One chat in the history rail. */
export interface ConversationSummary {
  conversation_id: string;
  /** The chat's first question, truncated server-side to 60 chars. */
  title: string;
  updated_at: number;
  turn_count: number;
}

/** One replayable turn: the question plus the response that answered it. */
export interface HistoryTurn {
  id: string;
  question: string;
  answer: string | null;
  citations_json: string | null;
  /** Full serialized `TurnResponse`. NULL for turns recorded before migration
   *  020 — those replay from `answer` + `citations_json` alone. */
  response_json: string | null;
  created_at: number;
}

export const listConversations = () =>
  invoke<ConversationSummary[]>("list_conversations");

export const conversationTurns = (conversationId: string) =>
  invoke<HistoryTurn[]>("conversation_turns", { conversationId });

/** Hide a chat from this user's history on every device (soft delete — the
 *  admin audit trail keeps the record). */
export const hideConversation = (conversationId: string) =>
  invoke<{ hidden: number }>("hide_conversation", { conversationId });

// ─── Entity commands (P4) ────────────────────────────────────────────────────

export const getEntityRelationships = (entityId: string) =>
  invoke<unknown[]>("get_entity_relationships", { entityId });

export const getEntitySources = (entityId: string) =>
  invoke<unknown[]>("get_entity_sources", { entityId });

export const getKnowledgeGraph = (limit?: number, offset?: number, sourceIds?: string[]) =>
  invoke<{ nodes: Entity[]; edges: unknown[] }>("get_knowledge_graph", {
    limit: limit ?? 500,
    offset: offset ?? 0,
    sourceIds: sourceIds && sourceIds.length > 0 ? sourceIds : null,
  });

// ─── Audit commands ───────────────────────────────────────────────────────────

export const getAuditLog = (limit?: number, offset?: number) =>
  invoke<AuditEvent[]>("get_audit_log", { limit: limit ?? 50, offset: offset ?? 0 });

// ─── System / model commands ─────────────────────────────────────────────────

export interface ModelStatus {
  exists: boolean;
  model_name: string;
  model_path: string;
  size_mb: number | null;
}

export const checkModel = () => invoke<ModelStatus>("check_model");

export const downloadModel = () => invoke<void>("download_model");

export const restartApp = () => invoke<void>("restart_app");

// ─── Knowledge-graph relation-type preferences ───────────────────────────────

export interface KgPreferences {
  relation_types: string[];
}

export const getKgPreferences = () =>
  invoke<KgPreferences>("get_kg_preferences");

export const setKgPreferences = (relationTypes: string[]) =>
  invoke<KgPreferences>("set_kg_preferences", { relationTypes });

// ─── GPU acceleration ─────────────────────────────────────────────────────────

export interface GpuSettings {
  enabled: boolean;
  vendor: string;       // "Nvidia" | "Amd" | "None"
  vram_gb: number;
  gpu_name: string;
  layers_auto: number;
  layers_total: number;
}

export const getGpuSettings = () => invoke<GpuSettings>("get_gpu_settings");

export const setGpuEnabled = (enabled: boolean) =>
  invoke<void>("set_gpu_enabled", { enabled });

// ─── Chat answer mode (extractive vs LLM) ─────────────────────────────────────
export const getChatExtractive = () => invoke<boolean>("get_chat_extractive");

export const setChatExtractive = (extractive: boolean) =>
  invoke<void>("set_chat_extractive", { extractive });

// ─── Query trace logs ─────────────────────────────────────────────────────────

export const openQueryLogsDir = () => invoke<string>("open_query_logs_dir");

export const readLatestQueryTrace = () =>
  invoke<[string, string]>("read_latest_query_trace");

export const downloadQueryTraceTo = (dest: string) =>
  invoke<string>("download_query_trace_to", { dest });

export const downloadAllQueryTracesTo = (dest: string) =>
  invoke<[number, string]>("download_all_query_traces_to", { dest });

export const downloadRecentQueryTracesTo = (dest: string, limit: number) =>
  invoke<[number, string]>("download_recent_query_traces_to", { dest, limit });

// ─── Admin query-debug panel (Track 3, web-only) ─────────────────────────────

export interface TraceSummary {
  id: string;
  started_at: string;
  query: string;
  capability: string;
  total_ms: number;
  outcome: string;
  tier_used: string;
  citations: number;
  answer_chars: number;
}

/** Recent query traces, newest first (admin only). Web transport only. */
export const listQueryTraces = (limit = 50) =>
  request<TraceSummary[]>("GET", `/query_traces?limit=${limit}`);

/** The full step-by-step `.log` text for one trace id (admin only). */
export const getQueryTrace = (id: string) =>
  request<string>("GET", `/query_traces/${encodeURIComponent(id)}`);

// ─── Cloud inference (multi-provider, OpenAI-compatible) ─────────────────────

export interface CloudSettings {
  enabled: boolean;
  /** "groq" | "cerebras" | "openrouter" | "together" | "deepseek" | "openai" | "fireworks" | "custom" */
  provider: string;
  api_key: string;
  model: string;
  /** Optional. When non-empty, overrides the provider's default endpoint URL.
   *  Required when `provider === "custom"`. */
  endpoint_url?: string;
}

export const getCloudSettings = () =>
  invoke<CloudSettings>("get_cloud_settings");

export const setCloudSettings = (settings: CloudSettings) =>
  invoke<void>("set_cloud_settings", { settings });

/**
 * Per-user preferences (2026-07-29). Distinct from the server-wide operator
 * settings above — these are keyed to the signed-in user, so one officer
 * turning the follow-up offer off does not change anyone else's answers.
 */
export interface UserPrefs {
  /** Whether answers end with a "Next steps" follow-up offer. Default true. */
  refinementOffer: boolean;
}

export const getUserPrefs = () => invoke<UserPrefs>("get_user_prefs");

/** Send only the fields being changed; the response carries the full set. */
export const setUserPrefs = (prefs: Partial<UserPrefs>) =>
  invoke<UserPrefs>("set_user_prefs", prefs);

export interface CleanOrphansReport {
  scanned: number;
  deleted: number;
  deleted_files: string[];
}

export const cleanOrphanSources = () =>
  invoke<CleanOrphansReport>("clean_orphan_sources");

export const resumePendingNer = () =>
  invoke<number>("resume_pending_ner");

// ─── Meeting Agent (Phase 2) ─────────────────────────────────────────────────

export interface Meeting {
  id: number;
  title: string;
  started_at: string;
  ended_at: string | null;
  language: string;
  model_path: string;
  status: string;
  error: string | null;
}

export interface MeetingSegment {
  id: number;
  meeting_id: number;
  t0_ms: number;
  t1_ms: number;
  text: string;
  created_at: string;
}

export const startMeeting = (title: string, language: string, modelPath: string) =>
  invoke<number>("start_meeting", { title, language, modelPath });

export const stopMeeting = (id: number) =>
  invoke<void>("stop_meeting", { id });

export const listMeetingSegments = (meetingId: number) =>
  invoke<MeetingSegment[]>("list_meeting_segments", { meetingId });

export const getMeeting = (id: number) =>
  invoke<Meeting | null>("get_meeting", { id });
