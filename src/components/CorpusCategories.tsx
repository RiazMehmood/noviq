/**
 * CorpusCategories — the admin's category manager (migration 021).
 *
 * Categories are an organisational overlay on the corpus: creating, renaming or
 * deleting one writes a single row and moves no document, no file and no chunk.
 * Deleting is therefore SAFE, and the UI says so plainly — its documents fall
 * back to Uncategorised (`ON DELETE SET NULL`) and the confirm quotes the count
 * the server computed rather than a generic "are you sure".
 *
 * Filing documents INTO a category lives in the document list, not here: that is
 * where the documents and the selection already are.
 *
 * Web + admin only — mounted from the Admin console's Indexing & Data tab.
 */
import { Component, For, Show, createSignal, onMount } from "solid-js";
import {
  categories,
  createCategory,
  deleteCategory,
  loadCategories,
  updateCategory,
} from "../stores/categories";

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

const CorpusCategories: Component = () => {
  const [newName, setNewName] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [note, setNote] = createSignal<string | null>(null);
  // The category being renamed, and the text in its box. Editing happens in
  // place: a rename is a one-word change and a modal for it is overkill.
  const [editingId, setEditingId] = createSignal<string | null>(null);
  const [editName, setEditName] = createSignal("");

  onMount(() => void loadCategories());

  const handleCreate = async () => {
    const name = newName().trim();
    if (!name) return;
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      // New categories go to the end of the display order. `sort_order` counts
      // existing rows rather than taking max+1 — the values need only order the
      // list, and the admin reorders with the arrows.
      await createCategory(name, categories().length);
      setNewName("");
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const handleRename = async (id: string) => {
    const name = editName().trim();
    const cat = categories().find((c) => c.id === id);
    if (!cat) return;
    if (!name || name === cat.name) {
      setEditingId(null);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await updateCategory(id, name, cat.sort_order);
      setEditingId(null);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  /**
   * Move a category one place up or down.
   *
   * Swaps the two `sort_order` values rather than renumbering the list, so a
   * reorder is two writes regardless of how many categories exist. Ties are
   * impossible to introduce this way as long as the seed values are distinct —
   * and where they are not, the server's secondary sort on name keeps the order
   * stable rather than flickering.
   */
  const handleMove = async (id: string, delta: -1 | 1) => {
    const list = categories();
    const i = list.findIndex((c) => c.id === id);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= list.length) return;
    setBusy(true);
    setError(null);
    try {
      await updateCategory(list[i].id, list[i].name, list[j].sort_order);
      await updateCategory(list[j].id, list[j].name, list[i].sort_order);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (id: string, name: string, count: number) => {
    // The count comes from the server's listing, and the wording is explicit
    // that documents survive: an admin who reads "delete Finance" and expects
    // nineteen instruments to go with it would never click it.
    const msg =
      count === 0
        ? `Delete the category “${name}”?`
        : `Delete the category “${name}”?\n\n` +
          `Its ${count} document(s) are NOT deleted — they move to Uncategorised ` +
          `and can be re-filed. Nothing is re-indexed.`;
    if (!confirm(msg)) return;
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      const released = await deleteCategory(id);
      setNote(
        released > 0
          ? `“${name}” deleted. ${released} document(s) moved to Uncategorised.`
          : `“${name}” deleted.`,
      );
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <p class="text-xs text-noviq-500 mb-3">
        Group the corpus so officers can find their way around it. A category is a{" "}
        <b>label only</b> — filing a document moves no file and re-indexes nothing,
        and deleting a category keeps its documents. File documents into a category
        from the <b>Documents</b> list above, using the checkboxes.
      </p>

      <div class="flex items-center gap-2 flex-wrap mb-3">
        <input
          class="flex-1 min-w-[12rem] rounded border border-noviq-300 px-2.5 py-1.5 text-sm
                 outline-none focus:border-noviq-400"
          placeholder="New category name, e.g. Finance"
          value={newName()}
          disabled={busy()}
          onInput={(e) => setNewName(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void handleCreate();
          }}
        />
        <button
          type="button"
          class="px-3 py-1.5 bg-noviq-800 text-white rounded text-sm hover:bg-noviq-700
                 disabled:opacity-50 disabled:cursor-not-allowed"
          onClick={handleCreate}
          disabled={busy() || !newName().trim()}
        >
          Add category
        </button>
      </div>

      <Show when={error()}>
        <p class="text-xs text-red-600 mb-2">{error()}</p>
      </Show>
      <Show when={note()}>
        <p class="text-xs text-green-700 mb-2">{note()}</p>
      </Show>

      <Show
        when={categories().length > 0}
        fallback={
          <p class="text-sm text-noviq-500">
            No categories yet. Every document is currently Uncategorised.
          </p>
        }
      >
        <div class="rounded-lg border border-noviq-line bg-white divide-y divide-noviq-line">
          <For each={categories()}>
            {(cat, i) => (
              <div class="flex items-center gap-2 px-3 py-2 text-xs">
                {/* Reorder. Disabled at the ends rather than hidden, so the
                    controls do not shift position row to row. */}
                <div class="flex flex-col leading-none flex-shrink-0">
                  <button
                    type="button"
                    class="px-1 text-noviq-500 hover:text-noviq-800 disabled:opacity-30"
                    title="Move up"
                    disabled={busy() || i() === 0}
                    onClick={() => handleMove(cat.id, -1)}
                  >
                    ▲
                  </button>
                  <button
                    type="button"
                    class="px-1 text-noviq-500 hover:text-noviq-800 disabled:opacity-30"
                    title="Move down"
                    disabled={busy() || i() === categories().length - 1}
                    onClick={() => handleMove(cat.id, 1)}
                  >
                    ▼
                  </button>
                </div>

                <Show
                  when={editingId() === cat.id}
                  fallback={
                    <span class="flex-1 truncate text-noviq-700 font-medium">{cat.name}</span>
                  }
                >
                  <input
                    class="flex-1 rounded border border-noviq-300 px-2 py-1 outline-none
                           focus:border-noviq-400"
                    value={editName()}
                    autofocus
                    disabled={busy()}
                    onInput={(e) => setEditName(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleRename(cat.id);
                      if (e.key === "Escape") setEditingId(null);
                    }}
                    onBlur={() => void handleRename(cat.id)}
                  />
                </Show>

                {/* Includes withdrawn documents — deliberately, so a category
                    never looks empty just before it is deleted. */}
                <span class="flex-shrink-0 text-noviq-500 tabular-nums">
                  {cat.document_count} doc{cat.document_count === 1 ? "" : "s"}
                </span>

                <Show when={editingId() !== cat.id}>
                  <button
                    type="button"
                    class="flex-shrink-0 rounded bg-noviq-100 px-2 py-0.5 text-noviq-700
                           transition-colors hover:bg-noviq-800 hover:text-white"
                    onClick={() => {
                      setEditName(cat.name);
                      setEditingId(cat.id);
                    }}
                    disabled={busy()}
                  >
                    Rename
                  </button>
                </Show>

                <button
                  type="button"
                  class="flex-shrink-0 rounded px-2 py-0.5 text-red-700 hover:bg-red-600
                         hover:text-white transition-colors disabled:opacity-50"
                  title="Delete the category — its documents move to Uncategorised"
                  onClick={() => handleDelete(cat.id, cat.name, cat.document_count)}
                  disabled={busy()}
                >
                  Delete
                </button>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
};

export default CorpusCategories;
