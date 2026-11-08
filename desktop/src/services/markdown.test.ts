import assert from "node:assert/strict";
import { test } from "node:test";
import { renderMarkdown } from "./markdown.ts";

test("renders lists, bold and code", () => {
  const html = renderMarkdown("## Novedades\n- Agrega **algo** nuevo\n- Usa `flex`");
  assert.equal(
    html,
    "<h3>Novedades</h3><ul><li>Agrega <strong>algo</strong> nuevo</li><li>Usa <code>flex</code></li></ul>",
  );
});

test("escapes raw HTML", () => {
  const html = renderMarkdown('<script>alert(1)</script> <img src=x onerror="y">');
  assert.ok(!html.includes("<script"));
  assert.ok(!html.includes("<img"));
  assert.ok(html.includes("&lt;script&gt;"));
});

test("keeps only web links", () => {
  assert.ok(renderMarkdown("[ok](https://github.com/a/b)").includes('href="https://github.com/a/b"'));
  const bad = renderMarkdown("[x](javascript:alert(1))");
  assert.ok(!bad.includes("href"));
  assert.ok(bad.includes("x"));
});

test("renders tables and code blocks", () => {
  const table = renderMarkdown("| A | B |\n|---|---|\n| 1 | 2 |");
  assert.ok(table.includes("<th>A</th>") && table.includes("<td>2</td>"));
  assert.equal(renderMarkdown("```\n<b>\n```"), "<pre><code>&lt;b&gt;</code></pre>");
});

test("joins paragraph lines", () => {
  assert.equal(renderMarkdown("uno\ndos\n\ntres"), "<p>uno dos</p><p>tres</p>");
});
