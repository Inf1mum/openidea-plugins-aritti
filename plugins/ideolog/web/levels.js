// Event counts of the log this editor group shows, with previous/next buttons that move the
// caret like Ideolog's navigation commands. The plugin's code detects the format and counts.
const frame = Math.random().toString(36).slice(2);
const editor = window.openidea.editor;
const format = document.getElementById("format");
const table = document.getElementById("levels");
const empty = document.getElementById("empty");
const capped = document.getElementById("capped");
let target = null;
let reply = null;
let asked = -1;
let timer = 0;

function ask(next) {
  if (!next?.path) {
    target = reply = null;
    return show();
  }
  if (next.path !== target?.path) reply = null;
  else if (next.version === asked) return;
  target = next;
  asked = next.version;
  window.openidea.postMessage(JSON.stringify({ op: "summary", id: frame, path: next.path }));
  show();
}

function show() {
  const message = !target ? "This editor shows no file." : !reply ? "Counting log events…" : reply.message;
  empty.hidden = !message;
  empty.textContent = message ?? "";
  table.hidden = capped.hidden = !!message;
  format.textContent = message ? "" : reply.format ? `Format: ${reply.format}` : "Format: plain text (no known format matches)";
  if (message) return;
  for (const row of table.rows) {
    const count = reply[row.dataset.level];
    row.querySelector("[data-count]").textContent = reply.firstLines && count ? `${count}+` : count;
    row.toggleAttribute("data-empty", !count);
  }
  capped.hidden = !reply.firstLines;
  capped.textContent = reply.firstLines ? `Counted in the first ${reply.firstLines.toLocaleString()} lines; the buttons search the whole log.` : "";
}

table.addEventListener("click", (e) => {
  const button = e.target.closest("button");
  if (!button || !target) return;
  const level = button.closest("tr").dataset.level;
  window.openidea.postMessage(JSON.stringify({ op: "go", path: target.path, level, forward: button.dataset.forward === "true" }));
});
document.getElementById("patterns").addEventListener("click", () => window.openidea.postMessage(JSON.stringify({ op: "patterns" })));

window.openidea.onMessage((text) => {
  const message = JSON.parse(text);
  // Replies go to the panel of every editor group.
  if (message.id !== frame || message.path !== target?.path) return;
  reply = message;
  show();
});

editor.get().then(ask);
editor.onTarget(ask);
// A growing or edited log is recounted after a pause, and only when its version moved:
// highlight changes come without the text, which `document.onChange` would copy every time.
editor.onHighlights(() => {
  clearTimeout(timer);
  timer = setTimeout(() => editor.get().then(ask), 800);
});
