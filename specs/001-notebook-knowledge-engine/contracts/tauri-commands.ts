/**
 * EchoAI — Notebook & Knowledge Engine
 * Tauri IPC Command Contracts
 *
 * These TypeScript interfaces define the complete IPC boundary between the
 * SolidJS frontend and the Rust backend. Each `invoke()` call maps 1:1 to a
 * Rust #[tauri::command] handler.
 *
 * Convention:
 *   - Commands that mutate state return the updated entity or void.
 *   - Commands that query return typed results.
 *   - All errors surface as thrown TauriError (caught in frontend via try/catch).
 *   - Timestamps are Unix milliseconds (INTEGER in SQLite).
 */

// ─── Shared Types ─────────────────────────────────────────────────────────────

export type IndexingStatus = 'pending' | 'processing' | 'indexed' | 'failed';
export type BatchStatus    = 'queued'  | 'running'    | 'complete' | 'partial';
export type EntityType     = 'person'  | 'org'        | 'date'     | 'project' | 'decision' | 'amount';
export type MessageRole    = 'user'    | 'assistant';
export type TierUsed       = 'cache'   | 'fts'        | 'vec'      | 'tier3'   | 'tier4';
export type ExportFormat   = 'pdf'     | 'txt';
export type BatchOperation = 'summarize' | 'extract_dates' | 'flag_keywords' | 'extract_actions';
export type AnalysisTemplate =
  | 'compare_sources'
  | 'extract_action_items'
  | 'find_contradictions'
  | 'generate_timeline'
  | 'summarize_all';

export interface Citation {
  source_id:   string;
  source_name: string;
  chunk_id:    string;
  excerpt:     string;
  page_number: number | null;
}

export interface QualityNotice {
  tier_used:   TierUsed;
  message:     string;   // e.g. "Running on lightweight model — results may be less detailed"
  constrained: boolean;  // true when downgraded from intended tier due to hardware
}

// ─── Notebook Management ──────────────────────────────────────────────────────

export interface Notebook {
  id:         string;
  name:       string;
  createdAt:  number;
  updatedAt:  number;
  sourceCount: number;  // denormalised count for UI
}

/** create_notebook: Creates a new empty notebook. */
export declare function create_notebook(args: {
  name: string;
}): Promise<Notebook>;

/** list_notebooks: Returns all notebooks ordered by updatedAt desc. */
export declare function list_notebooks(): Promise<Notebook[]>;

/** get_notebook: Returns a single notebook with metadata. */
export declare function get_notebook(args: {
  id: string;
}): Promise<Notebook>;

/** rename_notebook: Renames a notebook. Errors if name is empty. */
export declare function rename_notebook(args: {
  id:   string;
  name: string;
}): Promise<void>;

/** delete_notebook: Permanently deletes notebook + all derived data (cascades). */
export declare function delete_notebook(args: {
  id: string;
}): Promise<void>;

// ─── Source Management ────────────────────────────────────────────────────────

export interface Source {
  id:             string;
  filePath:       string;
  fileName:       string;
  format:         string;
  fileSizeBytes:  number;
  indexingStatus: IndexingStatus;
  errorReason:    string | null;
  lastModified:   number;
  indexedAt:      number | null;
  createdAt:      number;
}

/**
 * add_sources: Adds one or more local file paths to a notebook.
 * Files already indexed (by path) are linked without re-indexing.
 * New files are queued as 'pending' and indexed in the background.
 * Returns the list of Source records created or linked.
 */
export declare function add_sources(args: {
  notebookId: string;
  paths:      string[];
}): Promise<Source[]>;

/** remove_source: Unlinks a source from a notebook. Does NOT delete the index
 *  entry if the source is referenced by other notebooks or indexed folders. */
export declare function remove_source(args: {
  notebookId: string;
  sourceId:   string;
}): Promise<void>;

/** list_notebook_sources: Returns all sources in a notebook with current status. */
export declare function list_notebook_sources(args: {
  notebookId: string;
}): Promise<Source[]>;

/** retry_failed_source: Re-queues a failed source for re-indexing. */
export declare function retry_failed_source(args: {
  sourceId: string;
}): Promise<void>;

// ─── Indexed Folders (Semantic Recall) ───────────────────────────────────────

export interface IndexedFolder {
  id:             string;
  folderPath:     string;
  indexingStatus: 'pending' | 'scanning' | 'indexed' | 'partial';
  fileCount:      number;
  lastScannedAt:  number | null;
  createdAt:      number;
}

/** add_indexed_folder: Grants read access to a folder for global background indexing. */
export declare function add_indexed_folder(args: {
  path: string;
}): Promise<IndexedFolder>;

/** list_indexed_folders: Returns all currently indexed folders. */
export declare function list_indexed_folders(): Promise<IndexedFolder[]>;

/**
 * revoke_indexed_folder: Revokes access and PERMANENTLY removes all index data
 * derived from this folder (chunks, embeddings, entity mentions, cached answers).
 * Completes within 5 seconds per SC-009.
 */
export declare function revoke_indexed_folder(args: {
  id: string;
}): Promise<void>;

// ─── Query & Chat ─────────────────────────────────────────────────────────────

export interface QuerySession {
  id:         string;
  notebookId: string;
  createdAt:  number;
  updatedAt:  number;
}

export interface QueryMessage {
  id:           string;
  sessionId:    string;
  role:         MessageRole;
  content:      string;
  citations:    Citation[] | null;
  tierUsed:     TierUsed   | null;
  qualityNotice: QualityNotice | null;
  createdAt:    number;
}

export interface QueryResponse {
  message:       QueryMessage;
  qualityNotice: QualityNotice | null;
}

/** create_session: Opens a new chat session scoped to a notebook. */
export declare function create_session(args: {
  notebookId: string;
}): Promise<QuerySession>;

/**
 * query_notebook: Submits a user query. The backend routes through
 * Tier 0→1→2→3→4 in order, returning the cheapest satisfying answer.
 * Returns null answer with clear message if no relevant content found (SC-003).
 */
export declare function query_notebook(args: {
  sessionId: string;
  query:     string;
}): Promise<QueryResponse>;

/** get_session_history: Returns all messages in a session ordered by createdAt. */
export declare function get_session_history(args: {
  sessionId: string;
}): Promise<QueryMessage[]>;

// ─── Global Search (Semantic Recall) ─────────────────────────────────────────

export interface SearchResult {
  sourceId:     string;
  sourceName:   string;
  filePath:     string;
  lastModified: number;
  excerpt:      string;
  relevanceScore: number;  // 0.0–1.0
}

/**
 * global_search: Semantic search across ALL indexed folders and notebooks.
 * Routes through tier routing logic; returns up to 20 results ranked by relevance.
 * Results within 3 seconds on minimum hardware (SC-005).
 */
export declare function global_search(args: {
  query: string;
}): Promise<SearchResult[]>;

// ─── Knowledge Graph & Entities ───────────────────────────────────────────────

export interface Entity {
  id:              string;
  canonicalValue:  string;
  entityType:      EntityType;
  normalizedValue: string | null;
  sourceCount:     number;
  createdAt:       number;
}

export interface Relationship {
  id:               string;
  fromEntity:       Entity;
  toEntity:         Entity;
  relationshipType: string;
  sourceChunkId:    string | null;
}

export interface EntityQueryResult {
  entity:   Entity;
  mentions: { sourceId: string; sourceName: string; contextText: string }[];
}

/**
 * query_entities: Structured query against the extracted entity store.
 * Returns results from SQLite directly — no LLM inference (SC-004 / FR-024).
 * Results in under 1 second per acceptance scenario (US4).
 */
export declare function query_entities(args: {
  query:      string;           // natural language — parsed to SQL by query classifier
  entityType?: EntityType;      // optional filter
  dateFrom?:  number;           // Unix ms
  dateTo?:    number;
}): Promise<EntityQueryResult[]>;

/** get_entity_relationships: Returns all graph edges for an entity. */
export declare function get_entity_relationships(args: {
  entityId: string;
}): Promise<Relationship[]>;

/** get_entity_sources: Returns all source documents that mention an entity. */
export declare function get_entity_sources(args: {
  entityId: string;
}): Promise<Source[]>;

/**
 * get_knowledge_graph: Returns entity + relationship subgraph for rendering.
 * Paginated to max 500 entities per FR (Assumptions). Pass entityType filter
 * or search term to scope the subgraph.
 */
export declare function get_knowledge_graph(args: {
  entityType?: EntityType;
  search?:     string;
  limit?:      number;   // default 500
  offset?:     number;
}): Promise<{ entities: Entity[]; relationships: Relationship[] }>;

// ─── Analysis Templates ───────────────────────────────────────────────────────

export interface AnalysisResult {
  id:            string;
  notebookId:    string;
  template:      AnalysisTemplate;
  outputText:    string;
  citations:     Citation[];
  qualityNotice: QualityNotice | null;
  createdAt:     number;
}

/**
 * run_analysis: Runs a built-in analysis template across all sources in a notebook.
 * Available on all hardware tiers (FR-027a); quality notice included when
 * running on constrained hardware (8 GB, Tier 3 only).
 */
export declare function run_analysis(args: {
  notebookId: string;
  template:   AnalysisTemplate;
  params?:    Record<string, unknown>;  // e.g. { keywords: ['deadline'] }
}): Promise<AnalysisResult>;

/**
 * export_analysis: Exports an analysis result to a local file.
 * destinationPath is the full path including filename.
 */
export declare function export_analysis(args: {
  resultId:        string;
  format:          ExportFormat;
  destinationPath: string;
}): Promise<void>;

// ─── Batch Processing ─────────────────────────────────────────────────────────

export interface BatchJob {
  id:             string;
  folderPath:     string;
  operation:      BatchOperation;
  status:         BatchStatus;
  totalFiles:     number;
  processedFiles: number;
  failedFiles:    number;
  createdAt:      number;
  completedAt:    number | null;
}

export interface BatchResult {
  id:          string;
  jobId:       string;
  sourceId:    string | null;
  filePath:    string;
  status:      'success' | 'failed';
  outputText:  string | null;
  errorReason: string | null;
  createdAt:   number;
}

/** create_batch_job: Queues a batch processing job. Runs in background (FR-032). */
export declare function create_batch_job(args: {
  folderPath: string;
  operation:  BatchOperation;
  params?:    Record<string, unknown>;
}): Promise<BatchJob>;

/** get_batch_job: Returns current status of a batch job. */
export declare function get_batch_job(args: {
  id: string;
}): Promise<BatchJob>;

/** get_batch_results: Returns per-file results for a completed/partial batch job. */
export declare function get_batch_results(args: {
  jobId:  string;
  limit?: number;
  offset?: number;
}): Promise<BatchResult[]>;

// ─── Notebook Archive (Export / Import) ──────────────────────────────────────

/**
 * export_notebook: Exports notebook as a .echoai archive (ZIP).
 * destinationPath must include filename and .echoai extension.
 * Contains: manifest.json, sources/, db.sqlite (notebook-scoped).
 */
export declare function export_notebook(args: {
  notebookId:      string;
  destinationPath: string;
}): Promise<void>;

/**
 * import_notebook: Imports a .echoai archive.
 * If the archive's schema is outdated, sources are re-indexed automatically.
 * Returns the restored Notebook record.
 */
export declare function import_notebook(args: {
  archivePath: string;
}): Promise<Notebook>;

// ─── Audit Log ────────────────────────────────────────────────────────────────

export interface AuditEntry {
  id:          string;
  eventType:   string;
  entityId:    string | null;
  detailsJson: string;
  createdAt:   number;
}

/** get_audit_log: Returns audit entries, newest first, paginated. */
export declare function get_audit_log(args: {
  limit?:  number;
  offset?: number;
}): Promise<AuditEntry[]>;

// ─── System / Hardware ────────────────────────────────────────────────────────

export interface HardwareTier {
  ramGb:          number;
  hasDiscreteGpu: boolean;
  vramGb:         number | null;
  maxModelTier:   1 | 2 | 3 | 4 | 5;  // highest tier available on this hardware
  embeddingModel: string;              // 'nomic-embed-text-v1.5' | 'all-MiniLM-L6-v2'
}

/** get_hardware_tier: Returns detected hardware capabilities and active configuration. */
export declare function get_hardware_tier(): Promise<HardwareTier>;
