/**
 * App — T047 + T089 (Polish).
 * Routes: / ChatPage, /search SearchPage, /workflow, /meeting, /settings SettingsPage, /admin.
 *
 * SolidJS Router v0.15 pattern: nested routes where AppShell is the layout route at "/".
 * Child routes render into props.children (the RouteSectionProps children slot).
 * This is more reliable than root={AppShell} + props.children in WebKit/Linux.
 */
import { Component, createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Router, Route, useNavigate, useLocation } from "@solidjs/router";
import type { RouteSectionProps } from "@solidjs/router";
// The head mark on its own, trimmed and with the source PNG's white plate
// removed — `noviq_logo_3.png` is a 1.7 MB square whose wordmark is illegible at
// bar height and whose white background painted a visible box on the canvas.
import markUrl from "./assets/noviq_mark.png";
import ChatPage from "./pages/ChatPage";
import SearchPage from "./pages/SearchPage";
import SettingsPage from "./pages/SettingsPage";
import Workflow from "./pages/Workflow";
import MeetingAssistant from "./pages/MeetingAssistant";
import Admin from "./pages/Admin";
// F027 — My Projects
import ProjectsPage from "./pages/ProjectsPage";
import { projectsEnabled, refreshProjectsEnabled } from "./stores/projects";
import SharedChatPage from "./pages/SharedChatPage";
import { PrivacyPolicy, TermsOfService } from "./components/LegalPages";
import Overlay from "./pages/Overlay";
import ModelDownloadBanner from "./components/ModelDownloadBanner";
import ServerStatusBanner from "./components/ServerStatusBanner";
import NavBadge from "./components/NavBadge";
import TrialBanner from "./components/TrialBanner";
import { loadHardwareTier } from "./stores/system";
import { setForeground, WorkloadKind } from "./stores/scheduler";
import { startServerStatusPolling, stopServerStatusPolling } from "./stores/serverStatus";
import { isTauri, call } from "./api/transport";
import { authReady, currentUser, isAdmin, logout, restoreSession } from "./stores/auth";
import LoginScreen from "./components/LoginScreen";
import { useIsMobile } from "./lib/breakpoints";

/**
 * Nav glyphs — one `d` per page, all drawn on the same 24px grid at stroke 1.8
 * so they read as a set. Paths rather than components because each is a single
 * stroke and the nav renders them from a data list.
 */
const NAV_ICON = {
  chat: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
  search: "M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16zm10 2-4.35-4.35",
  projects: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
  workflow: "M6 4a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm0 12a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm12-6a2 2 0 1 1 0 4 2 2 0 0 1 0-4zM8 6h4a2 2 0 0 1 2 2v2M8 18h4a2 2 0 0 0 2-2v-2",
  meeting: "M9 11a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7zm-7 9a7 7 0 0 1 14 0M17.5 11a3 3 0 1 0-2-5.2M22 19.5a5.5 5.5 0 0 0-4-5.3",
};

const NavBar: Component = () => {
  const navigate = useNavigate();
  const location = useLocation();

  // Web-only: reveal the Admin link to admin sessions only (the server also
  // 403s the endpoints, so this is the UX half of the gate). Driven by the
  // shared auth store → reactive after login/logout. Hidden in Tauri.
  const showAdmin = () => !isTauri() && isAdmin();

  // Top-bar menus (profile avatar + notifications + the mobile page menu).
  // One open at a time.
  const [menuOpen, setMenuOpen] = createSignal(false);
  const [notifOpen, setNotifOpen] = createSignal(false);
  const [pagesOpen, setPagesOpen] = createSignal(false);

  // Below `md` the four page links plus the right-hand cluster cannot share one
  // row (they are all `flex-shrink-0`, so they used to overflow off-screen inside
  // the shell's `overflow-hidden`, i.e. become unreachable rather than scroll).
  // Under that width the links move into a dropdown behind a hamburger.
  const isMobile = useIsMobile();

  const closeAllMenus = () => { setMenuOpen(false); setNotifOpen(false); setPagesOpen(false); };

  // F027 — the My Projects nav entry appears only when the deployment gate is
  // open. ⚠️ This is PRESENTATION. Every project route refuses on its own when
  // the gate is closed (FR-038), so a client that ignores this flag — a stale
  // bundle, a crafted request — still gets nothing. Never treat this signal as
  // the gate; deep search learned that lesson once already.
  // Shared with the admin console via `stores/projects`, so toggling the gate
  // there updates this nav immediately instead of waiting for a page reload.
  onMount(() => { void refreshProjectsEnabled(); });

  /**
   * ⚠️ `trial` marks a page the free trial does not include. It is PRESENTATION:
   * the server refuses those routes for a trial session regardless (one
   * allow-list in `require_auth`). This exists so the officer gets a sentence
   * explaining why, instead of a raw 403 from a page that half-loaded.
   */
  const PAGES = (): Array<{ path: string; label: string; icon: string; trial?: boolean }> => [
    { path: "/", label: "Chat", icon: NAV_ICON.chat },
    { path: "/search", label: "Search", icon: NAV_ICON.search },
    ...(projectsEnabled()
      ? [{ path: "/projects", label: "My Projects", icon: NAV_ICON.projects, trial: true }]
      : []),
    { path: "/workflow", label: "Workflow", icon: NAV_ICON.workflow, trial: true },
    { path: "/meeting", label: "Meeting Assistant", icon: NAV_ICON.meeting, trial: true },
  ];

  const isTrial = () => currentUser()?.role === "trial";
  /** Set when a trial user taps a page their trial does not include. */
  const [trialNotice, setTrialNotice] = createSignal<string | null>(null);

  /** Navigate, unless this page is outside the trial — then say so. */
  const goPage = (p: { path: string; label: string; trial?: boolean }) => {
    if (p.trial && isTrial()) {
      setTrialNotice(
        `${p.label} is not part of the free trial — it is for organisations and paid accounts. ` +
          `Chat and Search are included.`,
      );
      window.setTimeout(() => setTrialNotice(null), 6000);
      return;
    }
    navigate(p.path);
  };

  const handleLogout = async () => {
    closeAllMenus();
    await logout();
    navigate("/");
  };

  const isActive = (path: string) =>
    path === "/" ? location.pathname === "/" : location.pathname.startsWith(path);

  const navClass = (path: string) =>
    `transition-colors ${isActive(path) ? "text-noviq-800 font-semibold" : "text-noviq-500 hover:text-noviq-ink"}`;

  // Inline (>= md) page link: icon + label, with the active tab underlined
  // against the bar's own bottom border. `-mb-px` pulls the 2px rule down over
  // that hairline so the two do not stack into a 3px edge.
  const navLinkClass = (path: string) =>
    `flex h-full items-center gap-1.5 border-b-2 -mb-px px-1 ${
      isActive(path) ? "border-noviq-700" : "border-transparent"
    } ${navClass(path)}`;


  // Signal to the backend which workload kind has foreground priority
  // based on the current view so background tasks yield appropriately.
  createEffect(() => {
    const path = location.pathname;
    let kind: WorkloadKind;
    if (path.startsWith("/settings")) {
      kind = "indexing";
    } else {
      kind = "chat"; // / and /search: user may initiate chat/search
    }
    setForeground(kind).catch(() => {/* backend not ready yet */});
  });

  return (
    <nav class="relative h-14 flex items-center gap-2 sm:gap-4 px-2 sm:px-4 bg-noviq-paper border-b border-noviq-line flex-shrink-0">
      {/* Brand lockup: mark + stacked wordmark and tagline. The tagline belongs
          HERE and only here (architect, 2026-08-25) — the chat window's hero
          carries the logo alone, with no "The Research Station" line under it.
          Hidden below `sm`, where the bar has no room for two stacked lines. */}
      <button
        onClick={() => navigate("/")}
        class="flex flex-shrink-0 items-center gap-2"
        title="Noviq — The Research Station"
        aria-label="Noviq — go to Chat"
      >
        <img src={markUrl} alt="" class="h-9 w-9 object-contain" />
        <span class="hidden text-left leading-none sm:block">
          <span class="block text-[15px] font-bold tracking-[0.14em] text-noviq-800">
            NOVIQ
          </span>
          <span class="mt-1 block text-[9px] tracking-wide text-noviq-500">
            The Research Station
          </span>
        </span>
      </button>
      {/* Hairline between the mark and the page links, as in the design. */}
      <div class="hidden md:block h-7 w-px bg-noviq-line flex-shrink-0" />

      {/* Page links — inline from `md` up, behind a hamburger below it. */}
      <Show
        when={!isMobile()}
        fallback={
          <div class="relative flex-shrink-0">
            <button
              class="p-1.5 rounded-lg text-noviq-500 hover:text-noviq-ink hover:bg-noviq-line/40 transition-colors"
              onClick={() => { setPagesOpen((v) => !v); setMenuOpen(false); setNotifOpen(false); }}
              title="Menu"
              aria-label="Menu"
              aria-expanded={pagesOpen()}
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
              </svg>
            </button>
            <Show when={pagesOpen()}>
              <div class="absolute left-0 mt-2 w-52 bg-noviq-panel border border-noviq-line rounded-lg shadow-lg z-50 py-1">
                <For each={PAGES()}>{(p) => (
                  <button
                    class={`flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-noviq-50 ${navClass(p.path)}`}
                    onClick={() => { setPagesOpen(false); goPage(p); }}
                  >
                    <svg class="h-[18px] w-[18px] flex-shrink-0" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                      <path d={p.icon} />
                    </svg>
                    {p.label}
                  </button>
                )}</For>
              </div>
            </Show>
          </div>
        }
      >
        <div class="flex h-full items-stretch gap-4 lg:gap-6 text-sm flex-shrink-0">
          <For each={PAGES()}>{(p) => (
            <button
              class={`${navLinkClass(p.path)} ${p.trial && isTrial() ? "opacity-55" : ""}`}
              onClick={() => goPage(p)}
              title={p.trial && isTrial() ? "Not included in the free trial" : undefined}
            >
              <svg class="h-[18px] w-[18px] flex-shrink-0" viewBox="0 0 24 24" fill="none"
                   stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d={p.icon} />
              </svg>
              <span>{p.label}</span>
            </button>
          )}</For>
        </div>
      </Show>

      {/* Right side: status badge + settings + notifications + profile avatar.
          `ml-auto` pins this whole cluster to the right edge of the bar.
          (Document search lives on the dedicated Search tab.) */}
      <div class="ml-auto flex items-center gap-1 sm:gap-1.5 flex-shrink-0">
        <NavBadge />
        {/* Settings (gear) — a USER surface. Web admins manage everything from
            the Admin console instead, so the gear is hidden for them. */}
        <Show when={!showAdmin()}>
        <button
          class={`p-1.5 rounded-lg hover:bg-noviq-line/40 ${navClass("/settings")}`}
          onClick={() => navigate("/settings")}
          title="Settings"
          aria-label="Settings"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
              d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
              d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
        </Show>

        {/* Notifications (bell) */}
        <div class="relative">
          <button
            class="p-1.5 rounded-lg text-noviq-500 hover:text-noviq-ink hover:bg-noviq-line/40 transition-colors"
            onClick={() => { setNotifOpen((v) => !v); setMenuOpen(false); setPagesOpen(false); }}
            title="Notifications"
            aria-label="Notifications"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9" />
            </svg>
          </button>
          <Show when={notifOpen()}>
            <div class="absolute right-0 mt-2 w-64 bg-noviq-panel border border-noviq-line rounded-lg shadow-lg z-50 p-3">
              <p class="text-xs font-semibold text-noviq-700 mb-1">Notifications</p>
              <p class="text-xs text-noviq-500">You're all caught up — no notifications.</p>
            </div>
          </Show>
        </div>

        {/* Profile avatar (web sessions only) → logout / user management */}
        <Show when={!isTauri() && currentUser()}>
          <div class="relative">
            <button
              class="w-8 h-8 rounded-full bg-noviq-700 text-white text-sm font-semibold flex items-center justify-center hover:bg-noviq-800 transition-colors"
              onClick={() => { setMenuOpen((v) => !v); setNotifOpen(false); setPagesOpen(false); }}
              title={currentUser()!.username}
              aria-label="Account menu"
            >
              {currentUser()!.username.charAt(0).toUpperCase()}
            </button>
            <Show when={menuOpen()}>
              <div class="absolute right-0 mt-2 w-52 bg-noviq-panel border border-noviq-line rounded-lg shadow-lg z-50 py-1">
                <div class="px-3 py-2 border-b border-noviq-line">
                  <p class="text-sm font-medium text-noviq-ink truncate">{currentUser()!.username}</p>
                  <p class="text-xs text-noviq-500 capitalize">{currentUser()!.role}</p>
                </div>
                {/* Admin-only: user management */}
                <Show when={showAdmin()}>
                  <button
                    class="w-full text-left px-3 py-2 text-sm text-noviq-700 hover:bg-noviq-line/40 transition-colors"
                    onClick={() => { setMenuOpen(false); navigate("/admin"); }}
                  >
                    Admin Console
                  </button>
                </Show>
                <button
                  class="w-full text-left px-3 py-2 text-sm text-red-500 hover:bg-noviq-line/40 transition-colors"
                  onClick={handleLogout}
                >
                  Logout
                </button>
              </div>
            </Show>
          </div>
        </Show>
      </div>

      {/* Trial refusal notice. A toast rather than a page: the officer never
          left Chat, so navigating them somewhere to read an error would be a
          worse answer than telling them where they are. */}
      <Show when={trialNotice()}>
        <div class="absolute inset-x-0 top-full z-50 mx-auto mt-1 w-[min(92vw,32rem)] rounded-xl
                    border border-amber-300 bg-amber-50 px-4 py-2.5 text-[13px] leading-relaxed
                    text-amber-900 shadow-lg">
          {trialNotice()}
        </div>
      </Show>

      {/* Click-away backdrop to close any open menu */}
      <Show when={menuOpen() || notifOpen() || pagesOpen()}>
        <div class="fixed inset-0 z-40" onClick={closeAllMenus} />
      </Show>
    </nav>
  );
};

// AppShell is a ROUTE component typed as RouteSectionProps — child routes render in props.children.
const AppShell: Component<RouteSectionProps> = (props) => {
  const [storageLow, setStorageLow] = createSignal(false);
  const [storageLowMsg, setStorageLowMsg] = createSignal("");

  onMount(() => {
    // Fire-and-forget — do not await so reactive context is preserved for onCleanup.
    loadHardwareTier();

    // Register cleanup synchronously so SolidJS can associate it with this root.
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen<{ path: string; free_gb: number }>("storage:low", (ev) => {
        setStorageLow(true);
        setStorageLowMsg(
          `Low disk space: ${ev.payload.free_gb.toFixed(1)} GB free. Indexing paused.`
        );
      }).then((fn) => { unlisten = fn; });
    });
    onCleanup(() => unlisten?.());
  });

  return (
    <div class="flex flex-col app-viewport overflow-hidden">
      <NavBar />
      {/* Renders nothing for non-trial users, so it is safe unconditionally. */}
      <TrialBanner />
      <Show when={storageLow()}>
        <div class="flex items-start sm:items-center justify-between gap-2 px-3 sm:px-4 py-2 bg-orange-900/40 border-b border-orange-700/50 text-orange-300 text-xs flex-shrink-0">
          <span class="min-w-0">⚠ {storageLowMsg()}</span>
          <button onClick={() => setStorageLow(false)} class="flex-shrink-0 text-orange-500 hover:text-orange-200">×</button>
        </div>
      </Show>
      <div class="flex-1 overflow-hidden">
        {props.children}
      </div>
    </div>
  );
};

const App: Component = () => {
  onMount(() => {
    // Web transport: the server gates every route behind a session token, so
    // validate/restore login before rendering the app. No-op in Tauri.
    restoreSession();
    // Web transport only: watch the server through deploy restarts (no-op in Tauri).
    startServerStatusPolling();
    onCleanup(() => stopServerStatusPolling());
  });

  // Web-only auth gate: wait for the session check, then show the login screen
  // until authenticated. Desktop (Tauri) has no auth → `authReady` is true with
  // no user, and this gate is bypassed. (Trial gating removed — not used now.)
  const needsLogin = () => !isTauri() && authReady() && !currentUser();
  const checkingAuth = () => !isTauri() && !authReady();

  return (
      <Show when={!checkingAuth()} fallback={
        <div class="fixed inset-0 flex items-center justify-center bg-gray-950 text-gray-500 text-sm">Loading…</div>
      }>
      <Show when={!needsLogin()} fallback={<LoginScreen />}>
      <>
      <Router>
        {/* AppShell wraps all pages as a layout route */}
        <Route path="/" component={AppShell}>
          <Route path="/" component={ChatPage} />
          {/* F027 fix (2026-08-07) — opening a specific conversation by id.
              The project page then navigated to `/chat/<id>` after "New chat",
              but this route did not exist, so the router matched nothing and the
              screen went BLANK. ChatPage reads `:conversationId` and reopens that
              chat instead of minting a new one. */}
          <Route path="/chat/:conversationId" component={ChatPage} />
          <Route path="/search" component={SearchPage} />
          <Route path="/workflow" component={Workflow} />
          <Route path="/meeting" component={MeetingAssistant} />
          <Route path="/settings" component={SettingsPage} />
          {/* F027 — My Projects. These routes are always registered: the server
              refuses every project call when the deployment gate is closed, so
              reaching the page without the feature simply shows its error. The
              nav entry is hidden separately, which is presentation only. */}
          <Route path="/projects" component={ProjectsPage} />
          {/* A chat INSIDE a project. Same ChatPage, project mode: its rails show
              only this project's chats and documents. Nested under the project so
              the URL says where you are and "back" is unambiguous. */}
          <Route path="/projects/:projectId/chat/:conversationId" component={ChatPage} />
          {/* US7 admin portal — self-guards (non-admins see "not authorized") */}
          <Route path="/admin" component={Admin} />
        </Route>
        {/* F027 — a shared chat link. Outside AppShell's nav: it is a read-only
            view of one conversation, and it still requires a signed-in Noviq
            user (the server refuses otherwise — FR-027). */}
        <Route path="/shared/:token" component={SharedChatPage} />
        {/* Overlay window — separate window, no AppShell nav bar */}
        <Route path="/overlay" component={Overlay} />
        {/* ⚠️ Privacy and Terms must resolve for SIGNED-IN users too. They are
            handled inside `LoginScreen` for signed-out visitors (which is what
            Google fetches), but a signed-in admin opening /privacy hit the
            router, matched nothing, and got a BLANK page. Google requires these
            URLs to work; "works only when logged out" is not good enough for a
            link we publish. */}
        <Route path="/privacy" component={PrivacyPolicy} />
        <Route path="/terms" component={TermsOfService} />
      </Router>
      {/* Model download banner — desktop only. The model lives on the server in
          the web deploy; a browser user can't (and shouldn't) download it, and
          `check_model` is a desktop command, so showing this on web is wrong. */}
      <Show when={isTauri()}>
        <ModelDownloadBanner />
      </Show>
      {/* "Server is updating" banner — web transport, during a deploy/restart */}
      <ServerStatusBanner />
    </>
      </Show>
      </Show>
  );
};

export default App;
