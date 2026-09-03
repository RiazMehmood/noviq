/**
 * Feature 015 — the single bureaucrat chat surface, styled as the "Research
 * Station" workspace (per `UI Design.jpeg`).
 *
 * Three columns:
 *  - Left rail: logo, New chat, the chat-session history, Help.
 *  - Center: the conversation (user bubble + grounded markdown answer with
 *    CITED / CONFIDENCE tags), and the composer.
 *  - Right rail: the indexed-corpus document tree (scoping checkboxes).
 *
 * Every capability (guidance, drafting, summarize, compare, conflict, amendment
 * chain, action items) is intent-routed behind one conversation via
 * `send_message`; the page streams phase progress + answer tokens and renders the
 * assembled answer with citations, offer-to-tailor, ungrounded flags, and the
 * context-fill notice.
 */
import { Component, For, JSX, Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import { useNavigate, useParams } from "@solidjs/router";
import { marked } from "marked";
import { sanitizeAnswerHtml } from "../lib/sanitizeHtml";
import { isAdmin, deepSearchEnabled } from "../stores/auth";
import { getUserPrefs, setUserPrefs } from "../api/index";
import type { UnlistenFn } from "@tauri-apps/api/event";
import logoUrl from "../assets/noviq_logo.png";
import CitationFeedback from "../components/CitationFeedback";
import ConfidentialComposer from "../components/ConfidentialComposer";
import DocumentTree from "../components/DocumentTree";
import ProjectDocuments from "../components/ProjectDocuments";
import AddProjectDocuments from "../components/AddProjectDocuments";
import {
  activeProject,
  chats as projectChats,
  documents as projectDocuments,
  deleteProject,
  deleteProjectChat,
  openProject,
  removeDocument,
  renameProject,
  shareProject,
  startProjectChat,
  type ProjectDocument,
} from "../stores/projects";
import { useIsCompact } from "../lib/breakpoints";
import { CONVERSATIONS_KEY, maskKey, turnsKey } from "../lib/chatCache";
import ProgressRing from "../components/ProgressRing";
import Markdown from "../components/Markdown";
import AnswerBody from "../components/AnswerBody";
import ChunkViewer from "../components/ChunkViewer";
import DeepSearchNetwork from "../components/DeepSearchNetwork";
import { refreshTrialStatus } from "../components/TrialBanner";
import CorpusToggle from "../components/CorpusToggle";
import { call } from "../api/transport";
import {
  MaskSession,
  maskedItemsFromRanges,
  highlightSegments,
  highlightMarkdown,
  type MaskedItem,
  type Span,
} from "../lib/maskEngine";
import {
  attachDocuments,
  isTauri,
  onIndexProgress,
  type ConversationAttachment,
} from "../api/transport";
/**
 * The empty-state example prompts. A titled card, not a bare sentence: the title
 * says what KIND of question this is and the sentence is the question that gets
 * dropped into the composer, so the three cards double as an answer to "what is
 * this for?" on an officer's first screen.
 */
interface ExamplePrompt {
  title: string;
  /** SVG `d` from `EXAMPLE_ICON`. */
  icon: string;
  /** The text placed in the composer on click — verbatim. */
  query: string;
}

/** Card glyphs, one 24px grid at stroke 1.7 so they read as a set. */
const EXAMPLE_ICON = {
  // Head (a closed rhombus at 45°), handle, then the block it strikes — the
  // one-stroke gavels read as a pen at 22px.
  gavel: "M15.5 2.5 21.5 8.5 18.5 11.5 12.5 5.5Z M14.5 9.5 7 17 M3.5 20h10",
  scales: "M12 3v18M7 21h10M5 7h14M5 7l-3 6a3 3 0 0 0 6 0zM19 7l3 6a3 3 0 0 1-6 0zM8 5l4-1 4 1",
  institution: "M3 10h18M4 10 12 4l8 6M6 10v7M10 10v7M14 10v7M18 10v7M3 20h18",
};

// ⚠️ The RUNNING turn lives outside this component (2026-08-11). Leaving `/`
// unmounts this page, and page-local turn state meant the answer was thrown away
// mid-generation. See `stores/chat.ts` for the full failure chain.
import {
  appendToken,
  busyIn,
  clearStreamingIfIdle,
  completedSignal,
  deepIn,
  phaseIn,
  resumeConversationId,
  runTurn,
  setPhase,
  stopTurn,
  streamingIn,
  takeCompletedTurns,
} from "../stores/chat";
import {
  conversationTurns,
  deleteConversationAttachments,
  deleteMyDocument,
  getQueryTrace,
  hideConversation,
  indexProgress,
  listConversationAttachments,
  listConversations,
  promoteAttachment,
  type HistoryTurn,
} from "../api/index";
import { UPLOAD_ACCEPT } from "../lib/uploadFormats";
import {
  assistExit,
  assistStatus,
  exportDraft,
  modelStatus,
  onAssistantProgress,
  onAssistantToken,
  type Citation,
  type ModelStatus,
  type TurnResponse,
  type WebResult,
} from "../api/assistant";

/**
 * Longest question the composer accepts, in characters.
 *
 * ⚠️ Mirrors the server's `NOVIQ_MAX_QUERY_CHARS` (default 2,000) — the SERVER
 * is the gate; this is the courtesy half, so a pasted chapter is stopped and
 * counted here rather than failing after a minute of GPU time. The reason for
 * the cap is not politeness: the prompt is carved out of a fixed context window,
 * so an enormous question evicts the retrieved law and the answer comes back as
 * "I don't have enough information". 2,000 chars ≈ 500 tokens ≈ 7% of the
 * prompt budget on the deployed 8,192-token window.
 *
 * If the server's limit is changed, change it here too — a client cap that is
 * LOWER than the server's is safe (it just refuses early); a higher one hands
 * the officer a rejection they could not have predicted.
 */
const MAX_QUERY_CHARS = 2000;

/**
 * One button on a collapsed rail: a 20px icon with a hover tooltip carrying the
 * label the rail can no longer show.
 *
 * The first attempt at collapsing left a bare 40px strip with a single chevron,
 * and the architect's objection was exactly the documented failure of this
 * pattern — a collapsed rail with no icons reads as "the panel is gone", not as
 * "the panel is closed". The established enterprise shape is an ICON RAIL:
 * 56–72px wide, 20–24px icons, every icon tooltipped, and a persistent chevron
 * for the toggle. Icons keep the rail's PURPOSE on screen; the tooltip restores
 * the label on demand. See the sidebar-UX sources in the 2026-08-14 notes.
 *
 * The tooltip is CSS-only (`group-hover`) rather than `title=`: the native one
 * takes about a second to appear, which is long enough for someone probing an
 * unfamiliar icon to give up first.
 */
const RailButton: Component<{
  label: string;
  /** Which way the tooltip flies out — away from the rail, never off-screen. */
  side: "left" | "right";
  onClick: () => void;
  accent?: boolean;
  children: JSX.Element;
}> = (props) => (
  <div class="group relative flex justify-center">
    <button
      onClick={props.onClick}
      aria-label={props.label}
      class={`flex h-10 w-10 items-center justify-center rounded-lg transition-colors duration-150 ${
        props.accent
          ? "bg-noviq-700 text-white hover:bg-noviq-800"
          : "text-noviq-500 hover:bg-noviq-line/50 hover:text-noviq-ink"
      }`}
    >
      {props.children}
    </button>
    <span
      role="tooltip"
      class={`pointer-events-none absolute top-1/2 z-50 -translate-y-1/2 whitespace-nowrap rounded-md
              bg-noviq-ink px-2 py-1 text-xs font-medium text-white opacity-0 shadow-lg
              transition-opacity duration-150 group-hover:opacity-100 ${
                props.side === "right" ? "left-full ml-2" : "right-full mr-2"
              }`}
    >
      {props.label}
    </span>
  </div>
);

/** Generate a fresh conversation id (each launch / "New chat" starts one). */
function newConversationId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `conv-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

interface Turn {
  role: "user" | "assistant";
  text: string;
  response?: TurnResponse;
  /** Confidential values masked in this exchange (for the confirmation UI). */
  masked?: MaskedItem[];
  /** scope-routing 2026-07: on an assistant turn, the exact (masked) question that
   *  produced this answer — so the "Search all documents" widen button can re-run
   *  it over the full corpus without re-masking. */
  query?: string;
}

/** Lightweight metadata for the chat-history list in the left rail. */
interface ConvMeta {
  id: string;
  title: string;
  updatedAt: number;
}

function loadConversations(): ConvMeta[] {
  try {
    const raw = localStorage.getItem(CONVERSATIONS_KEY);
    return raw ? (JSON.parse(raw) as ConvMeta[]) : [];
  } catch {
    return [];
  }
}

function saveConversations(list: ConvMeta[]) {
  try {
    localStorage.setItem(CONVERSATIONS_KEY, JSON.stringify(list));
  } catch {
    /* quota or serialization issue — non-fatal */
  }
}

function loadTurns(conversationId: string): Turn[] {
  try {
    const raw = localStorage.getItem(turnsKey(conversationId));
    return raw ? (JSON.parse(raw) as Turn[]) : [];
  } catch {
    return [];
  }
}

/**
 * Strip the heaviest field (per-citation `verbatim` — full quoted legal passages)
 * before persisting. Long legal transcripts otherwise bloat the ~5 MB localStorage
 * quota, the write silently fails, and reopening the chat shows nothing. The
 * trimmed citations still render fully (n / instrument / path / page); verbatim is
 * only used for the right-rail theme blurb + clipboard synthesis of the *current*
 * live session, so losing it on a reopened chat is harmless.
 */
function trimForStorage(turns: Turn[]): Turn[] {
  return turns.map((t) =>
    t.response
      ? {
          ...t,
          response: {
            ...t.response,
            citations: t.response.citations.map((c) => ({ ...c, verbatim: "" })),
          },
        }
      : t,
  );
}

function saveTurns(conversationId: string, turns: Turn[]) {
  try {
    localStorage.setItem(turnsKey(conversationId), JSON.stringify(trimForStorage(turns)));
  } catch {
    /* quota or serialization issue — non-fatal */
  }
}

// ── Account-bound history (migration 020) ────────────────────────────────────
//
// Chat history is now read back from the server, scoped to the signed-in user,
// so it follows the account onto any device instead of dying with the browser
// profile. The localStorage copy above is kept as a CACHE — it paints the rail
// without waiting for a round trip, and it is the only place the officer's
// UNMASKED questions exist (the server sees the masked form only, by design).
//
// Merge rule, therefore: the SERVER decides which conversations exist, the LOCAL
// cache is preferred for the contents of the ones it has. That gives the
// originating device full fidelity, a second device the masked-but-complete
// transcript, and — since the rail is the server's list — no way for one
// officer's cached chats to show up in another's rail on a shared browser.

/** True when history is account-bound. The desktop build has no session to scope
 *  by, so it keeps the purely local history it has always had. */
const serverHistory = () => !isTauri();

/**
 * Rebuild renderable turns from the server's per-turn records.
 *
 * Each server row is one exchange, so it expands into the two turns the
 * transcript renders. `response_json` (migration 020) restores the full
 * `TurnResponse` — citations, web results, the widen flag, the trace handle.
 * Rows written before that migration only have the answer and citations, so a
 * minimal response is synthesised from them: enough for citations to render and
 * be clicked, with `audit` deliberately left off so the top-bar context readout
 * falls back to the model's real window instead of reporting a zero.
 */
function historyToTurns(rows: HistoryTurn[]): Turn[] {
  const out: Turn[] = [];
  for (const row of rows) {
    out.push({ role: "user", text: row.question });

    let response: TurnResponse | undefined;
    if (row.response_json) {
      try {
        response = JSON.parse(row.response_json) as TurnResponse;
      } catch {
        /* corrupt row — fall through to the pre-020 reconstruction below */
      }
    }
    if (!response && row.citations_json) {
      try {
        response = {
          conversation_id: "",
          message_id: row.id,
          capability: "guidance",
          answer: row.answer ?? "",
          citations: JSON.parse(row.citations_json),
          offer: null,
          flags: [],
          context_fill: { utilization: 0, notice: false },
        } as unknown as TurnResponse;
      } catch {
        /* no citations recoverable — the answer text still renders */
      }
    }

    out.push({
      role: "assistant",
      text: row.answer ?? "",
      response,
      // The masked question, which is what a "Search all documents" re-run needs
      // — it goes back to the server, which only ever knew the masked form.
      query: row.question,
    });
  }
  return out;
}

/**
 * Did the model actually cite its sources, or are the chips a fallback?
 *
 * When the answer carries no `[N]` markers the server does NOT drop the
 * citations — it attaches the top-3 retrieved chunks BY RANK so the passages
 * are not lost (`query_router.rs`, "Citation verification"). Its own trace calls
 * that `Model cited: NO — top-3 by rank, supports no specific claim`, and it
 * builds a quality notice saying the answer "could not be verified against a
 * specific source".
 *
 * ⚠️ That notice never reaches us: `services/assistant::TurnResponse` has no
 * `quality_notice` field, so it is dropped between the router and the client.
 * Until it is plumbed through we re-derive the same fact the same way the server
 * does — by looking for indexed `[N]` markers in the prose. This can only
 * UNDER-report verification (the server also accepts a matched legacy reference),
 * which is the safe direction: it errs towards "check this", never towards a
 * confidence the answer has not earned.
 */
const modelCited = (answer: string): boolean => /\[\d+\]/.test(answer);

/** A compact labelled chip for the top-bar model/budget readout. */
const Stat: Component<{ label: string; value: string; title?: string; warn?: boolean }> = (props) => (
  <span
    title={props.title}
    class={`font-mono text-[11px] rounded px-2 py-1 whitespace-nowrap border ${
      props.warn
        ? "text-amber-700 bg-amber-50 border-amber-200"
        : "text-noviq-600 bg-noviq-panel border-noviq-line"
    }`}
  >
    <span class="text-noviq-400">{props.label}</span> {props.value}
  </span>
);

const ChatPage: Component = () => {
  const navigate = useNavigate();
  // F027 — `/chat/:conversationId`, or `/projects/:projectId/chat/:conversationId`.
  // Both empty on the plain `/` route.
  const routeParams = useParams<{ conversationId?: string; projectId?: string }>();
  // ⚠️ PROJECT MODE — the WORKSPACE (rails, header, "New chat" target).
  //
  // Distinct from the existing `inProject()` signal below, and deliberately so:
  // that one is answered by the SERVER ("does this conversation belong to a
  // project?") and decides whether the per-turn corpus tick is shown. This one
  // comes from the ROUTE and answers "which project's chats and documents should
  // the rails show?" — a question only the URL can answer. Keep both.
  //
  // Everything gated here is ADDITIVE; the global chat path must behave exactly
  // as before. Presentation only — the server scopes retrieval from
  // `conversation_id` via `project_chat` (FR-038).
  const projectId = () => routeParams.projectId ?? null;
  const inProjectRoute = () => projectId() !== null;
  const [projectSelected, setProjectSelected] = createSignal<string[] | null>(null);
  // 2026-08-29 — the project overview page is gone, so everything it owned is
  // reachable from here: the add-documents dialog, and Rename/Share/Delete for
  // the project itself. `projectNotice` carries the outcome of one of those
  // actions; it is the only place the officer is told what actually happened.
  const [addingDocs, setAddingDocs] = createSignal(false);
  const [projectMenuOpen, setProjectMenuOpen] = createSignal(false);
  const [projectNotice, setProjectNotice] = createSignal<string | null>(null);

  // ⚠️ Navigating between two chats of the SAME route reuses this component —
  // `onMount` does NOT run again. Reading the conversation id only in `onMount`
  // meant "New chat" (and clicking any other chat in the project rail) changed
  // the URL while the transcript stayed on the first conversation: a new chat
  // was created server-side and never shown. React to the param instead.
  createEffect(
    on(
      () => routeParams.conversationId,
      (cid) => {
        if (!cid || cid === conversationId()) return;
        selectConversation(cid);
      },
      { defer: true }, // onMount already opened the first one
    ),
  );
  const [conversationId, setConversationId] = createSignal("");
  const [conversations, setConversations] = createSignal<ConvMeta[]>([]);
  const [turns, setTurns] = createSignal<Turn[]>([]);
  const [input, setInput] = createSignal("");
  // The running turn lives in `stores/chat` and outlives this component; it is
  // keyed by conversation, so everything below asks about THIS conversation
  // only. ⚠️ There is deliberately no page-wide "busy": a turn generating in one
  // chat must never disable the composer in another (or in a new one) — that is
  // exactly what made "New chat" unusable while an answer was in flight.
  const generatingHere = () => busyIn(conversationId());
  const streaming = () => streamingIn(conversationId());
  const phase = () => phaseIn(conversationId());
  // Composer input, so "Edit" on a prior query can refocus it for correction.
  let composerInput: HTMLInputElement | undefined;

  // ── Responsive rails ──────────────────────────────────────────────────────
  // Below `lg` there is not room for a 256px chat rail AND a 288px document rail
  // AND a readable conversation, so both rails become slide-over drawers driven
  // by these signals. At `lg`+ they are static columns again and these are
  // ignored (the `lg:` classes win over the transform ones).
  // The rail SEMANTICS are unchanged from the 2026-07-02 swap — left is chat
  // sessions, right is the DocumentTree. Only their presentation is conditional.
  const isCompact = useIsCompact();
  const [leftRailOpen, setLeftRailOpen] = createSignal(false);
  const [rightRailOpen, setRightRailOpen] = createSignal(false);
  const closeRails = () => { setLeftRailOpen(false); setRightRailOpen(false); };

  // ── Desktop rail collapse ─────────────────────────────────────────────────
  // Separate from the drawer signals above, which only exist BELOW `lg`. At
  // `lg`+ the rails are static columns that were always on screen, leaving a
  // narrow reading column on a laptop. These collapse them to a thin strip that
  // keeps the way back visible — a rail that vanishes with no affordance is how
  // a user concludes the documents are gone.
  //
  // Persisted, because this is a workspace preference rather than per-visit
  // state: re-collapsing it on every load would be its own annoyance.
  const RAILS_KEY = "noviq.rails.collapsed";
  const [leftCollapsed, setLeftCollapsed] = createSignal(false);
  const [rightCollapsed, setRightCollapsed] = createSignal(false);
  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(RAILS_KEY) ?? "{}");
      setLeftCollapsed(!!saved.left);
      setRightCollapsed(!!saved.right);
    } catch {
      /* corrupt or absent — both rails stay open, which is the safe default */
    }
  });
  createEffect(() => {
    const v = JSON.stringify({ left: leftCollapsed(), right: rightCollapsed() });
    try {
      localStorage.setItem(RAILS_KEY, v);
    } catch {
      /* private mode / quota — collapsing still works for this session */
    }
  });

  // Opening one drawer closes the other — they overlap, and on a 390px screen
  // two stacked overlays leave no way back to the conversation.
  const toggleLeftRail = () => { setRightRailOpen(false); setLeftRailOpen((v) => !v); };
  const toggleRightRail = () => { setLeftRailOpen(false); setRightRailOpen((v) => !v); };

  // Returning to a wide viewport must not leave stale drawer state behind: the
  // rails are static there, and a lingering `true` would re-open a drawer the
  // moment the window narrowed again.
  createEffect(() => { if (!isCompact()) closeRails(); });

  // Confidential masking (F020): the label→value map for THIS conversation lives
  // only in the browser session (never sent to the server); `maskRanges` are the
  // active confidential spans the composer reports, applied on submit.
  const [maskRanges, setMaskRanges] = createSignal<Span[]>([]);
  let maskSession = new MaskSession();
  const loadMaskSession = (id: string) => {
    maskSession = MaskSession.fromJSON(id ? sessionStorage.getItem(maskKey(id)) : null);
  };
  const saveMaskSession = () => {
    const id = conversationId();
    if (id) sessionStorage.setItem(maskKey(id), maskSession.toJSON());
  };
  // Admin-ness and Deep-search availability both come from the canonical auth
  // store (validated at boot, before this page renders) — NOT a second per-page
  // `/me`, which could race the token restore and leave the toggle hidden for a
  // genuine admin. `isAdmin` gates the per-answer trace debug (Track 3);
  // `deepSearchEnabled` gates the Deep-search toggle (server enforces the same).
  /**
   * The citation whose source document is open in the viewer, opened from the
   * "Sources" list under an answer.
   *
   * The inline `[n]` markers have always been able to do this (`AnswerBody`
   * owns its own viewer), but the Sources list was plain `<li>` text — so when
   * the model emitted NO markers, which happens whenever the trace reports
   * `Model cited: NO`, the answer had no route to the document at all. The list
   * showed the instrument, section and page and left the officer to find it by
   * hand. `SharedChatPage` had already made its citations buttons; the main
   * chat had not.
   *
   * Only citations carrying BOTH `chunk_id` and `source_id` are openable — the
   * same condition `AnswerBody` uses, so an unopenable row stays plain text
   * rather than offering a button that goes nowhere.
   */
  const [openSource, setOpenSource] = createSignal<Citation | null>(null);
  const [debugBody, setDebugBody] = createSignal<string | null>(null);
  const [debugOpen, setDebugOpen] = createSignal(false);
  const openDebug = async (traceId: string) => {
    setDebugOpen(true);
    setDebugBody(null);
    try {
      setDebugBody(await getQueryTrace(traceId));
    } catch (e) {
      setDebugBody(`Failed to load trace: ${String(e)}`);
    }
  };

  // ── Document scope (right-rail checkboxes + @-mention chips) ───────────────
  // `docSelection`: the checkbox subset (`all` true = no restriction). `docCatalog`:
  // the corpus id+name list for @-mention typeahead. `mentionIds`: docs @-mentioned
  // in the composer. The effective scope sent with a turn is computed in `submit`.
  const [docSelection, setDocSelection] = createSignal<{ ids: string[]; all: boolean }>({
    ids: [],
    all: true,
  });
  const [docCatalog, setDocCatalog] = createSignal<{ id: string; name: string }[]>([]);
  /**
   * Corpus size for the "Repository" rail header, reported by `DocumentTree`
   * once it has loaded. Read from the tree rather than fetched again here: the
   * tree already excludes withdrawn documents, and a second count taken another
   * way would eventually disagree with the list sitting underneath it.
   * `null` until loaded, and in project mode (where the tree is not rendered).
   */
  const [corpusStats, setCorpusStats] = createSignal<{ files: number; groups: number } | null>(null);
  const RepositoryStats: Component = () => (
    <Show when={corpusStats()}>
      {(st) => (
        <p class="mt-0.5 text-[11px] text-noviq-500">
          <span class="font-semibold tabular-nums text-noviq-700">{st().files.toLocaleString()}</span> Files
          <span class="mx-1.5 text-noviq-300">·</span>
          <span class="font-semibold tabular-nums text-noviq-700">{st().groups.toLocaleString()}</span>
          {" "}{st().groups === 1 ? "Department" : "Departments"}
        </p>
      )}
    </Show>
  );
  /**
   * The project rail's own size line. `RepositoryStats` above is fed by
   * `DocumentTree`, which is NOT rendered in project mode — so the project rail
   * header carried the word "Documents" and no number at all.
   *
   * "2 indexing" appears only while something actually is: a document that is
   * not searchable yet must look pending (FR-008), and a permanent second number
   * reading "0 indexing" would be noise in the common case.
   */
  const ProjectDocStats: Component = () => {
    const total = () => projectDocuments().length;
    const indexing = () => projectDocuments().filter((d) => d.status === "pending").length;
    return (
      <p class="mt-0.5 text-[11px] text-noviq-500">
        <Show when={total() > 0} fallback={<span>None yet</span>}>
          <span class="font-semibold tabular-nums text-noviq-700">{total().toLocaleString()}</span>
          {" "}document{total() === 1 ? "" : "s"}
          <Show when={indexing() > 0}>
            <span class="mx-1.5 text-noviq-300">·</span>
            <span class="font-semibold tabular-nums text-noviq-700">{indexing()}</span> indexing
          </Show>
        </Show>
      </p>
    );
  };
  /** One header line for both modes, so the two cannot drift apart. */
  const RailStats: Component = () => (
    <Show when={inProjectRoute()} fallback={<RepositoryStats />}>
      <ProjectDocStats />
    </Show>
  );
  const [mentionIds, setMentionIds] = createSignal<string[]>([]);
  // Deep-search toggle (chat-persistent): one much richer, slower pass per turn.
  const [deepSearch, setDeepSearch] = createSignal(false);
  // "+" composer menu (attach file · deep search · web search).
  const [plusMenuOpen, setPlusMenuOpen] = createSignal(false);
  // Web search for the NEXT turn. Off by default and never sticky across a
  // reload: it sends the question to a public search engine, so it should be
  // a deliberate act each time rather than a setting someone forgets is on.
  // ⚠️ 2026-08-11 — that was the stated intent from the start, but nothing ever
  // cleared it, so one click left web search ON for every later question in the
  // session. Not cosmetic: with nothing attached, a web turn is EXCLUSIVE — the
  // legal corpus is not consulted at all — so a forgotten toggle silently turned
  // grounded legal answers into unverified web answers. `submit` clears it now,
  // exactly as it clears `corpusThisTurn`.
  const [webSearch, setWebSearch] = createSignal(false);
  // F027 — "also search the law" for the NEXT question only, inside a project.
  // Same reasoning as `webSearch` above and then some: leaving it on would put
  // enacted law into answers about the officer's own file without their asking,
  // which is precisely what a project exists to prevent. Cleared on every send
  // (FR-017) and stored nowhere — no column for it exists on the server.
  const [corpusThisTurn, setCorpusThisTurn] = createSignal(false);
  // Whether this conversation belongs to a project. Decides only whether the
  // corpus tick is SHOWN; the server derives the real project from the
  // conversation id and ignores anything the client claims (ADR-0029).
  const [inProject, setInProject] = createSignal(false);

  // F027 — does this conversation belong to a project? Asked whenever the
  // conversation changes. Purely to decide whether to SHOW the corpus tick; the
  // server derives the actual project from the conversation id and never trusts
  // the client's view of it, so being wrong here changes presentation only.
  createEffect(() => {
    const id = conversationId();
    if (!id) {
      setInProject(false);
      return;
    }
    void call<{ in_project: boolean }>("is_project_chat", { conversation_id: id })
      .then((r) => setInProject(r.in_project))
      .catch(() => setInProject(false)); // gate closed, or not a project chat
  });
  // ── "Next steps" follow-up suggestion (2026-07-29) ────────────────────────
  //
  // The persona ends a guidance answer with a rule, a `**Next steps**` line, and
  // ONE sentence naming what it could add next. Rendering that inline made it
  // read as more prose the officer had to retype; splitting it out turns it into
  // a chip that drops straight into the composer (same affordance as the
  // example-query chips below), and keeps the answer itself clean.
  //
  // Tolerant by design: the model may or may not emit the `---`, may bold or
  // head the label, and may wrap the sentence over several lines. Anything that
  // does not match is left in the answer untouched rather than silently dropped
  // — losing the tail of a legal answer is far worse than an unclicked chip.
  // The label arrives as PLAIN TEXT far more often than as markdown — the live
  // model writes "---\n\nNext steps  \n**Section 30(i) …**" (trace 892c769b).
  // An earlier version of this regex required `**Next steps**` or a heading and
  // so never matched, which left the section sitting in the answer and rendered
  // no chip. Accept all three forms, anchored to the END of the answer so the
  // phrase occurring mid-answer is never mistaken for the offer.
  const NEXT_STEPS_RE =
    /\n+(?:[-–—]{3,}[ \t]*\n+)?[ \t]*(?:#{1,6}[ \t]*)?(?:\*\*|__)?[ \t]*Next[ \t]+steps[ \t]*(?:\*\*|__)?[ \t]*:?[ \t]*\n+([\s\S]+?)\s*$/i;

  /** Registrable domain of a URL, for showing WHO says it without the clutter. */
  const hostOf = (url: string): string => {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return "";
    }
  };

  /**
   * Split a turn's web results into the ones the answer actually CITED and the
   * rest, keeping each result's original `[Wn]` number so the badge in the list
   * matches the marker in the prose.
   *
   * Presenting all results as equals buried the two or three the answer rests
   * on. Those are the ones a reader must be able to check; the rest are the
   * search's leftovers and belong behind a disclosure.
   */
  const webSplit = (turn: Turn): { cited: (WebResult & { n: number })[]; other: (WebResult & { n: number })[] } => {
    const all = (turn.response?.web_results ?? []).map((w, i) => ({ ...w, n: i + 1 }));
    const used = new Set<number>();
    for (const m of (turn.text ?? "").matchAll(/\[W(\d{1,3})\]/g)) used.add(Number(m[1]));
    return {
      cited: all.filter((w) => used.has(w.n)),
      other: all.filter((w) => !used.has(w.n)),
    };
  };

  /**
   * A question replayed from the server still carrying mask placeholders.
   *
   * Confidential values are replaced with `[NAME_1]`-style labels in the browser
   * before a question is sent, and the label→value map never leaves that browser
   * (F020). So a chat opened on a SECOND device shows the labels with nothing to
   * resolve them to. That is the design working, not a bug — but unexplained it
   * reads like corruption, so the turn says so plainly.
   *
   * `refill` is the test rather than the regex alone: on the originating device
   * the map is present, the labels resolve, and no note is shown.
   */
  const hasUnresolvedMask = (text: string): boolean =>
    /\[[A-Z][A-Z0-9_]*_\d+\]/.test(maskSession.refill(text));

  const splitNextSteps = (text: string): { body: string; suggestion: string | null } => {
    const m = text.match(NEXT_STEPS_RE);
    if (!m) return { body: text, suggestion: null };
    const suggestion = m[1]
      .replace(/\s+/g, " ")            // unwrap the sentence
      // Emphasis FIRST, then the bullet marker. The other order eats the first
      // `*` of a leading `**bold**` as if it were a bullet, leaving a stray
      // asterisk that the bold rule can then no longer match.
      .replace(/\*\*(.+?)\*\*/g, "$1")
      .replace(/__(.+?)__/g, "$1")
      .replace(/`(.+?)`/g, "$1")
      .replace(/^[-*•]\s*/, "")        // drop a bullet marker if the model used one
      .trim();
    // A suggestion that is empty, or long enough to be a paragraph of answer
    // rather than one offer, is not treated as a chip.
    if (!suggestion || suggestion.length > 320) return { body: text, suggestion: null };
    return { body: text.slice(0, m.index!).trimEnd(), suggestion };
  };

  // Per-user preference — mirrored here (not just in Settings) because the
  // settings gear is hidden for admins, who still ask questions and still need
  // a way to turn these off.
  const [showNextSteps, setShowNextSteps] = createSignal(true);
  onMount(async () => {
    try { setShowNextSteps((await getUserPrefs()).refinementOffer); }
    catch { /* preference unavailable (desktop build) — leave the default */ }
  });
  const setNextSteps = async (on: boolean) => {
    setShowNextSteps(on);              // optimistic — the control should feel instant
    try { await setUserPrefs({ refinementOffer: on }); }
    catch { setShowNextSteps(!on); }   // never claim to have saved what we did not
  };
  const hideNextSteps = () => setNextSteps(false);
  const toggleNextSteps = () => setNextSteps(!showNextSteps());

  // Above this many documents the scope banner collapses to a count: naming
  // every file is only useful while the scope is genuinely narrow.
  const MAX_SCOPE_CHIPS = 3;

  // Example prompts shown on the empty chat; clicking one fills the composer.
  //
  // Two sets, because the two modes search different things. Ordinary chat runs
  // against the Sindh corpus, so naming the E&D Rules teaches what this is FOR.
  // A PROJECT searches documents the user uploaded themselves — we have no idea
  // what is in them, so a corpus question there is worse than no example: it
  // asks about law the project does not contain and returns a refusal on the
  // officer's first click. Project prompts stay deliberately document-agnostic.
  //
  // Keyed off `inProjectRoute()` (the URL), not `inProject()` — the latter is
  // answered by the server and starts `false`, so it would render the corpus
  // list first and swap it under the reader.
  const CORPUS_EXAMPLE_QUERIES: ExamplePrompt[] = [
    {
      title: "Procedure",
      icon: EXAMPLE_ICON.gavel,
      query: "What is the procedure for disciplinary action against a government servant?",
    },
    {
      title: "Rules",
      icon: EXAMPLE_ICON.scales,
      query: "What penalties can be imposed under the E&D Rules?",
    },
    {
      title: "Legal Guidance",
      icon: EXAMPLE_ICON.institution,
      query: "Within what time can a dismissed civil servant file an appeal, and where?",
    },
  ];
  // ⚠️ Deliberately NOT summarise / compare / extract-dates-and-actions.
  // Those read as the obvious things to offer, and all three are the wrong
  // first click: SummarizeDoc and ActionItems sit behind `NOVIQ_MAPREDUCE`,
  // which is OFF, and cross-document conflict/compare answers "not yet" by
  // design. An example prompt that cannot be answered is worse than none —
  // it spends the user's first impression on a failure. These are plain
  // retrieval lookups, which is what the grounded path actually does.
  const PROJECT_EXAMPLE_QUERIES: ExamplePrompt[] = [
    { title: "Rules", icon: EXAMPLE_ICON.scales, query: "What rules apply here?" },
    { title: "Responsibility", icon: EXAMPLE_ICON.institution, query: "Who is responsible?" },
    { title: "Procedure", icon: EXAMPLE_ICON.gavel, query: "What is the procedure?" },
  ];
  const exampleQueries = () =>
    inProjectRoute() ? PROJECT_EXAMPLE_QUERIES : CORPUS_EXAMPLE_QUERIES;
  /**
   * How full THIS chat is, from the most recent answer. Measured server-side
   * (`query_router::ChatFill`): the share of the room for conversation history
   * that is gone — NOT how full the prompt is. The prompt is ~always ~100% full
   * because the retrieved law expands to fill it, so prompt-fullness would warn on
   * turn one, every turn.
   *
   * ~35% on turn one, ~77% at ten exchanges, 100% when the law budget hits its
   * floor. At 100% the prompt starts overflowing and `llm.rs` truncation drops the
   * law out of the middle — that is when answers become refusals, so that is where
   * we stop instead of letting the officer walk into it.
   */
  const latestFill = createMemo(() => {
    for (let i = turns().length - 1; i >= 0; i--) {
      const t = turns()[i];
      if (t.role === "assistant" && t.response) return t.response.context_fill;
    }
    return undefined;
  });
  /** Out of room: further questions would be answered from truncated law. */
  const chatIsFull = () => (latestFill()?.utilization ?? 0) >= 1;
  /** Filling up (server threshold, default 0.80) but still usable. */
  const chatFilling = () => !!latestFill()?.notice && !chatIsFull();

  // Notice Assist (F022): assist-mode banner. A MIRROR of the server's
  // `notice_assist_session` — set by a /help turn, cleared by Leave, and re-read
  // from the server (`refreshAssistMode`) whenever the conversation changes.
  const [assistMode, setAssistMode] = createSignal(false);
  // "Leave" round-trips to the server (the session is server state), so it can be
  // in flight, and it can fail — in which case the officer is STILL in assist mode
  // and has to be told, not silently left there with the banner hidden.
  const [leaving, setLeaving] = createSignal(false);
  const [leaveError, setLeaveError] = createSignal<string | null>(null);

  // The document scope for the NEXT turn. @-mention wins ("only that document");
  // else a narrowed checkbox subset; else undefined (every visible document).
  const effectiveSourceIds = (): string[] | undefined => {
    const mentions = mentionIds();
    if (mentions.length > 0) return mentions;
    const sel = docSelection();
    if (!sel.all && sel.ids.length > 0) return sel.ids;
    return undefined;
  };

  // ── F021 (US4): conversation attachments — web only ───────────────────────
  const [attachments, setAttachments] = createSignal<ConversationAttachment[]>([]);
  const [attachError, setAttachError] = createSignal<string | null>(null);
  const [attaching, setAttaching] = createSignal(false);
  // Attach progress, in two phases (F021 issue #1):
  //   1. `uploadPct` — real bytes-sent %, from XHR upload.onprogress.
  //   2. `indexPct`  — per-source indexing %, from the `/api/index/events` SSE
  //      stream. The Cloudflare quick tunnel buffers that stream to nothing, so
  //      a source with no tick renders an INDETERMINATE ring rather than a fake 0%.
  const [uploadPct, setUploadPct] = createSignal<number | null>(null);
  const [uploadNames, setUploadNames] = createSignal<string[]>([]);
  const [indexPct, setIndexPct] = createSignal<Record<string, number>>({});
  let attachInputRef: HTMLInputElement | undefined;

  // Documents offered in the @-mention typeahead: the corpus catalog plus this
  // conversation's live attachments (so "@<attachment>" scopes to it directly).
  const mentionDocs = createMemo(() => {
    const seen = new Set<string>();
    const out: { id: string; name: string }[] = [];
    for (const d of [
      ...docCatalog(),
      ...attachments()
        .filter((a) => a.status === "indexed")
        .map((a) => ({ id: a.sourceId, name: a.fileName })),
    ]) {
      if (seen.has(d.id)) continue;
      seen.add(d.id);
      out.push(d);
    }
    return out;
  });

  const loadAttachments = async (id: string) => {
    if (isTauri() || !id) return;
    try {
      setAttachments(await listConversationAttachments(id));
    } catch {
      /* endpoint unavailable (older server) — attachments UI stays empty */
    }
  };

  const handleAttachSelected = async (e: Event) => {
    const inputEl = e.currentTarget as HTMLInputElement;
    const files = Array.from(inputEl.files ?? []);
    if (files.length === 0) return;
    setAttaching(true);
    setAttachError(null);
    setUploadPct(0);
    setUploadNames(files.map((f) => f.name));
    try {
      await attachDocuments(conversationId(), files, setUploadPct);
      // Bytes are on the server; it now parses/OCRs/embeds before responding —
      // switch the ring to indeterminate for that phase.
      setUploadPct(null);
      await loadAttachments(conversationId());
    } catch (err) {
      setAttachError(String(err));
    } finally {
      setAttaching(false);
      setUploadPct(null);
      setUploadNames([]);
      inputEl.value = "";
    }
  };

  const handlePromote = async (a: ConversationAttachment) => {
    try {
      await promoteAttachment(a.sourceId);
      await loadAttachments(conversationId());
    } catch (err) {
      setAttachError(String(err));
    }
  };

  const handleRemoveAttachment = async (a: ConversationAttachment) => {
    try {
      await deleteMyDocument(a.sourceId);
      setAttachments((prev) => prev.filter((x) => x.sourceId !== a.sourceId));
    } catch (err) {
      setAttachError(String(err));
    }
  };

  // If Deep search isn't offered to this user (server-gated), never leave the
  // toggle stuck ON — e.g. a prior admin session's persisted pill. Reactive to the
  // store, so it also settles once the session resolves.
  createEffect(() => {
    if (!deepSearchEnabled()) setDeepSearch(false);
  });

  // F021 SSE-tunnel fallback: some HTTP proxies (e.g. trycloudflare quick
  // tunnels) buffer the `/api/index/events` SSE stream indefinitely, so the
  // completion event wired in onMount may never arrive. While any chip is
  // still pending/processing, poll the attachment list so statuses converge
  // even without a working event stream.
  const attachPollNeeded = createMemo(
    () =>
      !isTauri() &&
      attachments().some((a) => a.status === "pending" || a.status === "processing")
  );
  createEffect(() => {
    if (!attachPollNeeded()) return;
    const id = setInterval(() => void loadAttachments(conversationId()), 4000);
    onCleanup(() => clearInterval(id));
  });

  // Same reason, for the *percentage*: poll `index_progress` (a plain POST, so the
  // tunnel can't buffer it) while any chip is indexing, and feed the ring. Faster
  // than the status poll so the ring moves smoothly.
  createEffect(() => {
    if (!attachPollNeeded()) return;
    const ids = attachments()
      .filter((a) => a.status === "pending" || a.status === "processing")
      .map((a) => a.sourceId);
    if (ids.length === 0) return;
    const tick = async () => {
      try {
        const rows = await indexProgress(ids);
        if (rows.length === 0) return; // no tick yet → ring stays indeterminate
        setIndexPct((prev) => {
          const next = { ...prev };
          for (const r of rows) next[r.source_id] = r.percent;
          return next;
        });
      } catch {
        /* endpoint unavailable (older server) — ring stays indeterminate */
      }
    };
    void tick();
    const id = setInterval(tick, 1500);
    onCleanup(() => clearInterval(id));
  });

  // Friendly "working…" status shown until the first live phase/token event
  // arrives over the stream (SSE on web, Tauri events on desktop). Cycles
  // through reassuring messages so the retrieval seconds never look stuck.
  const STATUS_STEPS = [
    "Reading your documents…",
    "Thinking…",
    "Thinking some more…",
    "Connecting the rules…",
    "Drafting the answer…",
    "Polishing the wording…",
    "Almost done…",
  ];
  const [statusIdx, setStatusIdx] = createSignal(0);
  createEffect(() => {
    if (!generatingHere()) {
      setStatusIdx(0);
      return;
    }
    setStatusIdx(0);
    const id = setInterval(
      () => setStatusIdx((i) => Math.min(i + 1, STATUS_STEPS.length - 1)),
      5000,
    );
    onCleanup(() => clearInterval(id));
  });
  const [model, setModel] = createSignal<ModelStatus | null>(null);

  let unlistenProgress: UnlistenFn | undefined;
  let unlistenToken: UnlistenFn | undefined;
  let transcriptEl: HTMLDivElement | undefined;

  /**
   * Replace the history rail with this account's conversations from the server.
   *
   * The server's list is authoritative — it is the whole point of the feature —
   * but its titles come from the MASKED question, so where this browser still
   * has the original cached, that title is preferred. A failure leaves the
   * cached rail alone rather than blanking it: an offline or restarting server
   * should not look like lost history.
   */
  async function syncConversations() {
    if (!serverHistory()) return;
    try {
      const rows = await listConversations();
      const fromServer = rows.map((r) => ({
        id: r.conversation_id,
        title: localTitle(r.conversation_id) ?? r.title,
        updatedAt: r.updated_at,
      }));
      // ⚠️ MERGE, never replace (2026-08-11). The server lists a conversation
      // only once a turn has LANDED in `query_log`, so a chat whose question is
      // still generating — or whose turn failed — does not appear in `rows`.
      // Replacing the rail wholesale therefore ERASED the chat the officer was
      // waiting on: they switched to another chat, came back, and there was no
      // entry left to click. The answer was never lost (it sits in the store's
      // queue), but nothing could reach it.
      //
      // The server stays authoritative for everything it knows about; local-only
      // entries are kept and sorted back in by recency.
      const known = new Set(fromServer.map((c) => c.id));
      const localOnly = conversations().filter((c) => !known.has(c.id));
      setConversations(
        [...fromServer, ...localOnly].sort((a, b) => b.updatedAt - a.updatedAt),
      );
    } catch {
      /* offline / server restarting — keep the cached rail */
    }
  }

  /** This browser's cached (unmasked) title for a conversation, if it has one. */
  function localTitle(id: string): string | null {
    const cached = loadTurns(id);
    const first = cached.find((t) => t.role === "user")?.text?.trim();
    return first ? first.slice(0, 60) : null;
  }

  onMount(async () => {
    // Load past conversations for the history list, then open a fresh chat
    // (ChatGPT/Claude-style: every launch starts on a blank "New chat").
    // The cached list paints immediately; `syncConversations` then replaces it
    // with the account's real history from the server (migration 020), which is
    // what makes the rail identical on every device this officer signs in from.
    // ⚠️ In project mode the GLOBAL conversation cache must never populate the
    // rail — showing every chat the officer owns inside a project is exactly the
    // confusion this mode exists to remove.
    if (inProjectRoute()) {
      void openProject(projectId()!);
    } else {
      setConversations(loadConversations());
      void syncConversations();
    }
    // F027 fix — `/chat/:conversationId` opens THAT chat (the route a project's
    // "New chat" navigates to). Without a param this stays the ChatGPT-style
    // "every launch is a blank New chat" behaviour, unchanged...
    const routeId = routeParams.conversationId;
    // ...with ONE exception (2026-08-11): a turn that is still generating. The
    // officer left this page mid-answer (My Projects, Settings) and has come
    // back for it; dropping them on a blank New chat hides the very thing they
    // are waiting for, and the question they asked. Reopen that conversation so
    // the tokens keep landing where they can see them.
    const openId = routeId || resumeConversationId();
    const id = openId || newConversationId();
    setConversationId(id);
    loadMaskSession(id);
    setTurns([]);
    if (openId) {
      // Same hydration `selectConversation` does: cached turns first, and pull
      // from the server when this chat was written on another device.
      const cached = loadTurns(openId);
      railSeen.set(openId, cached.length);
      setTurns(cached);
      if (cached.length === 0 && serverHistory()) void hydrateFromServer(openId);
      void loadAttachments(openId);
    }
    unlistenProgress = await onAssistantProgress((p) => setPhase(p.phase));
    unlistenToken = await onAssistantToken((t) => appendToken(t));
    modelStatus().then(setModel).catch(() => {/* model not loaded yet */});

    // F021: keep attachment chips honest while the server indexes them (the
    // SSE progress stream fires for every indexing job; on completion of any,
    // refresh this conversation's attachment statuses).
    if (!isTauri()) {
      const unIdx = onIndexProgress((p) => {
        // Record the tick so the chip's ring can show real completeness. (Through
        // the quick tunnel this stream is buffered away and never fires — the ring
        // then stays indeterminate, which is the honest rendering.)
        setIndexPct((prev) => ({ ...prev, [p.source_id]: p.percent }));
        if (p.percent >= 100 && attachments().some((a) => a.status !== "indexed")) {
          loadAttachments(conversationId());
        }
      });
      onCleanup(() => unIdx());
    }
  });

  // Turn count already reflected in the rail, per conversation. OPENING a chat
  // hydrates `turns`, which is indistinguishable from answering in it unless we
  // remember how many turns each conversation had when it was opened. Without
  // this, merely reading an old chat pushed it to the top: the rail is ordered by
  // activity, and reading is not activity.
  const railSeen = new Map<string, number>();

  // Persist the transcript whenever it changes and keep the history list current.
  // `on(turns)` so the effect fires ONLY on a turns change — merely *switching*
  // conversations can never re-save with stale turns (which would wipe the chat we
  // just opened). A conversation enters the history list only once it has at least
  // one turn, so blank "New chat" sessions never clutter the rail.
  createEffect(on(turns, (t) => {
    const id = conversationId();
    if (!id) return;
    saveTurns(id, t);
    if (t.length === 0) return;
    const title = (t.find((x) => x.role === "user")?.text ?? "New chat").trim().slice(0, 60);
    const seen = railSeen.get(id) ?? 0;
    railSeen.set(id, t.length);
    // Only a genuinely NEW turn counts as activity and re-sorts the rail. A
    // conversation the rail has never heard of is still inserted, though —
    // otherwise a brand-new chat's first turn would go unlisted.
    const known = conversations().some((c) => c.id === id);
    if (t.length <= seen && known) return;
    setConversations((list) => [
      { id, title, updatedAt: Date.now() },
      ...list.filter((c) => c.id !== id),
    ]);
  }));

  // Persist the history list whenever it changes.
  createEffect(() => saveConversations(conversations()));

  // ChatGPT-style scroll: when a new turn is added, pin the LATEST USER question
  // near the TOP of the transcript and let the answer stream below it — the view
  // stays at the question so the officer reads top-to-bottom, instead of jumping
  // to the bottom of a long answer. We do NOT auto-follow streaming tokens.
  let prevTurnCount = 0;
  createEffect(() => {
    const len = turns().length;
    const el = transcriptEl;
    if (el && len > prevTurnCount) {
      requestAnimationFrame(() => {
        const users = el.querySelectorAll<HTMLElement>('[data-role="user"]');
        const node = users[users.length - 1];
        if (node) {
          el.scrollTop += node.getBoundingClientRect().top - el.getBoundingClientRect().top - 16;
        }
      });
    }
    prevTurnCount = len;
  });

  onCleanup(() => {
    unlistenProgress?.();
    unlistenToken?.();
  });

  const responses = createMemo(() =>
    turns().map((t) => t.response).filter((r): r is TurnResponse => !!r),
  );
  const latestResponse = createMemo(() => responses().at(-1) ?? null);

  // ── Model + budget readout (top bar) ────────────────────────────────────────
  const latestAudit = createMemo(() => latestResponse()?.audit ?? null);
  /** Active context window: the per-turn audited value if a turn ran, else the
   *  model's resolved window. */
  const windowSize = createMemo(() => latestAudit()?.n_ctx ?? model()?.n_ctx ?? null);
  const promptFits = createMemo(() => {
    const a = latestAudit();
    return a ? a.prompt_tokens + a.gen_reserve <= a.n_ctx && !a.must_fit_failed : true;
  });

  async function exportTurn(resp: TurnResponse) {
    try {
      const { text } = await exportDraft(resp.conversation_id, resp.message_id);
      await navigator.clipboard.writeText(text);
    } catch {
      /* clipboard or export unavailable — no-op */
    }
  }

  const [copiedId, setCopiedId] = createSignal("");
  /**
   * Build the clipboard text for an answer: the prose followed by a "Sources"
   * block listing each citation ([n] instrument — path (p.X)), so a pasted answer
   * carries its authorities and is self-contained. Answers with no citations copy
   * the prose alone.
   */
  function answerForCopy(turn: Turn): string {
    const cites = turn.response?.citations ?? [];
    if (cites.length === 0) return turn.text;
    const lines = cites.map((c) => {
      let s = `[${c.n}] ${c.instrument}`;
      if (c.path) s += ` — ${c.path}`;
      if (c.page != null) s += ` (p.${c.page})`;
      return s;
    });
    return `${turn.text}\n\nSources:\n${lines.join("\n")}`;
  }
  /**
   * Undefined-safe on purpose: a citation with a missing `instrument`/`path` would
   * otherwise throw here, and because the rich-copy path falls back to plain text on
   * any error, a single bad citation would silently downgrade the whole answer back
   * to raw markdown. The plain flavour tolerates it (template literals stringify),
   * so the HTML flavour must too.
   */
  function escapeHtml(s: unknown): string {
    return String(s ?? "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }
  /**
   * Rich (text/html) flavour of the same answer. Officers paste answers into Word,
   * where a text/plain clipboard lands as literal markdown ("## Grounds", "**Rule
   * 3**"). Word and Google Docs both prefer text/html when it is offered, so we put
   * BOTH flavours on the clipboard and let the target pick — Word renders real
   * headings/bold/lists, Notepad still gets the plain text.
   *
   * Pipeline mirrors <AnswerBody>: marked → sanitizeAnswerHtml. The same policy
   * applies here as on screen: an answer pasted into Word must not carry a
   * remote image that phones home when the document is opened.
   * Deliberately parses the RAW
   * answer (not AnswerBody's linkified copy) so the [n] markers paste as plain text
   * instead of dragging the hover-popover markup into the document.
   */
  function answerHtmlForCopy(turn: Turn): string {
    const cites = turn.response?.citations ?? [];
    // gfm/breaks passed explicitly rather than inherited from <AnswerBody>'s
    // module-level marked.setOptions — so the copy matches the render even if that
    // import ever goes away.
    let html = marked.parse(turn.text, { async: false, gfm: true, breaks: true }) as string;
    if (cites.length > 0) {
      const items = cites
        .map((c) => {
          let s = `[${c.n}] <strong>${escapeHtml(c.instrument)}</strong>`;
          if (c.path) s += ` — ${escapeHtml(c.path)}`;
          if (c.page != null) s += ` (p.${c.page})`;
          return `<li>${s}</li>`;
        })
        .join("");
      html += `<p><strong>Sources</strong></p><ul>${items}</ul>`;
    }
    return sanitizeAnswerHtml(html);
  }
  async function copyAnswer(turn: Turn, id: string) {
    const plain = answerForCopy(turn);
    const flash = () => {
      setCopiedId(id);
      setTimeout(() => setCopiedId((c) => (c === id ? "" : c)), 1500);
    };
    try {
      // ClipboardItem needs a secure context and isn't guaranteed in every webview
      // (notably the Tauri desktop shell), so fall through to plain text rather
      // than leaving the user with a Copy button that silently does nothing.
      if (typeof ClipboardItem !== "undefined" && navigator.clipboard?.write) {
        await navigator.clipboard.write([
          new ClipboardItem({
            "text/html": new Blob([answerHtmlForCopy(turn)], { type: "text/html" }),
            "text/plain": new Blob([plain], { type: "text/plain" }),
          }),
        ]);
        flash();
        return;
      }
      console.warn(
        "[copy] rich clipboard unavailable — pasting will be plain markdown.",
        { hasClipboardItem: typeof ClipboardItem !== "undefined", hasWrite: !!navigator.clipboard?.write },
      );
    } catch (err) {
      // Loud on purpose: this catch falls back to plain text, which looks to the
      // user exactly like "the feature didn't ship". Without this the failure is
      // indistinguishable from a stale bundle.
      console.warn("[copy] rich clipboard failed — falling back to plain text:", err);
    }
    try {
      await navigator.clipboard.writeText(plain);
      flash();
    } catch {
      /* clipboard unavailable — no-op */
    }
  }

  /**
   * F022: sync the assist banner to the SERVER's session for `id`.
   *
   * `assistMode` is only a mirror — the session lives in `notice_assist_session`
   * and, while it exists, `run_turn` routes every turn in that conversation into
   * the notice flow. Switching conversations therefore invalidates the mirror:
   * reopening a chat where `/help` was typed showed NO banner while the server was
   * still treating every question as a notice reply (assist mode with no sign of
   * it), and "New chat" carried a stale banner into a fresh chat that had no
   * session at all.
   *
   * Callers clear the banner first (the previous conversation's value is always
   * wrong for the new one); this then corrects it upward if a session really is
   * open. A brand-new UUID cannot have one, so `newChat` skips the round trip.
   */
  async function refreshAssistMode(id: string) {
    try {
      const { open } = await assistStatus(id);
      // A slow reply must not clobber a conversation the officer has since left.
      if (conversationId() === id) setAssistMode(open);
    } catch (err) {
      // Not fatal: the banner stays hidden, which is the common case and the same
      // state as before this call existed. Surface it rather than swallow it — if
      // this keeps failing, a live session would be invisible in the UI.
      console.warn(`assist_status failed for ${id}; banner may be out of date`, err);
    }
  }

  /**
   * F022: leave assist mode.
   *
   * The assist session is SERVER state (`notice_assist_session`): while its row
   * exists, `run_turn` routes every turn in this conversation into the notice flow,
   * and `assistMode()` is only a mirror of it. So leaving must reach the server —
   * flipping the signal alone would hide the banner while the officer's next
   * ordinary question was still silently treated as a notice reply.
   *
   * This used to `setInput("/cancel"); submit()` — literally typing the command and
   * pressing send — which reached the server but ran a visible chat turn (user
   * bubble, spinner, assistant reply) just to exit. Typing `/cancel` yourself still
   * works and is unchanged; the button no longer fakes a message to do it.
   */
  async function leaveAssist() {
    if (leaving()) return;
    setLeaving(true);
    setLeaveError(null);
    try {
      await assistExit(conversationId());
      setAssistMode(false);
    } catch (err) {
      // Never swallow this: on failure the session is still open server-side, so
      // the banner MUST stay up — hiding it would strand the officer in assist mode
      // with no sign of it.
      setLeaveError(`Couldn't leave assist mode (${String(err)}). Your next question would still be treated as a notice reply — try again, or type /cancel.`);
    } finally {
      setLeaving(false);
    }
  }

  async function submit(e?: Event) {
    e?.preventDefault();
    const raw = input().trim();
    if (!raw || generatingHere()) return;
    // Free trial: the server spends the question the moment it is ASKED, so the
    // strip is refreshed here rather than on completion. Fire-and-forget — a
    // failed count read must never block or fail a question.
    void refreshTrialStatus();
    // Out of room: the law no longer fits, so the prompt would be truncated and the
    // answer would come back as "I don't have enough information". Stop here — the
    // red banner already says why and offers a new chat. Better an honest wall than
    // a confident-looking refusal. Slash commands are exempt: they never retrieve.
    if (chatIsFull() && !/^\/(help|reply|audit|cancel|exit|stop|done)\b/i.test(raw)) return;
    setAttachError(null);
    // Attachment still indexing: `attachment_source_ids` only returns 'indexed'
    // rows, so sending now would silently fall back to the whole corpus instead of
    // scoping to the file (the file finished OCR a moment ago but isn't 'indexed'
    // yet). Hold the query, keep the typed question, and tell them to wait — then
    // one tap re-sends once the chip flips to indexed. Slash flows are exempt.
    {
      const isSlash = /^\/(help|reply|audit|cancel|exit|stop|done)\b/i.test(raw);
      const stillIndexing = attachments().some(
        (a) => a.status === "pending" || a.status === "processing",
      );
      if (stillIndexing && !isSlash) {
        setAttachError(
          "Your attached file is still being processed — wait for it to finish indexing, then send so the answer is scoped to it.",
        );
        return;
      }
    }
    // No document in scope: the officer explicitly DESELECTED every document
    // (not the default "all"), and hasn't @-mentioned or attached one. Silently
    // widening to the whole corpus here is misleading — ask them to pick a
    // document (or attach one) instead. Slash-command assist flows are exempt
    // (they operate on the attachment / their own text).
    {
      const sel = docSelection();
      const explicitlyEmpty = !sel.all && sel.ids.length === 0;
      const isSlash = /^\/(help|reply|audit|cancel|exit|stop|done)\b/i.test(raw);
      if (explicitlyEmpty && mentionIds().length === 0 && attachments().length === 0 && !isSlash) {
        setInput("");
        setTurns((t) => [
          ...t,
          { role: "user", text: raw },
          {
            role: "assistant",
            text: "No document is selected. Please select a document from the list — or attach a file — to get a grounded answer.",
          },
        ]);
        return;
      }
    }
    // F022: enter/leave assist mode on the slash commands.
    if (/^\/(help|reply|audit)\b/i.test(raw)) setAssistMode(true);
    if (/^\/(cancel|exit|stop|done)\b/i.test(raw)) setAssistMode(false);
    // F020: swap confidential spans for reversible labels BEFORE anything leaves
    // the browser. The officer's bubble shows the real text (`raw`); the server,
    // logs and DB only ever see `masked`. The label→value map (session-only) is
    // used to re-fill real values into the answer for display.
    const activeRanges = maskRanges();
    const maskedItems = maskedItemsFromRanges(activeRanges);
    const masked = maskSession.mask(raw, activeRanges);
    saveMaskSession();
    setInput("");
    setTurns((t) => [...t, { role: "user", text: raw, masked: maskedItems }]);
    // Document scope for this turn: @-mention chips → checkbox subset → all.
    const sourceIds = effectiveSourceIds();
    // F027 — read the corpus tick, then IMMEDIATELY clear it. Per question, not
    // per project or per session (FR-017): the next question starts excluded
    // again whether or not the user notices. Cleared here rather than in a
    // `finally` so a failed turn also resets it — a flag that survives an error
    // is exactly how it would silently persist.
    const includeCorpus = corpusThisTurn();
    setCorpusThisTurn(false);
    // Same per-question rule for web search, and read BEFORE it is cleared so
    // this turn still gets what the officer asked for. Cleared here rather than
    // in a `finally` so a failed turn also resets it.
    const web = webSearch();
    setWebSearch(false);
    await runInference(masked, maskedItems, { deep: deepSearch(), sourceIds, includeCorpus, web });
  }

  /**
   * Start one turn. The generation itself lives in `stores/chat`, so it survives
   * this component being unmounted (navigating to another tab mid-answer); the
   * answer is queued against its conversation and drained by `drainCompleted`
   * below — here, or in the next `ChatPage` to mount.
   *
   * `query: masked` rides along so a later "Search all documents" can re-run the
   * exact same question without re-masking.
   */
  async function runInference(
    masked: string,
    maskedItems: MaskedItem[],
    // ⚠️ `web` is PASSED IN, never read from the signal here: `submit` clears the
    // toggle before calling so it cannot leak into the next question, which
    // would leave this reading `false` for the very turn that asked for it.
    opts: {
      deep: boolean;
      sourceIds?: string[];
      widen?: boolean;
      includeCorpus?: boolean;
      web?: boolean;
    },
  ) {
    await runTurn(conversationId(), masked, maskedItems, {
      ...opts,
      nCtx: model()?.n_ctx ?? 0,
    });
  }

  /**
   * Render the answers that finished for the OPEN conversation. Runs on every
   * completion and on every conversation change, which covers the two cases that
   * used to lose an answer outright: the turn finishing while this page was
   * unmounted, and it finishing while the officer was reading a different chat.
   *
   * Masking is re-filled HERE and nowhere else — the label→value map is browser
   * session state that the store deliberately never sees.
   */
  createEffect(() => {
    completedSignal();
    const id = conversationId();
    if (!id) return;
    const done = takeCompletedTurns(id);
    if (done.length === 0) return;
    setTurns((t) => [
      ...t,
      ...done.map((d): Turn => {
        if (d.outcome.kind === "stopped") return { role: "assistant", text: "_Stopped._" };
        if (d.outcome.kind === "error")
          return { role: "assistant", text: `Error: ${d.outcome.message}` };
        const resp = d.outcome.response;
        return {
          role: "assistant",
          text: maskSession.refill(resp.answer),
          response: resp,
          masked: d.masked,
          query: d.query,
        };
      }),
    ]);
  });

  /**
   * "Search all documents" — re-run the answer's exact (masked) question over the
   * full corpus, ignoring the default narrow-to-attachment scope (`widen: true`).
   * A plain, non-deep search: it's the escape hatch from a file-scoped answer.
   */
  async function widenSearch(turn: Turn) {
    if (!turn.query || generatingHere()) return;
    // Push a user-side marker for the widen action FIRST: the ChatGPT-style scroll
    // effect pins the latest user turn near the top, so this moves the view down to
    // the new generating answer. A plain button click adds no turn, so without this
    // the view stayed stuck on the old (file-scoped) answer while the corpus answer
    // streamed unseen at the bottom.
    setTurns((t) => [...t, { role: "user", text: "Search all documents" }]);
    await runInference(turn.query, turn.masked ?? [], { deep: false, widen: true });
  }

  /**
   * Edit a previously-sent query: drop that turn and everything after it, then
   * load the text back into the composer so the officer can fix and re-send.
   * (Matches the familiar "edit message" model — the reworded query re-runs
   * cleanly instead of piling a correction onto the old, wrong answer.)
   */
  function editTurn(index: number, text: string) {
    if (generatingHere()) return;
    setTurns((t) => t.slice(0, index));
    clearStreamingIfIdle(conversationId());
    setInput(text);
    queueMicrotask(() => {
      composerInput?.focus();
      composerInput?.setSelectionRange(text.length, text.length);
    });
  }

  /** Start a blank chat without discarding any past conversation. */
  function newChat() {
    // On a narrow screen the rail is an overlay covering the conversation —
    // picking an action from it must hand the screen back.
    closeRails();
    // In a project, a new chat is a PROJECT chat: the server records it in
    // `project_chat`, which is what scopes retrieval for every turn in it.
    // Minting a bare uuid here would create a chat that belongs to nothing.
    if (inProjectRoute()) {
      void (async () => {
        try {
          const cid = await startProjectChat(projectId()!);
          await openProject(projectId()!); // refresh the rail with the new chat
          navigate(`/projects/${projectId()}/chat/${cid}`);
        } catch {
          /* the project page surfaces project errors; stay put on failure */
        }
      })();
      return;
    }
    const id = newConversationId();
    setConversationId(id);
    loadMaskSession(id);
    setMaskRanges([]);
    setTurns([]);
    clearStreamingIfIdle(id);
    setAttachments([]);
    setAttachError(null);
    // A fresh UUID provably has no assist session, so this needs no round trip —
    // but it MUST be cleared: the banner used to follow the officer out of assist
    // mode into a brand-new chat and claim a session that did not exist.
    setAssistMode(false);
    setLeaveError(null);
  }

  /** Reopen a past conversation (full questions + answers) from the history list. */
  function selectConversation(id: string) {
    closeRails();
    if (id === conversationId()) return;
    setConversationId(id);
    loadMaskSession(id);
    setMaskRanges([]);
    const cached = loadTurns(id);
    // Opening is not activity — tell the rail these turns are already accounted
    // for, so the transcript effect does not read them as new and re-sort.
    railSeen.set(id, cached.length);
    setTurns(cached);
    // Nothing cached => this chat was written on another device. Pull the
    // transcript from the server (migration 020). Only when the cache is empty:
    // the cached copy is strictly richer, since it holds the questions with the
    // officer's confidential values still in them.
    if (cached.length === 0 && serverHistory()) void hydrateFromServer(id);
    clearStreamingIfIdle(id);
    setAttachments([]);
    loadAttachments(id);
    // The previous conversation's banner is always wrong for this one: clear it,
    // then ask the server whether THIS conversation has a live assist session.
    // Without this, reopening a chat where `/help` was typed left the officer in
    // assist mode with no banner to show for it.
    setAssistMode(false);
    setLeaveError(null);
    void refreshAssistMode(id);
  }

  /**
   * Fetch a conversation written on another device and render it.
   *
   * Guarded on the conversation still being the open one: the officer may have
   * clicked on to a third chat while this was in flight, and dropping a stale
   * response into the transcript would show them someone else's answer to a
   * question they can't see.
   */
  async function hydrateFromServer(id: string) {
    try {
      const rows = await conversationTurns(id);
      if (rows.length === 0 || conversationId() !== id) return;
      const hydrated = historyToTurns(rows);
      // Same reason as `selectConversation`: a transcript pulled from the server
      // is history being displayed, not a turn being added.
      railSeen.set(id, hydrated.length);
      setTurns(hydrated);
    } catch {
      /* offline / server restarting — the chat opens empty rather than erroring */
    }
  }

  /** Permanently remove a conversation from history and storage. */
  /**
   * Delete a chat from a project. Does everything the global rail's delete does —
   * local cache, attachments, the migration-020 soft hide — PLUS dropping the
   * `project_chat` row, which nothing else removes (it cascades from `project`
   * only, so the chat would otherwise linger in the listing forever).
   */
  /**
   * Poll a project's documents while any of them is still being indexed.
   *
   * ⚠️ 2026-08-29 — indexing happens in the BACKGROUND on the server and nothing
   * pushes the result anywhere. `openProject` is called once when a document is
   * added, so the rail showed "indexing…" and then never changed: the only way
   * to see a document become searchable was to reload the page.
   *
   * The interval is created ONCE per project, not per document change. Signal
   * reads inside a timer callback are outside the effect's tracking scope, so
   * refreshing the store here does not re-run this effect and churn the timer —
   * which is exactly how a poll turns into a tight loop.
   */
  createEffect(() => {
    if (!inProjectRoute()) return;
    const pid = projectId()!;
    let pendingTicks = 0;
    let inFlight = false;
    const id = window.setInterval(() => {
      if (!projectDocuments().some((d) => d.status === "pending")) {
        pendingTicks = 0;
        return; // nothing to wait for — cost is one array scan, no request
      }
      pendingTicks += 1;
      // Every 4s for the first minute, then every 20s. A document that is stuck
      // pending (a failed parse that never reported) must not keep asking the
      // server fifteen times a minute for the rest of the session.
      if (pendingTicks > 15 && pendingTicks % 5 !== 0) return;
      if (inFlight) return;
      inFlight = true;
      void openProject(pid)
        .catch(() => {
          /* a failed refresh is not worth an error banner — the next tick retries */
        })
        .finally(() => {
          inFlight = false;
        });
    }, 4000);
    onCleanup(() => window.clearInterval(id));
  });

  /**
   * Feed the rail's tick-boxes into the same `docSelection` the composer and the
   * send path already consume, so neither needs a project branch. Named rather
   * than inline because removing a document has to re-apply it too.
   */
  function applyProjectSelection(sel: string[] | null) {
    setProjectSelected(sel);
    const ready = projectDocuments()
      .filter((d) => d.status === "ready")
      .map((d) => d.source_id);
    setDocSelection({ ids: sel ?? ready, all: sel === null });
  }

  /**
   * Remove a document from this project.
   *
   * ⚠️ "Removed" and "deleted" are different outcomes and the officer cannot tell
   * them apart from the rail — the server decides, based on whether the content is
   * still referenced elsewhere (`sources::is_referenced`). Report which happened.
   */
  function onRemoveProjectDocument(doc: ProjectDocument) {
    const name = doc.display_name || doc.file_name;
    if (!window.confirm(`Remove "${name}" from this project?`)) return;
    void (async () => {
      setProjectNotice(null);
      try {
        const res = await removeDocument(projectId()!, doc.source_id);
        // Prune the id from the scope before re-applying it: a removed document
        // left in the selection keeps the count off "all" for ever, so "Select
        // all" would sit there with nothing left to select.
        const sel = projectSelected();
        applyProjectSelection(sel === null ? null : sel.filter((x) => x !== doc.source_id));
        setProjectNotice(
          res.content_retained
            ? "Removed from this project. The document is still in use elsewhere, so it was kept."
            : "Removed and deleted.",
        );
      } catch (e) {
        setProjectNotice(e instanceof Error ? e.message : "Could not remove the document.");
      }
    })();
  }

  function onRenameProject() {
    const p = activeProject();
    if (!p) return;
    setProjectMenuOpen(false);
    const next = window.prompt("Rename project", p.name);
    if (!next || !next.trim() || next.trim() === p.name) return;
    void (async () => {
      try {
        await renameProject(p.id, next);
        await openProject(p.id); // `renameProject` refreshes the LIST, not this page
      } catch (e) {
        setProjectNotice(e instanceof Error ? e.message : "Could not rename the project.");
      }
    })();
  }

  function onShareProject() {
    const p = activeProject();
    if (!p) return;
    setProjectMenuOpen(false);
    const who = window.prompt(`Share "${p.name}" with which colleague? (username)`);
    if (!who || !who.trim()) return;
    // ⚠️ FR-024 — the disclosure is a requirement, not UI polish. Sharing gives
    // the recipient their OWN copy, and a document that reaches that copy can
    // never afterwards be recalled from it, even by deleting the original.
    const ok = window.confirm(
      `Share "${p.name}" with ${who.trim()}?

` +
        `They will get their own copy of this project, containing these documents. ` +
        `They can edit their copy freely and you will not see their changes.

` +
        `You will NOT be able to take these documents back afterwards — deleting them ` +
        `here will not remove them from their copy.`,
    );
    if (!ok) return;
    void (async () => {
      try {
        const res = await shareProject(p.id, who.trim());
        setProjectNotice(`Copy sent to ${res.shared_with}.`);
      } catch (e) {
        setProjectNotice(e instanceof Error ? e.message : "Could not share the project.");
      }
    })();
  }

  function onDeleteProject() {
    const p = activeProject();
    if (!p) return;
    setProjectMenuOpen(false);
    const n = p.chat_count === 1 ? "1 chat" : `${p.chat_count} chats`;
    const ok = window.confirm(
      `Delete "${p.name}"? Its ${n} will be deleted too.

` +
        `Documents you shared into someone else's project stay with them.`,
    );
    if (!ok) return;
    void (async () => {
      try {
        await deleteProject(p.id);
        navigate("/projects");
      } catch (e) {
        setProjectNotice(e instanceof Error ? e.message : "Could not delete the project.");
      }
    })();
  }

  function deleteProjectChatEntry(cid: string, e: Event) {
    e.stopPropagation();
    const wasOpen = cid === conversationId();
    void (async () => {
      try {
        await deleteProjectChat(cid);
      } catch {
        return; // membership still there — do not orphan the local state
      }
      localStorage.removeItem(turnsKey(cid));
      if (!isTauri()) deleteConversationAttachments(cid).catch(() => {});
      if (serverHistory()) hideConversation(cid).catch(() => {});
      await openProject(projectId()!); // refresh the rail from the server
      if (!wasOpen) return;
      // ⚠️ Do NOT call newChat() here. In project mode that calls
      // `startProjectChat`, which CREATES a chat server-side — so deleting the
      // chat you were reading immediately spawned a replacement "New chat" in
      // the same slot, and read as the deleted one coming back.
      //
      // Move to the most recent surviving chat instead, and fall back to the
      // projects LIST when none are left — 2026-08-29, the project overview page
      // it used to fall back to was retired. Creating a chat is something the
      // officer asks for, never a side effect of deleting one, so this must not
      // reach for `enterProject` either.
      const rest = projectChats();
      if (rest.length > 0) {
        navigate(`/projects/${projectId()}/chat/${rest[0].conversation_id}`);
      } else {
        navigate("/projects");
      }
    })();
  }

  function deleteConversation(id: string, e: Event) {
    e.stopPropagation();
    localStorage.removeItem(turnsKey(id));
    setConversations((list) => list.filter((c) => c.id !== id));
    // F021 (FR-019): server-side cleanup of this conversation's attachments
    // (chunks/embeddings/files). Fire-and-forget — local delete never blocks.
    if (!isTauri()) deleteConversationAttachments(id).catch(() => {});
    // Migration 020: hide it for this user on EVERY device, not just this
    // browser. A soft delete — the row stays in `query_log` so the admin usage
    // and export trail keeps its record of what was asked.
    if (serverHistory()) hideConversation(id).catch(() => {});
    if (id === conversationId()) newChat();
  }

  return (
    <div class="flex h-full bg-noviq-paper text-noviq-ink">
      {/* ── Left rail: chat sessions ──────────────────────────────────────────
          Static column at `lg`+; a slide-over drawer below it. The `lg:` classes
          are emitted after the base ones, so they win — no JS branch needed for
          the positioning, only for the open/closed transform. */}
      {/* Collapsed stand-in for the chat rail. `hidden lg:flex` so it exists only
          where the rail is a static column — below `lg` the rail is a drawer and
          the top bar already has its toggle. */}
      <Show when={leftCollapsed()}>
        <div class="hidden lg:flex w-14 flex-shrink-0 flex-col items-center gap-1 border-r
                    border-noviq-line bg-noviq-paper py-3">
          {/* Brand mark: anchors the strip as a rail rather than a stray border. */}
          <div class="mb-1 flex h-8 w-8 items-center justify-center rounded-lg bg-noviq-700
                      text-sm font-bold text-white">
            N
          </div>
          <RailButton label="Show chats" side="right" onClick={() => setLeftCollapsed(false)}>
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M13 5l7 7-7 7M5 5l7 7-7 7" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </RailButton>
          <div class="my-1 h-px w-6 bg-noviq-line" />
          {/* The two things the rail is FOR, still reachable while it is closed —
              collapsing should cost width, not function. */}
          <RailButton label="New chat" side="right" accent onClick={newChat}>
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 5v14M5 12h14" stroke-linecap="round" />
            </svg>
          </RailButton>
          <RailButton
            label={`Chat history (${conversations().length})`}
            side="right"
            onClick={() => setLeftCollapsed(false)}
          >
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z"
                stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </RailButton>
        </div>
      </Show>

      <aside
        class={`flex flex-col bg-noviq-paper border-r border-noviq-line
                fixed inset-y-0 left-0 z-40 w-72 max-w-[85vw] shadow-2xl
                transition-transform duration-200 ease-out
                lg:static lg:z-auto lg:w-64 lg:flex-shrink-0 lg:max-w-none
                lg:shadow-none lg:translate-x-0 lg:transition-none
                ${leftRailOpen() ? "translate-x-0" : "-translate-x-full"}
                ${leftCollapsed() ? "lg:hidden" : ""}`}
        aria-hidden={isCompact() && !leftRailOpen()}
      >
        {/* The brand lockup lives in the top bar (App.tsx); this rail says
            what the rail IS. The "Research Station" tagline was removed here on
            2026-08-25 (architect) — the logo alone carries the brand. */}
        <div class="flex items-center justify-between px-4 py-4">
          <div class="text-sm font-bold text-noviq-ink leading-tight">Chat</div>
          {/* Drawer dismiss — the backdrop also closes, but a visible control is
              the discoverable way out on touch. */}
          <Show when={isCompact()}>
            <button
              class="tap-target flex items-center justify-center rounded-lg p-1.5 text-noviq-500 hover:bg-noviq-line/40 hover:text-noviq-ink lg:hidden"
              onClick={closeRails}
              aria-label="Close chats"
            >
              <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 6l12 12M18 6L6 18" stroke-linecap="round" /></svg>
            </button>
          </Show>
          {/* Desktop collapse. Not inside the `isCompact` Show above: that one is
              the drawer dismiss, and the two must not be the same control. */}
          <button
            class="hidden lg:flex items-center justify-center rounded-lg p-1.5 text-noviq-500
                   hover:bg-noviq-line/40 hover:text-noviq-ink"
            onClick={() => setLeftCollapsed(true)}
            title="Hide chats"
            aria-label="Hide chats"
          >
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M15 18l-6-6 6-6" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </button>
        </div>

        {/* Project mode — name the project, give a way back to the list, and
            carry the three actions the retired overview page used to own
            (2026-08-29). A project now lives in ONE place, not two. */}
        <Show when={inProjectRoute()}>
          <div class="px-3 pb-2">
            <button
              onClick={() => navigate("/projects")}
              class="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-[11px] text-noviq-500 hover:bg-noviq-50 hover:text-noviq-ink"
            >
              <svg class="h-3.5 w-3.5 flex-shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M15 18l-6-6 6-6" stroke-linecap="round" stroke-linejoin="round" /></svg>
              All projects
            </button>
            <div class="relative mt-0.5 flex items-center gap-1 rounded-md px-2 py-1">
              <span class="min-w-0 flex-1 truncate text-sm font-semibold text-noviq-ink"
                    title={activeProject()?.name ?? "Project"}>
                {activeProject()?.name ?? "Project"}
              </span>
              <button
                onClick={() => setProjectMenuOpen((v) => !v)}
                title="Project actions"
                aria-label="Project actions"
                aria-expanded={projectMenuOpen()}
                class="flex-shrink-0 rounded p-1 text-noviq-500 hover:bg-noviq-100 hover:text-noviq-ink"
              >
                <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
                  <circle cx="12" cy="5" r="1.7" /><circle cx="12" cy="12" r="1.7" /><circle cx="12" cy="19" r="1.7" />
                </svg>
              </button>
              <Show when={projectMenuOpen()}>
                {/* Click-away backdrop — without one the menu stays open behind
                    every later click, which is how a "sticky" menu happens. */}
                <div class="fixed inset-0 z-40" onClick={() => setProjectMenuOpen(false)} />
                <div class="absolute right-1 top-full z-50 mt-1 w-40 overflow-hidden rounded-lg
                            border border-noviq-line bg-white py-1 shadow-lg">
                  <button class="block w-full px-3 py-1.5 text-left text-sm text-noviq-ink hover:bg-noviq-50"
                          onClick={onRenameProject}>
                    Rename
                  </button>
                  <button class="block w-full px-3 py-1.5 text-left text-sm text-noviq-ink hover:bg-noviq-50"
                          onClick={onShareProject}>
                    Share a copy…
                  </button>
                  <button class="block w-full px-3 py-1.5 text-left text-sm text-red-700 hover:bg-red-50"
                          onClick={onDeleteProject}>
                    Delete project
                  </button>
                </div>
              </Show>
            </div>
            <Show when={projectNotice()}>
              <p class="mt-1.5 rounded border border-noviq-200 bg-noviq-50 px-2 py-1.5 text-[11px] leading-snug text-noviq-800">
                {projectNotice()}
                <button class="ml-1 underline" onClick={() => setProjectNotice(null)}>dismiss</button>
              </p>
            </Show>
          </div>
        </Show>

        <div class="px-3 pb-2">
          <button
            onClick={newChat}
            class="w-full flex items-center justify-center gap-2 rounded-lg bg-noviq-700 hover:bg-noviq-800 text-white text-sm font-medium py-2 transition-colors"
          >
            <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 5v14M5 12h14" stroke-linecap="round" /></svg>
            New chat
          </button>
        </div>

        {/* Chats — the whole rail (documents moved to the right rail).
            In project mode this lists ONLY this project's chats. */}
        <div class="flex-1 min-h-0 flex flex-col">
          <div class="px-4 pt-3 pb-1 text-[10px] font-mono uppercase tracking-widest text-noviq-400 flex-shrink-0">
            {inProjectRoute() ? "Project chats" : "Recent chats"}
          </div>
          <div class="px-2 pb-1 flex-1 min-h-0 overflow-auto">
            <Show when={inProjectRoute()}>
              <Show
                when={projectChats().length > 0}
                fallback={<p class="px-2 py-2 text-xs text-noviq-400">No chats in this project yet.</p>}
              >
                <For each={projectChats()}>
                  {(c) => (
                    <div
                      onClick={() => navigate(`/projects/${projectId()}/chat/${c.conversation_id}`)}
                      class={`group flex items-center gap-2 rounded-md px-2 py-1.5 text-sm cursor-pointer transition-colors ${
                        c.conversation_id === conversationId()
                          ? "bg-noviq-100 text-noviq-ink"
                          : "text-noviq-ink hover:bg-noviq-50"
                      }`}
                    >
                      <svg class="h-4 w-4 flex-shrink-0 text-noviq-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" stroke-linejoin="round" /></svg>
                      <span class="flex-1 min-w-0 truncate font-medium">{c.title || "New chat"}</span>
                      <button
                        onClick={(e) => deleteProjectChatEntry(c.conversation_id, e)}
                        title="Delete chat"
                        class="flex-shrink-0 opacity-0 group-hover:opacity-100 text-noviq-400 hover:text-red-600 transition-opacity"
                      >
                        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2m2 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" stroke-linecap="round" stroke-linejoin="round" /></svg>
                      </button>
                    </div>
                  )}
                </For>
              </Show>
            </Show>

            <Show when={!inProjectRoute()}>
            <Show
              when={conversations().length > 0}
              fallback={<p class="px-2 py-2 text-xs text-noviq-400">No past chats yet.</p>}
            >
              <For each={conversations()}>
                {(c) => (
                  <div
                    onClick={() => selectConversation(c.id)}
                    class={`group flex items-center gap-2 rounded-md px-2 py-1.5 text-sm cursor-pointer transition-colors ${
                      c.id === conversationId()
                        ? "bg-noviq-100 text-noviq-ink"
                        : "text-noviq-ink hover:bg-noviq-50"
                    }`}
                  >
                    <svg class="h-4 w-4 flex-shrink-0 text-noviq-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" stroke-linejoin="round" /></svg>
                    <span class="flex-1 min-w-0 truncate font-medium">{c.title || "New chat"}</span>
                    <button
                      onClick={(e) => deleteConversation(c.id, e)}
                      title="Delete chat"
                      class="flex-shrink-0 opacity-0 group-hover:opacity-100 text-noviq-400 hover:text-red-600 transition-opacity"
                    >
                      <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 6h18M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2m2 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" stroke-linecap="round" stroke-linejoin="round" /></svg>
                    </button>
                  </div>
                )}
              </For>
            </Show>
            </Show>
          </div>
        </div>

        <div class="p-3 border-t border-noviq-line flex flex-col gap-2 text-sm text-noviq-600">
          <button onClick={() => navigate("/settings")} class="flex items-center gap-2 hover:text-noviq-ink transition-colors">
            <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><path d="M12 8v.01M11 12h1v4h1" stroke-linecap="round" stroke-linejoin="round" /></svg>
            Help
          </button>
        </div>
      </aside>

      {/* ── Center: workspace ─────────────────────────────────────────────── */}
      <main class="flex-1 min-w-0 flex flex-col">
        {/* Compact-only rail toggles. At `lg`+ both rails are always visible, so
            this bar would be dead chrome — it is not rendered at all. */}
        <Show when={isCompact()}>
          <div class="flex items-center justify-between gap-2 border-b border-noviq-line px-2 py-1.5 flex-shrink-0">
            <button
              class="tap-target inline-flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-sm text-noviq-600 hover:bg-noviq-line/40 hover:text-noviq-ink transition-colors"
              onClick={toggleLeftRail}
              aria-label="Show chats"
              aria-expanded={leftRailOpen()}
            >
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" stroke-linejoin="round" /></svg>
              Chats
            </button>
            <button
              class="tap-target inline-flex items-center gap-1.5 rounded-lg px-2 py-1.5 text-sm text-noviq-600 hover:bg-noviq-line/40 hover:text-noviq-ink transition-colors"
              onClick={toggleRightRail}
              aria-label="Show documents"
              aria-expanded={rightRailOpen()}
            >
              Documents
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 4h10l6 6v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z" stroke-linejoin="round" /><path d="M14 4v6h6" stroke-linejoin="round" /></svg>
            </button>
          </div>
        </Show>
        {/* transcript */}
        <div ref={transcriptEl} class="flex-1 min-h-0 overflow-auto px-3 py-4 sm:px-6 sm:py-6 space-y-4 sm:space-y-6">
          <For each={turns()}>
            {(turn, i) => (
              <Show
                when={turn.role === "assistant"}
                fallback={
                  <div data-role="user">
                    <div class="rounded-xl bg-noviq-panel border border-noviq-line px-3 py-3 sm:px-5 sm:py-4 text-[15px] text-noviq-ink whitespace-pre-wrap break-words">
                      <Show when={turn.masked && turn.masked.length > 0} fallback={turn.text}>
                        <For each={highlightSegments(turn.text, (turn.masked ?? []).map((m) => m.value))}>
                          {(seg) => (
                            <Show when={seg.masked} fallback={<span>{seg.text}</span>}>
                              <mark class="rounded bg-emerald-100 text-emerald-900 px-0.5" title="Masked before sending — the server never received this">{seg.text}</mark>
                            </Show>
                          )}
                        </For>
                      </Show>
                    </div>
                    <div class="mt-1 flex items-center justify-end gap-3">
                      {/* Replayed from another device: the labels are here, the
                          values are not, and they never will be. Say so. */}
                      <Show when={!turn.masked?.length && hasUnresolvedMask(turn.text)}>
                        <span
                          class="inline-flex items-center gap-1 text-[11px] text-noviq-400 mr-auto"
                          title="Confidential values are masked in the browser they were typed in and never sent to the server, so they cannot be shown on another device."
                        >
                          <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" /><path d="M7 11V7a5 5 0 0 1 10 0v4" stroke-linecap="round" /></svg>
                          Confidential values were entered on another device
                        </span>
                      </Show>
                      <Show when={turn.masked && turn.masked.length > 0}>
                        <span class="inline-flex items-center gap-1 text-[11px] text-emerald-700 mr-auto">
                          <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" /><path d="M7 11V7a5 5 0 0 1 10 0v4" stroke-linecap="round" /></svg>
                          {turn.masked!.length} detail{turn.masked!.length === 1 ? "" : "s"} masked before sending
                        </span>
                      </Show>
                      <button
                        type="button"
                        onClick={() => editTurn(i(), turn.text)}
                        disabled={generatingHere()}
                        title="Edit this query and re-run"
                        class="inline-flex items-center gap-1 text-[11px] text-noviq-500 hover:text-noviq-700 disabled:opacity-40 transition-colors"
                      >
                        <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 20h9M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z" stroke-linecap="round" stroke-linejoin="round" /></svg>
                        Edit
                      </button>
                    </div>
                  </div>
                }
              >
                <div class="flex gap-3" data-role="assistant">
                  <div class="h-7 w-7 mt-0.5 flex-shrink-0 rounded-md bg-noviq-700 text-white flex items-center justify-center">
                    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 19V5a2 2 0 0 1 2-2h9l5 5v11a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2z" stroke-linejoin="round" /></svg>
                  </div>
                  <div class="flex-1 min-w-0">
                    {/* Assistant answers with citations get hoverable [n] markers
                        (NotebookLM-style); masked turns keep the plain highlighter. */}
                    {/* Web answers get AnswerBody too. They carry no `citations`
                        (nothing here is an indexed instrument), so the old
                        condition dropped them to plain Markdown and every inline
                        [W1] rendered as dead text — the officer could see a
                        marker but not reach the page it stood for. */}
                    <Show
                      when={turn.response
                        && ((turn.response.citations?.length ?? 0) > 0
                            || (turn.response.web_results?.length ?? 0) > 0)
                        && !(turn.masked && turn.masked.length > 0)}
                      fallback={<Markdown text={turn.masked && turn.masked.length > 0 ? highlightMarkdown(splitNextSteps(turn.text).body, turn.masked.map((m) => m.value)) : splitNextSteps(turn.text).body} />}
                    >
                      <AnswerBody
                        text={splitNextSteps(turn.text).body}
                        citations={turn.response!.citations}
                        webResults={turn.response!.web_results}
                        projectId={projectId()}
                      />
                    </Show>

                    {/* Confirmation that the highlighted values came from masked details
                        that were never sent to the server. */}
                    <Show when={turn.masked && turn.masked.length > 0}>
                      <div class="mt-2 inline-flex items-center gap-1 text-[11px] text-emerald-700" title="These values were masked before sending and restored here on your screen only">
                        <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" /><path d="M7 11V7a5 5 0 0 1 10 0v4" stroke-linecap="round" /></svg>
                        highlighted = your masked details (restored on screen only)
                      </div>
                    </Show>

                    {/* Follow-up suggestion — directly under the ANSWER and ABOVE
                        Sources. It is an action the officer may want to take on
                        what they just read, so it belongs in the reading flow;
                        below the citations and the feedback widget it was past
                        the point where anyone is still looking. */}
                    <Show when={showNextSteps() && splitNextSteps(turn.text).suggestion}>
                      <div class="mt-3">
                        <div class="flex items-center gap-2 mb-1.5">
                          <span class="text-[11px] uppercase tracking-wide text-noviq-400">Next steps</span>
                          <button
                            type="button"
                            onClick={hideNextSteps}
                            class="text-[11px] text-noviq-400 hover:text-noviq-600 underline underline-offset-2"
                            title="Stop showing follow-up suggestions (also in the + menu)"
                          >
                            Hide these
                          </button>
                        </div>
                        <button
                          type="button"
                          disabled={generatingHere()}
                          onClick={() => {
                            const q = splitNextSteps(turn.text).suggestion;
                            if (!q) return;
                            // ALWAYS put it in the composer first. `submit()` has
                            // several SILENT early returns — chat full, no
                            // document in scope, a turn already running — so
                            // calling it alone can leave the click doing nothing
                            // at all, with no feedback. Filling the composer
                            // means the worst case is "press send yourself"
                            // rather than a dead button.
                            setInput(q);
                            // Then try to send it. Deferred a tick so the
                            // composer has committed the value before submit
                            // reads it back.
                            queueMicrotask(() => { void submit(); });
                          }}
                          class="w-full rounded-xl border border-noviq-line bg-white px-3 py-2 text-left text-[13px] text-noviq-700 hover:border-noviq-400 hover:bg-noviq-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                          title={generatingHere() ? "Wait for the current answer to finish" : "Ask this"}
                        >
                          {splitNextSteps(turn.text).suggestion}
                        </button>
                      </div>
                    </Show>

                    <Show when={turn.response}>
                      {(resp) => (
                        <>
                          <Show when={resp().citations.length > 0}>
                            <div class="mt-3 rounded-xl border border-noviq-line bg-white px-3 py-3 sm:px-5 sm:py-4">
                              <div class="text-sm font-semibold text-noviq-ink mb-2">Sources</div>
                              <ul class="space-y-1.5">
                                <For each={resp().citations}>
                                  {(c) => {
                                    const openable = () => !!(c.chunk_id && c.source_id);
                                    const label = (
                                      <>
                                        <span class="font-mono text-noviq-600">[{c.n}]</span>{" "}
                                        <span class="font-medium text-noviq-ink">{c.instrument}</span>
                                        <Show when={c.path}><span class="text-noviq-700"> — {c.path}</span></Show>
                                        <Show when={c.page != null}><span class="text-noviq-500"> · p.{c.page}</span></Show>
                                      </>
                                    );
                                    return (
                                      <li class="text-sm text-noviq-700 leading-snug">
                                        <Show when={openable()} fallback={<span>{label}</span>}>
                                          <button
                                            type="button"
                                            onClick={() => setOpenSource(c)}
                                            title="Open this source at the cited passage"
                                            class="text-left w-full rounded px-1 -mx-1 py-0.5 hover:bg-noviq-50
                                                   hover:text-noviq-900 transition-colors cursor-pointer"
                                          >
                                            {label}
                                          </button>
                                        </Show>
                                      </li>
                                    );
                                  }}
                                </For>
                              </ul>
                            </div>
                          </Show>

                          {/* Web results — rendered DETERMINISTICALLY whenever web
                              search was on, not left to the model to mention. The
                              first cut relied on the prose and the model correctly
                              chose not to pad a settled legal answer, so the
                              feature produced no visible evidence it had run
                              (trace 9059103b).

                              Visually distinct from Sources on purpose: amber, and
                              labelled "unverified". These are NOT citations, and a
                              reader must never mistake one for an indexed
                              instrument. */}
                          <Show when={(resp().web_results?.length ?? 0) > 0}>
                            {/* Split CITED from the rest. Listing all twelve
                                results as equals buried the three the answer
                                actually rests on, and made the block read as a
                                search-results dump rather than sources. The ones
                                the answer used are what a reader needs to check;
                                the rest are context and are folded away. */}
                            {(() => {
                              const cited = () => webSplit(turn).cited;
                              const other = () => webSplit(turn).other;
                              return (
                                <div class="mt-3 rounded-xl border border-amber-300 bg-amber-50 px-3 py-3 sm:px-5 sm:py-4">
                                  <div class="flex items-baseline gap-2 mb-2">
                                    <span class="text-sm font-semibold text-amber-900">
                                      {cited().length > 0 ? "Sources — from the web" : "From the web"}
                                    </span>
                                    <span class="text-[11px] text-amber-700">
                                      unverified — the public web, not your indexed documents
                                    </span>
                                  </div>

                                  <ul class="space-y-2">
                                    <For each={cited().length > 0 ? cited() : other()}>
                                      {(w) => (
                                        <li class="text-[13px]">
                                          <a
                                            href={w.url}
                                            target="_blank"
                                            rel="noopener noreferrer nofollow"
                                            class="text-amber-900 font-medium hover:underline break-words"
                                          >
                                            <span class="text-amber-600 font-mono text-[11px] mr-1">[W{w.n}]</span>
                                            {w.title}
                                          </a>
                                          <div class="text-amber-700/80 text-[11px] mt-0.5">
                                            {hostOf(w.url)}
                                            <Show when={w.published}> · {w.published}</Show>
                                          </div>
                                          <Show when={w.content}>
                                            <p class="text-amber-800/90 text-[12px] mt-0.5 line-clamp-2">{w.content}</p>
                                          </Show>
                                        </li>
                                      )}
                                    </For>
                                  </ul>

                                  <Show when={cited().length > 0 && other().length > 0}>
                                    <details class="mt-3">
                                      <summary class="cursor-pointer text-[12px] text-amber-800 hover:underline">
                                        {other().length} other result{other().length === 1 ? "" : "s"} the answer did not use
                                      </summary>
                                      <ul class="mt-2 space-y-1.5">
                                        <For each={other()}>
                                          {(w) => (
                                            <li class="text-[12px]">
                                              <a
                                                href={w.url}
                                                target="_blank"
                                                rel="noopener noreferrer nofollow"
                                                class="text-amber-900/90 hover:underline break-words"
                                              >
                                                <span class="text-amber-600 font-mono text-[11px] mr-1">[W{w.n}]</span>
                                                {w.title}
                                              </a>
                                              <span class="text-amber-700/70"> · {hostOf(w.url)}</span>
                                            </li>
                                          )}
                                        </For>
                                      </ul>
                                    </details>
                                  </Show>
                                </div>
                              );
                            })()}
                          </Show>


                          {/* scope-routing 2026-07: this answer searched only the
                              chat's attached file — offer to widen to the corpus. */}
                          <Show when={resp().scoped_to_attachment}>
                            <div class="mt-3 flex flex-wrap items-center gap-2 rounded-xl border border-noviq-line bg-noviq-panel px-4 py-3">
                              <span class="text-sm text-noviq-700">Searched only your attached file.</span>
                              <button
                                onClick={() => widenSearch(turn)}
                                disabled={generatingHere()}
                                title="Re-run this question across all documents"
                                class="inline-flex items-center gap-1 text-sm font-medium text-noviq-700 bg-white border border-noviq-200 rounded-lg px-3 py-1.5 hover:bg-noviq-100 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                              >
                                <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" stroke-linecap="round" /></svg>
                                Search all documents
                              </button>
                            </div>
                          </Show>

                          {/* An answer the model did not cite must not wear the
                              same badge as one it did. The server already knows
                              — it just cannot tell us yet (see `modelCited`). */}
                          <Show when={resp().citations.length > 0 && !modelCited(resp().answer)}>
                            <p class="mt-3 rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800">
                              This answer does not cite its sources. The documents below were the
                              closest matches retrieved for the question — they are not references
                              the answer stands on. Check them before relying on it.
                            </p>
                          </Show>

                          <div class="mt-3 flex flex-wrap gap-2">
                            <span class="font-mono text-[11px] uppercase tracking-wide text-noviq-500 bg-noviq-panel border border-noviq-line rounded px-2 py-1">
                              {modelCited(resp().answer)
                                ? `Cited ${resp().citations.length} ${resp().citations.length === 1 ? "time" : "times"}`
                                : `${resp().citations.length} related ${resp().citations.length === 1 ? "passage" : "passages"}`}
                            </span>
                            <span
                              class={`font-mono text-[11px] uppercase tracking-wide rounded px-2 py-1 border ${
                                resp().citations.length > 0 && !modelCited(resp().answer)
                                  ? "text-amber-700 bg-amber-50 border-amber-200"
                                  : "text-noviq-500 bg-noviq-panel border-noviq-line"
                              }`}
                            >
                              Confidence:{" "}
                              {resp().citations.length === 0
                                ? "Low"
                                : !modelCited(resp().answer)
                                  ? "Unverified"
                                  : resp().flags.length === 0
                                    ? "High"
                                    : "Medium"}
                            </span>
                            <Show when={resp().capability === "draft"}>
                              <button onClick={() => exportTurn(resp())} class="font-mono text-[11px] uppercase tracking-wide text-noviq-700 bg-noviq-100 border border-noviq-200 rounded px-2 py-1 hover:bg-noviq-200 transition-colors">
                                Copy draft
                              </button>
                            </Show>
                            {/* Track 3: admin-only trace debug for this answer. */}
                            <Show when={isAdmin() && resp().trace_id}>
                              <button onClick={() => openDebug(resp().trace_id!)} title="Show the step-by-step pipeline for this answer" class="font-mono text-[11px] uppercase tracking-wide text-noviq-700 bg-noviq-100 border border-noviq-200 rounded px-2 py-1 hover:bg-noviq-200 transition-colors">
                                Debug
                              </button>
                            </Show>
                            {/* Copy answer — right-aligned, same badge style. */}
                            <button
                              onClick={() => copyAnswer(turn, `t${i()}`)}
                              title="Copy answer with its sources — pastes with formatting into Word"
                              class="ml-auto inline-flex items-center gap-1 font-mono text-[11px] uppercase tracking-wide text-noviq-700 bg-noviq-100 border border-noviq-200 rounded px-2 py-1 hover:bg-noviq-200 transition-colors"
                            >
                              <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" stroke-linecap="round" /></svg>
                              {copiedId() === `t${i()}` ? "Copied" : "Copy"}
                            </button>
                          </div>

                          <Show when={resp().message_id}>
                            <CitationFeedback
                              messageId={resp().message_id}
                              conversationId={resp().conversation_id}
                            />
                          </Show>

                          {/* 2026-07-29: the hardcoded "Want me to refine this?" paragraph
                              was removed here. It was a second, generic offer shown BELOW the
                              citations, duplicating the model's own closing line — so every
                              answer carried two offers, neither of them about the question.
                              The contextual one renders directly under the answer instead. */}

                          <For each={resp().flags}>
                            {(f) => (
                              <p class="mt-2 text-sm text-amber-700">
                                Could not verify "{f.missing}" in the indexed documents.
                                <Show when={f.offer_to_index}> Index the source and I'll cite it precisely.</Show>
                              </p>
                            )}
                          </For>

                          {/* The fullness notice used to live here, buried in each
                              answer as grey text — and it promised "your case facts
                              carry over", which is not true (facts are per-chat and
                              are not injected). It is now ONE live banner at the
                              composer, driven by the measured number. */}
                        </>
                      )}
                    </Show>
                  </div>
                </div>
              </Show>
            )}
          </For>

          {/* ⚠️ `generatingHere`, not `busy`: the running turn is global now, and
              a turn generating in ANOTHER conversation must not paint a live
              answer under this one. */}
          <Show when={generatingHere()}>
            <div class="text-noviq-500">
              {/* Status text (top-left), a deep-only cost notice, then the
                  connecting-dots network — shown for BOTH normal and deep queries. */}
              <div class="text-sm text-noviq-600">
                <em class="not-italic">{phase() ? `${phase()}…` : STATUS_STEPS[statusIdx()]}</em>
              </div>
              {/* The notice describes the turn that is RUNNING, not the current
                  state of the toggle — which the officer can flip mid-answer. */}
              <Show when={deepIn(conversationId())}>
                <p class="mt-1 text-[12px] text-noviq-400">
                  Deep search — connecting provisions, clauses &amp; facts across your documents.
                  This is thorough, so it takes a little longer.
                </p>
              </Show>
              <DeepSearchNetwork query={[...turns()].reverse().find((t) => t.role === "user")?.text} />
              <Show when={streaming()}>
                <div class="mt-1"><Markdown text={maskSession.refill(streaming())} /></div>
              </Show>
            </div>
          </Show>

          <Show when={turns().length === 0 && !generatingHere()}>
            {/* ⚠️ `min-h-full` + `m-auto`, NOT `h-full` + `justify-center`.
                A flex container that centres content TALLER than itself pushes
                the overflow off BOTH ends, and the part above the start edge
                cannot be scrolled to — on a 414px phone the logo sat at -77px
                with `scrollTop` pinned at 0 and was simply unreachable. An
                `m-auto` child centres when there is room and scrolls normally
                when there is not. */}
            <div class="flex h-full flex-col px-4 py-2">
              <div class="m-auto flex w-full max-w-3xl flex-col items-center text-center">
              {/* Logo ONLY — no "Research Station" tagline under it (architect,
                  2026-08-25). The asset already carries the NOVIQ wordmark, so a
                  text lockup here would repeat it. */}
              {/* ⚠️ Sized in VIEWPORT HEIGHT, not by width breakpoints. Whether
                  this fits is a question about how tall the window is, and a
                  `sm:`/`lg:` ladder cannot answer it — a short 1440px window
                  overflowed exactly like a 768px one. `clamp` keeps it sane at
                  both extremes. */}
              <img
                src={logoUrl}
                alt="Noviq"
                class="mb-[clamp(0.3rem,1.4vh,1.25rem)] h-[clamp(2.1rem,9vh,7rem)]
                       w-[clamp(2.1rem,9vh,7rem)] object-contain"
              />
              <h2 class="max-w-xl text-[clamp(0.92rem,2.5vh,1.5rem)] font-semibold leading-snug text-noviq-ink">
                <Show
                  when={inProjectRoute()}
                  fallback={<>Ask about a procedure, a law,<br class="hidden sm:block" /> or ask me about legal guidance.</>}
                >
                  <Show
                    when={projectDocuments().length > 0}
                    fallback={<>Add documents to this project to start asking.</>}
                  >
                    Ask anything about the documents in this project.
                  </Show>
                </Show>
              </h2>

              {/* A project with no documents cannot answer any of the example
                  questions, so it is offered the one thing it needs instead.
                  Three cards inviting questions that would all come back empty
                  is worse than no cards at all. */}
              <Show
                when={!(inProjectRoute() && projectDocuments().length === 0)}
                fallback={
                  <div class="mt-[clamp(0.6rem,2.4vh,2rem)] w-full max-w-md rounded-2xl border
                              border-dashed border-noviq-line bg-noviq-panel px-6 py-8">
                    <p class="text-sm leading-relaxed text-noviq-600">
                      Everything you ask here is answered from this project's documents and
                      nothing else. Add the ones you want to work from.
                    </p>
                    <button
                      type="button"
                      onClick={() => setAddingDocs(true)}
                      class="mx-auto mt-5 flex items-center gap-2 rounded-lg bg-noviq-700 px-5 py-2
                             text-sm font-medium text-white transition-colors hover:bg-noviq-800"
                    >
                      <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M12 5v14M5 12h14" stroke-linecap="round" />
                      </svg>
                      Add documents
                    </button>
                  </div>
                }
              >
              {/* Example prompts — click to drop the text straight into the
                  composer. Titled cards, so the three read as the KINDS of
                  question this answers rather than as three loose sentences. */}
              <div class="mt-[clamp(0.45rem,2vh,1.75rem)] mb-[clamp(0.3rem,1.2vh,1rem)]
                          flex w-full max-w-3xl items-center gap-3">
                <span class="h-px flex-1 bg-noviq-line" />
                <span class="text-[11px] font-semibold uppercase tracking-widest text-noviq-500">Try asking</span>
                <span class="h-px flex-1 bg-noviq-line" />
              </div>
              <div class="grid w-full max-w-3xl gap-3 sm:grid-cols-3">
                <For each={exampleQueries()}>
                  {(ex) => (
                    <button
                      type="button"
                      onClick={() => { setInput(ex.query); }}
                      class="group flex items-center gap-3 rounded-2xl border border-noviq-line bg-white
                             p-[clamp(0.4rem,1.5vh,1rem)] text-left transition-colors
                             hover:border-noviq-400 hover:bg-noviq-50
                             sm:flex-col sm:items-start sm:gap-0"
                    >
                      <span class="flex h-[clamp(1.7rem,4.2vh,2.75rem)] w-[clamp(1.7rem,4.2vh,2.75rem)]
                                   flex-shrink-0 items-center justify-center rounded-full
                                   bg-noviq-100 text-noviq-700 sm:mb-[clamp(0.35rem,1.2vh,0.75rem)]">
                        <svg class="h-[55%] w-[55%]" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                             stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                          <path d={ex.icon} />
                        </svg>
                      </span>
                      {/* Stacked as a block so the row layout keeps title and
                          query together beside the icon instead of spreading
                          all four children across one line. */}
                      <span class="min-w-0 flex-1 sm:w-full sm:flex-none">
                        <span class="block text-[clamp(0.85rem,1.85vh,0.95rem)] font-semibold text-noviq-ink">
                          {ex.title}
                        </span>
                        {/* Same ink as the title — hierarchy comes from the title's
                            weight and size, not from fading this line out. */}
                        <span class="mt-[clamp(0.1rem,0.5vh,0.25rem)] block text-[clamp(0.75rem,1.65vh,0.82rem)] leading-snug text-noviq-ink">
                          {ex.query}
                        </span>
                      </span>
                      <svg class="h-4 w-4 flex-shrink-0 text-noviq-400 transition-colors group-hover:text-noviq-700
                                  sm:mt-[clamp(0.15rem,1.1vh,0.75rem)] sm:self-end"
                           viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                           stroke-linecap="round" stroke-linejoin="round">
                        <path d="M5 12h14M13 6l6 6-6 6" />
                      </svg>
                    </button>
                  )}
                </For>
              </div>
              </Show>
              </div>
            </div>
          </Show>
        </div>

        {/* composer — with live confidential-detail masking (F020) */}
        <div class="px-3 pb-3 sm:px-6 sm:pb-6 flex-shrink-0">
          {/* F021: conversation attachment chips (web only) */}
          <Show when={!isTauri() && (attachments().length > 0 || attachError() || attaching())}>
            <div class="mb-2 flex flex-wrap items-center gap-1.5">
              {/* In-flight upload: real % while bytes are sent, then indeterminate
                  while the server parses/OCRs/embeds (it responds only when done). */}
              <For each={attaching() ? uploadNames() : []}>
                {(name) => (
                  <span class="inline-flex items-center gap-1.5 rounded-full border border-noviq-line bg-noviq-panel px-2.5 py-1 text-[11px] text-noviq-600">
                    <ProgressRing
                      size={14}
                      percent={uploadPct() ?? undefined}
                      state="active"
                      title={
                        uploadPct() === null
                          ? "Reading the document (text, OCR, embeddings)…"
                          : `Uploading — ${Math.round(uploadPct()!)}%`
                      }
                    />
                    <span class="max-w-[10rem] truncate">{name}</span>
                    <span class="text-noviq-400">
                      {uploadPct() === null ? "reading…" : `${Math.round(uploadPct()!)}%`}
                    </span>
                  </span>
                )}
              </For>
              <For each={attachments()}>
                {(a) => (
                  <span
                    class={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] ${
                      a.status === "indexed"
                        ? "border-green-700/40 bg-green-900/10 text-green-600"
                        : a.status === "failed"
                          ? "border-red-700/40 bg-red-900/10 text-red-500"
                          : "border-noviq-line bg-noviq-panel text-noviq-500"
                    }`}
                    title={
                      a.status === "failed"
                        ? a.failReason ?? "Indexing failed"
                        : a.status === "indexed"
                          ? "Ready — this conversation can use it"
                          : "Indexing…"
                    }
                  >
                    {/* Leading glyph: ring while indexing, tick when ready, ✕ on failure. */}
                    <Show
                      when={a.status !== "indexed" && a.status !== "failed"}
                      fallback={
                        a.status === "indexed" ? (
                          <svg class="h-3.5 w-3.5 text-green-600" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
                            <path d="M20 6L9 17l-5-5" stroke-linecap="round" stroke-linejoin="round" />
                          </svg>
                        ) : (
                          <svg class="h-3.5 w-3.5 text-red-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
                            <path d="M18 6L6 18M6 6l12 12" stroke-linecap="round" stroke-linejoin="round" />
                          </svg>
                        )
                      }
                    >
                      <ProgressRing
                        size={14}
                        percent={indexPct()[a.sourceId]}
                        state="active"
                        title={
                          indexPct()[a.sourceId] !== undefined
                            ? `Indexing — ${Math.round(indexPct()[a.sourceId])}%`
                            : "Reading the document (text, OCR, embeddings)…"
                        }
                      />
                    </Show>
                    <span class="max-w-[10rem] truncate">{a.fileName}</span>
                    <Show when={a.status !== "indexed" && a.status !== "failed" && indexPct()[a.sourceId] !== undefined}>
                      <span class="text-noviq-400 tabular-nums">{Math.round(indexPct()[a.sourceId])}%</span>
                    </Show>
                    <Show when={a.status === "indexed"}>
                      <button
                        class="hover:underline text-noviq-500"
                        title="Move to My documents (keeps it after this chat)"
                        onClick={() => handlePromote(a)}
                      >
                        Keep
                      </button>
                    </Show>
                    <button
                      class="text-noviq-400 hover:text-red-500"
                      title="Remove attachment"
                      onClick={() => handleRemoveAttachment(a)}
                    >
                      ×
                    </button>
                  </span>
                )}
              </For>
              <Show when={attachError()}>
                <span class="text-[11px] text-red-500">{attachError()}</span>
              </Show>
            </div>
          </Show>
          {/* Chat fullness. Two states, because there are two different truths:
              filling up = answers are getting thinner (advice); full = the law no
              longer fits and answers would be truncated into refusals (a stop). */}
          <Show when={chatFilling()}>
            <div class="mb-2 flex items-center gap-2 rounded-lg border border-amber-300 bg-amber-50 px-3 py-1.5 text-[12px] text-amber-800">
              <span>⚠️</span>
              <span class="flex-1">
                <b>This chat is {Math.round((latestFill()?.utilization ?? 0) * 100)}% full.</b>{" "}
                The longer it runs, the less law Noviq can read for each answer — replies may get
                thinner. Start a new chat for the most complete answers.
              </span>
              <button
                type="button"
                class="rounded-full border border-amber-400 px-2 py-0.5 text-[11px] hover:bg-amber-100 whitespace-nowrap"
                onClick={newChat}
              >
                New chat
              </button>
            </div>
          </Show>
          <Show when={chatIsFull()}>
            <div class="mb-2 flex items-center gap-2 rounded-lg border border-red-300 bg-red-50 px-3 py-1.5 text-[12px] text-red-800">
              <span>🛑</span>
              <span class="flex-1">
                <b>This chat is full.</b> There's no room left to read the law properly, so further
                answers here would be unreliable. Start a new chat to continue — you'll need to
                restate anything Noviq must know.
              </span>
              <button
                type="button"
                class="rounded-full border border-red-400 bg-white px-2 py-0.5 text-[11px] font-semibold hover:bg-red-100 whitespace-nowrap"
                onClick={newChat}
              >
                New chat
              </button>
            </div>
          </Show>
          {/* F022: Notice Assist mode banner. */}
          <Show when={assistMode()}>
            <div class="mb-2 rounded-lg border border-noviq-300 bg-noviq-50 px-3 py-1.5 text-[12px] text-noviq-700">
              <div class="flex items-center gap-2">
                <span>🛡️</span>
                <span class="flex-1">
                  <b>Assist mode</b> — responding to a notice / show-cause / audit para. Attach the
                  document and answer the questions; I'll draft a grounded, cited reply.
                </span>
                <button
                  type="button"
                  class="rounded-full border border-noviq-300 px-2 py-0.5 text-[11px] hover:bg-noviq-100 disabled:opacity-50"
                  disabled={leaving()}
                  onClick={leaveAssist}
                >
                  {leaving() ? "Leaving…" : "Leave"}
                </button>
              </div>
              <Show when={leaveError()}>
                <p class="mt-1 text-[11px] text-red-500">{leaveError()}</p>
              </Show>
            </div>
          </Show>
          {/* Active document scope — the next answer is restricted to these docs.
              Chips are listed only for a genuinely NARROW scope. This was written
              for the @-mention case (one or two documents); deselecting one of
              nineteen rendered eighteen chips and buried the composer. Above the
              threshold the useful fact is not which files are in, but how many
              are OUT — so it collapses to a count, with the full list on hover. */}
          <Show when={effectiveSourceIds()}>
            {(ids) => {
              const names = () =>
                ids().map((id) => mentionDocs().find((d) => d.id === id)?.name ?? id);
              const excluded = () => Math.max(0, docCatalog().length - ids().length);
              return (
                <div class="mb-2 flex items-center gap-1.5 flex-wrap text-[11px]">
                  <span class="text-noviq-500">Answering only from:</span>
                  <Show
                    when={ids().length <= MAX_SCOPE_CHIPS}
                    fallback={
                      <span
                        class="inline-flex items-center gap-1 rounded-full border border-noviq-300 bg-noviq-50 px-2 py-0.5 text-noviq-700"
                        title={names().join("\n")}
                      >
                        📄 {ids().length} documents
                        <Show when={excluded() > 0}>
                          <span class="text-noviq-500">
                            · {excluded()} excluded
                          </span>
                        </Show>
                      </span>
                    }
                  >
                    <For each={ids()}>
                      {(id) => {
                        const doc = mentionDocs().find((d) => d.id === id);
                        return (
                          <span class="inline-flex items-center gap-1 rounded-full border border-noviq-300 bg-noviq-50 px-2 py-0.5 text-noviq-700">
                            📄 <span class="max-w-[12rem] truncate">{doc?.name ?? id}</span>
                          </span>
                        );
                      }}
                    </For>
                  </Show>
                </div>
              );
            }}
          </Show>
          {/* Deep-search indicator — inline, right-aligned above the composer: the
              mode + its cost + a one-click opt-out on a single line. */}
          {/* F027 (US3) — "also search the law", for this question only. Shown
              only inside a project, where the corpus is otherwise excluded; in
              ordinary chat the corpus is already searched and there is nothing
              to widen. Resets itself on every send (FR-017). */}
          <Show when={inProject()}>
            <div class="mb-2 flex justify-end">
              <CorpusToggle
                visible={true}
                checked={corpusThisTurn()}
                onChange={setCorpusThisTurn}
                disabled={generatingHere()}
              />
            </div>
          </Show>
          {/* Web search active — worded as a WARNING, not a feature badge. The
              officer's question leaves the network when this is on, and results
              are not law; both facts must be visible before they hit send. */}
          <Show when={webSearch()}>
            <div class="mb-2 flex justify-end">
              <span class="inline-flex items-center gap-2 rounded-full border border-amber-300 bg-amber-50 px-3 py-1 text-[12px] text-amber-900">
                <svg class="h-3.5 w-3.5 flex-shrink-0 text-amber-700" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a15 15 0 0 1 0 18a15 15 0 0 1 0-18z" /></svg>
                <span><span class="font-semibold">Web search on</span> — searching the public web only, not your documents. Results are not law.</span>
                <button
                  type="button"
                  onClick={() => setWebSearch(false)}
                  title="Turn web search off"
                  aria-label="Turn web search off"
                  class="ml-0.5 -mr-1 rounded-full p-0.5 text-amber-700 hover:bg-amber-200 hover:text-amber-900 transition-colors"
                >
                  <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M18 6L6 18M6 6l12 12" stroke-linecap="round" /></svg>
                </button>
              </span>
            </div>
          </Show>

          <Show when={deepSearch()}>
            <div class="mb-2 flex justify-end">
              <span class="inline-flex items-center gap-2 rounded-full border border-noviq-300 bg-noviq-50 px-3 py-1 text-[12px] text-noviq-700">
                <svg class="h-3.5 w-3.5 flex-shrink-0 text-noviq-600" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="7" /><path d="M21 21l-4.3-4.3" stroke-linecap="round" /></svg>
                <span><span class="font-semibold">Deep search on</span> — broader &amp; more thorough, but slower.</span>
                <button
                  type="button"
                  onClick={() => setDeepSearch(false)}
                  title="Turn deep search off"
                  aria-label="Turn deep search off"
                  class="ml-0.5 -mr-1 rounded-full p-0.5 text-noviq-500 hover:bg-noviq-200 hover:text-noviq-700 transition-colors"
                >
                  <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M18 6L6 18M6 6l12 12" stroke-linecap="round" /></svg>
                </button>
              </span>
            </div>
          </Show>
          <div class="rounded-2xl border border-noviq-line bg-white shadow-sm px-2 py-2 sm:px-4 sm:py-3">
            {/* Tighter gutters below `sm`: at 390px the "+" and send buttons plus
                `gap-3` left the input too narrow to read a question back in. */}
            <div class="flex items-start gap-2 sm:gap-3">
              {/* "+" composer menu: Attach file (web) · Deep search mode. */}
              <div class="relative flex-shrink-0 mt-0.5">
                <input
                  ref={attachInputRef}
                  type="file"
                  multiple
                  accept={UPLOAD_ACCEPT}
                  class="hidden"
                  onChange={handleAttachSelected}
                />
                <button
                  type="button"
                  onClick={() => setPlusMenuOpen((v) => !v)}
                  disabled={attaching()}
                  aria-haspopup="menu"
                  aria-expanded={plusMenuOpen()}
                  title="Add — attach a file or turn on deep search"
                  class="h-9 w-9 rounded-full border border-noviq-line text-noviq-500 hover:bg-noviq-50 hover:text-noviq-700 disabled:opacity-40 flex items-center justify-center transition-colors"
                >
                  <Show
                    when={!attaching()}
                    fallback={<ProgressRing size={20} stroke={2.5} percent={uploadPct() ?? undefined} state="active" />}
                  >
                    <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 5v14M5 12h14" stroke-linecap="round" /></svg>
                  </Show>
                </button>
                <Show when={plusMenuOpen()}>
                  {/* click-away backdrop */}
                  <div class="fixed inset-0 z-10" onClick={() => setPlusMenuOpen(false)} />
                  <div
                    role="menu"
                    class="absolute bottom-full left-0 z-20 mb-2 w-52 overflow-hidden rounded-xl border border-noviq-line bg-white shadow-lg py-1"
                  >
                    <Show when={!isTauri()}>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => { setPlusMenuOpen(false); attachInputRef?.click(); }}
                        class="flex w-full items-center gap-2.5 px-3 py-2 text-left text-[13px] text-noviq-700 hover:bg-noviq-50"
                      >
                        <svg class="h-4 w-4 text-noviq-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21.44 11.05l-9.19 9.19a5 5 0 0 1-7.07-7.07l9.19-9.19a3 3 0 0 1 4.24 4.24l-9.2 9.19a1 1 0 0 1-1.41-1.41l8.49-8.49" stroke-linecap="round" stroke-linejoin="round" /></svg>
                        Attach file
                      </button>
                    </Show>
                    {/* Deep search — admin-only while it's being smoothed (server-gated
                        via /me; ECHO_DEEP_AUDIENCE=all opens it to everyone). */}
                    <Show when={deepSearchEnabled()}>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => { setPlusMenuOpen(false); setDeepSearch(true); }}
                        class="flex w-full items-center gap-2.5 px-3 py-2 text-left text-[13px] text-noviq-700 hover:bg-noviq-50"
                      >
                        <svg class="h-4 w-4 text-noviq-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="7" /><path d="M21 21l-4.3-4.3" stroke-linecap="round" /></svg>
                        <span class="flex-1">Deep search mode</span>
                        <Show when={deepSearch()}>
                          <span class="text-[10px] font-semibold text-noviq-600">ON</span>
                        </Show>
                      </button>
                    </Show>

                    {/* Web search — a SEPARATE lane, per turn. Deliberately not
                        sticky: it sends the question to a public search engine,
                        so it should be a conscious choice each time rather than
                        a setting left on and forgotten. */}
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => { setPlusMenuOpen(false); setWebSearch((v) => !v); }}
                      class="flex w-full items-center gap-2.5 px-3 py-2 text-left text-[13px] text-noviq-700 hover:bg-noviq-50"
                    >
                      <svg class="h-4 w-4 text-noviq-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a15 15 0 0 1 0 18a15 15 0 0 1 0-18z" /></svg>
                      <span class="flex-1">Web search <span class="text-noviq-400">(web only)</span></span>
                      <Show when={webSearch()}>
                        <span class="text-[10px] font-semibold text-noviq-600">ON</span>
                      </Show>
                    </button>

                    {/* Follow-up suggestions. Lives here, not only in Settings:
                        the settings gear is hidden for admins (App.tsx), and once
                        the suggestions are hidden the "Hide these" link goes with
                        them — leaving no way back on. This is the reachable,
                        reversible control for every role. */}
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => { setPlusMenuOpen(false); void toggleNextSteps(); }}
                      class="flex w-full items-center gap-2.5 px-3 py-2 text-left text-[13px] text-noviq-700 hover:bg-noviq-50"
                    >
                      <svg class="h-4 w-4 text-noviq-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18h6M10 22h4M12 2a7 7 0 0 0-4 12.7V17h8v-2.3A7 7 0 0 0 12 2z" stroke-linecap="round" stroke-linejoin="round" /></svg>
                      <span class="flex-1">Suggest next steps</span>
                      <span class="text-[10px] font-semibold text-noviq-600">
                        {showNextSteps() ? "ON" : "OFF"}
                      </span>
                    </button>
                  </div>
                </Show>
              </div>
              <div class="flex-1 min-w-0">
                <ConfidentialComposer
                  value={input()}
                  onInput={setInput}
                  disabled={generatingHere()}
                  onSubmit={() => submit()}
                  onRanges={setMaskRanges}
                  documents={mentionDocs()}
                  onMentions={setMentionIds}
                  maxChars={MAX_QUERY_CHARS}
                />
              </div>
              {/* F026: Send while idle; Stop (■) while a turn is generating so the
                  officer can halt a wrong or slow query instead of waiting it out. */}
              {/* Stop belongs to the turn it would cancel: shown only in the
                  conversation that is generating. A turn running in another chat
                  still disables Send (one turn at a time), but must not be
                  stoppable from here — that would kill an answer the officer is
                  not even looking at. */}
              <Show
                when={generatingHere()}
                fallback={
                  <button
                    type="button"
                    onClick={() => submit()}
                    disabled={!input().trim()}
                    class="h-10 w-10 mt-0.5 flex-shrink-0 rounded-full bg-noviq-700 hover:bg-noviq-800 disabled:opacity-40 text-white flex items-center justify-center transition-colors"
                  >
                    <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 19V5M5 12l7-7 7 7" stroke-linecap="round" stroke-linejoin="round" /></svg>
                  </button>
                }
              >
                <button
                  type="button"
                  onClick={() => stopTurn(conversationId())}
                  title="Stop generating"
                  aria-label="Stop generating"
                  class="h-10 w-10 mt-0.5 flex-shrink-0 rounded-full bg-red-600 hover:bg-red-700 text-white flex items-center justify-center transition-colors"
                >
                  <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2" /></svg>
                </button>
              </Show>
            </div>
          </div>
        </div>
      </main>

      {/* ── Right rail: indexed documents ─────────────────────────────────────
          Mirror of the left rail — static at `lg`+, slide-over below it. */}
      <aside
        class={`flex flex-col border-l border-noviq-line bg-noviq-paper
                fixed inset-y-0 right-0 z-40 w-80 max-w-[85vw] shadow-2xl
                transition-transform duration-200 ease-out
                lg:static lg:z-auto lg:w-72 lg:flex-shrink-0 lg:max-w-none
                lg:shadow-none lg:translate-x-0 lg:transition-none
                ${rightRailOpen() ? "translate-x-0" : "translate-x-full"}
                ${rightCollapsed() ? "lg:hidden" : ""}`}
        aria-hidden={isCompact() && !rightRailOpen()}
      >
        {/* Desktop-only header: the compact one below is the drawer dismiss, and
            at `lg`+ there was no header at all, so the rail had nowhere to put a
            collapse control. */}
        <div class="hidden lg:flex items-start justify-between border-b border-noviq-line px-4 py-3">
          <div class="min-w-0">
            <div class="text-sm font-semibold text-noviq-ink">
              {inProjectRoute() ? "Documents" : "Repository"}
            </div>
            <RailStats />
          </div>
          <button
            class="flex items-center justify-center rounded-lg p-1.5 text-noviq-500
                   hover:bg-noviq-line/40 hover:text-noviq-ink"
            onClick={() => setRightCollapsed(true)}
            title="Hide documents"
            aria-label="Hide documents"
          >
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M9 18l6-6-6-6" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </button>
        </div>
        <Show when={isCompact()}>
          <div class="flex items-start justify-between border-b border-noviq-line px-4 py-3">
            <div class="min-w-0">
              <div class="text-sm font-semibold text-noviq-ink">
              {inProjectRoute() ? "Documents" : "Repository"}
            </div>
              <RailStats />
            </div>
            <button
              class="tap-target flex items-center justify-center rounded-lg p-1.5 text-noviq-500 hover:bg-noviq-line/40 hover:text-noviq-ink"
              onClick={closeRails}
              aria-label="Close documents"
            >
              <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 6l12 12M18 6L6 18" stroke-linecap="round" /></svg>
            </button>
          </div>
        </Show>
        {/* Project mode swaps the corpus tree for THIS project's documents.
            `DocumentTree` always loads the whole corpus and has no scoping prop,
            so rendering it here is what made a project look like it held every
            document in the deployment. */}
        <Show
          when={inProjectRoute()}
          fallback={
            <div class="flex-1 min-h-0 px-3 pt-4 pb-3 overflow-auto">
              <DocumentTree
                onSelectionChange={(ids, all) => setDocSelection({ ids, all })}
                onCatalog={setDocCatalog}
                onStats={setCorpusStats}
              />
            </div>
          }
        >
          <div class="flex-1 min-h-0 overflow-hidden">
            <ProjectDocuments
              documents={projectDocuments()}
              selected={projectSelected()}
              onSelectionChange={applyProjectSelection}
              onAdd={() => setAddingDocs(true)}
              onRemove={onRemoveProjectDocument}
            />
          </div>
        </Show>
      </aside>

      {/* Collapsed stand-in for the document rail — mirror of the left one. */}
      <Show when={rightCollapsed()}>
        <div class="hidden lg:flex w-14 flex-shrink-0 flex-col items-center gap-1 border-l
                    border-noviq-line bg-noviq-paper py-3">
          <RailButton label="Show documents" side="left" onClick={() => setRightCollapsed(false)}>
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M11 5L4 12l7 7M19 5l-7 7 7 7" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </RailButton>
          <div class="my-1 h-px w-6 bg-noviq-line" />
          {/* Carries the count, so the closed rail still answers the question the
              open one does: how much of the corpus is in scope right now. */}
          <div class="group relative flex justify-center">
            <button
              onClick={() => setRightCollapsed(false)}
              aria-label="Show documents in scope"
              class="relative flex h-10 w-10 items-center justify-center rounded-lg text-noviq-500
                     transition-colors duration-150 hover:bg-noviq-line/50 hover:text-noviq-ink"
            >
              <svg class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"
                  stroke-linecap="round" stroke-linejoin="round" />
                <path d="M14 2v6h6" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
              <Show when={docSelection().ids.length > 0}>
                <span class="absolute -right-0.5 -top-0.5 min-w-[1.05rem] rounded-full bg-noviq-700
                             px-1 text-[10px] font-semibold leading-4 text-white">
                  {docSelection().ids.length > 99 ? "99+" : docSelection().ids.length}
                </span>
              </Show>
            </button>
            <span
              role="tooltip"
              class="pointer-events-none absolute right-full top-1/2 z-50 mr-2 -translate-y-1/2
                     whitespace-nowrap rounded-md bg-noviq-ink px-2 py-1 text-xs font-medium
                     text-white opacity-0 shadow-lg transition-opacity duration-150
                     group-hover:opacity-100"
            >
              {docSelection().ids.length} document{docSelection().ids.length === 1 ? "" : "s"} in scope
            </span>
          </div>
        </div>
      </Show>

      {/* Adding documents used to be reachable only from the project overview
          page. With that page retired the dialog is hosted here, opened from the
          rail's footer button or the empty-state call to action. Unchanged
          otherwise — it still uploads, or attaches something already indexed. */}
      <Show when={addingDocs() && inProjectRoute()}>
        <AddProjectDocuments
          projectId={projectId()!}
          present={projectDocuments().map((d) => d.source_id)}
          onClose={() => setAddingDocs(false)}
          onAdded={(n) => setProjectNotice(n === 1 ? "1 document added." : `${n} documents added.`)}
        />
      </Show>

      {/* Backdrop behind whichever rail is open. Below the rails (z-30 vs z-40)
          and only mounted on compact screens, so it can never intercept clicks
          on the desktop three-column layout. */}
      <Show when={isCompact() && (leftRailOpen() || rightRailOpen())}>
        <div class="fixed inset-0 z-30 bg-black/40 lg:hidden" onClick={closeRails} aria-hidden="true" />
      </Show>

      {/* Source opened from the "Sources" list under an answer. `audience: "user"`
          is deliberate — an officer is not an admin, and the endpoint scopes to
          the documents they may already see (same choice AnswerBody makes). */}
      <Show when={openSource()}>
        {(c) => (
          <ChunkViewer
            sourceId={c().source_id!}
            fileName={c().instrument}
            focusChunkId={c().chunk_id!}
            audience="user"
            projectId={projectId()}
            onClose={() => setOpenSource(null)}
          />
        )}
      </Show>

      {/* Track 3: admin trace-debug modal for the picked answer. */}
      <Show when={debugOpen()}>
        <div
          class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
          onClick={() => setDebugOpen(false)}
        >
          <div
            class="bg-white rounded-lg shadow-xl max-w-4xl w-full max-h-[85vh] flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div class="flex items-center justify-between px-4 py-2 border-b border-noviq-line">
              <span class="text-sm font-semibold text-noviq-800">Query trace</span>
              <button
                onClick={() => setDebugOpen(false)}
                class="px-2 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-100"
              >
                Close
              </button>
            </div>
            <pre class="flex-1 overflow-auto p-3 text-xs font-mono text-noviq-ink whitespace-pre-wrap">
              {debugBody() ?? "Loading…"}
            </pre>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default ChatPage;
