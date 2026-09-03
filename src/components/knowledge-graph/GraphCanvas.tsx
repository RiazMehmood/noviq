/**
 * GraphCanvas — T075 (US4).
 * Force-directed knowledge graph: SVG nodes (entities) + edges (relationships).
 * Pan + zoom via pointer drag and wheel. Type-filter dropdown. 500-node cap.
 * Click node → EntityPanel sidebar.
 */
import {
  Component,
  createSignal,
  createEffect,
  For,
  Show,
  onMount,
  onCleanup,
  batch,
} from "solid-js";
import { getKnowledgeGraph } from "../../api/index";
import EntityPanel from "./EntityPanel";

// ─── Types ────────────────────────────────────────────────────────────────────

interface GraphNode {
  id: string;
  canonical_value: string;
  entity_type: string;
  source_count: number;
  // layout positions (mutated in-place by simulation)
  x: number;
  y: number;
  vx: number;
  vy: number;
}

interface GraphEdge {
  id: string;
  from_entity_id: string;
  to_entity_id: string;
  relation_type: string;
}

// ─── Constants ────────────────────────────────────────────────────────────────

const NODE_RADIUS = 11;
const EDGE_LABEL_OFFSET = 12; // perpendicular distance of label from edge line
const WIDTH = 1200;
const HEIGHT = 800;

function nodeRadius(sourceCount: number): number {
  // Radius grows with source_count but caps — hubs look important, not huge.
  return NODE_RADIUS + Math.min(8, Math.sqrt(Math.max(1, sourceCount)) * 2);
}

const TYPE_COLORS: Record<string, { fill: string; stroke: string; text: string }> = {
  person:   { fill: "#1e3a5f", stroke: "#3b82f6", text: "#93c5fd" },
  org:      { fill: "#2e1a4f", stroke: "#8b5cf6", text: "#c4b5fd" },
  project:  { fill: "#1a3a2f", stroke: "#10b981", text: "#6ee7b7" },
  decision: { fill: "#3a2a1a", stroke: "#f59e0b", text: "#fcd34d" },
  date:     { fill: "#2a2a2a", stroke: "#6b7280", text: "#d1d5db" },
  email:    { fill: "#1a3a3a", stroke: "#06b6d4", text: "#67e8f9" },
  amount:   { fill: "#3a3a1a", stroke: "#eab308", text: "#fde68a" },
};

function nodeColor(type: string) {
  return TYPE_COLORS[type] ?? { fill: "#1f2937", stroke: "#4b5563", text: "#9ca3af" };
}

// ─── Radial / hub layout ─────────────────────────────────────────────────────
// Deterministic BFS-tree layout. For each connected component: pick the
// highest-degree node as the root, place it at the centre, fan its direct
// neighbours around it as spokes, then recursively place deeper neighbours
// in a 120° sector around their parent's angle. Produces a "Co-ordinator
// with end-devices" star for hub graphs, and a gentle fan for chains.
//
// Why radial (not force-directed): our graphs are sparse trees/chains with
// very few edges. Force simulations on chains converge to straight lines
// with labels colliding. Geometric layout avoids that by construction.

function radialLayout(nodes: GraphNode[], edges: GraphEdge[]): void {
  // Adjacency map.
  const adj = new Map<string, Set<string>>();
  for (const n of nodes) adj.set(n.id, new Set());
  for (const e of edges) {
    adj.get(e.from_entity_id)?.add(e.to_entity_id);
    adj.get(e.to_entity_id)?.add(e.from_entity_id);
  }

  // Connected components.
  const visited = new Set<string>();
  const components: string[][] = [];
  for (const n of nodes) {
    if (visited.has(n.id)) continue;
    const comp: string[] = [];
    const stack = [n.id];
    visited.add(n.id);
    while (stack.length) {
      const id = stack.pop()!;
      comp.push(id);
      for (const nb of adj.get(id) ?? []) {
        if (!visited.has(nb)) {
          visited.add(nb);
          stack.push(nb);
        }
      }
    }
    components.push(comp);
  }

  // Orphans (size=1 components) go to a grid below connected components.
  const connectedComps = components.filter((c) => c.length > 1);
  const orphans = components.filter((c) => c.length === 1).flat();

  // Sort connected components biggest first.
  connectedComps.sort((a, b) => b.length - a.length);

  const nodeById = new Map(nodes.map((n) => [n.id, n]));
  const LEVEL_RADIUS = 170;

  // Estimate each connected component's outer radius so we can tile them.
  const compInfo = connectedComps.map((comp) => {
    // Rough: radius grows with BFS depth — but we don't know depth until we
    // run BFS. Use size heuristic: sqrt(size) levels × LEVEL_RADIUS.
    const est = Math.max(LEVEL_RADIUS, Math.ceil(Math.sqrt(comp.length)) * LEVEL_RADIUS);
    return { comp, radius: est };
  });

  // Tile components horizontally, centred on canvas mid-y.
  let cursorX = 0;
  const centers = compInfo.map(({ radius }) => {
    const cx = cursorX + radius;
    cursorX = cx + radius + 80;
    return { cx, cy: HEIGHT / 2, r: radius };
  });
  const layoutW = cursorX;
  const offsetX = WIDTH / 2 - layoutW / 2;
  centers.forEach((c) => (c.cx += offsetX));

  // Lay out each connected component.
  for (let ci = 0; ci < connectedComps.length; ci++) {
    const comp = connectedComps[ci];
    const center = centers[ci];

    // Root = highest-degree node.
    let root = comp[0];
    let maxDeg = -1;
    for (const id of comp) {
      const deg = adj.get(id)?.size ?? 0;
      if (deg > maxDeg) { maxDeg = deg; root = id; }
    }

    // BFS tree.
    const parent = new Map<string, string | null>([[root, null]]);
    const depth = new Map<string, number>([[root, 0]]);
    const children = new Map<string, string[]>();
    for (const id of comp) children.set(id, []);
    const queue = [root];
    while (queue.length) {
      const id = queue.shift()!;
      for (const nb of adj.get(id) ?? []) {
        if (!parent.has(nb)) {
          parent.set(nb, id);
          depth.set(nb, (depth.get(id) ?? 0) + 1);
          children.get(id)!.push(nb);
          queue.push(nb);
        }
      }
    }

    // Descendant count drives sector-width allocation.
    const descCount = new Map<string, number>();
    const countDesc = (id: string): number => {
      let total = 1;
      for (const c of children.get(id) ?? []) total += countDesc(c);
      descCount.set(id, total);
      return total;
    };
    countDesc(root);

    // Place root at component centre.
    const rootNode = nodeById.get(root)!;
    rootNode.x = center.cx;
    rootNode.y = center.cy;
    rootNode.vx = 0; rootNode.vy = 0;

    // Recursive radial placement.
    const place = (
      id: string,
      angleStart: number,
      angleEnd: number,
      parentAngle: number,
      isRoot: boolean,
    ): void => {
      const kids = children.get(id) ?? [];
      if (kids.length === 0) return;

      // Root: full circle. Inner nodes: 120° sector around parent's angle
      // (prevents chains from collapsing into a straight line).
      let start = angleStart;
      let end = angleEnd;
      if (!isRoot) {
        const half = Math.PI / 3; // 60° each side
        start = parentAngle - half;
        end = parentAngle + half;
      }

      const totalDesc = kids.reduce((s, k) => s + (descCount.get(k) ?? 1), 0);
      let cursor = start;
      const myDepth = depth.get(id) ?? 0;

      for (const k of kids) {
        const share = (descCount.get(k) ?? 1) / totalDesc;
        const kStart = cursor;
        const kEnd = cursor + (end - start) * share;
        const kAngle = (kStart + kEnd) / 2;

        const r = (myDepth + 1) * LEVEL_RADIUS;
        const kNode = nodeById.get(k)!;
        kNode.x = center.cx + Math.cos(kAngle) * r;
        kNode.y = center.cy + Math.sin(kAngle) * r;
        kNode.vx = 0; kNode.vy = 0;

        place(k, kStart, kEnd, kAngle, false);
        cursor = kEnd;
      }
    };

    place(root, 0, Math.PI * 2, 0, true);
  }

  // Orphans: simple grid below the connected components.
  if (orphans.length > 0) {
    const cols = Math.ceil(Math.sqrt(orphans.length));
    const spacing = 70;
    const gridW = (cols - 1) * spacing;
    const startX = WIDTH / 2 - gridW / 2;
    const maxConnectedY = centers.reduce(
      (max, c) => Math.max(max, c.cy + c.r),
      HEIGHT / 2,
    );
    const startY = maxConnectedY + 120;
    orphans.forEach((id, i) => {
      const col = i % cols;
      const row = Math.floor(i / cols);
      const n = nodeById.get(id)!;
      n.x = startX + col * spacing;
      n.y = startY + row * spacing;
      n.vx = 0; n.vy = 0;
    });
  }
}

function fitTransform(
  nodes: GraphNode[],
  viewW: number,
  viewH: number,
): { x: number; y: number; scale: number } {
  if (nodes.length === 0) return { x: 0, y: 0, scale: 1 };
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const n of nodes) {
    const r = nodeRadius(n.source_count) + 18; // include label overhang
    if (n.x - r < minX) minX = n.x - r;
    if (n.y - r < minY) minY = n.y - r;
    if (n.x + r > maxX) maxX = n.x + r;
    if (n.y + r > maxY) maxY = n.y + r;
  }
  const bboxW = Math.max(1, maxX - minX);
  const bboxH = Math.max(1, maxY - minY);
  const scale = Math.min(viewW / bboxW, viewH / bboxH, 2.5);
  const cx = (minX + maxX) / 2;
  const cy = (minY + maxY) / 2;
  return {
    x: viewW / 2 - cx * scale,
    y: viewH / 2 - cy * scale,
    scale,
  };
}

// ─── Component ────────────────────────────────────────────────────────────────

const ENTITY_TYPES = ["all", "person", "org", "project", "decision", "date", "email", "amount"];

interface GraphCanvasProps {
  sourceIds?: () => string[];
}

const GraphCanvas: Component<GraphCanvasProps> = (props) => {
  const [allNodes, setAllNodes] = createSignal<GraphNode[]>([]);
  const [allEdges, setAllEdges] = createSignal<GraphEdge[]>([]);
  const [typeFilter, setTypeFilter] = createSignal("all");
  const [showOrphans, setShowOrphans] = createSignal(false);
  const [selectedRelationTypes, setSelectedRelationTypes] = createSignal<string[]>([]);
  const [selectedNode, setSelectedNode] = createSignal<GraphNode | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);

  // Set of node IDs that appear on at least one edge.
  const connectedIds = () => {
    const set = new Set<string>();
    for (const e of allEdges()) {
      set.add(e.from_entity_id);
      set.add(e.to_entity_id);
    }
    return set;
  };
  const orphanCount = () =>
    allNodes().length - allNodes().filter((n) => connectedIds().has(n.id)).length;

  // Pan + zoom state.
  const [transform, setTransform] = createSignal({ x: 0, y: 0, scale: 1 });
  let svgRef: SVGSVGElement | undefined;
  let isPanning = false;
  let panStart = { x: 0, y: 0 };
  let panOrigin = { x: 0, y: 0 };

  // ── Load / reload graph data when source filter changes ──────────────────
  createEffect(async () => {
    const ids = props.sourceIds?.() ?? [];
    setLoading(true);
    setError(null);
    setSelectedNode(null);
    try {
      const data = await getKnowledgeGraph(500, 0, ids.length > 0 ? ids : undefined);
      const nodes: GraphNode[] = (data.nodes as any[]).map((n) => ({
        ...n,
        x: 0, y: 0, vx: 0, vy: 0,
      }));
      const edges: GraphEdge[] = (data.edges as any[]).map((e) => ({
        id: e.id,
        from_entity_id: e.from_entity_id,
        to_entity_id: e.to_entity_id,
        relation_type: e.relation_type,
      }));

      radialLayout(nodes, edges);
      batch(() => {
        setAllNodes(nodes);
        setAllEdges(edges);
        setTransform(fitTransform(nodes, WIDTH, HEIGHT));
      });
    } catch (e: any) {
      setError(e?.toString() ?? "Failed to load knowledge graph");
    } finally {
      setLoading(false);
    }
  });

  // ── Derived filtered sets ────────────────────────────────────────────────
  const visibleNodes = () => {
    const f = typeFilter();
    const connected = connectedIds();
    const hideOrphans = !showOrphans();
    return allNodes().filter((n) => {
      if (f !== "all" && n.entity_type !== f) return false;
      if (hideOrphans && !connected.has(n.id)) return false;
      return true;
    });
  };

  const availableRelationTypes = () => {
    const seen = new Set<string>();
    for (const e of allEdges()) seen.add(e.relation_type);
    return Array.from(seen).sort();
  };

  const visibleEdges = () => {
    const ids = new Set(visibleNodes().map((n) => n.id));
    const selected = selectedRelationTypes();
    return allEdges().filter(
      (e) =>
        ids.has(e.from_entity_id) &&
        ids.has(e.to_entity_id) &&
        (selected.length === 0 || selected.includes(e.relation_type)),
    );
  };

  const nodeById = () => new Map(allNodes().map((n) => [n.id, n]));

  // ── Pan + zoom handlers ──────────────────────────────────────────────────
  function onPointerDown(e: PointerEvent) {
    if ((e.target as Element).closest(".graph-node")) return; // let node handle
    isPanning = true;
    panStart = { x: e.clientX, y: e.clientY };
    panOrigin = { x: transform().x, y: transform().y };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!isPanning) return;
    const dx = e.clientX - panStart.x;
    const dy = e.clientY - panStart.y;
    setTransform((t) => ({ ...t, x: panOrigin.x + dx, y: panOrigin.y + dy }));
  }

  function onPointerUp() {
    isPanning = false;
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const factor = e.deltaY < 0 ? 1.1 : 0.9;
    setTransform((t) => {
      const newScale = Math.max(0.2, Math.min(4, t.scale * factor));
      return { ...t, scale: newScale };
    });
  }

  // ── Cleanup ──────────────────────────────────────────────────────────────
  onMount(() => {
    svgRef?.addEventListener("wheel", onWheel, { passive: false });
  });
  onCleanup(() => {
    svgRef?.removeEventListener("wheel", onWheel);
  });

  // ── Label truncation ─────────────────────────────────────────────────────
  function truncate(s: string, max = 12): string {
    return s.length > max ? s.slice(0, max) + "…" : s;
  }

  return (
    <div class="flex h-full bg-gray-950">
      {/* Main graph area */}
      <div class="flex-1 flex flex-col min-w-0">
        {/* Toolbar */}
        <div class="flex items-center gap-3 px-4 py-2 border-b border-gray-800 bg-gray-950">
          <span class="text-xs text-gray-500">
            {visibleNodes().length} entities · {visibleEdges().length} relationships
            <Show when={!showOrphans() && orphanCount() > 0}>
              <span class="text-gray-600"> · {orphanCount()} hidden</span>
            </Show>
          </span>
          <div class="flex items-center gap-3 ml-auto">
            <label class="flex items-center gap-1.5 text-xs text-gray-400 cursor-pointer select-none">
              <input
                type="checkbox"
                checked={showOrphans()}
                onChange={(e) => {
                  setShowOrphans(e.currentTarget.checked);
                  setSelectedNode(null);
                  setTransform(fitTransform(visibleNodes(), WIDTH, HEIGHT));
                }}
                class="accent-blue-500"
              />
              Show orphans
            </label>
            <label class="text-xs text-gray-400">Filter:</label>
            <select
              value={typeFilter()}
              onChange={(e) => {
                setTypeFilter(e.currentTarget.value);
                setSelectedNode(null);
                setTransform(fitTransform(visibleNodes(), WIDTH, HEIGHT));
              }}
              class="text-xs bg-gray-800 border border-gray-700 text-gray-300 rounded px-2 py-1 focus:outline-none focus:ring-1 focus:ring-blue-500"
            >
              <For each={ENTITY_TYPES}>
                {(t) => <option value={t}>{t === "all" ? "All types" : t}</option>}
              </For>
            </select>
            <Show when={availableRelationTypes().length > 0}>
              <label class="text-xs text-gray-400">Relations:</label>
              <select
                multiple
                size={Math.min(4, availableRelationTypes().length)}
                onChange={(e) => {
                  const opts = Array.from(e.currentTarget.selectedOptions).map((o) => o.value);
                  setSelectedRelationTypes(opts);
                  setTransform(fitTransform(visibleNodes(), WIDTH, HEIGHT));
                }}
                class="text-xs bg-gray-800 border border-gray-700 text-gray-300 rounded px-2 py-1 focus:outline-none focus:ring-1 focus:ring-blue-500"
                title="Hold Ctrl/Cmd to select multiple. No selection = show all."
              >
                <For each={availableRelationTypes()}>
                  {(t) => (
                    <option
                      value={t}
                      selected={selectedRelationTypes().includes(t)}
                    >
                      {t}
                    </option>
                  )}
                </For>
              </select>
            </Show>
          </div>
        </div>

        {/* SVG canvas */}
        <div class="flex-1 relative overflow-hidden">
          <Show when={loading()}>
            <div class="absolute inset-0 flex items-center justify-center text-gray-500 text-sm animate-pulse">
              Building graph…
            </div>
          </Show>

          <Show when={error()}>
            <div class="absolute inset-0 flex items-center justify-center text-red-400 text-sm">
              {error()}
            </div>
          </Show>

          <Show when={!loading() && allNodes().length === 0}>
            <div class="absolute inset-0 flex items-center justify-center text-gray-600 text-sm">
              No entities found. Index some documents first.
            </div>
          </Show>

          <Show when={!loading() && allNodes().length > 0}>
            <svg
              ref={svgRef}
              class="w-full h-full cursor-grab active:cursor-grabbing select-none"
              viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
              onPointerDown={onPointerDown}
              onPointerMove={onPointerMove}
              onPointerUp={onPointerUp}
            >
              <defs>
                <marker
                  id="arrow"
                  markerWidth="6"
                  markerHeight="6"
                  refX="5"
                  refY="3"
                  orient="auto"
                >
                  <path d="M0,0 L0,6 L6,3 z" fill="#4b5563" />
                </marker>
              </defs>

              <g transform={`translate(${transform().x},${transform().y}) scale(${transform().scale})`}>
                {/* Edges */}
                <For each={visibleEdges()}>
                  {(edge) => {
                    const nb = nodeById();
                    const src = nb.get(edge.from_entity_id);
                    const tgt = nb.get(edge.to_entity_id);
                    if (!src || !tgt) return null;
                    const sel = selectedNode();
                    const touchesSelected =
                      !!sel && (sel.id === src.id || sel.id === tgt.id);
                    const dense = visibleEdges().length > 30;
                    const showLabel =
                      touchesSelected || !dense || transform().scale >= 1.4;
                    const dx = tgt.x - src.x;
                    const dy = tgt.y - src.y;
                    const d = Math.sqrt(dx * dx + dy * dy) || 1;
                    // Perpendicular unit vector, flipped so label stays on the
                    // upper / left side for readability.
                    let nx = -dy / d;
                    let ny = dx / d;
                    if (ny > 0) { nx = -nx; ny = -ny; }
                    const mx = (src.x + tgt.x) / 2 + nx * EDGE_LABEL_OFFSET;
                    const my = (src.y + tgt.y) / 2 + ny * EDGE_LABEL_OFFSET;
                    return (
                      <g>
                        <line
                          x1={src.x}
                          y1={src.y}
                          x2={tgt.x}
                          y2={tgt.y}
                          stroke={touchesSelected ? "#6b7280" : "#4b5563"}
                          stroke-width={touchesSelected ? 2 : 1.2}
                          marker-end="url(#arrow)"
                        />
                        <Show when={showLabel}>
                          <text
                            x={mx}
                            y={my}
                            text-anchor="middle"
                            dominant-baseline="middle"
                            font-size="10"
                            font-weight="500"
                            fill="#cbd5e1"
                            stroke="#030712"
                            stroke-width="3.5"
                            paint-order="stroke"
                            class="pointer-events-none select-none"
                          >
                            {edge.relation_type}
                          </text>
                        </Show>
                      </g>
                    );
                  }}
                </For>

                {/* Nodes */}
                <For each={visibleNodes()}>
                  {(node) => {
                    const colors = nodeColor(node.entity_type);
                    const isSelected = () => selectedNode()?.id === node.id;
                    const r = nodeRadius(node.source_count);
                    return (
                      <g
                        class="graph-node cursor-pointer"
                        transform={`translate(${node.x},${node.y})`}
                        onClick={() =>
                          setSelectedNode(isSelected() ? null : node)
                        }
                      >
                        <circle
                          r={r + (isSelected() ? 3 : 0)}
                          fill={colors.fill}
                          stroke={isSelected() ? "#f9fafb" : colors.stroke}
                          stroke-width={isSelected() ? 2.5 : 1.5}
                        />
                        <text
                          y={r + 13}
                          text-anchor="middle"
                          font-size="11"
                          font-weight="500"
                          fill={colors.text}
                          stroke="#030712"
                          stroke-width="3"
                          paint-order="stroke"
                          class="pointer-events-none select-none"
                        >
                          {truncate(node.canonical_value, 18)}
                        </text>
                      </g>
                    );
                  }}
                </For>
              </g>
            </svg>

            {/* Zoom controls */}
            <div class="absolute bottom-4 right-4 flex flex-col gap-1">
              <button
                onClick={() => setTransform((t) => ({ ...t, scale: Math.min(4, t.scale * 1.2) }))}
                class="w-8 h-8 bg-gray-800 border border-gray-700 rounded text-gray-300 hover:text-white flex items-center justify-center text-sm"
              >
                +
              </button>
              <button
                onClick={() => setTransform((t) => ({ ...t, scale: Math.max(0.2, t.scale / 1.2) }))}
                class="w-8 h-8 bg-gray-800 border border-gray-700 rounded text-gray-300 hover:text-white flex items-center justify-center text-sm"
              >
                −
              </button>
              <button
                onClick={() => setTransform(fitTransform(visibleNodes(), WIDTH, HEIGHT))}
                class="w-8 h-8 bg-gray-800 border border-gray-700 rounded text-gray-300 hover:text-white flex items-center justify-center text-xs"
                title="Fit to view"
              >
                ⌂
              </button>
            </div>
          </Show>
        </div>
      </div>

      {/* Entity detail panel */}
      <Show when={selectedNode()}>
        {(node) => (
          <div class="w-72 flex-shrink-0">
            <EntityPanel
              entity={node()}
              onClose={() => setSelectedNode(null)}
            />
          </div>
        )}
      </Show>
    </div>
  );
};

export default GraphCanvas;
