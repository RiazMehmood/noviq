/**
 * The free-trial status strip: what is left of today's questions, and of the
 * three days.
 *
 * ⚠️ PRESENTATION ONLY. This reads `GET /api/trial/status`; it does not decide
 * anything. A trial user whose quota is gone is refused by the server on the
 * next question whether or not this strip is on screen, and a stale count here
 * changes nothing about what they can do.
 *
 * It renders nothing at all for non-trial users, so it is safe to mount
 * unconditionally in the shell.
 */
import { Component, createSignal, onCleanup, onMount, Show } from "solid-js";
import { request } from "../api/transport";
import { currentUser } from "../stores/auth";

interface TrialStatus {
  day_index: number;
  used_today: number;
  remaining_today: number;
  expired: boolean;
  revoked: boolean;
}

/** Kept in lockstep with `TRIAL_DAYS` in `db/repositories/trial.rs`. */
const TRIAL_DAYS = 3;

const [status, setStatus] = createSignal<TrialStatus | null>(null);

/**
 * Re-read the trial status. Exported so the chat page can call it after a turn
 * — the count changes on the server the moment a question is asked, and a strip
 * that only refreshes on a timer would tell the user they have 3 left while
 * they are spending them.
 */
export async function refreshTrialStatus(): Promise<void> {
  if (currentUser()?.role !== "trial") {
    setStatus(null);
    return;
  }
  try {
    const res = await request<{ trial: TrialStatus | null }>("GET", "/trial/status");
    setStatus(res.trial);
  } catch {
    // Never surface this: a failed status read must not look like a problem
    // with the user's trial. The server is the authority either way.
  }
}

const TrialBanner: Component = () => {
  let timer: number | undefined;

  onMount(() => {
    void refreshTrialStatus();
    // A slow safety net for the day rolling over while a tab sits open.
    timer = window.setInterval(() => void refreshTrialStatus(), 5 * 60 * 1000);
  });
  onCleanup(() => { if (timer) window.clearInterval(timer); });

  const daysLeft = () => Math.max(0, TRIAL_DAYS - (status()?.day_index ?? 0));

  return (
    <Show when={currentUser()?.role === "trial" && status()}>
      {(s) => (
        <div
          class="flex flex-shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b px-3 py-2 text-[12px] sm:px-4"
          classList={{
            "border-noviq-line bg-noviq-50 text-noviq-800": s().remaining_today > 0,
            "border-amber-300 bg-amber-50 text-amber-900": s().remaining_today === 0,
          }}
        >
          <span class="font-semibold">Free trial</span>
          <Show
            when={s().remaining_today > 0}
            fallback={<span>Today's 3 questions are used. They reset at midnight UTC.</span>}
          >
            <span>
              <span class="font-semibold tabular-nums">{s().remaining_today}</span> of 3 questions
              left today
            </span>
          </Show>
          <span class="text-noviq-400">·</span>
          <span>
            <span class="font-semibold tabular-nums">{daysLeft()}</span>{" "}
            {daysLeft() === 1 ? "day" : "days"} left
          </span>
          <span class="text-noviq-400">·</span>
          <span>Answers come from a small set of published instruments.</span>
        </div>
      )}
    </Show>
  );
};

export default TrialBanner;
