// Model answers are Markdown. Rendered with marked and always cleaned with DOMPurify: the text
// comes from a model (and may quote attachments), so it is never trusted as HTML.

import DOMPurify from "dompurify";
import { marked } from "marked";

marked.setOptions({ gfm: true, breaks: true });

export function renderMarkdown(text: string): string {
  const html = marked.parse(text, { async: false });
  return DOMPurify.sanitize(html, { USE_PROFILES: { html: true }, FORBID_TAGS: ["style", "form", "input", "button", "iframe"] });
}
