/**
 * ComingSoon — shared presentational "feature preview" page on the Noviq
 * light paper theme. Used by Workflow and Meeting Assistant tabs until those
 * features ship. Shows the feature pitch + (optional) an illustrative flow so
 * users understand what is coming, not just a blank "coming soon".
 */
import { Component, For, Show } from "solid-js";
import logoUrl from "../assets/noviq_mark.png";

export interface FlowStep {
  /** Who/where the file sits at this stage. */
  actor: string;
  /** What happens here. */
  detail: string;
}

interface Props {
  title: string;
  subtitle: string;
  description: string;
  /** Optional illustrative routing flow (rendered as a vertical step chain). */
  flow?: FlowStep[];
  /** Optional caption above the flow chain. */
  flowCaption?: string;
}

const ComingSoon: Component<Props> = (props) => {
  return (
    <div class="h-full overflow-y-auto bg-noviq-paper text-noviq-ink">
      <div class="mx-auto max-w-2xl px-4 py-8 sm:px-6 sm:py-12">
        <div class="flex items-center gap-3 mb-6">
          <img src={logoUrl} alt="Noviq" class="h-10 w-10 flex-shrink-0 object-contain" />
          <div class="min-w-0">
            {/* Wraps below `sm`: "Meeting Assistant" + the pill overflow 390px on one line. */}
            <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
              <h1 class="text-xl sm:text-2xl font-bold text-noviq-ink leading-tight">{props.title}</h1>
              <span class="rounded-full bg-noviq-100 border border-noviq-200 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-noviq-700">
                Coming soon
              </span>
            </div>
            <div class="text-sm text-noviq-600">{props.subtitle}</div>
          </div>
        </div>

        <p class="text-[15px] leading-relaxed text-noviq-700 mb-8">
          {props.description}
        </p>

        <Show when={props.flow && props.flow!.length > 0}>
          <div class="rounded-xl border border-noviq-line bg-noviq-panel p-4 sm:p-5">
            <Show when={props.flowCaption}>
              <div class="text-[11px] font-semibold uppercase tracking-wide text-noviq-500 mb-4">
                {props.flowCaption}
              </div>
            </Show>
            <ol class="space-y-0">
              <For each={props.flow}>
                {(step, i) => (
                  <li class="relative flex gap-3 pb-5 last:pb-0">
                    {/* connector line */}
                    <Show when={i() < props.flow!.length - 1}>
                      <span class="absolute left-[13px] top-7 bottom-0 w-px bg-noviq-200" />
                    </Show>
                    <span class="z-10 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-noviq-700 text-xs font-bold text-white">
                      {i() + 1}
                    </span>
                    <div class="pt-0.5">
                      <div class="text-sm font-semibold text-noviq-ink">{step.actor}</div>
                      <div class="text-[13px] text-noviq-600">{step.detail}</div>
                    </div>
                  </li>
                )}
              </For>
            </ol>
          </div>
        </Show>

        <p class="mt-8 text-xs text-noviq-400">
          This feature is in development and will be integrated soon.
        </p>
      </div>
    </div>
  );
};

export default ComingSoon;
