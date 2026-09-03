/**
 * RelationTypesPanel — lets the user configure which relation verbs the
 * Pass-2 LLM extracts when building the knowledge graph, plus free-text
 * custom verbs. Lives on the Knowledge Graph page (previously in Settings).
 */
import { Component, createSignal, For, onMount, Show } from "solid-js";
import { getKgPreferences, setKgPreferences } from "../../api/index";

const RELATION_GROUPS: {
  name: string;
  defaults: string[];
  extras: string[];
}[] = [
  {
    name: "Corporate / workplace",
    defaults: [
      "works_at",
      "reports_to",
      "part_of",
      "signed_with",
      "owns",
      "authored",
      "located_in",
      "mentions",
    ],
    extras: ["founded", "acquired", "invested_in", "employs"],
  },
  {
    name: "Academic / research",
    defaults: [],
    extras: [
      "cited_by",
      "co_authored",
      "supervised_by",
      "derived_from",
      "published_in",
      "extends_work_of",
    ],
  },
  {
    name: "Legal",
    defaults: [],
    extras: [
      "plaintiff_in",
      "defendant_in",
      "ruled_on",
      "testified_in",
      "represented_by",
      "appealed_to",
    ],
  },
  {
    name: "Code / technical",
    defaults: [],
    extras: ["imports", "calls", "extends", "implements", "tests", "depends_on"],
  },
  {
    name: "Medical",
    defaults: [],
    extras: ["treats", "causes", "diagnoses", "interacts_with", "prescribed_for"],
  },
  {
    name: "General",
    defaults: [],
    extras: ["related_to", "contains", "precedes", "follows", "similar_to"],
  },
];

function allKnownRelations(): string[] {
  const set = new Set<string>();
  for (const g of RELATION_GROUPS) {
    g.defaults.forEach((r) => set.add(r));
    g.extras.forEach((r) => set.add(r));
  }
  return [...set];
}

interface Props {
  onClose?: () => void;
}

const RelationTypesPanel: Component<Props> = (props) => {
  const [selected, setSelected] = createSignal<Set<string>>(new Set());
  const [custom, setCustom] = createSignal<string>("");
  const [saving, setSaving] = createSignal(false);
  const [message, setMessage] = createSignal<{ kind: "ok" | "err"; text: string } | null>(null);
  const [loaded, setLoaded] = createSignal(false);

  onMount(async () => {
    try {
      const prefs = await getKgPreferences();
      const known = new Set(allKnownRelations());
      const sel = new Set<string>();
      const custs: string[] = [];
      for (const r of prefs.relation_types) {
        sel.add(r);
        if (!known.has(r)) custs.push(r);
      }
      setSelected(sel);
      setCustom(custs.join(", "));
    } catch (e) {
      console.error("Failed to load KG preferences:", e);
      setMessage({ kind: "err", text: `Load failed: ${e}` });
    } finally {
      setLoaded(true);
    }
  });

  const toggle = (verb: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(verb)) next.delete(verb);
      else next.add(verb);
      return next;
    });
    setMessage(null);
  };

  const resetToDefaults = () => {
    const defaults = new Set<string>();
    for (const g of RELATION_GROUPS) g.defaults.forEach((r) => defaults.add(r));
    setSelected(defaults);
    setCustom("");
    setMessage(null);
  };

  const save = async () => {
    setSaving(true);
    setMessage(null);
    try {
      const customList = custom()
        .split(/[,\n]/)
        .map((s) => s.trim().toLowerCase().replace(/[\s-]+/g, "_"))
        .filter((s) => s.length > 0);
      const combined = [...selected(), ...customList];
      const unique = [...new Set(combined)];
      if (unique.length === 0) {
        setMessage({ kind: "err", text: "Select at least one relation type." });
        return;
      }
      const saved = await setKgPreferences(unique);
      const next = new Set(saved.relation_types);
      setSelected(next);
      const known = new Set(allKnownRelations());
      setCustom(saved.relation_types.filter((r) => !known.has(r)).join(", "));
      setMessage({
        kind: "ok",
        text: `Saved ${saved.relation_types.length} relation types. Applies to newly indexed files.`,
      });
    } catch (e) {
      setMessage({ kind: "err", text: String(e) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="h-full flex flex-col bg-gray-900 border-l border-gray-800 text-gray-200">
      <div class="flex items-center justify-between px-4 py-3 border-b border-gray-800 flex-shrink-0">
        <div>
          <h2 class="text-sm font-semibold text-gray-200">Relation Types</h2>
          <p class="text-[11px] text-gray-500 mt-0.5">
            Verbs the LLM extracts. Applies to newly indexed files.
          </p>
        </div>
        <Show when={props.onClose}>
          <button
            type="button"
            onClick={() => props.onClose?.()}
            class="p-1 text-gray-500 hover:text-gray-200 transition-colors"
            title="Close"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </Show>
      </div>

      <div class="flex-1 overflow-y-auto p-4 space-y-4">
        <Show when={loaded()} fallback={<p class="text-xs text-gray-500">Loading…</p>}>
          <For each={RELATION_GROUPS}>
            {(group) => (
              <div>
                <h3 class="text-xs font-semibold text-gray-300 mb-2">{group.name}</h3>
                <div class="grid grid-cols-1 gap-y-1.5">
                  <For each={[...group.defaults, ...group.extras]}>
                    {(verb) => (
                      <label class="flex items-center gap-2 text-xs text-gray-300 cursor-pointer hover:text-gray-100">
                        <input
                          type="checkbox"
                          checked={selected().has(verb)}
                          onChange={() => toggle(verb)}
                          class="w-3.5 h-3.5 rounded border-gray-600 bg-gray-700 text-blue-500 focus:ring-blue-500 focus:ring-offset-0"
                        />
                        <span class="font-mono">{verb}</span>
                        <Show when={group.defaults.includes(verb)}>
                          <span class="text-[10px] text-gray-600">default</span>
                        </Show>
                      </label>
                    )}
                  </For>
                </div>
              </div>
            )}
          </For>

          <div>
            <label class="block text-xs font-semibold text-gray-300 mb-1">
              Custom relation types
            </label>
            <p class="text-[11px] text-gray-500 mb-1.5">
              Comma-separated snake_case verbs. Example:{" "}
              <span class="font-mono">sponsors, inherits_from</span>
            </p>
            <input
              type="text"
              value={custom()}
              onInput={(e) => {
                setCustom(e.currentTarget.value);
                setMessage(null);
              }}
              placeholder="e.g. sponsors, inherits_from"
              class="w-full px-3 py-1.5 bg-gray-950 border border-gray-700 rounded text-xs text-gray-200 font-mono focus:outline-none focus:border-blue-500"
            />
          </div>

          <Show when={message()}>
            {(msg) => (
              <p class={`text-xs ${msg().kind === "ok" ? "text-green-400" : "text-red-400"}`}>
                {msg().text}
              </p>
            )}
          </Show>
        </Show>
      </div>

      <div class="flex items-center gap-2 px-4 py-3 border-t border-gray-800 flex-shrink-0">
        <button
          type="button"
          onClick={save}
          disabled={saving() || !loaded()}
          class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 text-xs text-white rounded transition-colors"
        >
          {saving() ? "Saving…" : "Save"}
        </button>
        <button
          type="button"
          onClick={resetToDefaults}
          disabled={saving() || !loaded()}
          class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 disabled:opacity-50 text-xs text-gray-200 rounded transition-colors"
        >
          Reset
        </button>
        <span class="text-[11px] text-gray-500 ml-auto">{selected().size} selected</span>
      </div>
    </div>
  );
};

export default RelationTypesPanel;
