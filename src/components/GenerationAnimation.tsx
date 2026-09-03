/**
 * GenerationAnimation — a "connecting the dots" constellation shown while Noviq
 * generates an answer. Labeled nodes (Query · Provision · Clause · Fact · Document)
 * light up, edges connect them, and pulses of light travel ALONG the edges toward
 * a converging "Answer" node — evoking Noviq linking the law together to reach the
 * answer. Especially fitting for deep search, whose multi-pass retrieval takes
 * longer. Pure inline SVG + CSS keyframes — no libraries, theme-aware.
 */
import { Component, For } from "solid-js";

// Constellation layout in a 260×120 viewBox. Source nodes on the left/middle
// converge to the ANSWER node on the right.
const NODES: { x: number; y: number; label: string; d: number }[] = [
  { x: 26, y: 24, label: "Query", d: 0.0 },
  { x: 20, y: 62, label: "Provision", d: 0.5 },
  { x: 34, y: 98, label: "Clause", d: 1.0 },
  { x: 104, y: 20, label: "Fact", d: 0.8 },
  { x: 110, y: 92, label: "Document", d: 1.3 },
  { x: 150, y: 60, label: "", d: 1.6 }, // hub (unlabeled)
];
const ANSWER = { x: 232, y: 60 };

// Edges from each source into the hub, then hub → answer. `[from, to]` indices
// into NODES; -1 = the ANSWER node.
const EDGES: [number, number, number][] = [
  [0, 5, 0.0],
  [1, 5, 0.3],
  [2, 5, 0.6],
  [3, 5, 0.9],
  [4, 5, 1.2],
  [5, -1, 1.6],
];

const GenerationAnimation: Component = () => {
  const pt = (i: number) => (i === -1 ? ANSWER : NODES[i]);
  return (
    <div class="w-full flex justify-center py-4">
      <svg viewBox="0 0 260 120" class="w-full max-w-[540px] h-auto overflow-visible" aria-hidden="true">
        <style>{`
          @keyframes nqEdge { 0%,100%{stroke-opacity:.12} 50%{stroke-opacity:.7} }
          @keyframes nqNode { 0%,100%{r:3;fill-opacity:.5} 50%{r:4.4;fill-opacity:1} }
          @keyframes nqAns  { 0%,100%{r:6;fill-opacity:.7} 50%{r:8.5;fill-opacity:1} }
          @keyframes nqHalo { 0%{r:6;opacity:.55} 100%{r:22;opacity:0} }
          @keyframes nqFlow { from{stroke-dashoffset:26} to{stroke-dashoffset:0} }
          @keyframes nqText { 0%,100%{opacity:.45} 50%{opacity:.95} }
          .nqE{stroke:#4d8c3a;stroke-width:1.3;stroke-linecap:round;animation:nqEdge 2.4s ease-in-out infinite}
          .nqFlow{stroke:#6faa5b;stroke-width:2;stroke-linecap:round;stroke-dasharray:5 21;animation:nqFlow 1.5s linear infinite}
          .nqN{fill:#4d8c3a;animation:nqNode 2.4s ease-in-out infinite}
          .nqAns{fill:#2e5724;animation:nqAns 2s ease-in-out infinite}
          .nqHalo{fill:none;stroke:#4d8c3a;stroke-width:1.2;animation:nqHalo 2.4s ease-out infinite}
          .nqT{fill:#3a6f2c;font-size:9px;font-weight:600;animation:nqText 2.4s ease-in-out infinite}
          .nqAT{fill:#2e5724;font-size:10px;font-weight:700}
          @media (prefers-color-scheme: dark){
            .nqE{stroke:#6faa5b}.nqFlow{stroke:#9cc78d}.nqN{fill:#9cc78d}.nqAns{fill:#c6dfbd}
            .nqHalo{stroke:#9cc78d}.nqT{fill:#9cc78d}.nqAT{fill:#c6dfbd}
          }
        `}</style>

        {/* base edges + a light-pulse traveling along each (the "connecting") */}
        <For each={EDGES}>
          {([f, t, delay]) => {
            const a = pt(f), b = pt(t);
            return (
              <>
                <line class="nqE" style={`animation-delay:${delay}s`} x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
                <line class="nqFlow" style={`animation-delay:${delay}s`} x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
              </>
            );
          }}
        </For>

        {/* source / hub nodes with labels */}
        <For each={NODES}>
          {(n) => (
            <>
              <circle class="nqN" style={`animation-delay:${n.d}s`} cx={n.x} cy={n.y} />
              {n.label && (
                <text class="nqT" style={`animation-delay:${n.d}s`} x={n.x} y={n.y - 7} text-anchor="middle">
                  {n.label}
                </text>
              )}
            </>
          )}
        </For>

        {/* converging ANSWER node */}
        <circle class="nqHalo" cx={ANSWER.x} cy={ANSWER.y} />
        <circle class="nqAns" cx={ANSWER.x} cy={ANSWER.y} />
        <text class="nqAT" x={ANSWER.x} y={ANSWER.y - 12} text-anchor="middle">Answer</text>
      </svg>
    </div>
  );
};

export default GenerationAnimation;
