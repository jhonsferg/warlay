// A small, safe Markdown renderer for GitHub release notes. Everything is
// HTML-escaped first, and only a fixed set of tags is produced, so the result
// can go through `v-html` without letting a release inject markup or scripts.

const ESCAPES: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

export function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (c) => ESCAPES[c]);
}

/** Only web links are kept; anything else (javascript:, data:) is dropped. */
function safeUrl(url: string): string | null {
  return /^https:\/\/[^\s"'<>]+$/i.test(url) || /^http:\/\/[^\s"'<>]+$/i.test(url) ? url : null;
}

/** Inline markup of one already-escaped line: code, bold, italic, links. */
function inline(escaped: string): string {
  const codes: string[] = [];
  let out = escaped.replace(/`([^`]+)`/g, (_, code: string) => {
    codes.push(`<code>${code}</code>`);
    return `\u0000${codes.length - 1}\u0000`;
  });
  out = out
    .replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_whole, label: string, url: string) => {
      // The url was escaped with the rest of the text; undo it for validation.
      const raw = url.replace(/&amp;/g, "&");
      const ok = safeUrl(raw);
      return ok ? `<a href="${escapeHtml(ok)}" target="_blank" rel="noopener noreferrer">${label}</a>` : label;
    })
    .replace(/\*\*([^*]+)\*\*|__([^_]+)__/g, (_, a?: string, b?: string) => `<strong>${a ?? b}</strong>`)
    .replace(/(^|[^*\w])\*([^*\s][^*]*)\*(?!\w)/g, "$1<em>$2</em>");
  return out.replace(/\u0000(\d+)\u0000/g, (_, i: string) => codes[Number(i)]);
}

const isTableRow = (line: string) => /^\s*\|.*\|\s*$/.test(line);
const isTableRule = (line: string) => /^\s*\|[\s:|-]+\|\s*$/.test(line);
const cells = (line: string) =>
  line
    .trim()
    .replace(/^\||\|$/g, "")
    .split("|")
    .map((c) => c.trim());

/** Renders the Markdown of a release into safe HTML. */
export function renderMarkdown(source: string): string {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const html: string[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    if (!line.trim()) {
      i++;
      continue;
    }

    // ``` fenced code
    if (/^\s*```/.test(line)) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i++]);
      i++;
      html.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
      continue;
    }

    const heading = /^(#{1,6})\s+(.*\S)\s*#*\s*$/.exec(line);
    if (heading) {
      // Release pages start at ## already; keep them small inside the card.
      const level = Math.min(6, Math.max(3, heading[1].length + 1));
      html.push(`<h${level}>${inline(escapeHtml(heading[2]))}</h${level}>`);
      i++;
      continue;
    }

    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      html.push("<hr>");
      i++;
      continue;
    }

    if (/^\s*>/.test(line)) {
      const quote: string[] = [];
      while (i < lines.length && /^\s*>/.test(lines[i])) quote.push(lines[i++].replace(/^\s*>\s?/, ""));
      html.push(`<blockquote>${inline(escapeHtml(quote.join(" ")))}</blockquote>`);
      continue;
    }

    if (isTableRow(line)) {
      const rows: string[][] = [];
      while (i < lines.length && isTableRow(lines[i])) {
        if (!isTableRule(lines[i])) rows.push(cells(lines[i]));
        i++;
      }
      const [head, ...body] = rows;
      const row = (tag: string, r: string[]) =>
        `<tr>${r.map((c) => `<${tag}>${inline(escapeHtml(c))}</${tag}>`).join("")}</tr>`;
      html.push(
        `<table><thead>${row("th", head)}</thead><tbody>${body.map((r) => row("td", r)).join("")}</tbody></table>`,
      );
      continue;
    }

    const bullet = /^\s*[-*+]\s+/;
    const ordered = /^\s*\d+[.)]\s+/;
    if (bullet.test(line) || ordered.test(line)) {
      const tag = bullet.test(line) ? "ul" : "ol";
      const marker = tag === "ul" ? bullet : ordered;
      const items: string[] = [];
      while (i < lines.length && marker.test(lines[i])) {
        items.push(`<li>${inline(escapeHtml(lines[i].replace(marker, "").trim()))}</li>`);
        i++;
      }
      html.push(`<${tag}>${items.join("")}</${tag}>`);
      continue;
    }

    // Paragraph: consecutive plain lines.
    const para: string[] = [];
    while (
      i < lines.length &&
      lines[i].trim() &&
      !/^\s*(```|#{1,6}\s|>|[-*+]\s|\d+[.)]\s)/.test(lines[i]) &&
      !isTableRow(lines[i])
    ) {
      para.push(lines[i++].trim());
    }
    html.push(`<p>${inline(escapeHtml(para.join(" ")))}</p>`);
  }

  return html.join("");
}
