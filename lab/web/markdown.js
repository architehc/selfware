// lab/web/markdown.js — minimal markdown renderer (no dependencies)
function renderMarkdown(src) {
  const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const inline = (s) =>
    esc(s)
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/\*([^*]+)\*/g, "<em>$1</em>")
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (m, text, href) => {
        const safe = href.replace(/"/g, "&quot;");
        // Internal #/lesson/... links must navigate in the same tab; only
        // external links get a new tab + noopener.
        const ext = safe.startsWith("#") ? "" : ' target="_blank" rel="noopener"';
        return `<a href="${safe}"${ext}>${text}</a>`;
      });
  const lines = src.split("\n");
  let html = "", i = 0, inList = false;
  const closeList = () => { if (inList) { html += "</ul>"; inList = false; } };
  while (i < lines.length) {
    const line = lines[i];
    if (line.startsWith("```")) {
      closeList();
      let buf = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("```")) buf.push(lines[i++]);
      i++;
      html += "<pre><code>" + esc(buf.join("\n")) + "</code></pre>";
      continue;
    }
    const h = line.match(/^(#{1,3})\s+(.*)/);
    if (h) { closeList(); html += `<h${h[1].length}>${inline(h[2])}</h${h[1].length}>`; i++; continue; }
    if (line.startsWith("> ")) { closeList(); html += `<blockquote>${inline(line.slice(2))}</blockquote>`; i++; continue; }
    if (line.startsWith("- ")) {
      if (!inList) { html += "<ul>"; inList = true; }
      html += `<li>${inline(line.slice(2))}</li>`; i++; continue;
    }
    if (line.trim() === "") { closeList(); i++; continue; }
    closeList();
    html += `<p>${inline(line)}</p>`; i++;
  }
  closeList();
  return html;
}
