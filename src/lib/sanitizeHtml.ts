/**
 * The single place answer HTML is sanitised before it reaches `innerHTML`.
 *
 * ── Why this module exists ──────────────────────────────────────────────────
 * S1 of the 2026-08-05 red-team remediation
 * (`docs/security/red-team-findings-2026-08-05.md`). An external tester got the
 * assistant to compose a data-exfiltration URL and present it in an answer
 * (turn 32). Nothing left the server — the assistant has no ability to fetch a
 * URL, only to run a search term — but the answer is rendered as Markdown, and
 * the renderer used DOMPurify's **default profile, which permits `<img>` and
 * remote `src`**. The payload it produced was a bare link, which needs a click.
 * The same payload shaped as a Markdown image would have made the reader's
 * browser fetch the attacker's address with **no click at all**.
 *
 * ⭐ So the fix is in the renderer, not the prompt: it holds no matter what the
 * model can be talked into writing. Guarding the model's output with more
 * instructions does not, as the canary result in that report showed.
 *
 * ⚠️ Three call sites used to sanitise with three different configs
 * (`AnswerBody`, `Markdown`, and the copy-to-clipboard path in `ChatPage`).
 * They all route through here now. Adding a fourth `DOMPurify.sanitize` call
 * elsewhere re-opens the hole silently — put it here instead.
 */
import DOMPurify from "dompurify";

/**
 * Tags that can pull a remote resource with **no user action**, plus the two
 * that rewrite how every other URL on the page resolves.
 *
 * `svg` and `math` are here because both can carry nested loaders
 * (`<image href>`, `<foreignObject>`) that are easy to miss. Answers are prose,
 * tables and citations — none of this is ever legitimate output for us.
 *
 * `form`/`input`/`button` do not auto-load, but an answer that can render a
 * login-looking form is a phishing surface for free; the model has no reason to
 * emit one.
 */
export const FORBIDDEN_TAGS = [
  // auto-loading media
  "img", "picture", "source", "video", "audio", "track",
  // embedded browsing contexts
  "iframe", "frame", "frameset", "embed", "object", "portal",
  // nested loaders
  "svg", "math",
  // remote stylesheets, and CSS that could carry url()
  "link", "style",
  // rewrite URL resolution / trigger navigation
  "base", "meta",
  // interactive surfaces an answer never needs
  "form", "input", "button", "textarea", "select", "option",
];

/**
 * Attributes that fetch or post somewhere on their own, independently of the
 * tag they sit on. Belt and braces: the tags above are already gone, but an
 * attribute allowed onto a surviving element would be enough.
 */
export const FORBIDDEN_ATTRS = [
  "srcset", "poster", "ping", "background", "lowsrc", "dynsrc",
  "formaction", "action", "xlink:href", "data-src", "loading",
];

/** `url(...)` inside an inline style — the CSS route to the same request. */
const CSS_URL = /url\s*\(/i;

let hooksInstalled = false;

/**
 * DOMPurify hooks are global to the instance, so install them exactly once.
 *
 * ⚠️ The `style` **attribute** is deliberately still allowed. F020 confidential
 * masking wraps refilled values in `<mark style="background:#dcfce7;…">`
 * (`lib/maskEngine.ts`), so forbidding it outright would silently stop masked
 * values being visibly flagged in answers. We strip only the dangerous part —
 * a `url(...)` — and leave colour and spacing alone.
 */
function installHooks(): void {
  if (hooksInstalled) return;
  hooksInstalled = true;

  DOMPurify.addHook("afterSanitizeAttributes", (node) => {
    const el = node as Element;
    if (typeof el.getAttribute !== "function") return;

    const style = el.getAttribute("style");
    if (style && CSS_URL.test(style)) el.removeAttribute("style");

    // A link that opens a new tab must not hand the opener over with it.
    // `[W1]` web citations already set this; a model-authored link would not.
    if (el.getAttribute("target")) {
      el.setAttribute("rel", "noopener noreferrer nofollow");
    }
  });
}

/**
 * Sanitise rendered Markdown for display.
 *
 * @param raw        HTML from `marked`.
 * @param extraAttrs Attributes this call site needs preserved beyond the
 *                   defaults (e.g. `AnswerBody`'s `data-n` citation hooks).
 *
 * Links survive: a `[W1]` web citation is a real link, and following one is a
 * deliberate click by the reader. What cannot survive is anything the browser
 * would fetch on its own.
 */
export function sanitizeAnswerHtml(raw: string, extraAttrs: string[] = []): string {
  installHooks();
  return DOMPurify.sanitize(raw, {
    ADD_ATTR: extraAttrs,
    FORBID_TAGS: FORBIDDEN_TAGS,
    FORBID_ATTR: FORBIDDEN_ATTRS,
  });
}
