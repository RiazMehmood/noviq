/**
 * The in-flight chat turn — module-level, deliberately OUTSIDE `ChatPage`.
 *
 * ⚠️ Why this store exists (2026-08-11). Every tab is a sibling route under
 * `AppShell`, so leaving `/` UNMOUNTS `ChatPage`. When the running turn lived in
 * component signals, navigating to My Projects or Settings mid-answer disposed
 * the reactive scope, and:
 *   1. `sendMessage` resolved into a dead scope — the answer was dropped;
 *   2. the `on(turns)` effect that caches the transcript was gone too, so the
 *      answer never even reached localStorage;
 *   3. coming back minted a BLANK new chat, and reopening the real one showed
 *      the question with no answer, permanently (the server-side hydration only
 *      fires for an empty cache).
 * The server had finished and persisted the turn the whole time. Only the client
 * had no way back to it.
 *
 * ⭐ Module state outlives every unmount, so the turn now survives navigation:
 * tokens keep arriving, the answer lands in a queue keyed by conversation, and
 * whichever `ChatPage` is mounted next drains it. The turn is NEVER cancelled on
 * navigation — someone who has waited 90 s for an answer wants it kept.
 *
 * This store owns the turn, not the transcript. `ChatPage` remains the single
 * owner of `turns` + its localStorage cache; it drains completed turns from here
 * and appends them, which is what keeps the rail, the cache and masking working
 * exactly as before.
 */
import { createSignal } from "solid-js";
import { isTauri, openChatStream } from "../api/transport";
import {
  cancelTurn,
  pollTurn,
  sendMessage,
  type AssistantPhase,
  type TurnResponse,
} from "../api/assistant";
import type { MaskedItem } from "../lib/maskEngine";

/** How a turn ended. The raw server answer — masking is re-filled by the page,
 *  which is the only place that holds the session's label→value map. */
export type TurnOutcome =
  | { kind: "answer"; response: TurnResponse }
  | { kind: "stopped" }
  | { kind: "error"; message: string };

/** A turn currently being generated. */
export interface ActiveTurn {
  conversationId: string;
  /** The exact masked question (an assistant turn stores it so "Search all
   *  documents" can re-run the same question without re-masking). */
  query: string;
  masked: MaskedItem[];
  deep: boolean;
  /** When this turn started. Picks the newest when several run at once. */
  startedAt: number;
}

export interface CompletedTurn extends ActiveTurn {
  outcome: TurnOutcome;
}

// ⚠️ Keyed BY CONVERSATION, not one global turn (2026-08-11, second pass).
//
// The first cut held a single active turn, which made `busy` global — and a
// global `busy` disables the composer everywhere, so a turn generating in one
// chat made it impossible to type in ANY other chat or in a new one. Worse,
// `runTurn` early-returned while busy, so a second send would have appended the
// question and silently produced no answer at all.
//
// One turn per conversation, any number of conversations. The server already
// serialises generation (`gen_gate`) and is built for concurrent users, so a
// second turn queues there rather than breaking anything here.
const [active, setActive] = createSignal<Record<string, ActiveTurn>>({});
const [streamingText, setStreamingText] = createSignal<Record<string, string>>({});
const [turnPhase, setTurnPhase] = createSignal<Record<string, AssistantPhase | null>>({});

/** Answers that finished but have not been rendered yet, per conversation.
 *  Non-reactive on purpose (a Map of arrays); `completedTick` is the signal that
 *  tells the page to come and drain it. */
const completed = new Map<string, CompletedTurn[]>();
const [completedTick, setCompletedTick] = createSignal(0);

/** Conversations the officer stopped, read on resolve so a cancelled turn
 *  renders a clean "Stopped." instead of an empty bubble or a spurious
 *  connection error. Per conversation for the same reason as everything else
 *  here — one chat being stopped must not mark another chat's turn stopped. */
const stopped = new Set<string>();

/** Is a turn generating in THIS conversation? There is deliberately no global
 *  "is anything running" — every caller cares about one conversation, and the
 *  global form is what disabled the whole UI. */
export const busyIn = (conversationId: string) => !!active()[conversationId];
/** Live answer tokens for one conversation's turn. */
export const streamingIn = (conversationId: string) => streamingText()[conversationId] ?? "";
export const phaseIn = (conversationId: string) => turnPhase()[conversationId] ?? null;
/** Whether that conversation's running turn used deep search. */
export const deepIn = (conversationId: string) => active()[conversationId]?.deep ?? false;

/**
 * The conversation to reopen when `ChatPage` mounts with no route parameter —
 * the most recently started turn still running, if any. Landing the officer on
 * a blank "New chat" while an answer they asked for is generating hides the one
 * thing they came back for.
 */
export const resumeConversationId = (): string | null => {
  const running = Object.values(active());
  if (running.length === 0) return null;
  return running.reduce((a, b) => (b.startedAt > a.startedAt ? b : a)).conversationId;
};

/** Desktop (Tauri) token/progress listeners carry no conversation id, so they
 *  feed the newest running turn. Web uses the per-conversation SSE stream opened
 *  in `runTurn` and never reaches these. */
export const appendToken = (t: string) => {
  const cid = resumeConversationId();
  if (cid) setStreamingText((s) => ({ ...s, [cid]: (s[cid] ?? "") + t }));
};
export const setPhase = (p: AssistantPhase | null) => {
  const cid = resumeConversationId();
  if (cid) setTurnPhase((s) => ({ ...s, [cid]: p }));
};

/** Clear one conversation's streaming bubble — ignored while its turn is live,
 *  so switching chats or starting a new one cannot blank an answer mid-flight. */
export function clearStreamingIfIdle(conversationId: string) {
  if (busyIn(conversationId)) return;
  setStreamingText((s) => {
    if (!(conversationId in s)) return s;
    const next = { ...s };
    delete next[conversationId];
    return next;
  });
}

/** Take (and remove) the answers waiting for this conversation. */
export function takeCompletedTurns(conversationId: string): CompletedTurn[] {
  const queued = completed.get(conversationId);
  if (!queued || queued.length === 0) return [];
  completed.delete(conversationId);
  return queued;
}

function complete(turn: ActiveTurn, outcome: TurnOutcome) {
  const queued = completed.get(turn.conversationId) ?? [];
  queued.push({ ...turn, outcome });
  completed.set(turn.conversationId, queued);
  setCompletedTick((n) => n + 1);
}

/** Subscribe-able: bumps whenever an answer lands in the queue. */
export const completedSignal = completedTick;

export interface RunTurnOptions {
  deep: boolean;
  sourceIds?: string[];
  widen?: boolean;
  includeCorpus?: boolean;
  web?: boolean;
  /** The model's context window, echoed into a recovered turn's audit so the
   *  top-bar readout stays sane (no audit rides along a `poll_turn` row). */
  nCtx?: number;
}

/**
 * Send one masked question and record the outcome for its conversation.
 *
 * Shared by the composer and the "Search all documents" widen button, so both
 * get the same live-SSE + tunnel-recovery handling. Resolves when the turn is
 * done; callers do not need to await it — the result is queued either way, which
 * is precisely what makes navigating away safe.
 */
export async function runTurn(
  conversationId: string,
  query: string,
  masked: MaskedItem[],
  opts: RunTurnOptions,
): Promise<void> {
  // Only ONE turn per conversation — a second question in the same chat would
  // race the first into the same transcript. A turn in a DIFFERENT chat is fine
  // and deliberately allowed.
  if (busyIn(conversationId)) return;
  const sentAt = Date.now();
  const turn: ActiveTurn = {
    conversationId,
    query,
    masked,
    deep: opts.deep,
    startedAt: sentAt,
  };
  stopped.delete(conversationId);
  setStreamingText((s) => ({ ...s, [conversationId]: "" }));
  setActive((a) => ({ ...a, [conversationId]: turn }));
  // Web: live token/phase SSE for this conversation's turn (desktop gets the
  // same events over Tauri listeners registered by the page).
  const closeStream = openChatStream(conversationId, {
    onToken: (t) =>
      setStreamingText((s) => ({ ...s, [conversationId]: (s[conversationId] ?? "") + t })),
    onPhase: (p) => setTurnPhase((s) => ({ ...s, [conversationId]: p as AssistantPhase })),
  });
  try {
    const resp = await sendMessage(
      conversationId,
      query,
      opts.sourceIds,
      opts.deep,
      opts.widen,
      opts.web,
      opts.includeCorpus,
    );
    complete(
      turn,
      stopped.has(conversationId) ? { kind: "stopped" } : { kind: "answer", response: resp },
    );
  } catch (err) {
    // Officer hit Stop → the server cancelled the turn; don't recover or alarm.
    if (stopped.has(conversationId)) {
      complete(turn, { kind: "stopped" });
    } else {
      // Web: the tunnel kills HTTP requests at ~100 s, but the server finishes
      // the turn in a detached task and persists it — poll the answer back
      // instead of surfacing a dead-connection error (long map-reduce turns).
      const recovered = !isTauri()
        ? await recoverTurn(conversationId, sentAt, opts.deep, opts.nCtx ?? 0)
        : null;
      complete(
        turn,
        recovered
          ? { kind: "answer", response: recovered }
          : stopped.has(conversationId)
            ? { kind: "stopped" }
            : { kind: "error", message: String(err) },
      );
    }
  } finally {
    closeStream();
    stopped.delete(conversationId);
    const drop = <T,>(m: Record<string, T>) => {
      const next = { ...m };
      delete next[conversationId];
      return next;
    };
    setActive(drop);
    setTurnPhase(drop);
    setStreamingText(drop);
  }
}

/**
 * F026 stop button — halt the in-flight turn. Marks it user-stopped (so the page
 * renders a clean "Stopped." rather than an empty answer or a connection error)
 * and signals the server to cancel at the next cooperative boundary. Idempotent
 * and best-effort: a failed signal (e.g. the turn just finished) is harmless.
 */
export async function stopTurn(conversationId: string): Promise<void> {
  if (!busyIn(conversationId)) return;
  stopped.add(conversationId);
  setTurnPhase((s) => ({ ...s, [conversationId]: null }));
  setStreamingText((s) => ({ ...s, [conversationId]: "Stopping…" }));
  try {
    await cancelTurn(conversationId);
  } catch {
    /* already finished / offline — the empty result is handled on resolve */
  }
}

/**
 * Poll `poll_turn` until the detached turn lands in `query_log` (or give up).
 * Map-reduce turns legitimately run for minutes, so the window is generous; a
 * turn that FAILED server-side never writes a row, so a real error still
 * surfaces once the window closes.
 *
 * ⚠️ This no longer abandons when the officer switches chats. It used to, back
 * when the recovered answer could only be rendered into the open transcript —
 * now it is queued against its own conversation, so leaving the chat (or the
 * page) is not a reason to throw the answer away.
 */
async function recoverTurn(
  conversationId: string,
  sinceMs: number,
  wasDeep: boolean,
  nCtx: number,
): Promise<TurnResponse | null> {
  const POLL_EVERY_MS = 6000;
  const GIVE_UP_AFTER_MS = 12 * 60 * 1000;
  const deadline = Date.now() + GIVE_UP_AFTER_MS;
  // Deep search legitimately runs for a couple of minutes; a genuinely dropped
  // connection there is expected, so reassure rather than alarm. A normal query
  // that drops is more likely a real hiccup, so keep the plainer wording.
  setStreamingText((s) => ({
    ...s,
    [conversationId]: wasDeep
      ? "Deep search is still working through your documents on the server — this can take a couple of minutes. Your answer will appear here as soon as it's ready…"
      : "Still preparing your answer on the server — hang tight, it'll appear here shortly…",
  }));
  while (Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, POLL_EVERY_MS));
    if (stopped.has(conversationId)) return null;
    try {
      const p = await pollTurn(conversationId, sinceMs);
      if (p.done && p.answer !== undefined) {
        return {
          conversation_id: conversationId,
          message_id: p.message_id ?? "",
          capability: "guidance",
          answer: p.answer,
          citations: p.citations ?? [],
          offer: null,
          flags: [],
          context_fill: { utilization: 0, notice: false },
          audit: {
            // No audit rides along the recovery row — echo the model's
            // window so the top-bar readout stays sane.
            n_ctx: nCtx,
            gen_reserve: 0,
            prompt_tokens: 0,
            kept_law: 0,
            law_overflow: false,
            kept_turns: 0,
            summary_included: false,
            must_fit_failed: false,
          },
        };
      }
    } catch {
      /* transient poll failure (tunnel hiccup) — keep waiting */
    }
  }
  return null;
}
