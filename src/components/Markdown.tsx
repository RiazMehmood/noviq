/**
 * Markdown — render assistant answers as formatted, sanitized HTML (headings,
 * subheadings, lists, bold, tables) instead of plain text. `marked` parses,
 * `sanitizeAnswerHtml` strips anything unsafe, Tailwind `prose` styles the
 * result in the Noviq paper theme. Used for both the final answer and the live
 * token stream (marked tolerates incomplete markdown mid-stream).
 *
 * ⚠️ Sanitising goes through `lib/sanitizeHtml`, never `DOMPurify.sanitize`
 * directly — see that module's header for what the default profile lets through.
 */
import { Component, createMemo } from "solid-js";
import { marked } from "marked";
import { sanitizeAnswerHtml } from "../lib/sanitizeHtml";

marked.setOptions({ gfm: true, breaks: true });

const Markdown: Component<{ text: string }> = (props) => {
  const html = createMemo(() => {
    const raw = marked.parse(props.text ?? "", { async: false }) as string;
    return sanitizeAnswerHtml(raw);
  });
  return (
    <div
      class="prose prose-sm max-w-none text-[15px] text-noviq-ink
             prose-headings:text-noviq-ink prose-headings:font-semibold
             prose-h1:text-lg prose-h2:text-base prose-h3:text-[15px]
             prose-p:my-2 prose-li:my-0.5 prose-ul:my-2 prose-ol:my-2
             prose-strong:text-noviq-ink prose-a:text-noviq-600
             prose-table:text-sm prose-th:text-noviq-600
             prose-code:text-noviq-700 prose-code:bg-noviq-panel prose-code:px-1 prose-code:rounded"
      innerHTML={html()}
    />
  );
};

export default Markdown;
