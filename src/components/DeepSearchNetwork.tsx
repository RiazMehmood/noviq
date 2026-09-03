/**
 * DeepSearchNetwork — a Canvas electric-grid network shown while DEEP search runs.
 * Plasma currents (teal / gold) travel node→node along a grid; each node they light
 * flares and a legal term (Acts · Provisions · Documents · Rules) fades in beside it,
 * then fades back — evoking Noviq searching the network of law to reach an answer.
 *
 * Vanilla Canvas (no libraries), sized to fill the chat width in a short dark panel
 * (landscape, unlike the standalone 4×6 portrait). Respects prefers-reduced-motion;
 * tears down its RAF loop + observer on unmount.
 */
import { Component, onMount, onCleanup } from "solid-js";

// Neon green + neon blue currents (glow makes them pop on the white chat bg).
const TEAL: [number, number, number] = [16, 222, 108];  // neon green
const GOLD: [number, number, number] = [12, 150, 255];  // neon blue

// Legal + search vocabulary the network cycles through alongside the officer's own
// query words, so the terms feel like live progress (reading the query → matching
// acts/rules → ranking passages) rather than a fixed four.
const LEGAL_TERMS = [
  "Acts", "Rules", "Provisions", "Sections", "Clauses", "Schedules", "Regulations",
  "Ordinances", "Amendments", "Definitions", "Citations", "Penalties", "Procedure",
];
const SEARCH_TERMS = [
  "Searching", "Matching", "Ranking", "Retrieving", "Connecting", "Reading",
  "Cross-refs", "Passages", "Documents", "Evidence",
];
const STOPWORDS = new Set([
  "the", "and", "for", "are", "was", "were", "what", "which", "who", "how", "why",
  "with", "from", "that", "this", "these", "those", "under", "into", "about", "does",
  "can", "you", "your", "all", "any", "has", "have", "had", "will", "shall", "may",
]);

/** Build the term pool: the officer's own query keywords (weighted 2×, so they show
 *  first/often) + legal terms + search terms. */
function buildTerms(query?: string): string[] {
  const words = (query ?? "")
    .replace(/@\S+/g, " ")
    .split(/[^\p{L}\p{N}]+/u)
    .map((w) => w.trim())
    .filter((w) => w.length > 3 && !STOPWORDS.has(w.toLowerCase()))
    .slice(0, 8)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1));
  const pool = [...words, ...words, ...LEGAL_TERMS, ...SEARCH_TERMS];
  return pool.length ? pool : [...LEGAL_TERMS, ...SEARCH_TERMS];
}

const DeepSearchNetwork: Component<{ query?: string }> = (props) => {
  let canvas: HTMLCanvasElement | undefined;
  const TERMS = buildTerms(props.query);

  onMount(() => {
    const cv = canvas!;
    const ctx = cv.getContext("2d")!;
    // Landscape grid tuned for a tall chat panel.
    const COLS = 13, ROWS = 9;
    let DPR = 1, W = 0, H = 0, padX = 0, padY = 0, gx = 0, gy = 0;
    type Node = { x: number; y: number; c: number; r: number; glow: number; term: string | null; termA: number; hue: [number, number, number] };
    let nodes: Node[] = [];
    const idx = (c: number, r: number) => r * COLS + c;

    function resize() {
      const rect = cv.getBoundingClientRect();
      DPR = Math.min(window.devicePixelRatio || 1, 2);
      W = Math.max(1, Math.round(rect.width * DPR));
      H = Math.max(1, Math.round(rect.height * DPR));
      cv.width = W; cv.height = H;
      padX = W * 0.05; padY = H * 0.16;
      gx = (W - padX * 2) / (COLS - 1);
      gy = (H - padY * 2) / (ROWS - 1);
      nodes = [];
      for (let r = 0; r < ROWS; r++)
        for (let c = 0; c < COLS; c++)
          nodes.push({ x: padX + c * gx, y: padY + r * gy, c, r, glow: 0, term: null, termA: 0, hue: TEAL });
    }

    type Pulse = { path: number[]; t: number; speed: number; hue: [number, number, number]; seg: number };
    const pulses: Pulse[] = [];
    function spawnPulse() {
      let pc = (Math.random() * COLS) | 0, pr = (Math.random() * ROWS) | 0;
      const hue = Math.random() < 0.5 ? TEAL : GOLD;
      const path = [idx(pc, pr)];
      const len = 4 + ((Math.random() * 5) | 0);
      let lastDir: string | null = null;
      for (let i = 0; i < len; i++) {
        const opts: [string, number, number][] = [];
        if (pc < COLS - 1 && lastDir !== "L") opts.push(["R", pc + 1, pr]);
        if (pc > 0 && lastDir !== "R") opts.push(["L", pc - 1, pr]);
        if (pr < ROWS - 1 && lastDir !== "U") opts.push(["D", pc, pr + 1]);
        if (pr > 0 && lastDir !== "D") opts.push(["U", pc, pr - 1]);
        if (!opts.length) break;
        const [dir, nc, nr] = opts[(Math.random() * opts.length) | 0];
        path.push(idx(nc, nr)); pc = nc; pr = nr; lastDir = dir;
      }
      pulses.push({ path, t: 0, speed: 0.012 + Math.random() * 0.012, hue, seg: 0 });
    }

    const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
    const rgba = (c: number[], a: number) => `rgba(${c[0]},${c[1]},${c[2]},${a})`;

    function drawGrid() {
      ctx.lineWidth = 0.5 * DPR; // thin hairlines
      ctx.strokeStyle = "rgba(60,90,120,0.06)"; // faint cool hairline on white
      for (let r = 0; r < ROWS; r++) { ctx.beginPath(); ctx.moveTo(padX, padY + r * gy); ctx.lineTo(W - padX, padY + r * gy); ctx.stroke(); }
      for (let c = 0; c < COLS; c++) { ctx.beginPath(); ctx.moveTo(padX + c * gx, padY); ctx.lineTo(padX + c * gx, H - padY); ctx.stroke(); }
      for (const n of nodes) { ctx.beginPath(); ctx.fillStyle = "rgba(60,90,120,0.14)"; ctx.arc(n.x, n.y, 1.3 * DPR, 0, 7); ctx.fill(); }
    }

    function drawPulses(dt: number) {
      ctx.lineCap = "round";
      for (let i = pulses.length - 1; i >= 0; i--) {
        const p = pulses[i];
        p.t += p.speed * dt;
        while (p.t >= 1 && p.seg < p.path.length - 2) { p.t -= 1; p.seg++; }
        const done = p.seg >= p.path.length - 2 && p.t >= 1;
        const head = Math.min(p.seg + p.t, p.path.length - 1);
        const TAIL = 2.2;
        ctx.save();
        for (let s = 0; s < p.path.length - 1; s++) {
          const a = nodes[p.path[s]], b = nodes[p.path[s + 1]];
          const behind = head - (s + 1);
          if (behind > TAIL || behind < -1) continue;
          let bx = b.x, by = b.y;
          if (s === Math.floor(head)) { const frac = head - s; bx = lerp(a.x, b.x, frac); by = lerp(a.y, b.y, frac); }
          const alpha = Math.max(0, 1 - Math.abs(behind) / TAIL);
          // soft outer glow
          ctx.strokeStyle = rgba(p.hue, alpha * 0.28); ctx.lineWidth = 6 * DPR;
          ctx.shadowBlur = 12 * DPR; ctx.shadowColor = rgba(p.hue, alpha * 0.55);
          ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(bx, by); ctx.stroke();
          // saturated core
          ctx.strokeStyle = rgba(p.hue, alpha * 0.95); ctx.lineWidth = 2 * DPR; ctx.shadowBlur = 4 * DPR;
          ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(bx, by); ctx.stroke();
        }
        ctx.restore();
        const hi = Math.round(head);
        const node = nodes[p.path[Math.min(hi, p.path.length - 1)]];
        node.glow = 1; node.hue = p.hue;
        if (!node.term || node.termA < 0.05) node.term = TERMS[(Math.random() * TERMS.length) | 0];
        node.termA = Math.min(1, node.termA + 0.12 * dt);
        if (done) pulses.splice(i, 1);
      }
    }

    function drawNodes(dt: number) {
      for (const n of nodes) {
        if (n.glow <= 0.001 && n.termA <= 0.001) continue;
        n.glow = Math.max(0, n.glow - 0.03 * dt);
        if (n.glow < 0.4) n.termA = Math.max(0, n.termA - 0.05 * dt);
        if (n.glow > 0.01) {
          ctx.save();
          const g = n.glow;
          ctx.shadowBlur = 16 * DPR * g; ctx.shadowColor = rgba(n.hue, 0.7 * g);
          ctx.fillStyle = rgba(n.hue, 0.95 * g);
          ctx.beginPath(); ctx.arc(n.x, n.y, (2 + 3.4 * g) * DPR, 0, 7); ctx.fill();
          ctx.restore();
        }
        if (n.termA > 0.01 && n.term) {
          ctx.save();
          const ty = n.r > 0 ? n.y - 11 * DPR : n.y + 18 * DPR;
          ctx.font = `${11 * DPR}px ui-sans-serif, system-ui, sans-serif`;
          ctx.textAlign = "center"; ctx.textBaseline = "middle";
          ctx.shadowBlur = 9 * DPR; ctx.shadowColor = rgba(n.hue, n.termA);
          ctx.fillStyle = rgba(n.hue, Math.min(1, n.termA));
          ctx.fillText(n.term, n.x, ty);
          ctx.restore();
        }
      }
    }

    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    let last = performance.now(), spawnAcc = 0, raf = 0;
    function frame(now: number) {
      const dt = Math.min(3, (now - last) / 16.67); last = now;
      // Transparent — the chat background (white/paper) shows through so the network
      // reads as part of the empty chat window.
      ctx.clearRect(0, 0, W, H);
      drawGrid();
      spawnAcc += dt;
      if (pulses.length < 3 && spawnAcc > (reduce ? 40 : 14)) { spawnPulse(); spawnAcc = 0; }
      if (pulses.length < 1) spawnPulse();
      drawPulses(dt);
      drawNodes(dt);
      raf = requestAnimationFrame(frame);
    }

    const ro = new ResizeObserver(resize);
    ro.observe(cv);
    resize(); spawnPulse(); spawnPulse();
    raf = requestAnimationFrame(frame);
    onCleanup(() => { cancelAnimationFrame(raf); ro.disconnect(); });
  });

  return (
    <div class="mt-3 w-full">
      {/* Shorter on phones — 300px of decorative canvas pushed the answer itself
          off a 390px screen. The canvas measures its own rect, so it re-fits. */}
      <canvas ref={canvas} class="block h-[180px] sm:h-[300px] w-full" aria-hidden="true" />
    </div>
  );
};

export default DeepSearchNetwork;
