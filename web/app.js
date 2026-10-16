// Fast Text — web frontend.
//
// The heavy lifting (buffer, undo/redo, search, syntax highlighting) happens in
// `fast-text-core` / `fast-text-syntax`, compiled to WebAssembly and exposed as
// `WasmEditor`. This file is only glue: it captures input, forwards it to the
// engine, and paints the engine's output.

import init, { WasmEditor, languageNames, version } from "./pkg/fast_text_wasm.js";

// The language list (and its friendly labels) come straight from the Rust
// engine, so the web picker stays in sync with the TUI and the library.
let LANGUAGES = [];

const SAMPLE = `// Fast Text — live web demo
// Everything below is colorized and searched by the Rust engine,
// compiled to WebAssembly.

fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn main() {
    let message = "hello from fast-text-core";
    for i in 0..10 {
        println!("fib({i}) = {}", fibonacci(i));
    }
}
`;

let editor = null;

const $ = (id) => document.getElementById(id);

async function main() {
  await init();

  $("st-file").textContent = `fast-text v${version()}`;

  const select = $("language");
  LANGUAGES = JSON.parse(languageNames());
  for (const name of LANGUAGES) {
    const opt = document.createElement("option");
    opt.value = name;
    opt.textContent = name;
    select.appendChild(opt);
  }

  const input = $("input");
  input.value = SAMPLE;
  editor = new WasmEditor(input.value, "demo.rs");
  select.value = "Rust";

  wire(input, select);
  refresh();
}

function wire(input, select) {
  input.addEventListener("input", refresh);
  input.addEventListener("scroll", syncScroll);
  input.addEventListener("keyup", updatePosition);
  input.addEventListener("click", updatePosition);
  input.addEventListener("select", updatePosition);

  select.addEventListener("change", () => {
    editor.setLanguage(select.value);
    editor.setText(input.value);
    refresh();
    message(`Language: ${editor.languageName()}`);
  });

  $("btn-sample").addEventListener("click", () => {
    input.value = SAMPLE;
    select.value = "Rust";
    editor.setLanguage("rust");
    refresh();
  });

  $("btn-next").addEventListener("click", () => findNext(true));
  $("btn-prev").addEventListener("click", () => findNext(false));
  $("btn-replace-all").addEventListener("click", replaceAll);
  $("btn-undo").addEventListener("click", () => { editor.undo(); pushBack(input); });
  $("btn-redo").addEventListener("click", () => { editor.redo(); pushBack(input); });

  $("find").addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); findNext(!e.shiftKey); }
  });

  input.addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "f") {
      e.preventDefault();
      $("find").focus();
      $("find").select();
    } else if (e.key === "Tab") {
      e.preventDefault();
      const { selectionStart, selectionEnd, value } = input;
      input.value = value.slice(0, selectionStart) + "\t" + value.slice(selectionEnd);
      input.selectionStart = input.selectionEnd = selectionStart + 1;
      refresh();
    }
  });
}

/** Re-reads the textarea, re-renders the preview and status bar. */
function refresh() {
  const input = $("input");
  editor.setText(input.value);
  $("preview").innerHTML = editor.highlightHtml();
  syncScroll();
  updateStatus();
}

/** Writes engine text back into the textarea (used by undo/redo/replace). */
function pushBack(input) {
  input.value = editor.text();
  $("preview").innerHTML = editor.highlightHtml();
  updateStatus();
}

function syncScroll() {
  const input = $("input");
  const preview = $("preview");
  preview.scrollTop = input.scrollTop;
  preview.scrollLeft = input.scrollLeft;
}

function findNext(forward) {
  const query = $("find").value;
  if (!query) { message("Type something to find"); return; }
  const cs = $("case").checked;
  const found = forward ? editor.findNext(query, cs) : editor.findPrev(query, cs);
  if (found) {
    selectInTextarea(editor.cursor(), editor.cursor() + query.length);
    message(`Found "${query}"`);
  } else {
    message(`No match for "${query}"`);
  }
}

function replaceAll() {
  const query = $("find").value;
  const replacement = $("replace").value;
  if (!query) { message("Type something to replace"); return; }
  const n = editor.replaceAll(query, replacement, $("case").checked);
  pushBack($("input"));
  message(`Replaced ${n} occurrence${n === 1 ? "" : "s"}`);
}

function selectInTextarea(start, end) {
  const input = $("input");
  input.focus();
  input.setSelectionRange(start, end);
  // Scroll the match into view.
  const before = input.value.slice(0, start);
  const line = before.split("\n").length - 1;
  const lineHeight = 20; // approximate; close enough for scroll-into-view
  const target = line * lineHeight - input.clientHeight / 2;
  input.scrollTop = Math.max(0, target);
  syncScroll();
  updatePosition();
}

function updatePosition() {
  const input = $("input");
  const pos = input.selectionStart;
  const before = input.value.slice(0, pos);
  const line = before.split("\n").length;
  const col = pos - (before.lastIndexOf("\n") + 1) + 1;
  $("st-pos").textContent = `Ln ${line}, Col ${col}`;
}

function updateStatus() {
  $("st-lang").textContent = editor.languageName();
  $("st-stats").textContent =
    `${editor.lineCount()} lines · ${editor.charCount()} chars`;
  $("st-dirty").textContent = editor.isDirty() ? "modified" : "";
  updatePosition();
}

function message(text) {
  $("message").textContent = text;
  clearTimeout(message._t);
  message._t = setTimeout(() => { $("message").textContent = ""; }, 2500);
}

main().catch((err) => {
  document.body.innerHTML =
    `<pre style="padding:20px;color:#e06c75">Failed to load WebAssembly:\n${err}\n\n` +
    `Serve this directory over HTTP (e.g. \`python3 -m http.server\`) — ` +
    `\`file://\` blocks module/WASM loading.</pre>`;
});
