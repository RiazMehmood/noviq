// Feature 027 (US3) — "also search the shared legal corpus", for ONE question.
//
// ⚠️ The value lives in the composer's local state and is sent on a single
// `query` call. It is not in any store, not in localStorage, and there is no
// column for it anywhere on the server (FR-017). That absence is the mechanism:
// nothing can carry it to the next turn because nothing holds it.
//
// The parent MUST reset it to false after each send. If you find yourself
// wanting to persist it "for convenience", read FR-017 first — the whole point
// of a project is that the law does not silently appear in answers about the
// officer's own file.

import { Show } from "solid-js";

interface Props {
  checked: boolean;
  onChange: (next: boolean) => void;
  /** Only meaningful inside a project; ordinary chat already searches the corpus. */
  visible: boolean;
  disabled?: boolean;
}

export default function CorpusToggle(props: Props) {
  return (
    <Show when={props.visible}>
      <label
        class="flex cursor-pointer select-none items-center gap-2 text-xs opacity-80"
        title="Search the Constitution, Acts and Rules alongside this project's documents, for this question only."
      >
        <input
          type="checkbox"
          checked={props.checked}
          disabled={props.disabled}
          onChange={(e) => props.onChange(e.currentTarget.checked)}
        />
        <span>Also search the law</span>
      </label>
    </Show>
  );
}
