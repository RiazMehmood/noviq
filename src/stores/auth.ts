/**
 * Web auth store (F014, US2) — the headless server enforces `require_auth` on
 * every route except `/health` and `/login`, so the browser thin client must
 * obtain and carry a session token. Desktop (Tauri) has no auth and never uses
 * this store.
 *
 * The transport keeps the bearer token in memory (`setAuthToken`); we persist it
 * in localStorage so a page reload survives, and restore it into the transport
 * synchronously at module load so any early authed request already carries it.
 */
import { createSignal } from "solid-js";
import { isTauri, setAuthToken, request, ApiError } from "../api/transport";
import { clearChatCache } from "../lib/chatCache";

const TOKEN_KEY = "noviq:authToken";

export interface CurrentUser {
  id: string;
  username: string;
  role: string;
  /** Whether this user may use Deep search (admin-only while it's being smoothed;
   *  ECHO_DEEP_AUDIENCE=all opens it to everyone). Server-computed on /me. */
  deep_search_enabled?: boolean;
}

const [currentUser, setCurrentUser] = createSignal<CurrentUser | null>(null);
const [authReady, setAuthReady] = createSignal(false);
export { currentUser, authReady };

export function isAdmin(): boolean {
  return currentUser()?.role === "admin";
}

/** Whether Deep search is offered to the signed-in user. Server-gated (admin-only
 *  for now); derived from the canonical, boot-validated session so it can't race a
 *  page-level second `/me`. */
export function deepSearchEnabled(): boolean {
  return !!currentUser()?.deep_search_enabled;
}

// Restore a persisted token into the transport immediately (before any request).
if (!isTauri() && typeof localStorage !== "undefined") {
  const saved = localStorage.getItem(TOKEN_KEY);
  if (saved) setAuthToken(saved);
}

interface LoginResponse {
  token: string;
  expires_at: number;
}

/** POST /api/login, persist the token, and load the current user. */
export async function login(username: string, password: string): Promise<void> {
  const res = await request<LoginResponse>("POST", "/login", { username, password });
  localStorage.setItem(TOKEN_KEY, res.token);
  setAuthToken(res.token);
  const me = await request<CurrentUser>("GET", "/me");
  setCurrentUser(me);
}

/**
 * Free-trial sign-in: exchange a Google ID token for a Noviq session.
 *
 * Mirrors `login` exactly from the token onwards — same storage key, same
 * `/me` refresh — so everything downstream (transport headers, `currentUser`,
 * the router's auth gate) cannot tell the two paths apart. The DIFFERENCE
 * between a trial and a full account lives entirely on the server, in the
 * session's role; nothing here should ever try to enforce it.
 */
export async function loginWithGoogle(credential: string): Promise<void> {
  const res = await request<LoginResponse>("POST", "/auth/google", { credential });
  localStorage.setItem(TOKEN_KEY, res.token);
  setAuthToken(res.token);
  const me = await request<CurrentUser>("GET", "/me");
  setCurrentUser(me);
}

/** Best-effort server logout + local clear. */
export async function logout(): Promise<void> {
  try {
    await request("POST", "/logout");
  } catch {
    /* already invalid / offline — clear locally regardless */
  }
  localStorage.removeItem(TOKEN_KEY);
  setAuthToken(null);
  setCurrentUser(null);
  // The transcript cache belongs to the officer who just signed out, and holds
  // their questions with the confidential values UNMASKED (the server only ever
  // stored the masked form). On a shared browser, leaving it behind would hand
  // that to whoever signs in next. History itself is safe on the server.
  clearChatCache();
}

/**
 * Validate any persisted token on boot. Sets `currentUser` when the token is
 * still valid; clears a stale one. Always flips `authReady` so the UI can
 * decide between the login screen and the app. No-op (ready, no user) in Tauri.
 */
export async function restoreSession(): Promise<void> {
  if (isTauri()) {
    setAuthReady(true);
    return;
  }
  const saved = typeof localStorage !== "undefined" ? localStorage.getItem(TOKEN_KEY) : null;
  if (!saved) {
    setAuthReady(true);
    return;
  }
  try {
    const me = await request<CurrentUser>("GET", "/me");
    setCurrentUser(me);
  } catch (e) {
    // 401 → stale token; drop it. Other errors (server restarting) also fall
    // through to the login screen, which the user can retry.
    if (e instanceof ApiError) {
      localStorage.removeItem(TOKEN_KEY);
      setAuthToken(null);
    }
  } finally {
    setAuthReady(true);
  }
}
