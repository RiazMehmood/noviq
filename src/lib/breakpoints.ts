/**
 * Responsive breakpoint primitives.
 *
 * The app was built desktop-only: fixed-width rails, a single-row nav bar, and a
 * root shell that clipped anything too wide rather than letting it scroll. These
 * helpers let a component ask "am I on a narrow screen?" reactively so a rail can
 * become a slide-over drawer instead of stealing 256px it doesn't have.
 *
 * Values match Tailwind 3's default screens exactly — a component that branches on
 * `isBelowLg()` in TS and styles with `lg:` in markup must agree about where the
 * boundary is, or the drawer and the static rail can both render at once.
 *
 * Hand-rolled rather than pulling in `@solid-primitives/media`: it is ~15 lines and
 * a new runtime dependency for the web bundle is not worth it.
 */
import { createSignal, onCleanup } from "solid-js";

/** Tailwind 3 default breakpoints, in px. Keep in sync with `tailwind.config.js`. */
export const SCREENS = {
  sm: 640,
  md: 768,
  lg: 1024,
  xl: 1280,
  "2xl": 1536,
} as const;

export type ScreenName = keyof typeof SCREENS;

/**
 * Reactive `window.matchMedia`. Returns a signal that tracks the query.
 *
 * SSR/non-browser safe: without `matchMedia` this reports `false` forever, which
 * yields the desktop layout — the historical behaviour, so nothing regresses.
 */
export function useMediaQuery(query: string): () => boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return () => false;
  }
  const mql = window.matchMedia(query);
  const [matches, setMatches] = createSignal(mql.matches);
  const onChange = (e: MediaQueryListEvent) => setMatches(e.matches);

  // Safari < 14 only has the deprecated add/removeListener pair.
  if (typeof mql.addEventListener === "function") {
    mql.addEventListener("change", onChange);
    onCleanup(() => mql.removeEventListener("change", onChange));
  } else {
    mql.addListener(onChange);
    onCleanup(() => mql.removeListener(onChange));
  }
  return matches;
}

/** True while the viewport is narrower than the named breakpoint. */
export function useBelow(screen: ScreenName): () => boolean {
  return useMediaQuery(`(max-width: ${SCREENS[screen] - 0.02}px)`);
}

/** True from the named breakpoint upward — the mirror of `useBelow`. */
export function useFrom(screen: ScreenName): () => boolean {
  return useMediaQuery(`(min-width: ${SCREENS[screen]}px)`);
}

/**
 * Phone-sized: below `md` (768px). The point at which both chat rails must fold
 * away and the nav bar collapses to a menu.
 */
export function useIsMobile(): () => boolean {
  return useBelow("md");
}

/**
 * Tablet-or-smaller: below `lg` (1024px). At 768–1023px there is room for one
 * rail but not two, so the rails become drawers here even though the nav bar
 * still fits as a row.
 */
export function useIsCompact(): () => boolean {
  return useBelow("lg");
}
