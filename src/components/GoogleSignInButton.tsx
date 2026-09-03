/**
 * "Sign in with Google" for the free trial, via Google Identity Services.
 *
 * ─── Why GIS and not a redirect flow ──────────────────────────────────────
 * GIS hands the browser a signed ID token, which our server verifies against
 * Google. That needs the CLIENT ID only — no client secret anywhere, nothing
 * secret in the bundle, and no redirect/callback route to secure.
 *
 * ⚠️ The client id below is PUBLIC by design: it travels in every OAuth request
 * and is visible to anyone who opens devtools. It is not a credential. If it is
 * ever rotated, change it here AND set `NOVIQ_GOOGLE_CLIENT_ID` on the server —
 * the two must agree, because the server refuses any token whose `aud` does not
 * match its own copy.
 *
 * ⚠️ This component is PRESENTATION. Every limit of the trial — 3 a day, 3 days,
 * the demo-only corpus, Chat and Search only — is enforced server-side. Nothing
 * here may be treated as a gate.
 */
import { Component, createSignal, onCleanup, onMount, Show } from "solid-js";
import { loginWithGoogle } from "../stores/auth";

const CLIENT_ID =
  "924471369073-2fc9ak4891prul7hfgsgp8252l29klea.apps.googleusercontent.com";

const GIS_SRC = "https://accounts.google.com/gsi/client";

declare global {
  interface Window {
    google?: any;
  }
}

/** Load the GIS script once per page, and resolve when it is usable. */
function loadGis(): Promise<void> {
  if (window.google?.accounts?.id) return Promise.resolve();
  const existing = document.querySelector<HTMLScriptElement>(`script[src="${GIS_SRC}"]`);
  if (existing) {
    return new Promise((resolve, reject) => {
      existing.addEventListener("load", () => resolve());
      existing.addEventListener("error", () => reject(new Error("failed to load Google")));
    });
  }
  return new Promise((resolve, reject) => {
    const s = document.createElement("script");
    s.src = GIS_SRC;
    s.async = true;
    s.defer = true;
    s.onload = () => resolve();
    s.onerror = () => reject(new Error("failed to load Google"));
    document.head.appendChild(s);
  });
}

interface Props {
  /** Called after a successful sign-in, so the caller can leave the demo screen. */
  onSignedIn?: () => void;
}

const GoogleSignInButton: Component<Props> = (props) => {
  let host: HTMLDivElement | undefined;
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [ready, setReady] = createSignal(false);
  let cancelled = false;

  const handleCredential = async (response: { credential?: string }) => {
    if (!response?.credential) {
      setError("Google did not return a sign-in. Please try again.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await loginWithGoogle(response.credential);
      props.onSignedIn?.();
    } catch (e: any) {
      // The server's messages are written for a person — the trial-expired and
      // not-configured cases both say what to do next — so surface them as-is.
      setError(
        e?.message ||
          "We could not start your trial. Please try again, or request a quote.",
      );
    } finally {
      setBusy(false);
    }
  };

  onMount(async () => {
    try {
      await loadGis();
      if (cancelled || !host) return;
      window.google.accounts.id.initialize({
        client_id: CLIENT_ID,
        callback: handleCredential,
        // No auto-select: signing someone in merely because they once did is
        // surprising on a page whose whole purpose is an explicit choice.
        auto_select: false,
        cancel_on_tap_outside: true,
      });
      window.google.accounts.id.renderButton(host, {
        theme: "outline",
        size: "large",
        shape: "pill",
        text: "signin_with",
        logo_alignment: "center",
        width: 280,
      });
      setReady(true);
    } catch {
      setError(
        "Google sign-in could not load. Check your connection, or request a quote instead.",
      );
    }
  });

  onCleanup(() => {
    cancelled = true;
  });

  return (
    <div class="flex flex-col items-center">
      {/* GIS renders its own button in here. Reserved height so the layout does
          not jump when it arrives. */}
      <div ref={host} class="min-h-[44px]" />

      <Show when={!ready() && !error()}>
        <p class="mt-2 text-[12px] text-noviq-500">Loading Google sign-in…</p>
      </Show>
      <Show when={busy()}>
        <p class="mt-3 text-[13px] text-noviq-700">Starting your trial…</p>
      </Show>
      <Show when={error()}>
        <p class="mt-3 max-w-sm rounded-lg border border-red-200 bg-red-50 px-3.5 py-2.5 text-[13px] leading-relaxed text-red-700">
          {error()}
        </p>
      </Show>
    </div>
  );
};

export default GoogleSignInButton;
