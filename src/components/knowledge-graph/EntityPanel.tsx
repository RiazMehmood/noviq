/**
 * EntityPanel — T074 (US4).
 * Entity detail sidebar: type badge, value, source count, source list with excerpts.
 */
import { Component, createSignal, For, onMount, Show } from "solid-js";
import { getEntitySources, getEntityRelationships } from "../../api/index";

interface EntityNode {
  id: string;
  canonical_value: string;
  entity_type: string;
  source_count: number;
}

interface Props {
  entity: EntityNode;
  onClose?: () => void;
}

function typeColor(type: string): string {
  switch (type) {
    case "person":    return "bg-blue-900 text-blue-300";
    case "org":       return "bg-purple-900 text-purple-300";
    case "project":   return "bg-green-900 text-green-300";
    case "decision":  return "bg-orange-900 text-orange-300";
    case "date":      return "bg-gray-700 text-gray-300";
    case "email":     return "bg-teal-900 text-teal-300";
    case "amount":    return "bg-yellow-900 text-yellow-300";
    default:          return "bg-gray-700 text-gray-400";
  }
}

const EntityPanel: Component<Props> = (props) => {
  const [sources, setSources] = createSignal<any[]>([]);
  const [relationships, setRelationships] = createSignal<any[]>([]);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    try {
      const [srcs, rels] = await Promise.all([
        getEntitySources(props.entity.id),
        getEntityRelationships(props.entity.id),
      ]);
      setSources(srcs as any[]);
      setRelationships(rels as any[]);
    } catch (e) {
      console.error("EntityPanel load error:", e);
    } finally {
      setLoading(false);
    }
  });

  return (
    <div class="flex flex-col h-full bg-gray-900 border-l border-gray-700">
      {/* Header */}
      <div class="flex items-start justify-between p-4 border-b border-gray-700">
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2 mb-1">
            <span class={`px-2 py-0.5 text-xs rounded-full font-medium ${typeColor(props.entity.entity_type)}`}>
              {props.entity.entity_type}
            </span>
          </div>
          <h3 class="text-sm font-semibold text-gray-100 break-words">
            {props.entity.canonical_value}
          </h3>
          <p class="text-xs text-gray-500 mt-1">
            Mentioned in {props.entity.source_count} source{props.entity.source_count !== 1 ? "s" : ""}
          </p>
        </div>
        <Show when={props.onClose}>
          <button
            onClick={props.onClose}
            class="ml-2 p-1 text-gray-600 hover:text-gray-400 transition-colors flex-shrink-0"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </Show>
      </div>

      <div class="flex-1 overflow-y-auto p-4 space-y-5">
        <Show when={loading()}>
          <div class="text-sm text-gray-500 animate-pulse">Loading…</div>
        </Show>

        {/* Source documents */}
        <Show when={!loading() && sources().length > 0}>
          <section>
            <h4 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3">
              Source Documents
            </h4>
            <div class="space-y-2">
              <For each={sources()}>
                {(src) => (
                  <div class="bg-gray-800 rounded-lg p-3 border border-gray-700">
                    <p class="text-xs font-medium text-gray-300 mb-1">{src.file_name}</p>
                    <p class="text-xs text-gray-500 line-clamp-3">{src.excerpt}</p>
                  </div>
                )}
              </For>
            </div>
          </section>
        </Show>

        {/* Relationships */}
        <Show when={!loading() && relationships().length > 0}>
          <section>
            <h4 class="text-xs font-semibold text-gray-400 uppercase tracking-wider mb-3">
              Relationships ({relationships().length})
            </h4>
            <div class="space-y-1">
              <For each={relationships()}>
                {(rel) => (
                  <div class="flex items-center gap-2 text-xs text-gray-400 py-1">
                    <span class="text-gray-600">
                      {rel.from_entity_id === props.entity.id ? "→" : "←"}
                    </span>
                    <span class="text-blue-400">{rel.relation_type}</span>
                    <span class="text-gray-500 truncate">
                      {rel.from_entity_id === props.entity.id
                        ? rel.to_entity_id
                        : rel.from_entity_id}
                    </span>
                  </div>
                )}
              </For>
            </div>
          </section>
        </Show>

        <Show when={!loading() && sources().length === 0 && relationships().length === 0}>
          <p class="text-xs text-gray-600">No sources or relationships found.</p>
        </Show>
      </div>
    </div>
  );
};

export default EntityPanel;
