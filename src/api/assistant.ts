/**
 * Feature 015 — typed invoke() wrappers + event subscriptions for the unified
 * bureaucrat chat assistant (src-tauri/src/commands/assistant.rs). Field names
 * mirror the Rust serde boundary (snake_case — TurnResponse has no rename_all)
 * and the command contract (contracts/assistant-commands.md).
 */
// T010: route command calls through the transport seam (Tauri `invoke` on the
// desktop, HTTP `POST /api/<cmd>` in the browser). Event subscriptions below
// stay Tauri-only — the web transport returns the full TurnResponse from
// `sendMessage`, so live token/progress events are a desktop-only enhancement.
import { call as invoke } from "./transport";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Citation {
  n: number;
  instrument: string;
  path: string;
  verbatim: string;
  page: number | null;
  /** Present when the citation came from a single chunk — enables click-through. */
  chunk_id?: string;
  source_id?: string;
  /**
   * F027 (FR-016) — `"project"` (a document the user put in this project) or
   * `"corpus"` (enacted law). Set only on project turns, where an answer can
   * draw on both at once.
   *
   * ⚠️ Must be rendered whenever present. An officer reading "Section 9" needs
   * to know whether that is the Procurement Rules or their own draft contract;
   * mistaking one for the other is the failure Citation Discipline exists to
   * prevent.
   */
  provenance?: "project" | "corpus";
}

export interface Flag {
  missing: string;
  offer_to_index: boolean;
}

export interface ContextFill {
  utilization: number;
  notice: boolean;
}

/** Per-turn budget audit (T047) — the `Σ prompt + reserve ≤ n_ctx` proof. */
export interface TurnAudit {
  n_ctx: number;
  gen_reserve: number;
  prompt_tokens: number;
  kept_law: number;
  law_overflow: boolean;
  kept_turns: number;
  summary_included: boolean;
  must_fit_failed: boolean;
}

export type Capability =
  | "guidance"
  | "draft"
  | "summarize_doc"
  | "compare"
  | "conflict"
  | "amendment_chain"
  | "action_items"
  | "count";

export interface TurnResponse {
  conversation_id: string;
  /** Persisted assistant-turn id — the handle for `exportDraft` on draft turns. */
  message_id: string;
  /** Query-trace id for this turn (Track 3) — handle for the admin "Debug this
   *  answer" link. Empty on transports that don't write a trace. */
  trace_id?: string;
  capability: Capability;
  answer: string;
  citations: Citation[];
  offer: "tailor" | null;
  flags: Flag[];
  context_fill: ContextFill;
  audit: TurnAudit;
  /** scope-routing 2026-07: retrieval was scoped to the chat's attachment by
   *  default (narrow-first). When true the UI shows a "Search all documents"
   *  button that re-runs the same question widened to the full corpus. */
  scoped_to_attachment?: boolean;
  /** Web results for this turn, when web search was on (2026-07-29). These are
   *  NOT citations: they are unverified public web sources and are rendered in
   *  their own section, never alongside `citations`, which mean indexed law. */
  web_results?: WebResult[];
}

/** One public web-search result. Snippet only — see the searxng service docs. */
export interface WebResult {
  title: string;
  url: string;
  content: string;
  score?: number;
  published?: string;
}

export interface CarriedFact {
  key: string;
  value: string;
}

export interface NewChat {
  conversation_id: string;
  carried_facts: CarriedFact[];
  recap: string;
}

export interface ModelStatus {
  model_file: string;
  n_ctx: number;
  trained_ctx: number;
  ram_clamped: boolean;
  source: "auto" | "ECHO_N_CTX";
}

export type AssistantPhase =
  | "classify"
  | "retrieve"
  | "map"
  | "reduce"
  | "generate"
  | "transform"
  | "additive";

export interface AssistantProgress {
  phase: AssistantPhase;
}

/**
 * Primary turn entrypoint — intent-routed; serves all capabilities.
 *
 * `sourceIds` is the composer's document scope (document-tree checkboxes +
 * @-mention chips). Omit / empty → no restriction ("every document"); a
 * non-empty list narrows retrieval to those sources (the server still
 * intersects it with what the user is allowed to see).
 */
export function sendMessage(
  conversationId: string,
  text: string,
  sourceIds?: string[],
  deep?: boolean,
  widen?: boolean,
  web?: boolean,
  includeCorpus?: boolean,
): Promise<TurnResponse> {
  const args: Record<string, unknown> = { conversationId, text };
  if (sourceIds && sourceIds.length > 0) args.sourceIds = sourceIds;
  if (deep) args.deep = true;
  // F027 — include the shared legal corpus in THIS question, inside a project.
  // Sent per turn and stored nowhere; the next call omits it again (FR-017).
  // Note there is no `projectId` here and deliberately cannot be: the server
  // derives the project from `conversationId`, so a crafted request cannot name
  // one (ADR-0029).
  if (includeCorpus) args.includeCorpus = true;
  // "Search all documents" widen button (scope-routing 2026-07): re-run the same
  // question over the full corpus, ignoring the default narrow-to-attachment.
  if (widen) args.widen = true;
  // Web search (2026-07-29): ALSO run a public web search and show it as a
  // separate, separately cited section. Never alters corpus retrieval.
  if (web) args.web = true;
  return invoke<TurnResponse>("send_message", args);
}

/** Result of `poll_turn` — a turn recovered from `query_log` after the HTTP
 *  response was lost (the tunnel kills requests at ~100 s; the server finishes
 *  the turn in a detached task and persists it). */
export interface PolledTurn {
  done: boolean;
  message_id?: string;
  question?: string;
  answer?: string;
  citations?: Citation[];
  created_at?: number;
}

/** Poll for a turn completed at/after `sinceMs` in this conversation (web-only
 *  recovery path — the desktop's in-process IPC can't drop mid-turn). */
export function pollTurn(conversationId: string, sinceMs: number): Promise<PolledTurn> {
  return invoke<PolledTurn>("poll_turn", { conversationId, sinceMs });
}

/** F026 stop button — cancel this conversation's in-flight chat turn. Idempotent:
 *  `cancelled=false` just means no live turn was found (it already finished). */
export function cancelTurn(conversationId: string): Promise<{ cancelled: boolean }> {
  return invoke<{ cancelled: boolean }>("cancel_turn", { conversationId });
}

/** F022 — is Notice Assist open for this conversation? The banner mirrors server
 *  state, and the browser can't infer it: opening a past conversation says nothing
 *  about whether `/help` was typed in it. Ask, don't assume. */
export function assistStatus(conversationId: string): Promise<{ open: boolean }> {
  return invoke<{ open: boolean }>("assist_status", { conversationId });
}

/** F022 — leave Notice Assist without running a chat turn. The assist session is
 *  server state and the banner only mirrors it, so leaving must reach the server:
 *  clearing the flag client-side would hide the banner while every following
 *  question stayed routed into the notice flow. Idempotent: `left=false` means
 *  there was no open session. */
export function assistExit(conversationId: string): Promise<{ left: boolean }> {
  return invoke<{ left: boolean }>("assist_exit", { conversationId });
}

/** T027 — render a produced draft to plain text (no letterhead/signature). */
export function exportDraft(conversationId: string, messageId: string): Promise<{ text: string }> {
  return invoke<{ text: string }>("export_draft", { conversationId, messageId });
}

/** Feature 018 — the two citation-feedback selections (codes match the backend). */
export type CitationAccuracy = "all" | "some" | "wrong" | "unsure";
export type CitationCompleteness = "yes" | "missing" | "na";

export interface FeedbackInput {
  messageId: string;
  conversationId?: string;
  citationAccuracy?: CitationAccuracy;
  citationCompleteness?: CitationCompleteness;
}

/**
 * Feature 018 (US1) — submit citation feedback for one answer. Either question
 * may be sent alone; a re-tap supersedes the prior value server-side. Anchored to
 * the answer's `message_id`. Best-effort: callers should not block the UI on it.
 */
export function submitFeedback(f: FeedbackInput): Promise<{ ok: boolean }> {
  return invoke<{ ok: boolean }>("submit_feedback", {
    messageId: f.messageId,
    conversationId: f.conversationId,
    citationAccuracy: f.citationAccuracy,
    citationCompleteness: f.citationCompleteness,
  });
}

/** T029 — active model + the window derived for it (scales with the model). */
export function modelStatus(): Promise<ModelStatus> {
  return invoke<ModelStatus>("model_status");
}

/** T041 — resolve named documents to indexed source ids for scope confirmation. */
export function confirmDocuments(
  conversationId: string,
  candidates: string[],
): Promise<{ confirmed: string[] }> {
  return invoke<{ confirmed: string[] }>("confirm_documents", { conversationId, candidates });
}

/** T045 (FR-018b) — start a fresh chat carrying pinned case facts + a recap. */
export function newChatFromNotice(fromConversationId: string): Promise<NewChat> {
  return invoke<NewChat>("new_chat_from_notice", { fromConversationId });
}

/** Phase progress: classify | retrieve | map | reduce | generate. */
export function onAssistantProgress(
  cb: (p: AssistantProgress) => void,
): Promise<UnlistenFn> {
  return listen<AssistantProgress>("assistant://progress", (ev) => cb(ev.payload));
}

/** Incremental answer tokens (reused from the grounded pipeline). */
export function onAssistantToken(cb: (token: string) => void): Promise<UnlistenFn> {
  return listen<{ token: string }>("chat://token", (ev) => cb(ev.payload.token));
}

/** Final assembled turn (also returned by `sendMessage`). */
export function onAssistantComplete(cb: (r: TurnResponse) => void): Promise<UnlistenFn> {
  return listen<TurnResponse>("assistant://complete", (ev) => cb(ev.payload));
}
