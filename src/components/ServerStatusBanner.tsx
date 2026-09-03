/**
 * F014 — server-status banner.
 *
 * Two DISTINCT states, deliberately worded differently (2026-07-24):
 *   - `deploying`   — /api/health reported the deploy sentinel: a rebuild really is
 *                     in flight, and the poller reloads the page when it lands.
 *   - `unreachable` — the health fetch threw: VPN dropped, server down, or the
 *                     Cloudflare quick-tunnel URL rotated out from under an open tab.
 *
 * Collapsing both into "updating to a new version" (the original behaviour) made an
 * unreachable server *look* like a deploy loop and cost a whole misdiagnosis — the
 * badge was read as proof the app was rebuilding itself. Never claim a deploy we
 * cannot see. See stores/serverStatus.ts, which already tracks the two separately.
 */
import { Component, Match, Switch } from "solid-js";
import { serverMessage, serverStatus } from "../stores/serverStatus";

const ServerStatusBanner: Component = () => {
  return (
    <Switch>
      <Match when={serverStatus() === "deploying"}>
        <div class="fixed top-0 inset-x-0 z-50 flex items-center justify-center gap-2 px-4 py-2 bg-amber-500 text-amber-950 text-sm font-medium shadow-md">
          <span class="inline-block h-3 w-3 rounded-full border-2 border-amber-950/40 border-t-amber-950 animate-spin" />
          <span>
            Noviq is updating to a new version — this will only take a moment. The
            page will refresh automatically when it’s ready.
          </span>
        </div>
      </Match>
      {/* Planned maintenance — chat is deliberately offline (e.g. vLLM stopped so
          the GPU can run indexing). Nothing is being rebuilt, so this must NOT say
          "updating to a new version"; the reason comes from the operator via
          /api/health `message`. Slate, not amber: this is not a deploy. */}
      <Match when={serverStatus() === "maintenance"}>
        <div class="fixed top-0 inset-x-0 z-50 flex items-center justify-center gap-2 px-4 py-2 bg-slate-700 text-slate-50 text-sm font-medium shadow-md">
          <span class="inline-block h-3 w-3 rounded-full border-2 border-slate-50/40 border-t-slate-50 animate-spin" />
          <span>
            {serverMessage() ||
              "Noviq is temporarily paused for maintenance — chat will return shortly."}
          </span>
        </div>
      </Match>
      <Match when={serverStatus() === "unreachable"}>
        <div class="fixed top-0 inset-x-0 z-50 flex items-center justify-center gap-2 px-4 py-2 bg-rose-600 text-rose-50 text-sm font-medium shadow-md">
          <span class="inline-block h-3 w-3 rounded-full border-2 border-rose-50/40 border-t-rose-50 animate-spin" />
          <span>
            Can’t reach the Noviq server — retrying. Check your VPN connection; if
            the address changed you may need to reload with the current link.
          </span>
        </div>
      </Match>
    </Switch>
  );
};

export default ServerStatusBanner;
