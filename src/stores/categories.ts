/**
 * Corpus categories (migration 021) — one shared list, one loader.
 *
 * Two surfaces render categories: the manager in the Admin console (create /
 * rename / reorder / delete) and the document list that groups by them. A rename
 * in one must not leave the other showing the old name, so both read THIS signal
 * rather than each fetching their own copy.
 *
 * Every mutation here re-reads the server list instead of patching the local
 * array: `document_count` is computed server-side, and a delete releases
 * documents to Uncategorised — outcomes the client cannot derive without
 * re-implementing the query. Re-reading is one cheap request against a table
 * that holds a handful of rows.
 *
 * Web-only. `request` throws in Tauri, and there is no admin console on desktop.
 */
import { createSignal } from "solid-js";
import { isTauri, request } from "../api/transport";
import { isAdmin } from "./auth";

export interface CorpusCategory {
  id: string;
  name: string;
  sort_order: number;
  created_at: number;
  /** Documents filed here, INCLUDING withdrawn ones — see the repository note. */
  document_count: number;
}

const [categories, setCategories] = createSignal<CorpusCategory[]>([]);
const [categoriesLoaded, setCategoriesLoaded] = createSignal(false);

export { categories, categoriesLoaded };

/** Name for a `category_id`, with the Uncategorised fallback every caller needs. */
export function categoryName(id: string | null | undefined): string {
  if (!id) return "Uncategorised";
  return categories().find((c) => c.id === id)?.name ?? "Uncategorised";
}

/**
 * Load the category list. Safe to call from several components on mount.
 *
 * Failure leaves the list empty rather than throwing: categories are an
 * organisational overlay, and a corpus that cannot be grouped must still be
 * listed. `categoriesLoaded` is set either way so a caller can tell "no
 * categories exist" from "not fetched yet".
 */
export async function loadCategories(): Promise<void> {
  if (isTauri()) return;
  try {
    // Same signal for both audiences, different source. `/admin/categories`
    // carries `document_count` and is admin-gated; `/categories` is names and
    // order for any signed-in user. An officer calling the admin route would get
    // a 403 and an empty sidebar, so the role picks the endpoint — but there is
    // still only ONE list in the app, which is what stops the manager and the
    // pickers from disagreeing after a rename.
    const path = isAdmin() ? "/admin/categories" : "/categories";
    setCategories(await request<CorpusCategory[]>("GET", path));
  } catch {
    setCategories([]);
  } finally {
    setCategoriesLoaded(true);
  }
}

/** Create a category. The server rejects a duplicate name with a 400 the UI shows verbatim. */
export async function createCategory(name: string, sortOrder = 0): Promise<void> {
  await request("POST", "/admin/categories", { name, sortOrder });
  await loadCategories();
}

/** Rename and/or reorder. Both fields are always sent — the route takes the pair. */
export async function updateCategory(id: string, name: string, sortOrder: number): Promise<void> {
  await request("POST", `/admin/categories/${encodeURIComponent(id)}`, { name, sortOrder });
  await loadCategories();
}

/**
 * Delete a category. Returns how many documents fell back to Uncategorised, so
 * the caller can say what happened rather than implying they were deleted too.
 */
export async function deleteCategory(id: string): Promise<number> {
  const res = await request<{ released: number }>(
    "POST",
    `/admin/categories/${encodeURIComponent(id)}/delete`,
  );
  await loadCategories();
  return res.released ?? 0;
}

/** File documents into a category, or with `null` back to Uncategorised. */
export async function categorizeSources(
  sourceIds: string[],
  categoryId: string | null,
): Promise<number> {
  const res = await request<{ moved: number }>("POST", "/admin/sources/categorize", {
    sourceIds,
    categoryId,
  });
  // The counts on the category chips are now stale by exactly this many.
  await loadCategories();
  return res.moved ?? 0;
}

/**
 * Withdraw a document from service, or return it.
 *
 * Does NOT reload categories: the count deliberately includes withdrawn
 * documents, so disabling one changes no count.
 */
export async function setSourceDisabled(sourceId: string, disabled: boolean): Promise<void> {
  await request("POST", `/admin/sources/${encodeURIComponent(sourceId)}/disabled`, { disabled });
}
