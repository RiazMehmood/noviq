/**
 * GlobalSearchBar — T054 (US2).
 * Debounced semantic search across all indexed content.
 * Ctrl+K / Cmd+K to focus.
 */
import { Component, createSignal, onCleanup, onMount } from "solid-js";

interface Props {
  onSearch: (query: string) => void;
  placeholder?: string;
  class?: string;
}

const GlobalSearchBar: Component<Props> = (props) => {
  let inputRef: HTMLInputElement | undefined;
  const [value, setValue] = createSignal("");
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  // Ctrl+K / Cmd+K global shortcut to focus.
  const handleKeydown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "k") {
      e.preventDefault();
      inputRef?.focus();
    }
  };

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
  });

  onCleanup(() => {
    window.removeEventListener("keydown", handleKeydown);
    if (debounceTimer !== undefined) clearTimeout(debounceTimer);
  });

  const handleInput = (e: InputEvent) => {
    const q = (e.currentTarget as HTMLInputElement).value;
    setValue(q);

    if (debounceTimer !== undefined) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      if (q.trim().length >= 2) {
        props.onSearch(q.trim());
      } else if (q.trim().length === 0) {
        props.onSearch("");
      }
    }, 300);
  };

  return (
    <div class={`relative ${props.class ?? ""}`}>
      {/* Search icon */}
      <svg
        class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-noviq-500 pointer-events-none"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
        />
      </svg>
      <input
        ref={inputRef}
        type="text"
        value={value()}
        onInput={handleInput}
        placeholder={props.placeholder ?? "Search all notebooks… (Ctrl+K)"}
        class="w-full pl-9 pr-16 py-2 bg-white border border-noviq-line rounded-lg
               text-sm text-noviq-ink placeholder-noviq-400
               focus:outline-none focus:ring-1 focus:ring-noviq-500 focus:border-noviq-500"
      />
      {/* Keyboard hint badge */}
      <span class="absolute right-3 top-1/2 -translate-y-1/2 flex gap-1">
        <kbd class="px-1.5 py-0.5 text-xs bg-noviq-100 text-noviq-600 rounded border border-noviq-line">
          Ctrl
        </kbd>
        <kbd class="px-1.5 py-0.5 text-xs bg-noviq-100 text-noviq-600 rounded border border-noviq-line">
          K
        </kbd>
      </span>
    </div>
  );
};

export default GlobalSearchBar;
