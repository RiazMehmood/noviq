/**
 * F014 (ADR-0017) — transport seam.
 *
 * One call surface for both runtimes so the UI code is shared (FR-001/FR-003):
 *   - Desktop (Tauri): dispatch to in-process IPC via `invoke`.
 *   - Web (browser):   POST to the headless server's HTTP API at `/api/<cmd>`.
 *
 * Detection is by `window.__TAURI__` (present only inside the Tauri window).
 * Pages/stores should import `call` from here instead of `invoke` directly;
 * migration of existing call sites is tracked by T010.
 */
import { invoke } from "@tauri-apps/api/core";

/** True when running inside the Tauri desktop shell. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI__" in window;
}

/** Session bearer token for the web transport (set by login, US2/T017). */
let authToken: string | null = null;
export function setAuthToken(token: string | null): void {
  authToken = token;
}

/** Indexing progress tick (mirrors the Rust `IndexingProgress` serde shape). */
export interface IndexProgress {
  source_id: string;
  percent: number;
  stage: string;
}

/**
 * Subscribe to live indexing progress (T024/T025). Desktop → Tauri
 * `source:indexing-progress` event; web → `EventSource` over the SSE endpoint
 * `GET /api/index/events?token=…` (the browser `EventSource` API can't send an
 * Authorization header, so the bearer token rides in the query string). Returns
 * an unsubscribe function (sync).
 */
export function onIndexProgress(cb: (p: IndexProgress) => void): () => void {
  if (isTauri()) {
    let un: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) =>
      listen<IndexProgress>("source:indexing-progress", (e) => cb(e.payload)).then((f) => {
        un = f;
      }),
    );
    return () => un?.();
  }
  const qs = authToken ? `?token=${encodeURIComponent(authToken)}` : "";
  const es = new EventSource(`/api/index/events${qs}`);
  es.onmessage = (e) => {
    try {
      cb(JSON.parse(e.data) as IndexProgress);
    } catch {
      /* ignore keep-alive comments / malformed frames */
    }
  };
  return () => es.close();
}

/** Handlers for the live chat-turn stream (web transport). */
export interface ChatStreamHandlers {
  onToken?: (token: string) => void;
  onPhase?: (phase: string) => void;
}

/**
 * Subscribe to the live token/phase stream of one conversation's in-flight turn
 * (`GET /api/chat/events?token=…&conversation_id=…`). Web-only — the desktop
 * gets the same events over Tauri (`onAssistantToken`/`onAssistantProgress`),
 * so this returns a no-op closer in Tauri. Open it right before `sendMessage`
 * and close it when the response lands.
 */
export function openChatStream(conversationId: string, h: ChatStreamHandlers): () => void {
  if (isTauri()) return () => {};
  const qs =
    `?token=${encodeURIComponent(authToken ?? "")}` +
    `&conversation_id=${encodeURIComponent(conversationId)}`;
  const es = new EventSource(`/api/chat/events${qs}`);
  es.addEventListener("chat://token", (e) => {
    try {
      h.onToken?.((JSON.parse((e as MessageEvent).data) as { token: string }).token);
    } catch { /* malformed frame */ }
  });
  es.addEventListener("assistant://progress", (e) => {
    try {
      h.onPhase?.((JSON.parse((e as MessageEvent).data) as { phase: string }).phase);
    } catch { /* malformed frame */ }
  });
  return () => es.close();
}

/** Structured error mirroring the server's `{error:{code,message}}` envelope. */
export class ApiError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.name = "ApiError";
  }
}

/**
 * Web-only REST call against an explicit `/api` path/method (e.g. the US7 admin
 * endpoints `GET /api/admin/users`, `POST /api/admin/users/:id/disable`, which
 * don't follow the `POST /api/<cmd>` command convention). Throws in Tauri —
 * these surfaces are server-only.
 */
export async function request<T = unknown>(
  method: "GET" | "POST" | "PUT" | "DELETE",
  path: string,
  body?: unknown,
): Promise<T> {
  if (isTauri()) {
    throw new ApiError("unsupported", "REST endpoints are only available over the web transport");
  }
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (authToken) headers["Authorization"] = `Bearer ${authToken}`;

  const res = await fetch(`/api${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  if (!res.ok) {
    let code = String(res.status);
    let message = res.statusText;
    try {
      const errBody = await res.json();
      if (errBody?.error) {
        code = errBody.error.code ?? code;
        message = errBody.error.message ?? message;
      }
    } catch {
      /* non-JSON error body — keep status text */
    }
    throw new ApiError(code, message);
  }

  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** One file the server declined, with a user-readable reason (F021 upload UX). */
export interface UploadRejected {
  file_name: string;
  reason: string;
}

/**
 * An upload that holds several legal instruments rather than one. Indexing still
 * ran; this is a prompt to split it, because a compendium indexed as one
 * document makes every "Rule 4" inside it ambiguous. Admin uploads only.
 */
export interface MultiInstrumentNotice {
  file_name: string;
  instrument_count: number;
  sample_titles: string[];
}

/**
 * An upload whose TITLE closely matches something already indexed. Indexing
 * still ran — this warns, it never blocks, because only the uploader can tell an
 * amended instrument (two documents) from a re-download (one document twice).
 * Byte-identical files never reach here; those are deduped by content.
 */
export interface SimilarNotice {
  file_name: string;
  matches: string[];
}

/** Response of `POST /api/upload_documents` — accepted folder + per-file rejections. */
export interface UploadOutcome {
  folder: unknown | null;
  rejected: UploadRejected[];
  /** Absent when nothing uploaded held more than one instrument. */
  multi_instrument?: MultiInstrumentNotice[];
  /** Absent when nothing uploaded resembled an existing document. */
  similar_existing?: SimilarNotice[];
  /**
   * Source ids the server bound to the project, when `projectId` was sent.
   * The only trustworthy confirmation of a project upload: since migration 024
   * these documents are hidden from folder listings by design, so re-deriving
   * them client-side finds nothing.
   */
  attached?: string[];
}

/**
 * Web-only multipart upload of the user's own files to the server, which then
 * indexes them (F014 `POST /api/upload_documents`). The browser sets the
 * multipart boundary, so we must NOT set Content-Type ourselves.
 */
export async function uploadDocuments(
  files: File[],
  projectId?: string,
  categoryId?: string,
): Promise<UploadOutcome> {
  if (isTauri()) {
    throw new ApiError("unsupported", "upload is only available over the web transport");
  }
  const form = new FormData();
  for (const f of files) {
    // Preserve the relative path for folder uploads; the server flattens to a
    // safe basename anyway.
    const rel = (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name;
    form.append("files", f, rel);
  }
  // F027 — uploading INTO a project. The server has always read this field
  // (it owns the project check, its own upload folder and the membership
  // rows); the client simply never sent it, so every project upload took the
  // ordinary corpus path and inherited its visibility. Sending it is what
  // keeps a project document out of everyone else's document tree.
  if (projectId) form.append("projectId", projectId);
  // Chosen in the add-folder dialog before the upload starts, so a large corpus
  // is filed as it indexes instead of landing in one Uncategorised pile that has
  // to be sorted by hand afterwards. Admin-only; the server rejects it otherwise.
  if (categoryId) form.append("categoryId", categoryId);
  const headers: Record<string, string> = {};
  if (authToken) headers["Authorization"] = `Bearer ${authToken}`;

  const res = await fetch(`/api/upload_documents`, { method: "POST", headers, body: form });
  if (!res.ok) {
    let code = String(res.status);
    let message = res.statusText;
    try {
      const errBody = await res.json();
      if (errBody?.error) {
        code = errBody.error.code ?? code;
        message = errBody.error.message ?? message;
      }
    } catch {
      /* non-JSON error body */
    }
    throw new ApiError(code, message);
  }
  if (res.status === 204) return { folder: null, rejected: [] };
  const body = (await res.json()) as Partial<UploadOutcome>;
  // ⚠️ Copy the WHOLE outcome, not just folder+rejected (2026-08-29). Rebuilding
  // a fresh object here silently dropped `attached`, so every project upload
  // reported "Uploaded, but no document was indexed" although the server had
  // already attached the rows — the one confirmation the client cannot re-derive
  // (a project document is hidden from folder listings by design). The same drop
  // also killed the split-instrument prompt and the near-duplicate warning.
  return {
    folder: body.folder ?? null,
    rejected: body.rejected ?? [],
    multi_instrument: body.multi_instrument ?? [],
    similar_existing: body.similar_existing ?? [],
    attached: body.attached ?? [],
  };
}

/** One conversation attachment row (F021 US4). */
export interface ConversationAttachment {
  sourceId: string;
  fileName: string;
  status: string;
  failReason?: string | null;
}

/**
 * Web-only (F021 US4): attach files to ONE conversation — the server indexes
 * them scoped to that conversation only (`POST /api/attach_document`,
 * multipart: conversationId + files).
 */
export async function attachDocuments(
  conversationId: string,
  files: File[],
  /**
   * Upload progress 0–100 (bytes sent / bytes total). `fetch` cannot report
   * upload progress, so this uses `XMLHttpRequest`, whose `upload.onprogress`
   * works even through the Cloudflare quick tunnel (unlike the SSE index stream,
   * which the tunnel buffers to nothing). Reaching 100 means the bytes are on the
   * server; the server then parses/OCRs/embeds before responding, which is why
   * the caller switches to an indeterminate ring at that point.
   */
  onUploadProgress?: (percent: number) => void,
): Promise<ConversationAttachment[]> {
  if (isTauri()) {
    throw new ApiError("unsupported", "attachments are only available over the web transport");
  }
  const form = new FormData();
  form.append("conversationId", conversationId);
  for (const f of files) form.append("files", f, f.name);

  return new Promise<ConversationAttachment[]>((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", `/api/attach_document`);
    if (authToken) xhr.setRequestHeader("Authorization", `Bearer ${authToken}`);
    // Do NOT set Content-Type — the browser must add the multipart boundary.

    xhr.upload.onprogress = (e) => {
      if (onUploadProgress && e.lengthComputable && e.total > 0) {
        onUploadProgress(Math.min(100, (e.loaded / e.total) * 100));
      }
    };
    xhr.onerror = () => reject(new ApiError("network", "upload failed — check your connection"));
    xhr.onabort = () => reject(new ApiError("aborted", "upload cancelled"));
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        try {
          resolve(JSON.parse(xhr.responseText) as ConversationAttachment[]);
        } catch {
          reject(new ApiError("bad_response", "server returned an unreadable response"));
        }
        return;
      }
      let code = String(xhr.status);
      let message = xhr.statusText;
      try {
        const errBody = JSON.parse(xhr.responseText);
        if (errBody?.error) {
          code = errBody.error.code ?? code;
          message = errBody.error.message ?? message;
        }
      } catch { /* non-JSON error body */ }
      reject(new ApiError(code, message));
    };
    xhr.send(form);
  });
}

/**
 * Web-only: GET an authenticated endpoint and save the response body as a file
 * download. A plain `<a download>` link can't carry the bearer token, so we
 * fetch with the Authorization header, then trigger the download from a blob
 * object-URL. Used by the admin per-user query export (JSON/CSV).
 */
/**
 * Open an authenticated file in a NEW TAB rather than saving it (2026-07-28,
 * tester request: verify a citation's page without leaving the answer).
 *
 * The API needs a bearer token, so a plain `<a target="_blank">` cannot be
 * used — fetch the bytes, then hand the browser a blob URL. `fragment`
 * (e.g. `#page=17`) is appended so a PDF opens AT the cited page.
 *
 * The blob URL is revoked on a delay, not immediately: the new tab needs it to
 * still resolve when it loads. Popup blockers can stop `window.open` — the
 * caller gets `false` and can tell the user.
 */
export async function openAuthedInNewTab(path: string, fragment = ""): Promise<boolean> {
  if (isTauri()) {
    throw new ApiError("unsupported", "opening a source is only available over the web transport");
  }
  const headers: Record<string, string> = {};
  if (authToken) headers["Authorization"] = `Bearer ${authToken}`;
  const res = await fetch(`/api${path}`, { method: "GET", headers });
  if (!res.ok) throw new ApiError(String(res.status), res.statusText);
  const url = URL.createObjectURL(await res.blob());
  const win = window.open(url + fragment, "_blank", "noopener");
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
  return win != null;
}

export async function downloadAuthed(path: string, filename: string): Promise<void> {
  if (isTauri()) {
    throw new ApiError("unsupported", "download is only available over the web transport");
  }
  const headers: Record<string, string> = {};
  if (authToken) headers["Authorization"] = `Bearer ${authToken}`;
  const res = await fetch(`/api${path}`, { method: "GET", headers });
  if (!res.ok) throw new ApiError(String(res.status), res.statusText);
  const blob = await res.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/**
 * Invoke a backend command. `cmd` is the Tauri command name, which maps 1:1 to
 * `POST /api/<cmd>` on the web transport (contracts/http-api.md).
 */
export async function call<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isTauri()) {
    return invoke<T>(cmd, args);
  }

  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (authToken) headers["Authorization"] = `Bearer ${authToken}`;

  const res = await fetch(`/api/${cmd}`, {
    method: "POST",
    headers,
    body: JSON.stringify(args ?? {}),
  });

  if (!res.ok) {
    let code = String(res.status);
    let message = res.statusText;
    try {
      const body = await res.json();
      if (body?.error) {
        code = body.error.code ?? code;
        message = body.error.message ?? message;
      }
    } catch {
      /* non-JSON error body — keep status text */
    }
    throw new ApiError(code, message);
  }

  // 204 No Content → undefined.
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}
