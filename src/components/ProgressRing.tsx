/**
 * ProgressRing — a compact circular progress indicator.
 *
 * Two modes:
 *   - **determinate**  (`percent` is a number 0–100): the arc sweeps clockwise
 *     from 12 o'clock, so completeness is readable at a glance.
 *   - **indeterminate** (`percent` is undefined): a short arc spins. Used when
 *     work is in flight but the server can't report a percentage — e.g. the
 *     `/api/index/events` SSE stream is buffered to nothing by the Cloudflare
 *     quick tunnel, so indexing ticks never reach the browser.
 *
 * `state` colours the ring: active (accent) · done (green) · error (red).
 */
import { Component, Show } from "solid-js";

interface Props {
  /** 0–100. Omit for an indeterminate (spinning) ring. */
  percent?: number;
  /** Outer diameter in px. */
  size?: number;
  /** Stroke width in px. */
  stroke?: number;
  state?: "active" | "done" | "error";
  /** Render the number inside the ring (only sensible at size ≥ 28). */
  showLabel?: boolean;
  title?: string;
}

const ProgressRing: Component<Props> = (props) => {
  const size = () => props.size ?? 14;
  const stroke = () => props.stroke ?? 2;
  const state = () => props.state ?? "active";

  // Geometry: radius inset by half the stroke so the ring never clips.
  const r = () => (size() - stroke()) / 2;
  const circumference = () => 2 * Math.PI * r();
  const clamped = () => Math.max(0, Math.min(100, props.percent ?? 0));
  const determinate = () => typeof props.percent === "number";

  const color = () =>
    state() === "done" ? "#16a34a" : state() === "error" ? "#dc2626" : "currentColor";

  // Indeterminate: draw a quarter arc and let CSS spin the whole SVG.
  const dashArray = () =>
    determinate()
      ? `${(clamped() / 100) * circumference()} ${circumference()}`
      : `${circumference() * 0.25} ${circumference()}`;

  return (
    <span
      class="inline-flex items-center justify-center flex-shrink-0"
      title={props.title ?? (determinate() ? `${Math.round(clamped())}%` : "Working…")}
      role="progressbar"
      aria-valuenow={determinate() ? Math.round(clamped()) : undefined}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <span class="relative inline-flex" style={{ width: `${size()}px`, height: `${size()}px` }}>
        <svg
          width={size()}
          height={size()}
          viewBox={`0 0 ${size()} ${size()}`}
          class={determinate() ? undefined : "animate-spin"}
          style={{ "animation-duration": "0.9s" }}
        >
          {/* track */}
          <circle
            cx={size() / 2}
            cy={size() / 2}
            r={r()}
            fill="none"
            stroke="currentColor"
            stroke-width={stroke()}
            opacity="0.18"
          />
          {/* progress arc — rotated so 0% starts at 12 o'clock */}
          <circle
            cx={size() / 2}
            cy={size() / 2}
            r={r()}
            fill="none"
            stroke={color()}
            stroke-width={stroke()}
            stroke-linecap="round"
            stroke-dasharray={dashArray()}
            transform={`rotate(-90 ${size() / 2} ${size() / 2})`}
            style={determinate() ? { transition: "stroke-dasharray 200ms linear" } : undefined}
          />
        </svg>
        <Show when={props.showLabel && determinate()}>
          <span
            class="absolute inset-0 flex items-center justify-center font-mono tabular-nums text-noviq-600"
            style={{ "font-size": `${Math.max(8, size() * 0.32)}px` }}
          >
            {Math.round(clamped())}
          </span>
        </Show>
      </span>
    </span>
  );
};

export default ProgressRing;
