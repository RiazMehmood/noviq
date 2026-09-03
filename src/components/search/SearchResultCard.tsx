/**
 * SearchResultCard — T055 (US2).
 * Renders a single global search result. Click the main area to open the
 * source file with the OS default app; chevron toggles a full-excerpt view.
 */
import { Component, createSignal, Show } from "solid-js";
import { SearchResult } from "../../api/index";
import { isTauri } from "../../api/transport";

interface Props {
  result: SearchResult;
  onClick?: (result: SearchResult) => void;
}

function formatDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function formatScore(score: number): string {
  return `${Math.round(score * 100)}%`;
}

/** Desktop only — hands the file to the OS default application. */
async function openInDefaultApp(filePath: string) {
  try {
    const { open } = await import("@tauri-apps/plugin-shell");
    await open(filePath);
  } catch (e) {
    console.warn("Could not open file:", e);
  }
}

const SearchResultCard: Component<Props> = (props) => {
  const [expanded, setExpanded] = createSignal(false);
  const [copied, setCopied] = createSignal(false);

  const handleOpen = () => {
    props.onClick?.(props.result);
    // Desktop only. On the web `@tauri-apps/plugin-shell` is absent, so this
    // threw into a `console.warn` the user never saw — clicking a result simply
    // did nothing. The browser has no business opening OS file paths anyway;
    // there the parent's `onClick` opens the passage in the source drawer.
    if (isTauri()) openInDefaultApp(props.result.file_path);
  };

  const handleCopyPath = async (e: MouseEvent) => {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(props.result.file_path);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (err) {
      console.warn("Clipboard write failed:", err);
    }
  };

  const toggleExpand = (e: MouseEvent) => {
    e.stopPropagation();
    setExpanded((v) => !v);
  };

  return (
    <div
      onClick={handleOpen}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          handleOpen();
        }
      }}
      class="w-full text-left p-4 bg-white rounded-lg border border-noviq-line
             hover:border-blue-600 hover:bg-gray-750 transition-colors cursor-pointer
             focus:outline-none focus:border-noviq-500"
    >
      {/* Header row */}
      <div class="flex items-center justify-between mb-2 gap-2">
        <div class="flex items-center gap-2 min-w-0">
          <svg
            class="w-4 h-4 text-noviq-600 flex-shrink-0"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            />
          </svg>
          <span class="text-sm font-medium text-noviq-ink truncate">
            {props.result.file_name}
          </span>
          {props.result.page_number !== null && (
            <span class="text-xs text-noviq-500 flex-shrink-0">
              p. {props.result.page_number}
            </span>
          )}
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <span class="text-xs text-noviq-600 font-mono">
            {formatScore(props.result.relevance_score)}
          </span>
          <button
            type="button"
            onClick={toggleExpand}
            title={expanded() ? "Collapse" : "Show full excerpt"}
            class="p-1 rounded hover:bg-noviq-200 text-noviq-600 hover:text-noviq-ink"
          >
            <svg
              class={`w-4 h-4 transition-transform ${expanded() ? "rotate-180" : ""}`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M19 9l-7 7-7-7"
              />
            </svg>
          </button>
        </div>
      </div>

      {/* Excerpt — clamped unless expanded */}
      <p
        class={`text-sm text-noviq-600 leading-relaxed mb-2 ${
          expanded() ? "whitespace-pre-wrap" : "line-clamp-3"
        }`}
      >
        {props.result.excerpt}
      </p>

      {/* Footer: file path + date + actions */}
      <div class="flex items-center justify-between gap-2">
        <span
          class="text-xs text-noviq-400 truncate"
          title={props.result.file_path}
        >
          {props.result.file_path}
        </span>
        <div class="flex items-center gap-2 flex-shrink-0">
          <Show when={expanded()}>
            <button
              type="button"
              onClick={handleCopyPath}
              class="text-xs text-noviq-500 hover:text-noviq-600 px-2 py-0.5 rounded
                     hover:bg-noviq-200 transition-colors"
            >
              {copied() ? "Copied!" : "Copy path"}
            </button>
          </Show>
          <span class="text-xs text-noviq-400">
            {formatDate(props.result.last_modified)}
          </span>
        </div>
      </div>
    </div>
  );
};

export default SearchResultCard;
