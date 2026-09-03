/**
 * Feature 018 (US1) — citation feedback on one answer.
 *
 * Exactly two single-select questions, one tap each: citation ACCURACY and
 * citation COMPLETENESS. No 👍/👎, no reason chips, no free-text. Each tap is
 * sent to the server (best-effort) anchored to the answer's `messageId`; a
 * re-tap supersedes the prior value server-side.
 *
 * Labels are the officer-facing wording locked in the spec (Clarifications
 * 2026-07-03); the codes match the backend enums. The layout is responsive —
 * each question's label sits above its chips, which wrap on narrow screens.
 */
import { createSignal, For, Show } from "solid-js";
import {
  submitFeedback,
  type CitationAccuracy,
  type CitationCompleteness,
} from "../api/assistant";

const ACCURACY: { code: CitationAccuracy; label: string }[] = [
  { code: "all", label: "All citations correct" },
  { code: "some", label: "Some citations correct" },
  { code: "wrong", label: "Citations wrong/missing" },
  { code: "unsure", label: "Not sure" },
];

const COMPLETENESS: { code: CitationCompleteness; label: string }[] = [
  { code: "yes", label: "Fully cited" },
  { code: "missing", label: "Some citations missing" },
  { code: "na", label: "Not applicable" },
];

const CHIP_BASE =
  "text-[12px] leading-none rounded-full px-3 py-1.5 border transition-colors " +
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-noviq-400";
const CHIP_ON = "bg-noviq-700 text-white border-noviq-700";
const CHIP_OFF =
  "bg-white text-noviq-700 border-noviq-line hover:bg-noviq-50";

export default function CitationFeedback(props: {
  messageId: string;
  conversationId?: string;
}) {
  const [accuracy, setAccuracy] = createSignal<CitationAccuracy | null>(null);
  const [completeness, setCompleteness] = createSignal<CitationCompleteness | null>(null);
  const [failed, setFailed] = createSignal(false);

  const pickAccuracy = (code: CitationAccuracy) => {
    setAccuracy(code);
    setFailed(false);
    void submitFeedback({
      messageId: props.messageId,
      conversationId: props.conversationId,
      citationAccuracy: code,
    }).catch(() => setFailed(true));
  };

  const pickCompleteness = (code: CitationCompleteness) => {
    setCompleteness(code);
    setFailed(false);
    void submitFeedback({
      messageId: props.messageId,
      conversationId: props.conversationId,
      citationCompleteness: code,
    }).catch(() => setFailed(true));
  };

  return (
    <Show when={props.messageId}>
      {/* Tinted noviq-50 + accent left edge so it reads apart from the white answer cards. */}
      <section class="mt-3 rounded-xl border border-noviq-200 border-l-4 border-l-noviq-500 bg-noviq-50 px-4 py-4 sm:px-5">
        {/* Header — small "Feedback" eyebrow, then the friendly ask. */}
        <div class="flex flex-wrap items-center gap-x-2 gap-y-1 mb-3">
          <span class="font-mono text-[10px] uppercase tracking-widest text-noviq-500 bg-noviq-panel border border-noviq-line rounded px-2 py-0.5">
            Feedback
          </span>
          <span class="text-sm font-semibold text-noviq-ink">Help us to improve</span>
          <span class="w-full sm:w-auto text-[12px] text-noviq-500">
            How were the citations in this answer?
          </span>
        </div>

        <div class="space-y-3">
          {/* Q1 — accuracy */}
          <div>
            <div class="text-[12px] font-medium text-noviq-600 mb-1.5">Are the citations correct?</div>
            <div class="flex flex-wrap gap-2">
              <For each={ACCURACY}>
                {(opt) => (
                  <button
                    type="button"
                    onClick={() => pickAccuracy(opt.code)}
                    aria-pressed={accuracy() === opt.code}
                    class={`${CHIP_BASE} ${accuracy() === opt.code ? CHIP_ON : CHIP_OFF}`}
                  >
                    {opt.label}
                  </button>
                )}
              </For>
            </div>
          </div>

          {/* Q2 — completeness */}
          <div>
            <div class="text-[12px] font-medium text-noviq-600 mb-1.5">Are the citations complete?</div>
            <div class="flex flex-wrap gap-2">
              <For each={COMPLETENESS}>
                {(opt) => (
                  <button
                    type="button"
                    onClick={() => pickCompleteness(opt.code)}
                    aria-pressed={completeness() === opt.code}
                    class={`${CHIP_BASE} ${completeness() === opt.code ? CHIP_ON : CHIP_OFF}`}
                  >
                    {opt.label}
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>

        <Show when={(accuracy() || completeness()) && !failed()}>
          <p class="mt-3 text-[12px] italic text-noviq-600">Thanks — your feedback was recorded.</p>
        </Show>
        <Show when={failed()}>
          <p class="mt-3 text-[12px] italic text-amber-700">
            Couldn't save that just now — please try again.
          </p>
        </Show>
      </section>
    </Show>
  );
}
