/**
 * Admin portal (US7, T047, FR-022) — manage beta testers.
 *
 * Web-only (server transport). Lists accounts, creates a tester (returns a
 * one-time temp password to hand over), and disables/enables access. The page
 * self-guards: it calls `GET /api/me` and shows "not authorized" for non-admins
 * (the server independently 403s every admin endpoint via the `AdminUser`
 * guard, so this is only UX).
 */
import { Component, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { call, downloadAuthed, request } from "../api/transport";
import IndexedFolders from "../components/IndexedFolders";
import CorpusCategories from "../components/CorpusCategories";
import {
  refreshProjectsEnabled,
  setProjectsEnabled as setProjectsEnabledShared,
} from "../stores/projects";

// Admin console tabs. `Users` first (default) so the page opens where it did.
type AdminTab = "users" | "inbox" | "indexing" | "features" | "inference" | "traces" | "audit";
const ADMIN_TABS: { id: AdminTab; label: string }[] = [
  { id: "users", label: "Users" },
  // Migration 028 — quote requests from the public landing page. Sits second
  // because an unanswered lead is time-sensitive in a way nothing else on this
  // console is; it carries an unread count on the tab for the same reason.
  { id: "inbox", label: "Inbox" },
  { id: "indexing", label: "Indexing & Data" },
  // 2026-08-07 — deployment-wide feature availability. The My Projects gate
  // used to live under "Model & Inference", which is not what it is: it decides
  // what users can DO, not how answers are generated. Anything that turns a
  // capability on or off for the whole deployment belongs here.
  { id: "features", label: "Features" },
  { id: "inference", label: "Model & Inference" },
  { id: "traces", label: "Traces" },
  // 2026-08-13 — the audit trail. Distinct from "Traces": traces are a
  // debugging view of how one answer was built and are pruned; this is the
  // retained record of who did what, and it is what an inspection asks for.
  { id: "audit", label: "Audit Log" },
];

/**
 * One quote request (`GET /api/admin/quotes`), migration 028.
 *
 * ⚠️ Every string here was typed by an ANONYMOUS member of the public — this is
 * the only table in the system written without a session. Render it as text and
 * nothing else: no `innerHTML`, no link built from `message`, no interpolation
 * into anything that executes. `email` is rendered as a `mailto:` and that is
 * the only place any of it becomes a URL.
 *
 * `source_ip` is deliberately NOT returned by the API — it exists for rate
 * limiting, not for reading.
 */
interface QuoteRow {
  id: string;
  organisation: string;
  contact_name: string | null;
  email: string;
  phone: string | null;
  sector: string | null;
  org_size: string | null;
  message: string | null;
  status: "new" | "read" | "replied" | "archived";
  admin_note: string | null;
  created_at: number; // ms epoch
  updated_at: number;
  emailed_at: number | null;
}

/**
 * One free-trial sign-up (`GET /api/admin/trials`), migration 029.
 *
 * ⚠️ Real people's names and email addresses, shown here so the team can follow
 * up about Noviq. The privacy policy states that plainly and the demo screen
 * says it at the point of sign-in — if this list is ever used for something
 * else, both of those have to change first.
 */
interface TrialRow {
  user_id: string;
  email: string;
  display_name: string | null;
  started_at: number; // ms epoch
  revoked_at: number | null;
  used_today: number;
}

/** One row of the audit trail (`GET /api/admin/audit`). */
interface AuditRow {
  id: string;
  created_at: number; // ms epoch, UTC
  user_id: string | null;
  username: string | null;
  action: string | null;
  target: string | null;
  /** Document title when `target` names a source; null otherwise. */
  target_name: string | null;
  details_json: string | null;
}

interface AuditPage {
  total: number;
  offset: number;
  limit: number;
  rows: AuditRow[];
  actions: string[];
}

/** Plain-English labels for the action names written by the server. */
const AUDIT_ACTION_LABELS: Record<string, string> = {
  document_open: "Opened document",
  document_read: "Read document",
  document_denied: "Refused — not permitted",
  global_search: "Searched corpus",
  index: "Indexed documents",
  login: "Signed in",
  attach: "Attached document",
  promote: "Promoted attachment",
  delete_conversation: "Deleted conversation",
  share_request: "Share request",
  admin_create_user: "Created user",
  admin_disable_user: "Disabled user",
  admin_enable_user: "Enabled user",
};

interface UserRow {
  id: string;
  username: string;
  role: string;
  created_at: number;
  disabled_at: number | null;
  // Usage rollup (F014 follow-on).
  query_count: number;
  last_active: number | null; // ms epoch
  avg_cpu_pct: number | null;
  peak_ram_mb: number | null;
  avg_gpu_util_pct: number | null;
  avg_latency_ms: number | null;
}

interface QueryLogRow {
  id: string;
  conversation_id: string | null;
  question: string;
  answer: string | null;
  citations_json: string | null;
  latency_ms: number | null;
  cpu_pct: number | null;
  ram_mb: number | null;
  gpu_util_pct: number | null;
  gpu_mem_mb: number | null;
  created_at: number; // ms epoch
}

interface MeResp {
  id: string;
  username: string;
  role: string;
}

interface ChunkDoc {
  source_id: string;
  file_name: string;
  chunk_count: number;
}

// Multi-instrument inspection. The pipeline assumes one file == one instrument;
// the SLGA Compendium is twelve rule-sets in one PDF, each numbering its
// provisions from 1, which is what makes "Rule 4" ambiguous across instruments.
// `evidence` says which signals vouch for a boundary, and it is the only thing
// on this panel worth reading carefully:
//   both        — contents list and short-title clause agree (safe)
//   toc_only    — listed in the contents, no clause found; the start page is the
//                 book's own claim and is the likeliest to be off by a page
//   clause_only — found in the text but absent from the contents list
interface Segment {
  start: number;
  end: number;
  title: string;
  evidence: "both" | "toc_only" | "clause_only";
  is_instrument: boolean;
  restart: boolean;
  first_line: string;
}

interface Inspection {
  source_id: string;
  file_name: string;
  is_container: boolean;
  instrument_count: number;
  pages: number;
  segments: Segment[];
  page_offset: number;
  offset_derived: boolean;
  notes: string[];
}

// A row of the editable plan. `evidence` is carried through from detection —
// "manual" for a row the operator added — so the reviewer can still see which
// boundaries a machine vouched for after editing.
interface PlanRow {
  start: number;
  end: number;
  title: string;
  include: boolean;
  evidence: Segment["evidence"] | "manual";
  restart: boolean;
  first_line: string;
}

interface SplitResult {
  created: { source_id: string; title: string; file_name: string; page_offset: number }[];
  /// Page ranges belonging to no instrument — reported so they are dropped
  /// knowingly rather than by accident.
  dropped_pages: [number, number][];
  original_retired_to: string;
}

// Derived-layers "pending" status — which documents still need each layer built.
interface PendingDoc {
  source_id: string;
  file_name: string;
  /** Owning username for a user's own document; null for the shared corpus. */
  owner?: string | null;
}
interface EdgeCount {
  edge_type: string;
  count: number;
}
interface DerivedStatus {
  summaries_pending: PendingDoc[];
  structure_pending: PendingDoc[];
  graph_edges: EdgeCount[];
}

// Duplicate review (migration 022). `content` groups are byte-identical and safe
// to collapse; `name` groups only look alike and need a human — an amended
// instrument beside its base text is two documents, not one.
interface DuplicateDoc {
  source_id: string;
  file_name: string;
  owner?: string | null;
  chunk_count: number;
  created_at: number;
  conversation_id?: string | null;
}
interface DuplicateGroup {
  kind: "content" | "name";
  docs: DuplicateDoc[];
}
interface DuplicateReport {
  groups: DuplicateGroup[];
  group_count: number;
  document_count: number;
}

// Track 3 — one row of the query-debug list (mirrors `system::TraceSummary`).
interface TraceRow {
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

/** One selectable model, as served by `GET /admin/providers`. Everything past
 *  `id`/`tier` is optional — the catalogue describes a moving target, and a
 *  missing figure renders as "—" rather than being guessed at. */
interface CatalogModel {
  id: string;
  tier: "free" | "paid";
  context?: number | null;
  input_price?: number | null;
  output_price?: number | null;
  rpm?: number | null;
  tpm?: number | null;
  rpd?: number | null;
  shutdown_date?: string | null;
  note?: string | null;
}

/** One provider: connection shape plus the commercial terms that must be visible
 *  before a key is pasted, not buried in documentation. */
interface CatalogProvider {
  id: string;
  label: string;
  base_url: string;
  prefix: string;
  endpoint: string;
  key_placeholder: string;
  console_url?: string | null;
  free_tier: string;
  trains_on_data: boolean;
  non_commercial_only: boolean;
  notes: string;
  models: CatalogModel[];
}

/** Result of the live key+model probe (`POST /admin/inference/validate`). */
interface ProbeResult {
  ok: boolean;
  status: number;
  reason: string;
  message: string;
  model?: string;
  url?: string;
  elapsed_ms?: number;
}

const fmtNum = (n: number | null | undefined, digits = 0): string =>
  n === null || n === undefined ? "—" : n.toFixed(digits);
const fmtTime = (ms: number | null): string =>
  ms ? new Date(ms).toLocaleString() : "never";

const Admin: Component = () => {
  const [isAdmin, setIsAdmin] = createSignal(false);
  const [checked, setChecked] = createSignal(false);
  const [users, setUsers] = createSignal<UserRow[]>([]);
  const [error, setError] = createSignal("");
  const [newUsername, setNewUsername] = createSignal("");
  const [newRole, setNewRole] = createSignal("user");
  const [tempPassword, setTempPassword] = createSignal<{ username: string; password: string } | null>(null);
  const [copied, setCopied] = createSignal(false);
  // Per-user activity (recent queries), keyed by user id; which row is expanded.
  const [expanded, setExpanded] = createSignal<string | null>(null);
  const [activity, setActivity] = createSignal<Record<string, QueryLogRow[]>>({});
  // Chunk export — documents to pick from, and the current selection ("" = all).
  const [chunkDocs, setChunkDocs] = createSignal<ChunkDoc[]>([]);
  const [chunkDocId, setChunkDocId] = createSignal<string>("");
  // GPU re-embed of the corpus (one-shot; pauses chat ~7–10 min).
  const [reindexing, setReindexing] = createSignal(false);
  const [reindexMsg, setReindexMsg] = createSignal<string>("");
  // Multi-instrument inspection (read-only proposal for a human to confirm).
  const [inspectId, setInspectId] = createSignal<string>("");
  const [inspecting, setInspecting] = createSignal(false);
  const [inspection, setInspection] = createSignal<Inspection | null>(null);
  const [inspectErr, setInspectErr] = createSignal<string>("");

  const [splitting, setSplitting] = createSignal(false);
  const [splitDone, setSplitDone] = createSignal<SplitResult | null>(null);
  // The editable plan. Seeded from the detector, then owned by the operator —
  // the detector's count is a floor (the Compendium's own contents list omits an
  // instrument the text contains), so a proposal that cannot be corrected would
  // force a choice between an incomplete split and none at all.
  const [plan, setPlan] = createSignal<PlanRow[]>([]);

  function seedPlan(ins: Inspection) {
    setPlan(
      ins.segments.map((s) => ({
        start: s.start,
        end: s.end,
        title: s.title,
        include: s.is_instrument,
        evidence: s.evidence,
        restart: s.restart,
        first_line: s.first_line,
      })),
    );
  }

  const included = () =>
    plan()
      .filter((r) => r.include)
      .slice()
      .sort((a, b) => a.start - b.start);

  /// Every problem that would make the split lose or duplicate text. Mirrors the
  /// server's validation so the operator sees it while editing rather than as a
  /// rejection after committing.
  const planProblems = createMemo<string[]>(() => {
    const ins = inspection();
    if (!ins) return [];
    const rows = included();
    const out: string[] = [];
    if (rows.length === 0) out.push("No instruments selected.");
    let prevEnd = 0;
    let prevTitle = "";
    for (const r of rows) {
      const name = r.title.trim() || "(unnamed)";
      if (!r.title.trim()) out.push(`An included instrument has no name — the name is what it gets cited by.`);
      if (!Number.isFinite(r.start) || !Number.isFinite(r.end) || r.start < 1 || r.end < 1)
        out.push(`${name}: pages must be 1 or greater.`);
      else if (r.start > r.end) out.push(`${name}: starts on p.${r.start} but ends on p.${r.end}.`);
      else if (r.end > ins.pages)
        out.push(`${name}: p.${r.end} is past the end of the document (${ins.pages} pages).`);
      else if (r.start <= prevEnd)
        out.push(
          `${name} starts on p.${r.start} but "${prevTitle}" runs to p.${prevEnd} — ranges must not overlap.`,
        );
      if (r.end >= r.start) {
        prevEnd = Math.max(prevEnd, r.end);
        prevTitle = name;
      }
    }
    return out;
  });

  /// Pages no included instrument covers. Not an error — front matter is meant
  /// to be dropped — but it must be visible while editing, because an
  /// accidentally-dropped instrument looks exactly like intentionally-dropped
  /// front matter once it is gone.
  const droppedRanges = createMemo<[number, number][]>(() => {
    const ins = inspection();
    if (!ins) return [];
    const gaps: [number, number][] = [];
    let cursor = 1;
    for (const r of included()) {
      if (r.start > cursor) gaps.push([cursor, r.start - 1]);
      cursor = Math.max(cursor, r.end + 1);
    }
    if (cursor <= ins.pages) gaps.push([cursor, ins.pages]);
    return gaps;
  });

  function updateRow(i: number, patch: Partial<PlanRow>) {
    setPlan((rows) => rows.map((r, j) => (j === i ? { ...r, ...patch } : r)));
  }

  function addRow() {
    const ins = inspection();
    const last = included().at(-1);
    const start = Math.min((last?.end ?? 0) + 1, ins?.pages ?? 1);
    setPlan((rows) => [
      ...rows,
      {
        start,
        end: ins?.pages ?? start,
        title: "",
        include: true,
        evidence: "manual",
        restart: false,
        first_line: "",
      },
    ]);
  }

  async function runInspect() {
    const id = inspectId();
    if (!id) return;
    setInspecting(true);
    setInspectErr("");
    setInspection(null);
    setSplitDone(null);
    try {
      const ins = await request<Inspection>("GET", `/admin/sources/${id}/inspect`);
      setInspection(ins);
      seedPlan(ins);
    } catch (e) {
      setInspectErr(e instanceof Error ? e.message : String(e));
    } finally {
      setInspecting(false);
    }
  }

  /// Carry out the split. Only the segments marked as instruments are kept —
  /// front matter and closing schedules are dropped, which is why the confirm
  /// step spells out exactly which pages will not survive.
  async function runSplit() {
    const ins = inspection();
    if (!ins || planProblems().length > 0) return;
    const pieces = included().map((r) => ({
      start: r.start,
      end: r.end,
      title: r.title.trim(),
    }));

    const dropped = droppedRanges().reduce((n, [a, b]) => n + (b - a + 1), 0);
    const ok = window.confirm(
      `Split "${ins.file_name}" into ${pieces.length} separate documents?\n\n` +
        `${dropped} of ${ins.pages} pages belong to no instrument and will NOT be ` +
        `indexed.\n\n` +
        `The original stays on disk but is removed from the corpus, and each ` +
        `instrument is indexed under its own name. This cannot be undone ` +
        `automatically.`,
    );
    if (!ok) return;

    setSplitting(true);
    setInspectErr("");
    try {
      const res = await request<SplitResult>("POST", `/admin/sources/${inspectId()}/split`, {
        pieces,
      });
      setSplitDone(res);
      setInspection(null);
      await loadChunkDocs();
    } catch (e) {
      setInspectErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSplitting(false);
    }
  }

  // Which console tab is showing.
  const [activeTab, setActiveTab] = createSignal<AdminTab>("users");

  // Derived-layers pipeline: build summaries / graph / structure. Which action
  // is running (null = idle), and the last result message. `pipelineScope`
  // reuses the chunk-export document picker ("" = all documents).
  const [pipelineBusy, setPipelineBusy] = createSignal<string | null>(null);
  const [pipelineMsg, setPipelineMsg] = createSignal<string>("");
  // Per-layer "pending" status (which docs still need each derived layer), so the
  // admin sees what to build. Refreshed on mount, after firing, and (while a build
  // runs) on a poll so documents visibly drop off their list as they complete.
  const [derived, setDerived] = createSignal<DerivedStatus | null>(null);
  const [derivedWatching, setDerivedWatching] = createSignal(false);
  const [dupes, setDupes] = createSignal<DuplicateReport | null>(null);
  const [dupesBusy, setDupesBusy] = createSignal(false);
  let derivedPoll: number | undefined;

  const loadDerived = async () => {
    try {
      setDerived(await request<DerivedStatus>("GET", "/admin/derived_status"));
    } catch {
      /* non-fatal: leave the last status showing */
    }
  };

  // Duplicate review scans the whole corpus and is read-only, so it is fetched
  // on demand rather than polled alongside the pending lists.
  const loadDupes = async () => {
    setDupesBusy(true);
    try {
      setDupes(await request<DuplicateReport>("GET", "/admin/duplicates"));
    } catch {
      /* non-fatal: leave the last report showing */
    } finally {
      setDupesBusy(false);
    }
  };

  // Refetch whenever the tab regains focus.
  //
  // `loadDerived` otherwise runs on mount and, while a build THIS tab started is
  // running, on a poll. A build finished any other way — another admin, a direct
  // API call, a server-side job — never reaches this tab, so the panel can sit on
  // hours-old state and report work as pending long after it completed. That
  // happened on 2026-07-27 and cost real time chasing a phantom.
  //
  // Cheap: one small GET when the operator comes back to the tab.
  const refreshDerivedOnFocus = () => {
    if (document.visibilityState === "visible") void loadDerived();
  };

  // Poll the pending status every 5s so the lists shrink live while a background
  // build (summaries / structure) runs. Stops when nothing is pending or after a
  // 20-minute cap (whichever first). Idempotent — a second call is a no-op.
  const watchDerived = () => {
    if (derivedPoll) return;
    setDerivedWatching(true);
    let ticks = 0;
    derivedPoll = window.setInterval(async () => {
      ticks += 1;
      await loadDerived();
      const d = derived();
      const empty =
        !!d && d.summaries_pending.length === 0 && d.structure_pending.length === 0;
      if (ticks >= 240 || empty) {
        window.clearInterval(derivedPoll);
        derivedPoll = undefined;
        setDerivedWatching(false);
      }
    }, 5000);
  };

  // POST a pipeline endpoint, surfacing the JSON result (or error) in-line.
  // `build_summaries` accepts an optional { source_id } to scope to one doc.
  const runPipeline = async (
    action: string,
    path: string,
    body?: Record<string, unknown>,
    confirmMsg?: string,
  ) => {
    if (confirmMsg && !confirm(confirmMsg)) return false;
    setPipelineBusy(action);
    setPipelineMsg("");
    try {
      const res = await request<Record<string, unknown>>("POST", path, body);
      const status = String(res.status ?? "done");
      // Compact one-line summary of whatever stats the endpoint returned.
      const extra = Object.entries(res)
        .filter(([k]) => k !== "status")
        .map(([k, v]) => `${k}=${v}`)
        .join(" · ");
      setPipelineMsg(`${action}: ${status}${extra ? " — " + extra : ""}`);
      return true;
    } catch (e: any) {
      setPipelineMsg(`${action} failed: ${e?.message ?? e}`);
      return false;
    } finally {
      setPipelineBusy(null);
      void loadDerived();
    }
  };

  const buildSummaries = async () => {
    const id = chunkDocId();
    const scope = id
      ? chunkDocs().find((d) => d.source_id === id)?.file_name ?? "1 document"
      : "ALL documents";
    const fired = await runPipeline(
      "Build summaries",
      "/build_summaries",
      id ? { source_id: id } : {},
      `Build whole-document summaries for ${scope}?\n\nThis runs one model pass per document (minutes for the whole corpus) in the background. Continue?`,
    );
    if (fired) watchDerived();
  };

  // Build the summary for ONE specific document — fired from its row in the
  // "Build summaries — pending" list, so the admin picks the exact document
  // instead of going through the chunk-export dropdown.
  const buildOneSummary = async (sourceId: string, fileName: string) => {
    const fired = await runPipeline(
      "Build summaries",
      "/build_summaries",
      { source_id: sourceId },
      `Build the whole-document summary for ${fileName}?\n\nRuns a model pass in the background.`,
    );
    if (fired) watchDerived();
  };

  const rebuildGraph = () =>
    void runPipeline("Rebuild graph", "/rebuild_graph", undefined,
      "Regenerate the entire legal graph overlay (intra + cross-document) from existing data? Deterministic, no re-index.");
  const linkGraph = () =>
    void runPipeline("Link graph", "/link_graph");
  const reindexStructure = async () => {
    const fired = await runPipeline("Re-index structure", "/reindex_structure", undefined,
      "Re-run structure extraction for every document below the current parser version? Re-embedding is slow (minutes) and runs in the background.");
    if (fired) watchDerived();
  };

  // Inference engine: local GPU (vLLM) ⇄ hosted API (Groq / Google / OpenRouter / …).
  // The API key is write-only (the server never returns it; `hasKey` reflects
  // whether one is set). Selecting a provider auto-fills its base URL + API path
  // prefix and swaps the model list to that provider's catalogue.
  //
  // The catalogue is fetched from `GET /admin/providers` rather than hardcoded
  // here: model ids retire on a ~12-month cycle (see
  // docs/llm-provider-api-analysis-2026-07.pdf §6.3), so a list baked into this
  // bundle would be partly dead within a year and would need a full frontend
  // rebuild to correct. The server ships a bundled default and honours an
  // operator override file.
  const OTHER = "__other__"; // sentinel: model typed by hand rather than picked
  const CUSTOM = "custom"; // the free-form OpenAI-compatible provider

  const [providers, setProviders] = createSignal<CatalogProvider[]>([]);
  const [catalogVersion, setCatalogVersion] = createSignal("");
  const [contextWarnTokens, setContextWarnTokens] = createSignal(16000);
  const [infEngine, setInfEngine] = createSignal<"vllm" | "remote">("vllm");
  // F027 — deployment-wide My Projects gate (FR-037). Read on mount below.
  const [myProjectsEnabled, setMyProjectsEnabled] = createSignal(false);
  /** Transient confirmation under a Features switch — evidence the flip took. */
  const [featureSaved, setFeatureSaved] = createSignal<string | null>(null);
  const [infProvider, setInfProvider] = createSignal<string>("groq");
  const [infModel, setInfModel] = createSignal("llama-3.3-70b-versatile");
  const [infModelPick, setInfModelPick] = createSignal("llama-3.3-70b-versatile"); // list value (or OTHER)
  const [infBaseUrl, setInfBaseUrl] = createSignal("https://api.groq.com/openai");
  const [infPrefix, setInfPrefix] = createSignal("/v1");
  const [infEndpoint, setInfEndpoint] = createSignal<"chat" | "completions">("chat");
  const [infHasKey, setInfHasKey] = createSignal(false);
  const [infKey, setInfKey] = createSignal(""); // new key to submit (never populated from server)
  const [infSaving, setInfSaving] = createSignal(false);
  const [infMsg, setInfMsg] = createSignal("");
  // Model list filter: show everything, only free models, or only paid ones.
  const [modelFilter, setModelFilter] = createSignal<"all" | "free" | "paid">("all");
  // Live key+model probe (`POST /admin/inference/validate`).
  const [probing, setProbing] = createSignal(false);
  const [probe, setProbe] = createSignal<ProbeResult | null>(null);

  const providerSpec = (id: string): CatalogProvider | undefined =>
    providers().find((p) => p.id === id);

  // False when the server predates `/admin/providers` (or the fetch failed). The
  // form must stay fully usable in that case: fall back to hand-entered URL,
  // prefix and model id rather than presenting disabled fields with no picker.
  const catalogReady = () => providers().length > 0;
  // Fields the catalogue normally manages are editable under Custom, and also
  // whenever there is no catalogue to manage them.
  const manualConn = () => !catalogReady() || infProvider() === CUSTOM;

  const currentProvider = () => providerSpec(infProvider());
  const currentModels = () => currentProvider()?.models ?? [];
  const visibleModels = () =>
    currentModels().filter((m) => modelFilter() === "all" || m.tier === modelFilter());
  const currentModelSpec = () => currentModels().find((m) => m.id === infModel());

  // Typical Noviq RAG prompt: ~8K tokens of retrieved passages plus the system and
  // grounding prefix. Used to derive the *effective* request rate (below) and to
  // flag models whose context cannot hold it.
  const RAG_PROMPT_TOKENS = 8000;

  // Sustainable requests/minute at our real prompt size.
  //
  // Free tiers advertise requests/day, which flatters them badly for RAG: our
  // traffic is token-heavy per request, so the tokens/minute cap binds long before
  // the daily one. Groq's headline 14,400/day looks like ~10/min of headroom, but
  // at 6,000 TPM and an 8K prompt the real sustained rate is ~0.75/min. Showing
  // the headline would generate 429s users were told could not happen.
  const effectiveRpm = (m: CatalogModel): number | null => {
    if (!m.tpm) return null;
    const fromTokens = m.tpm / RAG_PROMPT_TOKENS;
    return m.rpm ? Math.min(fromTokens, m.rpm) : fromTokens;
  };

  const fmtRate = (n: number) =>
    n >= 10 ? `${Math.round(n)}/min` : n >= 1 ? `${n.toFixed(1)}/min` : `${(n * 60).toFixed(0)}/hour`;

  const fmtCtx = (n?: number | null) =>
    !n ? "—" : n >= 1_000_000 ? `${n / 1_000_000}M` : `${Math.round(n / 1000)}K`;

  const fmtPrice = (m: CatalogModel) =>
    m.input_price == null && m.output_price == null
      ? null
      : `$${(m.input_price ?? 0).toFixed(2)} / $${(m.output_price ?? 0).toFixed(2)} per 1M`;

  // Days until a published retirement date, or null when none is published.
  const daysUntilShutdown = (m: CatalogModel): number | null => {
    if (!m.shutdown_date) return null;
    const t = Date.parse(`${m.shutdown_date}T00:00:00Z`);
    if (Number.isNaN(t)) return null;
    return Math.ceil((t - Date.now()) / 86_400_000);
  };

  // The tag set. Free/paid alone would hide three conditions that are outright
  // disqualifying for a legal corpus — training on submitted data, non-commercial
  // licensing, and a context window too small to hold retrieved passages — so
  // each gets its own visible chip rather than a footnote.
  interface Tag {
    label: string;
    cls: string;
    title: string;
  }
  const modelTags = (m: CatalogModel, p: CatalogProvider): Tag[] => {
    const tags: Tag[] = [];
    tags.push(
      m.tier === "free"
        ? { label: "FREE", cls: "bg-emerald-100 text-emerald-800 border-emerald-300", title: p.free_tier || "Free tier" }
        : { label: "PAID", cls: "bg-sky-100 text-sky-800 border-sky-300", title: "Metered per token" },
    );
    if (p.trains_on_data) {
      tags.push({
        label: "TRAINS ON YOUR DATA",
        cls: "bg-rose-100 text-rose-800 border-rose-300",
        title: "This provider may train on submitted prompts on the tier you are likely to land on. For a non-public corpus this is a policy decision.",
      });
    }
    if (p.non_commercial_only) {
      tags.push({
        label: "NON-COMMERCIAL",
        cls: "bg-rose-100 text-rose-800 border-rose-300",
        title: "The free key forbids commercial use — it cannot legally back a customer deployment.",
      });
    }
    if (m.context != null && m.context < contextWarnTokens()) {
      tags.push({
        label: `CONTEXT ${fmtCtx(m.context)}`,
        cls: "bg-amber-100 text-amber-900 border-amber-300",
        title: `Below a typical Noviq RAG prompt (~${RAG_PROMPT_TOKENS / 1000}K tokens). Retrieved passages will be truncated, which is how ungrounded answers get produced.`,
      });
    }
    const d = daysUntilShutdown(m);
    if (d != null && d <= 90) {
      tags.push({
        label: d <= 0 ? "RETIRED" : `RETIRING ${d}d`,
        cls: "bg-rose-100 text-rose-800 border-rose-300",
        title: `Provider has published a shutdown date of ${m.shutdown_date}.`,
      });
    } else if (d != null) {
      tags.push({
        label: `ends ${m.shutdown_date}`,
        cls: "bg-noviq-100 text-noviq-600 border-noviq-line",
        title: `Provider has published a shutdown date of ${m.shutdown_date}.`,
      });
    }
    return tags;
  };

  // Infer which provider a stored base URL belongs to (for the load path).
  const providerFromBaseUrl = (url: string): string => {
    const u = (url || "").replace(/\/+$/, "");
    const hit = providers().find((p) => p.id !== CUSTOM && p.base_url === u);
    return hit?.id ?? CUSTOM;
  };

  // Point the model selection at `model`: pick it if the provider lists it, else
  // fall to the "Other…" free-text path so arbitrary ids round-trip.
  const syncModelPick = (provider: string, model: string) => {
    setInfModel(model);
    const known = providerSpec(provider)?.models.some((m) => m.id === model) ?? false;
    setInfModelPick(known ? model : OTHER);
  };

  // Switch provider: adopt its base URL + prefix + endpoint and default to its
  // first model (custom keeps whatever is typed). Called from the provider picker
  // only — the load path sets fields directly so it never clobbers stored config.
  const selectProvider = (id: string) => {
    setInfProvider(id);
    setProbe(null);
    setModelFilter("all");
    const spec = providerSpec(id);
    if (spec && id !== CUSTOM) {
      setInfBaseUrl(spec.base_url);
      setInfPrefix(spec.prefix);
      setInfEndpoint(spec.endpoint === "completions" ? "completions" : "chat");
      syncModelPick(id, spec.models[0]?.id ?? "");
    }
  };

  // Model row click: OTHER opens the free-text box; else adopt the pick.
  const selectModel = (val: string) => {
    setInfModelPick(val);
    setProbe(null);
    if (val !== OTHER) setInfModel(val);
  };

  const loadProviders = async () => {
    try {
      const r = await request<{
        version: string;
        context_warn_tokens: number;
        providers: CatalogProvider[];
      }>("GET", "/admin/providers");
      setProviders(r.providers ?? []);
      setCatalogVersion(r.version ?? "");
      if (r.context_warn_tokens) setContextWarnTokens(r.context_warn_tokens);
    } catch {
      /* older server without the catalogue — the picker falls back to free text */
    }
  };

  const loadInference = async () => {
    try {
      const r = await request<{
        engine: string;
        remote_base_url: string;
        remote_model: string;
        remote_endpoint: string;
        remote_prefix: string;
        has_api_key: boolean;
      }>("GET", "/admin/inference");
      setInfEngine(r.engine === "remote" ? "remote" : "vllm");
      if (r.remote_base_url) setInfBaseUrl(r.remote_base_url);
      // `remote_prefix` may be absent (older server) or "" (Gemini) — treat only
      // `undefined` as "unknown" so an intentional empty prefix survives.
      setInfPrefix(r.remote_prefix === undefined ? "/v1" : r.remote_prefix);
      const provider = providerFromBaseUrl(r.remote_base_url);
      setInfProvider(provider);
      setInfEndpoint(r.remote_endpoint === "completions" ? "completions" : "chat");
      if (r.remote_model) syncModelPick(provider, r.remote_model);
      setInfHasKey(r.has_api_key);
    } catch {
      /* older server without the endpoint — leave defaults */
    }
  };

  // Probe the live provider with the pending key + selected model, without saving.
  //
  // Validating the key alone is not enough: entitlement is per model. One valid
  // Google key returns 200 on Flash and 404 on Pro, so "the key works" is not an
  // answer to "will this selection work".
  const testInference = async () => {
    setProbing(true);
    setProbe(null);
    try {
      const body: Record<string, string> = {
        model: infModel().trim(),
        base_url: infBaseUrl().trim(),
        prefix: infPrefix().trim(),
      };
      if (infKey().trim()) body.api_key = infKey().trim();
      setProbe(await request<ProbeResult>("POST", "/admin/inference/validate", body));
    } catch (e: any) {
      setProbe({ ok: false, status: 0, reason: "error", message: e?.message ?? "probe failed" });
    } finally {
      setProbing(false);
    }
  };

  const saveInference = async () => {
    setInfSaving(true);
    setInfMsg("");
    try {
      const body: Record<string, string> = {
        engine: infEngine(),
        model: infModel().trim(),
        base_url: infBaseUrl().trim(),
        endpoint: infEndpoint(),
        // Always send prefix (empty string is meaningful — Gemini has no `/v1`).
        prefix: infPrefix().trim(),
      };
      if (infKey().trim()) body.api_key = infKey().trim();
      const res = await request<{ ok: boolean; engine: string }>("POST", "/admin/inference", body);
      setInfKey("");
      await loadInference();
      setInfMsg(
        res.engine === "remote"
          ? "Saved — chat now uses the hosted API."
          : "Saved — chat now uses the local GPU.",
      );
    } catch (e: any) {
      setInfMsg(e?.message ?? "failed to save inference settings");
    } finally {
      setInfSaving(false);
    }
  };

  const toggleActivity = async (u: UserRow) => {
    if (expanded() === u.id) {
      setExpanded(null);
      return;
    }
    setExpanded(u.id);
    if (!activity()[u.id]) {
      try {
        const rows = await request<QueryLogRow[]>("GET", `/admin/users/${u.id}/activity`);
        setActivity((prev) => ({ ...prev, [u.id]: rows }));
      } catch (e: any) {
        setError(e?.message ?? "failed to load activity");
      }
    }
  };

  const exportUser = async (u: UserRow, format: "json" | "csv") => {
    try {
      await downloadAuthed(
        `/admin/users/${u.id}/export?format=${format}`,
        `queries-${u.username}.${format}`,
      );
    } catch (e: any) {
      setError(e?.message ?? "failed to export");
    }
  };

  const loadChunkDocs = async () => {
    try {
      const rows = await request<ChunkDoc[]>("GET", "/admin/chunks/documents");
      setChunkDocs(rows);
    } catch (e: any) {
      setError(e?.message ?? "failed to load documents");
    }
  };

  // Download chunks as a CSV (opens in Excel). No selection = the whole corpus.
  const exportChunks = async () => {
    const id = chunkDocId();
    try {
      if (id) {
        await downloadAuthed(`/admin/chunks/export?source_id=${encodeURIComponent(id)}`, `chunks.csv`);
      } else {
        await downloadAuthed(`/admin/chunks/export`, `all-chunks.csv`);
      }
    } catch (e: any) {
      setError(e?.message ?? "failed to export chunks");
    }
  };

  // Kick off a one-shot GPU re-embed of the whole corpus. The server just drops a
  // trigger file; a systemd path-unit runs the orchestration (stop vLLM → GPU ONNX
  // Runtime → reindex → restore). Chat shows the "updating" banner meanwhile.
  const gpuReindex = async () => {
    if (
      !confirm(
        "Re-embed the whole corpus on the GPU?\n\nChat pauses for ~7–10 minutes " +
          "while this runs (the app shows an \"updating\" banner and comes back " +
          "automatically). Continue?",
      )
    )
      return;
    setReindexing(true);
    setReindexMsg("");
    try {
      const res = await request<{ ok: boolean; message: string }>("POST", "/admin/gpu_reindex");
      setReindexMsg(res.message);
    } catch (e: any) {
      setReindexMsg(e?.message ?? "failed to start GPU re-embed");
    } finally {
      setReindexing(false);
    }
  };

  const copyPassword = async (pw: string) => {
    try {
      await navigator.clipboard.writeText(pw);
    } catch {
      // Clipboard API needs a secure context; fall back to a hidden textarea.
      const ta = document.createElement("textarea");
      ta.value = pw;
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand("copy"); } catch { /* ignore */ }
      document.body.removeChild(ta);
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const loadUsers = async () => {
    try {
      const rows = await request<UserRow[]>("GET", "/admin/users");
      setUsers(rows);
      setError("");
    } catch (e: any) {
      setError(e?.message ?? "failed to load users");
    }
  };

  // Admin-gated sharing (2026-07-28): the pending share-request review queue.
  interface ShareReqRow {
    id: string; sourceId: string; requesterUsername: string | null;
    fileName: string | null; displayName: string | null; createdAt: number;
  }
  const [shareReqs, setShareReqs] = createSignal<ShareReqRow[]>([]);
  const [shareReqMsg, setShareReqMsg] = createSignal("");
  const loadShareReqs = async () => {
    try {
      setShareReqs(await request<ShareReqRow[]>("GET", "/admin/share_requests"));
    } catch {
      /* older server without the endpoint */
    }
  };
  const decideShareReq = async (r: ShareReqRow, approve: boolean) => {
    setShareReqMsg("");
    try {
      await request("POST", `/admin/share_requests/${r.id}/decide`, { approve });
      setShareReqMsg(
        `${approve ? "Approved" : "Rejected"}: ${r.displayName ?? r.fileName ?? r.sourceId}`,
      );
      await loadShareReqs();
    } catch (e: any) {
      setShareReqMsg(e?.message ?? "decision failed");
    }
  };

  // F021 (FR-010): per-user document storage usage.
  interface UsageRow { userId: string; username: string; usedBytes: number; usedFiles: number }
  const [usageRows, setUsageRows] = createSignal<UsageRow[]>([]);
  const loadUsage = async () => {
    try {
      setUsageRows(await request<UsageRow[]>("GET", "/admin/usage"));
    } catch {
      /* older server without the endpoint */
    }
  };

  // Track 3 — query-debug panel: recent traces + a selected trace's full log.
  const [traces, setTraces] = createSignal<TraceRow[]>([]);
  const [tracesMsg, setTracesMsg] = createSignal("");
  const [traceBody, setTraceBody] = createSignal<string | null>(null);
  const [traceSel, setTraceSel] = createSignal<TraceRow | null>(null);
  const loadTraces = async () => {
    setTracesMsg("");
    try {
      setTraces(await request<TraceRow[]>("GET", "/query_traces?limit=100"));
    } catch (e: any) {
      setTracesMsg(e?.message ?? "failed to load traces");
    }
  };
  const openTrace = async (t: TraceRow) => {
    setTraceSel(t);
    setTraceBody(null);
    try {
      setTraceBody(await request<string>("GET", `/query_traces/${encodeURIComponent(t.id)}`));
    } catch (e: any) {
      setTraceBody(`Failed to load trace: ${e?.message ?? e}`);
    }
  };

  // ── Audit log (2026-08-13) ────────────────────────────────────────────────
  // Filters live in signals rather than the URL: the console is a single page
  // and an admin refining a date range should not be building history entries.
  const AUDIT_PAGE = 100;
  const [auditRows, setAuditRows] = createSignal<AuditRow[]>([]);
  const [auditTotal, setAuditTotal] = createSignal(0);
  const [auditOffset, setAuditOffset] = createSignal(0);
  const [auditActions, setAuditActions] = createSignal<string[]>([]);
  const [auditMsg, setAuditMsg] = createSignal("");
  const [auditBusy, setAuditBusy] = createSignal(false);
  const [auditLoaded, setAuditLoaded] = createSignal(false);
  const [afFrom, setAfFrom] = createSignal("");
  const [afTo, setAfTo] = createSignal("");
  const [afUser, setAfUser] = createSignal("");
  const [afAction, setAfAction] = createSignal("");
  const [afSearch, setAfSearch] = createSignal("");
  const [auditSel, setAuditSel] = createSignal<AuditRow | null>(null);

  /** The filter as a query string — ONE builder, shared by the table and both
   *  downloads, so an export can never cover a different span than the screen. */
  const auditParams = (extra?: Record<string, string>) => {
    const p = new URLSearchParams();
    if (afFrom()) p.set("from", afFrom());
    if (afTo()) p.set("to", afTo());
    if (afUser()) p.set("user_id", afUser());
    if (afAction()) p.set("actions", afAction());
    if (afSearch().trim()) p.set("search", afSearch().trim());
    for (const [k, v] of Object.entries(extra ?? {})) p.set(k, v);
    return p.toString();
  };

  const loadAudit = async (offset = 0) => {
    setAuditBusy(true);
    setAuditMsg("");
    try {
      const qs = auditParams({ limit: String(AUDIT_PAGE), offset: String(offset) });
      const page = await request<AuditPage>("GET", `/admin/audit?${qs}`);
      setAuditRows(page.rows);
      setAuditTotal(page.total);
      setAuditOffset(page.offset);
      setAuditActions(page.actions);
      setAuditLoaded(true);
    } catch (e: any) {
      // The server rejects a malformed or inverted date range rather than
      // silently returning everything — surface exactly what it said.
      setAuditMsg(e?.message ?? "failed to load the audit log");
    } finally {
      setAuditBusy(false);
    }
  };

  /** Download the CURRENT filter. `whole` ignores the date boxes for the
   *  "everything ever recorded" case, which is a separate, deliberate action. */
  const downloadAudit = async (format: "csv" | "json", whole = false) => {
    setAuditMsg("");
    const p = new URLSearchParams(auditParams({ format }));
    if (whole) {
      p.delete("from");
      p.delete("to");
    }
    const span = whole ? "all" : [afFrom() || "start", afTo() || "now"].join("_to_");
    try {
      await downloadAuthed(`/admin/audit/export?${p.toString()}`, `noviq-audit-${span}.${format}`);
    } catch (e: any) {
      setAuditMsg(e?.message ?? "download failed");
    }
  };

  /** Fill the date boxes with a common span, then reload. */
  const auditQuickRange = (days: number) => {
    const iso = (d: Date) => d.toISOString().slice(0, 10);
    const today = new Date();
    const start = new Date(today);
    start.setUTCDate(start.getUTCDate() - (days - 1));
    setAfFrom(iso(start));
    setAfTo(iso(today));
    void loadAudit(0);
  };

  const auditClear = () => {
    setAfFrom("");
    setAfTo("");
    setAfUser("");
    setAfAction("");
    setAfSearch("");
    void loadAudit(0);
  };

  /** Switch tabs, loading the audit page the first time it is opened — it is a
   *  heavier query than the rest of the console and should not run on every
   *  admin page load. */
  // ── Inbox: quote requests from the landing page (migration 028) ──────────
  const [quotes, setQuotes] = createSignal<QuoteRow[]>([]);
  const [quotesUnread, setQuotesUnread] = createSignal(0);
  const [quotesLoading, setQuotesLoading] = createSignal(false);
  const [quotesError, setQuotesError] = createSignal<string | null>(null);
  const [quotesLoaded, setQuotesLoaded] = createSignal(false);
  /** "" = the inbox proper (everything except archived). */
  const [quoteFilter, setQuoteFilter] = createSignal("");
  /** Per-row note drafts, keyed by request id. */
  const [quoteNotes, setQuoteNotes] = createSignal<Record<string, string>>({});
  /** Id of the row whose status write is in flight, so only its buttons lock. */
  const [quoteBusy, setQuoteBusy] = createSignal<string | null>(null);

  const loadQuotes = async () => {
    setQuotesLoading(true);
    setQuotesError(null);
    try {
      const qs = quoteFilter() ? `?status=${encodeURIComponent(quoteFilter())}` : "";
      const res = await request<{ requests: QuoteRow[]; unread: number }>(
        "GET",
        `/admin/quotes${qs}`,
      );
      setQuotes(res.requests ?? []);
      // ⚠️ `unread` counts status='new' across the WHOLE table, not the filtered
      // page — the badge must not drop to zero just because you filtered.
      setQuotesUnread(res.unread ?? 0);
      setQuotesLoaded(true);
    } catch (e) {
      setQuotesError(e instanceof Error ? e.message : String(e));
    } finally {
      setQuotesLoading(false);
    }
  };

  const setQuoteStatus = async (id: string, status: QuoteRow["status"]) => {
    setQuoteBusy(id);
    setQuotesError(null);
    try {
      const note = quoteNotes()[id]?.trim();
      await request("POST", `/admin/quotes/${id}/status`, {
        status,
        // Omit rather than send "" — the server treats null as "leave the
        // existing note alone", so an empty box must not wipe what is there.
        note: note ? note : undefined,
      });
      await loadQuotes();
    } catch (e) {
      setQuotesError(e instanceof Error ? e.message : String(e));
    } finally {
      setQuoteBusy(null);
    }
  };

  // ── Trial sign-ups (migration 029) — shown under the same tab as quote
  //    requests: both are people who might become customers.
  const [trials, setTrials] = createSignal<TrialRow[]>([]);
  const loadTrials = async () => {
    try {
      const res = await request<{ trials: TrialRow[] }>("GET", "/admin/trials");
      setTrials(res.trials ?? []);
    } catch (e) {
      setQuotesError(e instanceof Error ? e.message : String(e));
    }
  };
  const revokeTrial = async (id: string) => {
    try {
      await request("POST", `/admin/trials/${id}/revoke`);
      await loadTrials();
    } catch (e) {
      setQuotesError(e instanceof Error ? e.message : String(e));
    }
  };

  const pickTab = (id: AdminTab) => {
    setActiveTab(id);
    if (id === "audit" && !auditLoaded()) void loadAudit(0);
    if (id === "inbox" && !quotesLoaded()) { void loadQuotes(); void loadTrials(); }
  };

  const fmtAuditTime = (ms: number) =>
    new Date(ms).toISOString().replace("T", " ").replace(/\..*/, "");

  onMount(async () => {
    try {
      const me = await request<MeResp>("GET", "/me");
      if (me.role === "admin") {
        setIsAdmin(true);
        // F027 — reflect the real server state rather than assuming off, so the
        // checkbox never disagrees with what the backend is enforcing.
        // Read through the SHARED store so the checkbox, the main nav and the
        // server can never disagree — three copies of one flag is how the
        // "needs a browser refresh" bug happened in the first place.
        setMyProjectsEnabled(await refreshProjectsEnabled());
        await loadUsers();
        await loadChunkDocs();
        await loadDerived();
        await loadUsage();
        await loadShareReqs();
        // Catalogue first: the inference load path maps a stored base URL back
        // onto a provider and validates the stored model against its list, so it
        // needs the catalogue already in hand.
        await loadProviders();
        await loadInference();
        await loadTraces();
        document.addEventListener("visibilitychange", refreshDerivedOnFocus);
        window.addEventListener("focus", refreshDerivedOnFocus);
      }
    } catch {
      /* not authenticated / not web → leave isAdmin false */
    } finally {
      setChecked(true);
    }
  });

  onCleanup(() => {
    document.removeEventListener("visibilitychange", refreshDerivedOnFocus);
    window.removeEventListener("focus", refreshDerivedOnFocus);
  });

  const createUser = async (e: Event) => {
    e.preventDefault();
    const username = newUsername().trim();
    if (!username) return;
    try {
      const resp = await request<{ username: string; temp_password: string }>(
        "POST",
        "/admin/users",
        { username, role: newRole() },
      );
      setTempPassword({ username: resp.username, password: resp.temp_password });
      setNewUsername("");
      setNewRole("user");
      await loadUsers();
    } catch (e: any) {
      setError(e?.message ?? "failed to create user");
    }
  };

  const toggleDisabled = async (u: UserRow) => {
    const action = u.disabled_at ? "enable" : "disable";
    try {
      await request("POST", `/admin/users/${u.id}/${action}`);
      await loadUsers();
    } catch (e: any) {
      setError(e?.message ?? `failed to ${action} user`);
    }
  };

  return (
    <div class="p-3 sm:p-6 overflow-auto h-full bg-noviq-paper text-noviq-ink">
      <div class="max-w-6xl mx-auto">
      <h1 class="text-lg font-semibold text-noviq-ink mb-4">Admin Console</h1>

      <Show when={checked()} fallback={<p class="text-sm text-noviq-500">Loading…</p>}>
        <Show
          when={isAdmin()}
          fallback={<p class="text-sm text-red-500">Not authorized — admin access required.</p>}
        >
          {/* Tab bar — scrolls sideways rather than wrapping or overflowing:
              the tabs are a single row of short labels, and wrapping them would
              push the panel content down a line on every narrow screen. */}
          <div class="flex gap-1 mb-6 border-b border-noviq-line overflow-x-auto">
            <For each={ADMIN_TABS}>
              {(t) => (
                <button
                  class={`px-3 py-2 text-sm -mb-px border-b-2 whitespace-nowrap flex-shrink-0 transition-colors ${
                    activeTab() === t.id
                      ? "border-noviq-800 text-noviq-800 font-semibold"
                      : "border-transparent text-noviq-500 hover:text-noviq-ink"
                  }`}
                  onClick={() => pickTab(t.id)}
                >
                  {t.label}
                  {/* Unread leads are the one thing on this console that decays
                      if nobody looks, so the count rides the tab itself. */}
                  <Show when={t.id === "inbox" && quotesUnread() > 0}>
                    <span class="ml-1.5 rounded-full bg-noviq-700 px-1.5 py-px text-[10px] font-semibold tabular-nums text-white">
                      {quotesUnread()}
                    </span>
                  </Show>
                </button>
              )}
            </For>
          </div>

          <Show when={error()}>
            <p class="text-sm text-red-500 mb-3">{error()}</p>
          </Show>

          {/* ══ INBOX TAB ══ — quote requests from the public landing page.
              ⚠️ Everything rendered below is anonymous public input. It is shown
              as TEXT only; the single place any of it becomes a URL is the
              `mailto:` built from `email`. */}
          <Show when={activeTab() === "inbox"}>
            <div class="mb-4 flex flex-wrap items-center gap-3">
              <div>
                <h2 class="text-lg font-semibold text-noviq-ink">Quote requests</h2>
                <p class="mt-0.5 text-xs text-noviq-500">
                  Organisations who asked to be contacted from noviq.pk.
                </p>
              </div>
              <div class="ml-auto flex items-center gap-2">
                <select
                  class="rounded-lg border border-noviq-line bg-white px-2.5 py-1.5 text-xs text-noviq-ink"
                  value={quoteFilter()}
                  onChange={(e) => { setQuoteFilter(e.currentTarget.value); void loadQuotes(); }}
                >
                  <option value="">Inbox (not archived)</option>
                  <option value="new">New</option>
                  <option value="read">Read</option>
                  <option value="replied">Replied</option>
                  <option value="archived">Archived</option>
                </select>
                <button
                  class="rounded-lg border border-noviq-line px-2.5 py-1.5 text-xs hover:bg-noviq-50 disabled:opacity-50"
                  onClick={() => void loadQuotes()}
                  disabled={quotesLoading()}
                >
                  {quotesLoading() ? "Loading…" : "Refresh"}
                </button>
              </div>
            </div>

            <Show when={quotesError()}>
              <p class="mb-3 rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                {quotesError()}
              </p>
            </Show>

            <Show
              when={quotes().length > 0}
              fallback={
                <p class="rounded-lg border border-noviq-line bg-white px-4 py-8 text-center text-sm text-noviq-500">
                  {quotesLoading() ? "Loading…" : "No requests here yet."}
                </p>
              }
            >
              <div class="space-y-3">
                <For each={quotes()}>
                  {(q) => (
                    <div
                      class="rounded-xl border bg-white p-4"
                      classList={{
                        "border-noviq-300 ring-1 ring-noviq-200": q.status === "new",
                        "border-noviq-line": q.status !== "new",
                        "opacity-70": q.status === "archived",
                      }}
                    >
                      <div class="flex flex-wrap items-start gap-x-3 gap-y-1">
                        <span class="text-[15px] font-semibold text-noviq-ink">{q.organisation}</span>
                        <span
                          class="rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
                          classList={{
                            "bg-noviq-700 text-white": q.status === "new",
                            "bg-noviq-100 text-noviq-700": q.status === "read",
                            "bg-green-100 text-green-800": q.status === "replied",
                            "bg-noviq-line text-noviq-600": q.status === "archived",
                          }}
                        >
                          {q.status}
                        </span>
                        <Show when={q.sector}>
                          <span class="rounded-full border border-noviq-line px-2 py-0.5 text-[10px] text-noviq-600">
                            {q.sector}
                          </span>
                        </Show>
                        <span class="ml-auto text-[11px] tabular-nums text-noviq-500">
                          {fmtAuditTime(q.created_at)}
                        </span>
                      </div>

                      <div class="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[13px] text-noviq-700">
                        <Show when={q.contact_name}>
                          <span>{q.contact_name}</span>
                        </Show>
                        <a class="font-medium text-noviq-800 underline" href={`mailto:${q.email}`}>
                          {q.email}
                        </a>
                        <Show when={q.phone}>
                          <span>{q.phone}</span>
                        </Show>
                        <Show when={q.org_size}>
                          <span class="text-noviq-600">{q.org_size} users</span>
                        </Show>
                      </div>

                      <Show when={q.message}>
                        <p class="mt-3 whitespace-pre-wrap rounded-lg bg-noviq-50 px-3 py-2.5 text-[13px] leading-relaxed text-noviq-ink">
                          {q.message}
                        </p>
                      </Show>

                      <div class="mt-3 flex flex-wrap items-center gap-2">
                        <input
                          class="min-w-0 flex-1 rounded-lg border border-noviq-line px-2.5 py-1.5 text-xs text-noviq-ink placeholder:text-noviq-400"
                          placeholder={q.admin_note ? `Note: ${q.admin_note}` : "Add a note…"}
                          value={quoteNotes()[q.id] ?? ""}
                          onInput={(e) =>
                            setQuoteNotes({ ...quoteNotes(), [q.id]: e.currentTarget.value })
                          }
                        />
                        <For each={[
                          { s: "read" as const, label: "Mark read" },
                          { s: "replied" as const, label: "Replied" },
                          { s: "archived" as const, label: "Archive" },
                        ]}>
                          {(a) => (
                            <button
                              class="rounded-lg border border-noviq-line px-2.5 py-1.5 text-xs hover:bg-noviq-50 disabled:opacity-40"
                              disabled={quoteBusy() === q.id || q.status === a.s}
                              onClick={() => void setQuoteStatus(q.id, a.s)}
                            >
                              {a.label}
                            </button>
                          )}
                        </For>
                      </div>

                      {/* Says whether the notification email actually went out.
                          Mail is optional and best-effort, so "not emailed" is a
                          normal state, not a failure — but the admin should know
                          which it is before assuming they were told. */}
                      <p class="mt-2 text-[11px] text-noviq-500">
                        {q.emailed_at
                          ? `Notification emailed ${fmtAuditTime(q.emailed_at)}`
                          : "No notification email (mail not configured, or it failed)"}
                      </p>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </Show>

          {/* ══ TRIAL SIGN-UPS ══ — same tab as quote requests: both are people
              who might become customers. Rendered under the requests so an
              unanswered lead stays the first thing on screen. */}
          <Show when={activeTab() === "inbox"}>
            <div class="mt-10">
              <div class="mb-3 flex flex-wrap items-center gap-3">
                <div>
                  <h2 class="text-lg font-semibold text-noviq-ink">
                    Free-trial sign-ups
                    <span class="ml-2 rounded-full bg-noviq-100 px-2 py-0.5 text-[11px] font-medium tabular-nums text-noviq-700">
                      {trials().length}
                    </span>
                  </h2>
                  <p class="mt-0.5 text-xs text-noviq-500">
                    People who tried Noviq from the landing page. The privacy policy says we
                    contact them about the product; it does not permit anything else.
                  </p>
                </div>
                <button
                  class="ml-auto rounded-lg border border-noviq-line px-2.5 py-1.5 text-xs hover:bg-noviq-50"
                  onClick={() => void loadTrials()}
                >
                  Refresh
                </button>
              </div>

              <Show
                when={trials().length > 0}
                fallback={
                  <p class="rounded-lg border border-noviq-line bg-white px-4 py-6 text-center text-sm text-noviq-500">
                    Nobody has started a trial yet.
                  </p>
                }
              >
                <div class="overflow-x-auto rounded-xl border border-noviq-line bg-white">
                  <table class="w-full text-left text-[13px]">
                    <thead class="border-b border-noviq-line text-[11px] uppercase tracking-wide text-noviq-500">
                      <tr>
                        <th class="px-3 py-2 font-semibold">Name</th>
                        <th class="px-3 py-2 font-semibold">Email</th>
                        <th class="px-3 py-2 font-semibold">Started</th>
                        <th class="px-3 py-2 font-semibold">Today</th>
                        <th class="px-3 py-2 font-semibold">Status</th>
                        <th class="px-3 py-2" />
                      </tr>
                    </thead>
                    <tbody>
                      <For each={trials()}>
                        {(t) => (
                          <tr class="border-b border-noviq-line/60 last:border-0">
                            <td class="px-3 py-2 text-noviq-ink">{t.display_name || "—"}</td>
                            <td class="px-3 py-2">
                              <a class="font-medium text-noviq-800 underline" href={`mailto:${t.email}`}>
                                {t.email}
                              </a>
                            </td>
                            <td class="px-3 py-2 tabular-nums text-noviq-600">
                              {fmtAuditTime(t.started_at)}
                            </td>
                            <td class="px-3 py-2 tabular-nums text-noviq-600">{t.used_today} / 3</td>
                            <td class="px-3 py-2">
                              <Show
                                when={!t.revoked_at}
                                fallback={<span class="text-noviq-500">Revoked</span>}
                              >
                                <span class="text-green-700">Active</span>
                              </Show>
                            </td>
                            <td class="px-3 py-2 text-right">
                              <Show when={!t.revoked_at}>
                                <button
                                  class="rounded-lg border border-noviq-line px-2 py-1 text-xs hover:bg-noviq-50"
                                  onClick={() => void revokeTrial(t.user_id)}
                                  title="End this trial now and drop its sessions"
                                >
                                  Revoke
                                </button>
                              </Show>
                            </td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </Show>
            </div>
          </Show>

          {/* ══ USERS TAB ══ */}
          <Show when={activeTab() === "users"}>
          {/* Admin-gated sharing: pending share requests (2026-07-28) */}
          <Show when={shareReqs().length > 0 || shareReqMsg()}>
            <div class="mb-6 p-4 border border-amber-300 bg-amber-50 rounded-lg">
              <div class="flex items-center justify-between mb-2">
                <span class="text-sm font-semibold text-amber-800">
                  Share requests awaiting review ({shareReqs().length})
                </span>
                <button
                  class="text-xs text-amber-700 hover:text-amber-900"
                  onClick={() => void loadShareReqs()}
                >
                  Refresh
                </button>
              </div>
              <Show when={shareReqMsg()}>
                <div class="text-xs text-amber-700 mb-2">{shareReqMsg()}</div>
              </Show>
              <For each={shareReqs()}>
                {(r) => (
                  <div class="flex items-center gap-3 py-1.5 border-t border-amber-200 text-sm">
                    <span class="flex-1 truncate" title={r.fileName ?? undefined}>
                      {r.displayName ?? r.fileName ?? r.sourceId}
                    </span>
                    <span class="text-xs text-amber-700">
                      by {r.requesterUsername ?? "unknown"} ·{" "}
                      {new Date(r.createdAt * 1000).toLocaleDateString()}
                    </span>
                    <button
                      class="text-xs px-2 py-0.5 border border-amber-400 rounded hover:bg-amber-100"
                      onClick={() =>
                        void downloadAuthed(
                          `/admin/sources/${encodeURIComponent(r.sourceId)}/download`,
                          r.fileName ?? "document",
                        )
                      }
                    >
                      Download
                    </button>
                    <button
                      class="text-xs px-2 py-0.5 bg-noviq-800 text-white rounded hover:bg-noviq-700"
                      onClick={() => void decideShareReq(r, true)}
                    >
                      Approve
                    </button>
                    <button
                      class="text-xs px-2 py-0.5 border border-red-300 text-red-600 rounded hover:bg-red-50"
                      onClick={() => void decideShareReq(r, false)}
                    >
                      Reject
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>
          {/* Create form */}
          <form onSubmit={createUser} class="flex items-end gap-2 mb-6">
            <label class="flex flex-col text-xs text-noviq-600">
              Username
              <input
                class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm"
                value={newUsername()}
                onInput={(e) => setNewUsername(e.currentTarget.value)}
                placeholder="tester1"
              />
            </label>
            <label class="flex flex-col text-xs text-noviq-600">
              Role
              <select
                class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm"
                value={newRole()}
                onChange={(e) => setNewRole(e.currentTarget.value)}
              >
                <option value="user">user</option>
                <option value="admin">admin</option>
              </select>
            </label>
            <button
              type="submit"
              class="px-3 py-1.5 bg-noviq-800 text-white rounded text-sm hover:bg-noviq-700"
            >
              Create
            </button>
          </form>

          {/* One-time temp password — high-contrast so it's readable + copyable */}
          <Show when={tempPassword()}>
            {(tp) => (
              <div class="mb-6 p-4 bg-slate-900 border border-amber-400 rounded-lg">
                <div class="flex items-center justify-between mb-2">
                  <span class="text-sm font-semibold text-amber-300">
                    One-time password for {tp().username}
                  </span>
                  <button
                    type="button"
                    class="px-2.5 py-1 text-xs font-medium bg-amber-400 text-slate-900 rounded
                           hover:bg-amber-300 transition-colors"
                    onClick={() => copyPassword(tp().password)}
                  >
                    {copied() ? "Copied ✓" : "Copy"}
                  </button>
                </div>
                <code class="block px-3 py-2 bg-black/40 border border-slate-700 rounded
                             font-mono text-base text-white tracking-wide select-all break-all">
                  {tp().password}
                </code>
                <p class="mt-2 text-xs text-amber-200/80">
                  Copy and hand this over now — it won't be shown again.
                </p>
              </div>
            )}
          </Show>

          {/* F021: per-user document storage usage (uploads + attachments). */}
          <Show when={usageRows().length > 0}>
            <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg">
              <p class="text-xs font-semibold text-noviq-600 uppercase tracking-wider mb-2">
                Document storage per user
              </p>
              {/* Explicit dark text: the global body color is light (dark-theme
                  default) and would be invisible on this light card. */}
              {/* Only three short columns — these fit at 390px unchanged. */}
              <table class="w-full text-sm text-noviq-ink">
                <thead>
                  <tr class="text-left text-xs text-noviq-600">
                    <th class="py-1 pr-4">User</th>
                    <th class="py-1 pr-4">Files</th>
                    <th class="py-1">Storage</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={usageRows()}>
                    {(r) => (
                      <tr class="border-t border-noviq-line/60">
                        <td class="py-1 pr-4">{r.username}</td>
                        <td class="py-1 pr-4">{r.usedFiles}</td>
                        <td class="py-1">{(r.usedBytes / (1024 * 1024)).toFixed(1)} MB</td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </Show>
          </Show>{/* ══ end USERS TAB ══ */}

          {/* ══ INDEXING & DATA TAB ══ */}
          <Show when={activeTab() === "indexing"}>

          {/* Corpus / folder management — upload, per-file re-index / deep-index /
              delete. Same manager the desktop app shows under Settings. */}
          <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg">
            <IndexedFolders />
          </div>

          {/* Corpus organisation (migration 021). Sits directly under the
              document list because that is where filing happens — this block
              only defines the categories the list files INTO. */}
          <details class="mb-6 rounded-lg border border-noviq-line bg-noviq-50">
            <summary class="cursor-pointer select-none px-4 py-2.5 text-xs font-semibold
                            uppercase tracking-wider text-noviq-600">
              Corpus categories — how the document list is grouped
            </summary>
            <div class="px-4 pb-4 pt-1">
              <CorpusCategories />
            </div>
          </details>

          {/* Does a document hold more than one legal instrument? Read-only: it
              proposes boundaries and never applies them, because a wrong split
              silently truncates an instrument — worse than not splitting. */}
          <details class="mb-6 rounded-lg border border-noviq-line bg-noviq-50">
            <summary class="cursor-pointer select-none px-4 py-2.5 text-xs font-semibold
                            uppercase tracking-wider text-noviq-600">
              Document structure — does this file hold several instruments?
            </summary>
            <div class="px-4 pb-4 pt-1">
              <p class="text-xs text-noviq-500 mb-3">
                Indexing treats one file as one law. A compendium breaks that: each
                rule-set inside it numbers its provisions from 1, so "Rule 4" stops
                being a unique reference. This checks a document and proposes where
                each instrument begins. <b>Nothing is changed</b> — confirm the ranges
                before any split is carried out.
              </p>
              <div class="flex items-center gap-2 flex-wrap mb-3">
                <select
                  class="px-2 py-1.5 border border-noviq-300 rounded text-sm bg-white w-full sm:w-auto sm:min-w-[18rem]"
                  value={inspectId()}
                  onChange={(e) => setInspectId(e.currentTarget.value)}
                >
                  <option value="">Select a document…</option>
                  <For each={chunkDocs()}>
                    {(d) => <option value={d.source_id}>{d.file_name}</option>}
                  </For>
                </select>
                <button
                  type="button"
                  class="px-3 py-1.5 bg-noviq-800 text-white rounded text-sm hover:bg-noviq-700
                         disabled:opacity-50 disabled:cursor-not-allowed"
                  onClick={runInspect}
                  disabled={!inspectId() || inspecting()}
                  title="Read the document and propose instrument boundaries (no changes are made)"
                >
                  {inspecting() ? "Inspecting…" : "Inspect"}
                </button>
              </div>

              <Show when={inspectErr()}>
                <p class="text-xs text-red-600 mb-2">{inspectErr()}</p>
              </Show>

              <Show when={splitDone()}>
                {(res) => (
                  <div class="rounded border border-green-300 bg-green-50 p-3 mb-3">
                    <p class="text-sm mb-2">
                      Split into <b>{res().created.length} documents</b>. Each is now
                      indexed under its own name and cited that way.
                    </p>
                    <ul class="space-y-0.5">
                      <For each={res().created}>
                        {(c) => (
                          <li class="text-xs font-mono text-noviq-600">
                            {c.title}
                            <Show when={c.page_offset > 0}>
                              <span class="text-noviq-400">
                                {" "}
                                · page {c.page_offset} in the printed volume = page 0 here
                              </span>
                            </Show>
                          </li>
                        )}
                      </For>
                    </ul>
                    <Show when={res().dropped_pages.length > 0}>
                      <p class="text-xs text-noviq-500 mt-2">
                        Not indexed:{" "}
                        {res()
                          .dropped_pages.map(([a, b]) => (a === b ? `p.${a}` : `pp.${a}–${b}`))
                          .join(", ")}
                      </p>
                    </Show>
                    <p class="text-xs text-noviq-500 mt-2">
                      Original kept at <span class="font-mono">{res().original_retired_to}</span>
                    </p>
                    <p class="text-xs text-noviq-500 mt-1">
                      Run <b>Build summaries</b> and <b>Rebuild graph</b> next — the
                      derived layers still describe the old single document.
                    </p>
                  </div>
                )}
              </Show>

              <Show when={inspection()}>
                {(ins) => (
                  <div class="rounded border border-noviq-line bg-white p-3">
                    <p class="text-sm mb-2">
                      <b>{ins().file_name}</b> — {ins().pages} pages.{" "}
                      <Show
                        when={ins().is_container}
                        fallback={
                          <span class="text-noviq-600">
                            Reads as a <b>single instrument</b>. Nothing to split.
                          </span>
                        }
                      >
                        <span class="text-amber-700">
                          Holds <b>{ins().instrument_count} instruments</b> — indexing
                          it as one document is what makes provision numbers ambiguous.
                        </span>
                      </Show>
                    </p>

                    <Show when={ins().is_container}>
                      <p class="text-xs text-noviq-500 mb-2">
                        Edit anything below. The detector's count is a <b>floor</b> — the
                        Compendium's own contents list omits an instrument its text
                        contains — so add or correct rows where it is wrong.
                      </p>
                      {/* Scroll container: the table has a `min-w` so its four
                          columns stay legible, which would otherwise widen the
                          whole page instead of scrolling inside this box. */}
                      <div class="overflow-x-auto">
                      <table class="w-full min-w-[34rem] text-xs border-collapse">
                        <thead>
                          <tr class="text-left text-noviq-500 border-b border-noviq-line">
                            <th class="py-1 pr-2 font-medium w-8">Keep</th>
                            <th class="py-1 pr-2 font-medium w-32">Pages</th>
                            <th class="py-1 pr-2 font-medium">Instrument (cited as)</th>
                            <th class="py-1 pr-2 font-medium w-40">Evidence</th>
                          </tr>
                        </thead>
                        <tbody>
                          <For each={plan()}>
                            {(row, i) => (
                              <tr class="border-b border-noviq-100 align-top">
                                <td class="py-1 pr-2">
                                  <input
                                    type="checkbox"
                                    checked={row.include}
                                    onChange={(e) =>
                                      updateRow(i(), { include: e.currentTarget.checked })
                                    }
                                    title="Uncheck to leave these pages out of the split"
                                  />
                                </td>
                                <td class="py-1 pr-2 whitespace-nowrap">
                                  <input
                                    type="number"
                                    min="1"
                                    max={ins().pages}
                                    value={row.start}
                                    class="w-14 px-1 py-0.5 border border-noviq-300 rounded font-mono"
                                    onInput={(e) =>
                                      updateRow(i(), { start: parseInt(e.currentTarget.value, 10) })
                                    }
                                  />
                                  <span class="text-noviq-400">–</span>
                                  <input
                                    type="number"
                                    min="1"
                                    max={ins().pages}
                                    value={row.end}
                                    class="w-14 px-1 py-0.5 border border-noviq-300 rounded font-mono"
                                    onInput={(e) =>
                                      updateRow(i(), { end: parseInt(e.currentTarget.value, 10) })
                                    }
                                  />
                                </td>
                                <td class="py-1 pr-2">
                                  <input
                                    type="text"
                                    value={row.title}
                                    placeholder="Name this instrument"
                                    class="w-full px-1 py-0.5 border border-noviq-300 rounded"
                                    onInput={(e) => updateRow(i(), { title: e.currentTarget.value })}
                                  />
                                  {/* One line of real text is the cheapest way for a
                                      human to tell whether a boundary is right. */}
                                  <div class="text-noviq-400 font-mono truncate max-w-[34rem]">
                                    {row.first_line}
                                  </div>
                                </td>
                                <td class="py-1 pr-2 whitespace-nowrap">
                                  <Show when={row.evidence === "both"}>
                                    <span class="text-green-700">contents + text</span>
                                  </Show>
                                  <Show when={row.evidence === "toc_only"}>
                                    <span class="text-amber-700" title="Start page is the book's own claim — verify it">
                                      contents only
                                    </span>
                                  </Show>
                                  <Show when={row.evidence === "clause_only"}>
                                    <span class="text-amber-700" title="Found in the text but missing from the book's contents list">
                                      text only
                                    </span>
                                  </Show>
                                  <Show when={row.evidence === "manual"}>
                                    <span class="text-noviq-500">added by you</span>
                                  </Show>
                                  <Show when={row.restart}>
                                    <span class="text-noviq-400"> · numbering restarts</span>
                                  </Show>
                                </td>
                              </tr>
                            )}
                          </For>
                        </tbody>
                      </table>
                      </div>

                      <div class="mt-2 flex items-center gap-3 flex-wrap">
                        <button
                          type="button"
                          class="px-2 py-1 border border-noviq-300 text-noviq-700 rounded text-xs hover:bg-noviq-100"
                          onClick={addRow}
                        >
                          + Add an instrument the detector missed
                        </button>
                        <button
                          type="button"
                          class="px-2 py-1 border border-noviq-300 text-noviq-700 rounded text-xs hover:bg-noviq-100"
                          onClick={() => seedPlan(ins())}
                          title="Discard your edits and go back to what the detector proposed"
                        >
                          Reset to detected
                        </button>
                      </div>

                      {/* Problems block the split; dropped pages only need to be
                          seen, because an accidentally-dropped instrument looks
                          exactly like intentionally-dropped front matter. */}
                      <Show when={planProblems().length > 0}>
                        <ul class="mt-3 space-y-0.5">
                          <For each={planProblems()}>
                            {(p) => <li class="text-xs text-red-600">— {p}</li>}
                          </For>
                        </ul>
                      </Show>
                      <Show when={droppedRanges().length > 0}>
                        <p class="mt-2 text-xs text-noviq-500">
                          Will not be indexed:{" "}
                          {droppedRanges()
                            .map(([a, b]) => (a === b ? `p.${a}` : `pp.${a}–${b}`))
                            .join(", ")}
                        </p>
                      </Show>

                      <Show when={ins().notes.length > 0}>
                        <ul class="mt-3 space-y-1">
                          <For each={ins().notes}>
                            {(n) => <li class="text-xs text-noviq-500">— {n}</li>}
                          </For>
                        </ul>
                      </Show>

                      <div class="mt-3 flex items-center gap-3 flex-wrap">
                        <button
                          type="button"
                          class="px-3 py-1.5 bg-amber-700 text-white rounded text-sm
                                 hover:bg-amber-800 disabled:opacity-50 disabled:cursor-not-allowed"
                          onClick={runSplit}
                          disabled={splitting() || planProblems().length > 0}
                          title={
                            planProblems().length > 0
                              ? "Fix the problems listed above first"
                              : "Cut this file into one document per instrument and index each"
                          }
                        >
                          {splitting()
                            ? "Splitting…"
                            : `Split into ${included().length} document${
                                included().length === 1 ? "" : "s"
                              }`}
                        </button>
                        {/* The third outcome. A single Act whose Schedules restart
                            their numbering looks exactly like a container, and
                            without an easy way to say no someone will accept a bad
                            split rather than fight the UI. */}
                        <button
                          type="button"
                          class="px-3 py-1.5 border border-noviq-300 text-noviq-700 rounded text-sm
                                 hover:bg-noviq-100"
                          onClick={() => {
                            setInspection(null);
                            setPlan([]);
                          }}
                          title="Leave this document exactly as it is"
                        >
                          This is one document — leave it
                        </button>
                        <span class="text-xs text-noviq-500">
                          Check any row marked <b>contents only</b>: its start page comes
                          from the book's index rather than its text.
                        </span>
                      </div>
                    </Show>
                  </div>
                )}
              </Show>
            </div>
          </details>

          {/* Corpus-wide maintenance jobs. Collapsed by default: these act on the
              WHOLE corpus and are run occasionally, so they must not compete for
              attention with the per-document work above, which is the daily task. */}
          <details class="mb-6 rounded-lg border border-noviq-line bg-noviq-50">
            <summary class="cursor-pointer select-none px-4 py-2.5 text-xs font-semibold
                            uppercase tracking-wider text-noviq-600">
              Corpus maintenance — summaries, graph, GPU re-embed
            </summary>
            <div class="px-4 pb-4 pt-1">
          {/* Derived layers — build the pre-baked whole-document summaries (the
              fast-path for "summarize <doc>"), and (re)build the legal graph
              overlay / re-run structure extraction. The document picker below
              scopes "Build summaries" to one doc; the others act corpus-wide. */}
          <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg">
            <p class="text-xs font-semibold text-noviq-600 uppercase tracking-wider mb-1">
              Derived layers (pipeline)
            </p>
            <p class="text-xs text-noviq-500 mb-3">
              Run these AFTER indexing / re-embedding a document, in this order:
              <b> Build summaries</b> → <b>Rebuild graph</b>. "Build summaries" uses the
              document picker below (or all documents); the others are corpus-wide.
            </p>
            <div class="flex items-center gap-2 flex-wrap">
              <button
                type="button"
                class="px-3 py-1.5 bg-noviq-800 text-white rounded text-sm hover:bg-noviq-700 disabled:opacity-50 disabled:cursor-not-allowed"
                onClick={buildSummaries}
                disabled={pipelineBusy() !== null}
                title="Build the ingest-time whole-document summaries (the summarize fast-path)"
              >
                {pipelineBusy() === "Build summaries" ? "Building…" : "Build summaries"}
              </button>
              <button
                type="button"
                class="px-3 py-1.5 border border-noviq-300 text-noviq-700 rounded text-sm hover:bg-noviq-100 disabled:opacity-50 disabled:cursor-not-allowed"
                onClick={rebuildGraph}
                disabled={pipelineBusy() !== null}
                title="Regenerate the whole legal graph overlay (intra + cross-document) from existing data"
              >
                {pipelineBusy() === "Rebuild graph" ? "Rebuilding…" : "Rebuild graph"}
              </button>
              <button
                type="button"
                class="px-3 py-1.5 border border-noviq-300 text-noviq-700 rounded text-sm hover:bg-noviq-100 disabled:opacity-50 disabled:cursor-not-allowed"
                onClick={linkGraph}
                disabled={pipelineBusy() !== null}
                title="Rebuild only the cross-document link overlay"
              >
                {pipelineBusy() === "Link graph" ? "Linking…" : "Link graph (cross-doc)"}
              </button>
              <button
                type="button"
                class="px-3 py-1.5 border border-noviq-300 text-noviq-700 rounded text-sm hover:bg-noviq-100 disabled:opacity-50 disabled:cursor-not-allowed"
                onClick={reindexStructure}
                disabled={pipelineBusy() !== null}
                title="Re-run structure extraction for documents below the current parser version (slow)"
              >
                {pipelineBusy() === "Re-index structure" ? "Re-indexing…" : "Re-index structure"}
              </button>
              <Show when={pipelineMsg()}>
                <span class="text-xs text-noviq-600 break-all">{pipelineMsg()}</span>
              </Show>
            </div>

            {/* Per-layer PENDING status — which documents still need each layer.
                Summaries + structure are per-document; the graph overlay is
                corpus-wide so it shows a stat. Refreshes live while a build runs. */}
            <div class="mt-4 grid gap-3 sm:grid-cols-2">
              <div class="text-xs">
                <div class="font-semibold text-noviq-700 mb-1">
                  Build summaries — pending
                  <Show when={derivedWatching()}>
                    <span class="ml-1 font-normal text-noviq-400">(refreshing…)</span>
                  </Show>
                </div>
                <Show
                  when={derived() && derived()!.summaries_pending.length > 0}
                  fallback={<div class="text-noviq-400">{derived() ? "All documents have summaries ✓" : "…"}</div>}
                >
                  <ul class="space-y-1 text-noviq-600">
                    <For each={derived()!.summaries_pending}>
                      {(d) => (
                        <li class="flex items-center gap-2">
                          <button
                            type="button"
                            class="shrink-0 px-2 py-0.5 bg-noviq-800 text-white rounded text-[11px] hover:bg-noviq-700 disabled:opacity-50 disabled:cursor-not-allowed"
                            onClick={() => buildOneSummary(d.source_id, d.file_name)}
                            disabled={pipelineBusy() !== null}
                            title={`Build the whole-document summary for ${d.file_name}`}
                          >
                            Build
                          </button>
                          <span class="break-all">{d.file_name}</span>
                          <Show when={d.owner}>
                            <span
                              class="shrink-0 px-1.5 py-0.5 rounded bg-noviq-100 text-noviq-500 text-[10px]"
                              title="A user's own document, not part of the shared corpus"
                            >
                              {d.owner}
                            </span>
                          </Show>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </div>

              <div class="text-xs">
                <div class="font-semibold text-noviq-700 mb-1 flex items-center gap-2">
                  <span>Possible duplicates</span>
                  <button
                    type="button"
                    class="px-2 py-0.5 bg-noviq-800 text-white rounded text-[11px] hover:bg-noviq-700 disabled:opacity-50"
                    onClick={loadDupes}
                    disabled={dupesBusy()}
                    title="Scan the corpus for documents stored more than once"
                  >
                    {dupesBusy() ? "Scanning…" : "Scan"}
                  </button>
                </div>
                <Show when={dupes()} fallback={<div class="text-noviq-400">Not scanned yet.</div>}>
                  <Show
                    when={dupes()!.groups.length > 0}
                    fallback={<div class="text-noviq-400">No duplicates found ✓</div>}
                  >
                    <div class="text-noviq-500 mb-1">
                      {dupes()!.group_count} group(s), {dupes()!.document_count} document(s).
                      Review only — nothing is deleted here.
                    </div>
                    <ul class="space-y-2 text-noviq-600">
                      <For each={dupes()!.groups}>
                        {(g) => (
                          <li class="border border-noviq-200 rounded p-1.5">
                            <div class="mb-1">
                              <span
                                class={
                                  "px-1.5 py-0.5 rounded text-[10px] " +
                                  (g.kind === "content"
                                    ? "bg-red-100 text-red-700"
                                    : "bg-amber-100 text-amber-700")
                                }
                                title={
                                  g.kind === "content"
                                    ? "Byte-identical files — the same document stored more than once"
                                    : "Similar titles only — check before treating these as one document"
                                }
                              >
                                {g.kind === "content" ? "identical content" : "similar name"}
                              </span>
                              <span class="ml-1 text-noviq-400">
                                ×{g.docs.length}
                              </span>
                            </div>
                            <ul class="pl-2 space-y-0.5">
                              <For each={g.docs}>
                                {(d) => (
                                  <li class="flex items-baseline gap-2">
                                    <span class="break-all">{d.file_name}</span>
                                    <span class="shrink-0 text-noviq-400 text-[10px]">
                                      {d.chunk_count} chunks
                                      {d.owner ? ` · ${d.owner}` : " · corpus"}
                                      {d.conversation_id ? " · chat attachment" : ""}
                                    </span>
                                  </li>
                                )}
                              </For>
                            </ul>
                          </li>
                        )}
                      </For>
                    </ul>
                  </Show>
                </Show>
              </div>

              <div class="text-xs">
                <div class="font-semibold text-noviq-700 mb-1">Re-index structure — pending</div>
                <Show
                  when={derived() && derived()!.structure_pending.length > 0}
                  fallback={<div class="text-noviq-400">{derived() ? "All documents up to date ✓" : "…"}</div>}
                >
                  <ul class="list-disc pl-4 space-y-0.5 text-noviq-600">
                    <For each={derived()!.structure_pending}>
                      {(d) => <li class="break-all">{d.file_name}</li>}
                    </For>
                  </ul>
                </Show>
              </div>

              <div class="text-xs sm:col-span-2">
                <div class="font-semibold text-noviq-700 mb-1">Graph overlay (corpus-wide)</div>
                <Show
                  when={derived() && derived()!.graph_edges.length > 0}
                  fallback={<div class="text-noviq-400">{derived() ? "No graph edges yet — run Rebuild graph" : "…"}</div>}
                >
                  <div class="text-noviq-600 break-all">
                    <For each={derived()!.graph_edges}>
                      {(e, i) => <span>{i() > 0 ? " · " : ""}{e.edge_type}: {e.count}</span>}
                    </For>
                  </div>
                </Show>
              </div>
            </div>
          </div>

          {/* Chunk export — the corpus-level ingestion audit (per-document export
              lives on the document's own row, in the viewer). */}
          <div class="mb-6 p-4 bg-white border border-noviq-line rounded-lg">
            <p class="text-xs font-semibold text-noviq-600 uppercase tracking-wider mb-1">
              Chunk export
            </p>
            <p class="text-xs text-noviq-500 mb-2">
              One row per chunk across every document — document, citation label, unit
              path, page, length and content.
            </p>
            <button
              type="button"
              class="px-3 py-1.5 border border-noviq-line rounded text-sm hover:bg-noviq-50"
              onClick={exportChunks}
            >
              Export all chunks (CSV)
            </button>
          </div>

          {/* GPU re-embed — re-run indexing for the whole corpus with embeddings on
              the GPU (~50× faster than CPU). Stops vLLM for the duration, so chat
              pauses; the app shows an "updating" banner and recovers automatically. */}
          <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg">
            <div class="flex items-center gap-3 flex-wrap">
              <button
                type="button"
                class="px-3 py-1.5 bg-noviq-800 text-white rounded text-sm hover:bg-noviq-700 disabled:opacity-50 disabled:cursor-not-allowed"
                onClick={gpuReindex}
                disabled={reindexing()}
                title="Re-embed the whole corpus on the GPU (pauses chat ~7–10 min)"
              >
                {reindexing() ? "Starting…" : "GPU re-embed corpus"}
              </button>
              <Show when={reindexMsg()}>
                <span class="text-xs text-noviq-600">{reindexMsg()}</span>
              </Show>
            </div>
            <p class="mt-2 text-xs text-noviq-500">
              Re-indexes every document with embeddings on the GPU. Chat pauses ~7–10 min
              (vLLM stops to free VRAM) and returns automatically when done.
            </p>
          </div>

            </div>
          </details>

</Show>{/* ══ end INDEXING & DATA TAB ══ */}

          {/* ══ MODEL & INFERENCE TAB ══ */}
          {/* ── Features: deployment-wide capability switches ─────────────── */}
          <Show when={activeTab() === "features"}>
            <p class="mb-4 text-sm text-noviq-600">
              These switches decide what every user of this deployment can do. They apply at
              once — no restart, and no need to reload the page.
            </p>


            {/* F027 — My Projects availability, deployment-wide (FR-037).
                Applies at runtime, no restart. Turning it OFF deletes NOTHING:
                projects, documents, chats and share links are all retained and
                come back unchanged when it is turned on again (FR-039). */}
            <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg text-noviq-ink">
              <p class="text-xs font-semibold text-noviq-600 uppercase tracking-wider mb-3">
                My Projects
              </p>
              <label class="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={myProjectsEnabled()}
                  onChange={async (e) => {
                    const next = e.currentTarget.checked;
                    try {
                      // Shared store — updates the main nav at once, no reload.
                      await setProjectsEnabledShared(next);
                      setMyProjectsEnabled(next);
                      setFeatureSaved("My Projects " + (next ? "enabled" : "disabled"));
                      window.setTimeout(() => setFeatureSaved(null), 2500);
                    } catch {
                      e.currentTarget.checked = !next;
                      setFeatureSaved("Could not save — the setting is unchanged.");
                      window.setTimeout(() => setFeatureSaved(null), 4000);
                    }
                  }}
                />
                <span>Let users create their own project workspaces</span>
              </label>
              <p class="mt-2 text-xs text-noviq-600">
                When off, projects are hidden and unreachable for everyone — including by direct
                request and by any existing chat share link. Nothing is deleted.
              </p>
              {/* Explicit confirmation. The switch already applies instantly; this
                  exists because a silent toggle gives no evidence it took. */}
              <Show when={featureSaved()}>
                <p class="mt-3 rounded bg-noviq-100 px-3 py-1.5 text-xs text-noviq-800">
                  {featureSaved()}
                </p>
              </Show>
            </div>

            <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg text-noviq-ink">
              <p class="text-xs font-semibold text-noviq-600 uppercase tracking-wider mb-3">
                Also gated, but not from here
              </p>
              <ul class="list-disc pl-5 text-xs text-noviq-600 space-y-1">
                <li><b>Deep search</b> — admin-only, enforced by the server on every request.</li>
                <li><b>Web search</b> — available to every user, off by default, per turn.</li>
                <li>
                  <b>Retrieval tuning</b> (<code>ECHO_POOL_OVERFETCH</code>,
                  <code>ECHO_CITATION_STRICT</code>) — environment settings on the server;
                  changing them needs a restart.
                </li>
              </ul>
            </div>
          </Show>

          <Show when={activeTab() === "inference"}>
          {/* Inference engine — switch chat generation between the local GPU (vLLM)
              and a hosted OpenAI-compatible API (e.g. Groq). Applies immediately,
              no restart. The API key is write-only; the server never returns it. */}
          <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg text-noviq-ink">
            <p class="text-xs font-semibold text-noviq-600 uppercase tracking-wider mb-3">
              Inference engine
            </p>
            <div class="flex items-end gap-3 flex-wrap">
              <label class="flex flex-col text-xs text-noviq-600">
                Generate with
                <select
                  class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm w-full sm:w-auto sm:min-w-[14rem]"
                  value={infEngine()}
                  onChange={(e) => setInfEngine(e.currentTarget.value as "vllm" | "remote")}
                >
                  <option value="vllm">Local GPU (vLLM)</option>
                  <option value="remote">Hosted API (Groq / OpenAI-compatible)</option>
                </select>
              </label>
              <button
                type="button"
                class="px-3 py-1.5 bg-noviq-800 text-white rounded text-sm hover:bg-noviq-700 disabled:opacity-50 disabled:cursor-not-allowed"
                onClick={saveInference}
                disabled={infSaving()}
              >
                {infSaving() ? "Saving…" : "Save"}
              </button>
              <Show when={infMsg()}>
                <span class="text-xs text-noviq-600">{infMsg()}</span>
              </Show>
            </div>

            {/* Remote-API fields — only relevant when the hosted API is selected. */}
            <Show when={infEngine() === "remote"}>
              <div class="mt-3 grid grid-cols-1 sm:grid-cols-2 gap-3">
                <label class="flex flex-col text-xs text-noviq-600">
                  Provider
                  <select
                    class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm"
                    value={infProvider()}
                    onChange={(e) => selectProvider(e.currentTarget.value)}
                  >
                    <For each={providers()}>
                      {(p) => <option value={p.id}>{p.label}</option>}
                    </For>
                  </select>
                </label>
                <label class="flex flex-col text-xs text-noviq-600">
                  API key
                  <input
                    type="password"
                    autocomplete="off"
                    class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm font-mono"
                    value={infKey()}
                    onInput={(e) => setInfKey(e.currentTarget.value)}
                    placeholder={
                      infHasKey()
                        ? "•••••••• (a key is set — leave blank to keep it)"
                        : currentProvider()?.key_placeholder ?? "API key"
                    }
                  />
                  <Show when={currentProvider()?.console_url}>
                    <a
                      class="mt-1 text-[11px] text-noviq-600 underline hover:text-noviq-800"
                      href={currentProvider()!.console_url!}
                      target="_blank"
                      rel="noreferrer noopener"
                    >
                      Get a key from {currentProvider()!.label} →
                    </a>
                  </Show>
                </label>
              </div>

              {/* Provider terms an operator must see BEFORE pasting a key — the
                  free allowance, and any data-training / licensing condition that
                  would disqualify the provider outright for a legal corpus. */}
              <Show when={currentProvider()}>
                {(p) => (
                  <div class="mt-3 text-xs text-noviq-600 space-y-1">
                    <Show when={p().free_tier}>
                      <p>
                        <span class="font-semibold text-noviq-700">Free tier:</span> {p().free_tier}
                      </p>
                    </Show>
                    <Show when={p().notes}>
                      <p>{p().notes}</p>
                    </Show>
                    <Show when={p().trains_on_data}>
                      <p class="px-2 py-1 rounded bg-rose-50 border border-rose-200 text-rose-800">
                        ⚠ This provider may train on submitted prompts on its free tier. Noviq
                        sends full retrieved passages in every query — treat this as a data-boundary
                        decision, not a settings choice.
                      </p>
                    </Show>
                    <Show when={p().non_commercial_only}>
                      <p class="px-2 py-1 rounded bg-rose-50 border border-rose-200 text-rose-800">
                        ⚠ This provider's free key is licensed for non-commercial use only and
                        cannot back a customer deployment.
                      </p>
                    </Show>
                  </div>
                )}
              </Show>

              {/* Model list. A dropdown cannot carry the tags that decide whether a
                  model is usable here, so this is an explicit list: tier, context,
                  price and the *effective* request rate at our real prompt size. */}
              <Show when={infProvider() !== CUSTOM && currentModels().length > 0}>
                <div class="mt-4">
                  <div class="flex items-center justify-between mb-1.5 flex-wrap gap-2">
                    <span class="text-xs font-semibold text-noviq-700">
                      Models
                      <span class="ml-1.5 font-normal text-noviq-500">
                        ({visibleModels().length} of {currentModels().length})
                      </span>
                    </span>
                    <div class="flex gap-1">
                      <For each={["all", "free", "paid"] as const}>
                        {(f) => (
                          <button
                            type="button"
                            class={`px-2 py-0.5 text-[11px] rounded border ${
                              modelFilter() === f
                                ? "bg-noviq-800 text-white border-noviq-800"
                                : "bg-white text-noviq-600 border-noviq-line hover:bg-noviq-50"
                            }`}
                            onClick={() => setModelFilter(f)}
                          >
                            {f === "all" ? "All" : f === "free" ? "Free" : "Paid"}
                          </button>
                        )}
                      </For>
                    </div>
                  </div>

                  <div class="border border-noviq-line rounded max-h-72 overflow-y-auto divide-y divide-noviq-line bg-white">
                    <For each={visibleModels()}>
                      {(m) => {
                        const selected = () => infModelPick() === m.id;
                        const eff = effectiveRpm(m);
                        return (
                          <button
                            type="button"
                            class={`w-full text-left px-3 py-2 hover:bg-noviq-50 ${
                              selected() ? "bg-noviq-100" : ""
                            }`}
                            onClick={() => selectModel(m.id)}
                          >
                            <div class="flex items-start gap-2 flex-wrap">
                              <span
                                class={`font-mono text-xs ${
                                  selected() ? "font-semibold text-noviq-900" : "text-noviq-ink"
                                }`}
                              >
                                {selected() ? "● " : "○ "}
                                {m.id}
                              </span>
                              <For each={modelTags(m, currentProvider()!)}>
                                {(t) => (
                                  <span
                                    class={`px-1.5 py-0.5 rounded text-[10px] font-bold border ${t.cls}`}
                                    title={t.title}
                                  >
                                    {t.label}
                                  </span>
                                )}
                              </For>
                            </div>
                            <div class="mt-0.5 text-[11px] text-noviq-500 flex gap-3 flex-wrap">
                              <span>ctx {fmtCtx(m.context)}</span>
                              <Show when={fmtPrice(m)}>
                                <span>{fmtPrice(m)}</span>
                              </Show>
                              {/* The effective rate, derived from the TPM cap at our
                                  real prompt size — not the provider's headline RPD,
                                  which overstates RAG throughput by ~10x. */}
                              <Show when={eff !== null}>
                                <span
                                  title={`Derived from ${m.tpm} tokens/min at a ~${
                                    RAG_PROMPT_TOKENS / 1000
                                  }K-token RAG prompt. The provider's headline is ${
                                    m.rpd ? `${m.rpd} requests/day` : `${m.rpm}/min`
                                  }, which does not account for prompt size.`}
                                >
                                  ~{fmtRate(eff!)} sustained
                                </span>
                              </Show>
                              <Show when={m.rpd}>
                                <span class="text-noviq-400">({m.rpd}/day cap)</span>
                              </Show>
                            </div>
                            <Show when={m.note}>
                              <div class="mt-0.5 text-[11px] text-noviq-600 italic">{m.note}</div>
                            </Show>
                          </button>
                        );
                      }}
                    </For>

                    {/* Any id the provider adds after this catalogue was written. */}
                    <button
                      type="button"
                      class={`w-full text-left px-3 py-2 hover:bg-noviq-50 ${
                        infModelPick() === OTHER ? "bg-noviq-100" : ""
                      }`}
                      onClick={() => selectModel(OTHER)}
                    >
                      <span class="text-xs text-noviq-600">
                        {infModelPick() === OTHER ? "● " : "○ "}Other — enter a model id by hand
                      </span>
                    </button>
                  </div>
                </div>
              </Show>

              <div class="mt-3 grid grid-cols-1 sm:grid-cols-2 gap-3">
                {/* Free-text model id: the Custom provider, or "Other…" above. */}
                <Show when={manualConn() || infModelPick() === OTHER}>
                  <label class="flex flex-col text-xs text-noviq-600">
                    Model id
                    <input
                      class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm font-mono"
                      value={infModel()}
                      onInput={(e) => setInfModel(e.currentTarget.value)}
                      placeholder="model id"
                    />
                  </label>
                </Show>
                {/* Base URL + prefix + endpoint are auto-managed per provider; editable
                    only under Custom. Shown read-only otherwise for transparency. */}
                <label class="flex flex-col text-xs text-noviq-600">
                  Base URL
                  <input
                    class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm disabled:bg-noviq-100 disabled:text-noviq-500"
                    value={infBaseUrl()}
                    disabled={!manualConn()}
                    onInput={(e) => setInfBaseUrl(e.currentTarget.value)}
                    placeholder="https://api.example.com"
                  />
                </label>
                <label class="flex flex-col text-xs text-noviq-600">
                  API path prefix
                  <input
                    class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm font-mono disabled:bg-noviq-100 disabled:text-noviq-500"
                    value={infPrefix()}
                    disabled={!manualConn()}
                    onInput={(e) => setInfPrefix(e.currentTarget.value)}
                    placeholder="/v1 (empty for Gemini)"
                  />
                </label>
                <Show when={manualConn()}>
                  <label class="flex flex-col text-xs text-noviq-600">
                    Endpoint
                    <select
                      class="mt-1 px-2 py-1 border border-noviq-line rounded text-sm"
                      value={infEndpoint()}
                      onChange={(e) => setInfEndpoint(e.currentTarget.value as "chat" | "completions")}
                    >
                      <option value="chat">chat (…/chat/completions)</option>
                      <option value="completions">completions (…/completions)</option>
                    </select>
                  </label>
                </Show>
              </div>

              {/* Live probe. Entitlement is per model, not per key — one valid
                  Google key returns 200 on Flash and 404 on Pro — so the only
                  honest check is against the model actually selected. Runs without
                  saving, so a bad pick never reaches production. */}
              <div class="mt-3 flex items-center gap-2 flex-wrap">
                <button
                  type="button"
                  class="px-3 py-1.5 border border-noviq-line rounded text-sm text-noviq-700 hover:bg-noviq-100 disabled:opacity-50 disabled:cursor-not-allowed"
                  onClick={testInference}
                  disabled={probing() || (!infKey().trim() && !infHasKey())}
                  title={
                    !infKey().trim() && !infHasKey()
                      ? "Enter an API key first"
                      : "Send a 1-token request to this provider and model without saving"
                  }
                >
                  {probing() ? "Testing…" : "Test key & model"}
                </button>
                <Show when={probe()}>
                  {(r) => (
                    <span
                      class={`text-xs px-2 py-1 rounded border ${
                        r().ok
                          ? "bg-emerald-50 border-emerald-300 text-emerald-800"
                          : "bg-rose-50 border-rose-300 text-rose-800"
                      }`}
                    >
                      {r().ok ? "✓ " : "✗ "}
                      {r().message}
                    </span>
                  )}
                </Show>
              </div>

              <p class="mt-2 text-xs text-noviq-500">
                {infHasKey() ? "A key is currently stored. " : "No key stored yet. "}
                The key is write-only — it is never shown again after saving. Requests go to{" "}
                <code>{infBaseUrl()}{infPrefix()}{infEndpoint() === "completions" ? "/completions" : "/chat/completions"}</code>.
                <Show when={catalogVersion()}>
                  {" "}Catalogue {catalogVersion()} — published limits drift; the probe above is
                  what to trust.
                </Show>
              </p>
            </Show>
            <Show when={infEngine() === "vllm"}>
              <p class="mt-2 text-xs text-noviq-500">
                Chat generates on the local A40 GPU via the vLLM sidecar (offline). Switch to the
                hosted API to offload generation to Groq.
              </p>
            </Show>
          </div>
          </Show>{/* ══ end MODEL & INFERENCE TAB ══ */}

          {/* ══ TRACES TAB ══ */}
          <Show when={activeTab() === "traces"}>
          {/* Track 3 — Query Debug: recent traces; click a row for the full
              step-by-step pipeline (classify → scope → retrieval/graph → LLM). */}
          <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg">
            <div class="flex items-center justify-between mb-2 gap-2 flex-wrap">
              <p class="text-xs font-semibold text-noviq-600 uppercase tracking-wider">
                Query Debug
              </p>
              <div class="flex items-center gap-1.5">
                <button
                  onClick={() => downloadAuthed(`/traces/recent?limit=30`, `noviq-traces-last30.md`).catch((e: any) => setTracesMsg(e?.message ?? "download failed"))}
                  class="px-2.5 py-1 text-xs font-medium border border-noviq-line rounded hover:bg-noviq-100"
                  title="Download the most recent 30 traces as one file"
                >
                  Download last 30
                </button>
                <button
                  onClick={() => downloadAuthed(`/traces/all`, `noviq-traces-all.md`).catch((e: any) => setTracesMsg(e?.message ?? "download failed"))}
                  class="px-2.5 py-1 text-xs font-medium border border-noviq-line rounded hover:bg-noviq-100"
                  title="Download every trace combined into one file"
                >
                  Download all
                </button>
                <button
                  onClick={loadTraces}
                  class="px-2.5 py-1 text-xs font-medium border border-noviq-line rounded hover:bg-noviq-100"
                >
                  Refresh
                </button>
              </div>
            </div>
            <Show when={tracesMsg()}>
              <p class="text-xs text-red-500 mb-2">{tracesMsg()}</p>
            </Show>
            <Show
              when={traces().length > 0}
              fallback={<p class="text-xs text-noviq-500">No query traces yet — run a chat query.</p>}
            >
              {/* 6 columns, all short — horizontal scroll is enough here; the
                  `min-w` stops them crushing into unreadable slivers. */}
              <div class="max-h-80 overflow-auto border border-noviq-line rounded bg-white">
                <table class="w-full min-w-[40rem] text-sm text-noviq-ink">
                  <thead class="sticky top-0 bg-noviq-50">
                    <tr class="text-left text-noviq-500 border-b border-noviq-line">
                      <th class="py-1.5 px-2">Time (UTC)</th>
                      <th class="py-1.5 px-2">Question</th>
                      <th class="py-1.5 px-2">Capability</th>
                      <th class="py-1.5 px-2 text-right">ms</th>
                      <th class="py-1.5 px-2">Outcome</th>
                      <th class="py-1.5 px-2 text-right">Cites</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={traces()}>
                      {(t) => (
                        <tr
                          class="border-b border-noviq-line/60 hover:bg-noviq-100 cursor-pointer"
                          onClick={() => openTrace(t)}
                          title="Open the full step-by-step trace"
                        >
                          <td class="py-1.5 px-2 whitespace-nowrap tabular-nums text-noviq-500">
                            {t.started_at.replace("T", " ").replace(/\..*/, "")}
                          </td>
                          <td class="py-1.5 px-2 max-w-xs truncate">{t.query}</td>
                          <td class="py-1.5 px-2">{t.capability || "—"}</td>
                          <td class="py-1.5 px-2 text-right tabular-nums">{t.total_ms}</td>
                          <td class="py-1.5 px-2">{t.outcome}</td>
                          <td class="py-1.5 px-2 text-right tabular-nums">{t.citations}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>
          </div>

          {/* Trace detail modal — the full `.log` text for the picked query. */}
          <Show when={traceSel()}>
            <div
              class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
              onClick={() => setTraceSel(null)}
            >
              <div
                class="bg-white rounded-lg shadow-xl max-w-4xl w-full max-h-[85vh] flex flex-col"
                onClick={(e) => e.stopPropagation()}
              >
                <div class="flex items-center justify-between px-4 py-2 border-b border-noviq-line">
                  <span class="text-sm font-semibold text-noviq-800 truncate">
                    Trace {traceSel()?.id} · {traceSel()?.capability || "—"}
                  </span>
                  <button
                    onClick={() => setTraceSel(null)}
                    class="px-2 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-100"
                  >
                    Close
                  </button>
                </div>
                <pre class="flex-1 overflow-auto p-3 text-xs font-mono text-noviq-ink whitespace-pre-wrap">
                  {traceBody() ?? "Loading…"}
                </pre>
              </div>
            </div>
          </Show>
          </Show>{/* ══ end TRACES TAB ══ */}

          {/* ══ AUDIT LOG TAB ══
              Who did what, and when. Filter by day range, actor, action or free
              text, then download exactly what is on screen — whole log or one
              slice. Read-only by design: there is no delete control, because an
              audit trail an admin can prune on demand carries little weight. */}
          <Show when={activeTab() === "audit"}>
          <div class="mb-6 p-4 bg-noviq-50 border border-noviq-line rounded-lg">
            <div class="flex items-center justify-between mb-3 gap-2 flex-wrap">
              <div>
                <p class="text-xs font-semibold text-noviq-600 uppercase tracking-wider">
                  Audit Log
                </p>
                <p class="text-xs text-noviq-500 mt-0.5">
                  Document access, searches, sign-ins and administrative actions.
                  Times are UTC.
                </p>
              </div>
              <div class="flex items-center gap-1.5 flex-wrap">
                <button
                  onClick={() => downloadAudit("csv")}
                  class="px-2.5 py-1 text-xs font-medium border border-noviq-line rounded hover:bg-noviq-100"
                  title="Download the rows matching the filters above, as a spreadsheet"
                >
                  Download filtered (CSV)
                </button>
                <button
                  onClick={() => downloadAudit("json")}
                  class="px-2.5 py-1 text-xs font-medium border border-noviq-line rounded hover:bg-noviq-100"
                  title="Same rows, as JSON"
                >
                  JSON
                </button>
                <button
                  onClick={() => downloadAudit("csv", true)}
                  class="px-2.5 py-1 text-xs font-medium border border-noviq-line rounded hover:bg-noviq-100"
                  title="Download the entire log, ignoring the date range"
                >
                  Download whole log
                </button>
              </div>
            </div>

            {/* ── Filters ── */}
            <div class="flex flex-wrap items-end gap-2 mb-3">
              <label class="flex flex-col gap-0.5">
                <span class="text-[0.65rem] uppercase tracking-wider text-noviq-500">From</span>
                <input
                  type="date"
                  value={afFrom()}
                  onInput={(e) => setAfFrom(e.currentTarget.value)}
                  class="px-2 py-1 text-xs border border-noviq-line rounded bg-white"
                />
              </label>
              <label class="flex flex-col gap-0.5">
                <span class="text-[0.65rem] uppercase tracking-wider text-noviq-500">To</span>
                <input
                  type="date"
                  value={afTo()}
                  onInput={(e) => setAfTo(e.currentTarget.value)}
                  class="px-2 py-1 text-xs border border-noviq-line rounded bg-white"
                />
              </label>
              <label class="flex flex-col gap-0.5">
                <span class="text-[0.65rem] uppercase tracking-wider text-noviq-500">User</span>
                <select
                  value={afUser()}
                  onChange={(e) => setAfUser(e.currentTarget.value)}
                  class="px-2 py-1 text-xs border border-noviq-line rounded bg-white max-w-[10rem]"
                >
                  <option value="">Everyone</option>
                  <For each={users()}>
                    {(u) => <option value={u.id}>{u.username}</option>}
                  </For>
                </select>
              </label>
              <label class="flex flex-col gap-0.5">
                <span class="text-[0.65rem] uppercase tracking-wider text-noviq-500">Action</span>
                <select
                  value={afAction()}
                  onChange={(e) => setAfAction(e.currentTarget.value)}
                  class="px-2 py-1 text-xs border border-noviq-line rounded bg-white max-w-[12rem]"
                >
                  <option value="">All actions</option>
                  <For each={auditActions()}>
                    {(a) => <option value={a}>{AUDIT_ACTION_LABELS[a] ?? a}</option>}
                  </For>
                </select>
              </label>
              <label class="flex flex-col gap-0.5 flex-1 min-w-[10rem]">
                <span class="text-[0.65rem] uppercase tracking-wider text-noviq-500">Contains</span>
                <input
                  type="text"
                  value={afSearch()}
                  placeholder="document, user, detail…"
                  onInput={(e) => setAfSearch(e.currentTarget.value)}
                  onKeyDown={(e) => e.key === "Enter" && loadAudit(0)}
                  class="px-2 py-1 text-xs border border-noviq-line rounded bg-white w-full"
                />
              </label>
              <button
                onClick={() => loadAudit(0)}
                disabled={auditBusy()}
                class="px-2.5 py-1 text-xs font-medium border border-noviq-line rounded hover:bg-noviq-100 disabled:opacity-50"
              >
                {auditBusy() ? "Loading…" : "Apply"}
              </button>
              <button
                onClick={auditClear}
                class="px-2.5 py-1 text-xs font-medium border border-noviq-line rounded hover:bg-noviq-100"
              >
                Clear
              </button>
            </div>

            {/* Quick spans — the ranges an inspection actually asks for. */}
            <div class="flex items-center gap-1.5 mb-3 flex-wrap">
              <span class="text-[0.65rem] uppercase tracking-wider text-noviq-500">Quick</span>
              <button onClick={() => auditQuickRange(1)} class="px-2 py-0.5 text-xs border border-noviq-line rounded hover:bg-noviq-100">Today</button>
              <button onClick={() => auditQuickRange(7)} class="px-2 py-0.5 text-xs border border-noviq-line rounded hover:bg-noviq-100">Last 7 days</button>
              <button onClick={() => auditQuickRange(30)} class="px-2 py-0.5 text-xs border border-noviq-line rounded hover:bg-noviq-100">Last 30 days</button>
            </div>

            <Show when={auditMsg()}>
              <p class="text-xs text-red-500 mb-2">{auditMsg()}</p>
            </Show>

            <Show
              when={auditRows().length > 0}
              fallback={
                <p class="text-xs text-noviq-500">
                  {auditBusy() ? "Loading…" : "No audit entries match these filters."}
                </p>
              }
            >
              <div class="max-h-96 overflow-auto border border-noviq-line rounded bg-white">
                <table class="w-full min-w-[48rem] text-sm text-noviq-ink">
                  <thead class="sticky top-0 bg-noviq-50">
                    <tr class="text-left text-noviq-500 border-b border-noviq-line">
                      <th class="py-1.5 px-2">Time (UTC)</th>
                      <th class="py-1.5 px-2">User</th>
                      <th class="py-1.5 px-2">Action</th>
                      <th class="py-1.5 px-2">Document / target</th>
                      <th class="py-1.5 px-2">Details</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={auditRows()}>
                      {(r) => (
                        <tr
                          class="border-b border-noviq-line/60 hover:bg-noviq-100 cursor-pointer"
                          onClick={() => setAuditSel(r)}
                          title="Show the full entry"
                        >
                          <td class="py-1.5 px-2 whitespace-nowrap tabular-nums text-noviq-500">
                            {fmtAuditTime(r.created_at)}
                          </td>
                          <td class="py-1.5 px-2 whitespace-nowrap">{r.username ?? "—"}</td>
                          <td
                            class={`py-1.5 px-2 whitespace-nowrap ${
                              r.action === "document_denied" ? "text-red-600 font-medium" : ""
                            }`}
                          >
                            {AUDIT_ACTION_LABELS[r.action ?? ""] ?? r.action ?? "—"}
                          </td>
                          <td class="py-1.5 px-2 max-w-xs truncate">
                            {r.target_name ?? r.target ?? "—"}
                          </td>
                          <td class="py-1.5 px-2 max-w-xs truncate text-noviq-500">
                            {r.details_json ?? ""}
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>

              {/* Paging — the count is the honest total for the filter, not the
                  page size, so an admin can see how much a download will hold. */}
              <div class="flex items-center justify-between mt-2 gap-2 flex-wrap">
                <span class="text-xs text-noviq-500 tabular-nums">
                  {auditOffset() + 1}–{Math.min(auditOffset() + auditRows().length, auditTotal())}
                  {" of "}
                  {auditTotal()} entries
                </span>
                <div class="flex items-center gap-1.5">
                  <button
                    onClick={() => loadAudit(Math.max(0, auditOffset() - AUDIT_PAGE))}
                    disabled={auditOffset() === 0 || auditBusy()}
                    class="px-2.5 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-100 disabled:opacity-40"
                  >
                    Previous
                  </button>
                  <button
                    onClick={() => loadAudit(auditOffset() + AUDIT_PAGE)}
                    disabled={auditOffset() + auditRows().length >= auditTotal() || auditBusy()}
                    class="px-2.5 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-100 disabled:opacity-40"
                  >
                    Next
                  </button>
                </div>
              </div>
            </Show>
          </div>

          {/* Entry detail — the raw row, for when the truncated cell isn't enough. */}
          <Show when={auditSel()}>
            <div
              class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
              onClick={() => setAuditSel(null)}
            >
              <div
                class="bg-white rounded-lg shadow-xl max-w-2xl w-full max-h-[85vh] flex flex-col"
                onClick={(e) => e.stopPropagation()}
              >
                <div class="flex items-center justify-between px-4 py-2 border-b border-noviq-line">
                  <span class="text-sm font-semibold text-noviq-800 truncate">
                    {AUDIT_ACTION_LABELS[auditSel()?.action ?? ""] ?? auditSel()?.action ?? "Entry"}
                    {" · "}
                    {fmtAuditTime(auditSel()!.created_at)} UTC
                  </span>
                  <button
                    onClick={() => setAuditSel(null)}
                    class="px-2 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-100"
                  >
                    Close
                  </button>
                </div>
                <div class="flex-1 overflow-auto p-3 text-xs text-noviq-ink space-y-1.5">
                  <p><span class="text-noviq-500">User:</span> {auditSel()?.username ?? "—"} <span class="text-noviq-400">({auditSel()?.user_id ?? "no id"})</span></p>
                  <p><span class="text-noviq-500">Action:</span> {auditSel()?.action ?? "—"}</p>
                  <p><span class="text-noviq-500">Document:</span> {auditSel()?.target_name ?? "—"}</p>
                  <p><span class="text-noviq-500">Target id:</span> {auditSel()?.target ?? "—"}</p>
                  <p class="text-noviq-500">Details:</p>
                  <pre class="font-mono whitespace-pre-wrap break-all bg-noviq-50 p-2 rounded border border-noviq-line">
                    {auditSel()?.details_json ?? "—"}
                  </pre>
                </div>
              </div>
            </div>
          </Show>
          </Show>{/* ══ end AUDIT LOG TAB ══ */}

          {/* ══ USERS TAB (continued): the user list table ══ */}
          <Show when={activeTab() === "users"}>
          {/* User list with per-user usage (queries, last active, CPU/RAM/GPU) */}
          {/* `stack-table`: 10 columns cannot be read at 390px, so below `md`
              each row becomes a labelled card (see index.css). */}
          <table class="stack-table w-full text-sm border-collapse">
            <thead>
              <tr class="text-left text-noviq-500 border-b border-noviq-line">
                <th class="py-2">Username</th>
                <th class="py-2">Role</th>
                <th class="py-2">Status</th>
                <th class="py-2 text-right">Queries</th>
                <th class="py-2">Last active</th>
                <th class="py-2 text-right">Avg CPU</th>
                <th class="py-2 text-right">Peak RAM</th>
                <th class="py-2 text-right">Avg GPU</th>
                <th class="py-2 text-right">Avg ms</th>
                <th class="py-2"></th>
              </tr>
            </thead>
            <tbody>
              <For each={users()}>
                {(u) => (
                  <>
                    <tr class="border-b border-noviq-line/50">
                      <td data-label="Username" class="py-2 font-medium">{u.username}</td>
                      <td data-label="Role" class="py-2">{u.role}</td>
                      <td data-label="Status" class="py-2">
                        <Show when={u.disabled_at} fallback={<span class="text-green-600">active</span>}>
                          <span class="text-red-500">disabled</span>
                        </Show>
                      </td>
                      <td data-label="Queries" class="py-2 text-right tabular-nums">{u.query_count}</td>
                      <td data-label="Last active" class="py-2 text-xs text-noviq-600 whitespace-nowrap">{fmtTime(u.last_active)}</td>
                      <td data-label="Avg CPU" class="py-2 text-right tabular-nums">{fmtNum(u.avg_cpu_pct)}%</td>
                      <td data-label="Peak RAM" class="py-2 text-right tabular-nums">{fmtNum(u.peak_ram_mb)} MB</td>
                      <td data-label="Avg GPU" class="py-2 text-right tabular-nums">
                        {u.avg_gpu_util_pct === null ? "—" : `${fmtNum(u.avg_gpu_util_pct)}%`}
                      </td>
                      <td data-label="Avg ms" class="py-2 text-right tabular-nums">{fmtNum(u.avg_latency_ms)}</td>
                      <td data-label="" class="py-2">
                        <div class="flex flex-wrap items-center justify-end gap-1.5">
                          <button
                            class="px-2 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-50"
                            onClick={() => toggleActivity(u)}
                            title="Show recent queries"
                          >
                            {expanded() === u.id ? "Hide" : "Activity"}
                          </button>
                          <button
                            class="px-2 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-50"
                            onClick={() => exportUser(u, "json")}
                            title="Download all queries as JSON"
                          >
                            JSON
                          </button>
                          <button
                            class="px-2 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-50"
                            onClick={() => exportUser(u, "csv")}
                            title="Download all queries as CSV"
                          >
                            CSV
                          </button>
                          <button
                            class="px-2 py-1 text-xs border border-noviq-line rounded hover:bg-noviq-50"
                            onClick={() => toggleDisabled(u)}
                          >
                            {u.disabled_at ? "Enable" : "Disable"}
                          </button>
                        </div>
                      </td>
                    </tr>
                    <Show when={expanded() === u.id}>
                      <tr>
                        <td data-label="" colspan="10" class="bg-noviq-panel/40 px-3 py-3">
                          <Show
                            when={(activity()[u.id]?.length ?? 0) > 0}
                            fallback={<p class="text-xs text-noviq-500">No queries recorded yet.</p>}
                          >
                            <div class="space-y-2 max-h-96 overflow-auto">
                              <For each={activity()[u.id]}>
                                {(q) => (
                                  <div class="border border-noviq-line rounded p-2 bg-white">
                                    <div class="flex items-center justify-between text-xs text-noviq-500 mb-1">
                                      <span>{fmtTime(q.created_at)}</span>
                                      <span class="tabular-nums">
                                        {fmtNum(q.latency_ms)} ms · CPU {fmtNum(q.cpu_pct)}% · RAM{" "}
                                        {fmtNum(q.ram_mb)} MB
                                        {q.gpu_util_pct === null
                                          ? ""
                                          : ` · GPU ${fmtNum(q.gpu_util_pct)}% / ${fmtNum(q.gpu_mem_mb)} MB`}
                                      </span>
                                    </div>
                                    <p class="text-sm text-noviq-800 font-medium">{q.question}</p>
                                    <Show when={q.answer}>
                                      <p class="text-xs text-noviq-600 mt-1 whitespace-pre-wrap line-clamp-4">
                                        {q.answer}
                                      </p>
                                    </Show>
                                  </div>
                                )}
                              </For>
                            </div>
                          </Show>
                        </td>
                      </tr>
                    </Show>
                  </>
                )}
              </For>
            </tbody>
          </table>
          </Show>{/* ══ end USERS TAB (continued) ══ */}
        </Show>
      </Show>
      </div>
    </div>
  );
};

export default Admin;
