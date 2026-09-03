/**
 * SearchPage — T056 (US2).
 * Global semantic search across all indexed content.
 */
import { Component, createSignal, For, Show } from "solid-js";
import { globalSearch, SearchResult } from "../api/index";
import GlobalSearchBar from "../components/search/GlobalSearchBar";
import SearchResultCard from "../components/search/SearchResultCard";
import ChunkViewer from "../components/ChunkViewer";

const SearchPage: Component = () => {
  const [results, setResults] = createSignal<SearchResult[]>([]);
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [lastQuery, setLastQuery] = createSignal("");
  // The result whose passage is open in the source drawer. Clicking a result used
  // to `console.log` and nothing else, so a search was a dead end — you could see
  // that a document matched but never open it.
  const [openDoc, setOpenDoc] = createSignal<SearchResult | null>(null);

  const handleSearch = async (query: string) => {
    if (!query) {
      setResults([]);
      setLastQuery("");
      return;
    }
    setLastQuery(query);
    setIsLoading(true);
    setError(null);
    try {
      const r = await globalSearch(query);
      setResults(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div class="flex flex-col h-full overflow-hidden bg-noviq-paper text-noviq-ink">
      {/* Search bar header */}
      <div class="px-3 sm:px-6 py-4 border-b border-noviq-line bg-noviq-paper">
        <h1 class="text-lg font-semibold text-noviq-ink mb-3">Global Search</h1>
        <GlobalSearchBar onSearch={handleSearch} class="max-w-2xl" />
        <Show when={lastQuery()}>
          <p class="mt-2 text-xs text-noviq-500">
            {isLoading()
              ? "Searching…"
              : `${results().length} result${results().length !== 1 ? "s" : ""} for "${lastQuery()}"`}
          </p>
        </Show>
      </div>

      {/* Results */}
      <div class="flex-1 overflow-y-auto px-3 sm:px-6 py-4">
        <Show when={error()}>
          <div class="text-sm text-red-700 bg-red-50 border border-red-200 rounded-lg p-3 mb-4">
            {error()}
          </div>
        </Show>

        <Show when={!isLoading() && !lastQuery()}>
          <div class="flex flex-col items-center justify-center h-64 text-center">
            <svg
              class="w-12 h-12 text-noviq-300 mb-3"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
              />
            </svg>
            <p class="text-noviq-500 text-sm">
              Search across all indexed documents and folders
            </p>
            <p class="text-noviq-400 text-xs mt-1">
              Press <kbd class="px-1 bg-noviq-100 rounded text-noviq-600">Ctrl+K</kbd> to start
            </p>
          </div>
        </Show>

        <Show when={!isLoading() && lastQuery() && results().length === 0 && !error()}>
          <div class="text-center py-16">
            <p class="text-noviq-500 text-sm">No results found for "{lastQuery()}"</p>
            <p class="text-noviq-400 text-xs mt-1">
              Try different keywords or index more documents in Settings.
            </p>
          </div>
        </Show>

        <Show when={isLoading()}>
          <div class="space-y-3">
            {[1, 2, 3].map(() => (
              <div class="h-24 bg-noviq-100 rounded-lg animate-pulse" />
            ))}
          </div>
        </Show>

        <Show when={!isLoading() && results().length > 0}>
          <div class="space-y-3 max-w-3xl">
            <For each={results()}>
              {(result) => (
                <SearchResultCard result={result} onClick={setOpenDoc} />
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* Source drawer — the SAME component a clicked citation opens, so a search
          hit and a citation land the reader in the same place, scrolled to the
          matching passage. `audience="user"` is required: an officer is not an
          admin, and this reads through the visibility-scoped endpoint. */}
      <Show when={openDoc()}>
        {(doc) => (
          <ChunkViewer
            sourceId={doc().source_id}
            fileName={doc().file_name}
            focusChunkId={doc().chunk_id}
            audience="user"
            onClose={() => setOpenDoc(null)}
          />
        )}
      </Show>
    </div>
  );
};

export default SearchPage;
