// Fast Text — web frontend.
//
// The heavy lifting (buffer, undo/redo, search, syntax highlighting, themes,
// the keymap) happens in `fast-text-core` / `fast-text-syntax` /
// `fast-text-theme` / `fast-text-plugin`, compiled to WebAssembly and exposed as
// `WasmEditor` plus a handful of free functions. This file is only glue: it
// captures input, forwards it to the engine, and paints the engine's output.
//
// Keyboard behaviour is *not* hard-coded here: every shortcut is looked up in
// the shared keymap exported from Rust, so Ctrl+S, Ctrl+P, Alt+T and friends do
// the same thing in the browser as in the terminal and desktop builds.

import init, {
  WasmEditor,
  commandForKey,
  keybindings,
  languageNames,
  themeCss,
  themeNames,
  version,
} from "./pkg/fast_text_wasm.js";

// The language list (and its friendly labels) come straight from the Rust
// engine, so the web picker stays in sync with the TUI and the library.
let LANGUAGES = [];
// `key -> {command, title}` from the shared keymap.
let KEYMAP = new Map();
let COMMANDS = [];
let THEMES = [];
let themeIndex = 0;

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

  // Shared keymap, straight from `fast-text-plugin`.
  for (const entry of JSON.parse(keybindings())) {
    KEYMAP.set(entry.key, entry);
    if (!COMMANDS.some((c) => c.command === entry.command)) {
      COMMANDS.push(entry);
    }
  }

  // Shared themes, straight from `fast-text-theme`.
  THEMES = JSON.parse(themeNames());
  const themeSelect = $("theme");
  for (const name of THEMES) {
    const opt = document.createElement("option");
    opt.value = name;
    opt.textContent = name;
    themeSelect.appendChild(opt);
  }
  themeSelect.value = "dark";
  themeSelect.addEventListener("change", () => applyTheme(themeSelect.value));
  applyTheme("dark");

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
    // Do not rebuild the document: that would discard the undo history.
    syncEngineFromInput();
    editor.setLanguage(select.value);
    $("#preview").innerHTML = editor.highlightHtml();
    updateStatus();
    message(`Language: ${editor.languageName()}`);
  });

  $("btn-sample").addEventListener("click", () => {
    input.value = SAMPLE;
    select.value = "Rust";
    editor.setLanguage("rust");
    editor.setText(SAMPLE);
    refresh();
  });

  $("btn-next").addEventListener("click", () => findNext(true));
  $("btn-prev").addEventListener("click", () => findNext(false));
  $("btn-replace-all").addEventListener("click", replaceAll);
  $("btn-undo").addEventListener("click", () => runCommand("undo"));
  $("btn-redo").addEventListener("click", () => runCommand("redo"));
  $("btn-palette").addEventListener("click", showPalette);

  // Buttons advertise the same shortcuts the editor uses.
  annotateShortcut("btn-undo", "undo");
  annotateShortcut("btn-redo", "redo");

  $("find").addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      findNext(!e.shiftKey);
    }
  });

  window.addEventListener("keydown", handleShortcut);
}

/** Adds the shared shortcut to a button's tooltip. */
function annotateShortcut(id, command) {
  const button = $(id);
  const binding = COMMANDS.find((entry) => entry.command === command);
  if (button && binding) {
    button.title = `${binding.title} (${binding.key})`;
  }
}

/** Resolves a KeyboardEvent to the canonical key spelling used by Rust. */
function canonicalKey(event) {
  const parts = [];
  if (event.ctrlKey || event.metaKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");

  const named = {
    Escape: "Esc",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    " ": "Space",
    Tab: "Tab",
    Enter: "Enter",
    Backspace: "Backspace",
    Delete: "Delete",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
  };
  let key = named[event.key] || event.key;
  if (key.length === 1) key = key.toUpperCase();
  parts.push(key);
  return parts.join("+");
}

function handleShortcut(event) {
  // Let the palette and find boxes handle their own keys.
  if (event.target && event.target.id === "palette-input") return;
  const key = canonicalKey(event);
  const binding = KEYMAP.get(key);
  if (!binding) return;
  // Every default binding includes Ctrl or Alt, so plain typing is never
  // swallowed. Still guard against an empty normalized key.
  if (!event.ctrlKey && !event.metaKey && !event.altKey) return;
  event.preventDefault();
  runCommand(binding.command, binding.title);
}

/** Runs a command id from the shared keymap. */
function runCommand(command, title = command) {
  const input = $("input");
  switch (command) {
    case "save":
    case "save_as":
      downloadBuffer();
      break;
    case "new":
    case "close":
      editor.setText("");
      input.value = "";
      refresh();
      message("New buffer");
      break;
    case "undo":
      editor.undo();
      pushBack(input);
      message("Undo");
      break;
    case "redo":
      editor.redo();
      pushBack(input);
      message("Redo");
      break;
    case "select_all":
      input.focus();
      input.select();
      break;
    case "find":
      $("find").focus();
      $("find").select();
      break;
    case "find_next":
      findNext(true);
      break;
    case "find_previous":
      findNext(false);
      break;
    case "toggle_theme":
      cycleTheme();
      break;
    case "toggle_comment":
      toggleComment();
      break;
    case "file_finder":
      message("Go to file needs a workspace — use the terminal or desktop app");
      break;
    case "command_palette":
      showPalette();
      break;
    default:
      message(`${title} is not available in the web demo`);
  }
}

/** Applies a shared theme's CSS custom properties. */
function applyTheme(name) {
  let style = $("theme-css");
  if (!style) {
    style = document.createElement("style");
    style.id = "theme-css";
    document.head.appendChild(style);
  }
  style.textContent = themeCss(name);
  $("#theme").value = name;
  message(`Theme: ${name}`);
}

function cycleTheme() {
  if (THEMES.length === 0) return;
  themeIndex = (themeIndex + 1) % THEMES.length;
  applyTheme(THEMES[themeIndex]);
}

/** Downloads the buffer, which is the web equivalent of saving a file. */
function downloadBuffer() {
  syncEngineFromInput();
  const blob = new Blob([editor.text()], { type: "text/plain;charset=utf-8" });
  const link = document.createElement("a");
  const base = ($("tab-name").textContent || "fast-text").trim() || "fast-text";
  link.href = URL.createObjectURL(blob);
  link.download = base === "untitled" ? "fast-text.txt" : `${base}.txt`;
  link.click();
  URL.revokeObjectURL(link.href);
  message("Downloaded");
}

/** Mirrors the textarea's edits into the engine as one undoable range edit. */
function syncEngineFromInput() {
  const input = $("input");
  const next = input.value;
  const current = editor.text();
  if (next === current) {
    syncSelection(input);
    return;
  }
  const before = Array.from(current);
  const after = Array.from(next);
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) {
    start += 1;
  }
  let oldEnd = before.length;
  let newEnd = after.length;
  while (oldEnd > start && newEnd > start && before[oldEnd - 1] === after[newEnd - 1]) {
    oldEnd -= 1;
    newEnd -= 1;
  }
  editor.replaceRange(start, oldEnd, after.slice(start, newEnd).join(""));
  syncSelection(input);
}

/** Copies the textarea selection into the engine (character offsets). */
function syncSelection(input) {
  const value = input.value;
  const anchor = Array.from(value.slice(0, input.selectionStart)).length;
  const head = Array.from(value.slice(0, input.selectionEnd)).length;
  editor.setSelection(anchor, head);
}

/** Re-reads the textarea, re-renders the preview and status bar. */
function refresh() {
  const input = $("input");
  syncEngineFromInput();
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
  syncEngineFromInput();
  const cs = $("case").checked;
  const found = forward ? editor.findNext(query, cs) : editor.findPrev(query, cs);
  if (found) {
    const start = editor.cursor();
    const end = start + Array.from(query).length;
    selectInTextarea(start, end);
    message(`Found "${query}"`);
  } else {
    message(`No match for "${query}"`);
  }
}

function replaceAll() {
  const query = $("find").value;
  const replacement = $("replace").value;
  if (!query) { message("Type something to replace"); return; }
  syncEngineFromInput();
  const n = editor.replaceAll(query, replacement, $("case").checked);
  pushBack($("input"));
  message(`Replaced ${n} occurrence${n === 1 ? "" : "s"}`);
}

/** Toggles `//` comments on the selected lines (the shared `Ctrl+/`). */
function toggleComment() {
  const input = $("input");
  const { selectionStart, selectionEnd, value } = input;
  const lineStart = value.lastIndexOf("\n", selectionStart - 1) + 1;
  let lineEnd = value.indexOf("\n", selectionEnd);
  if (lineEnd === -1) lineEnd = value.length;
  const lines = value.slice(lineStart, lineEnd).split("\n");
  const commented = lines.every(
    (line) => line.trim() === "" || line.trimStart().startsWith("//"),
  );
  const next = lines
    .map((line) => {
      if (line.trim() === "") return line;
      if (commented) {
        const index = line.indexOf("//");
        return line.slice(0, index) + line.slice(index + 2).replace(/^ /, "");
      }
      const indent = line.search(/\S/);
      return line.slice(0, indent) + "// " + line.slice(indent);
    })
    .join("\n");
  input.value = value.slice(0, lineStart) + next + value.slice(lineEnd);
  input.setSelectionRange(lineStart, lineStart + next.length);
  refresh();
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

/** A tiny command palette over the shared keymap (Ctrl+Shift+P). */
function showPalette() {
  const overlay = $("palette");
  const input = $("palette-input");
  const list = $("palette-list");
  overlay.classList.remove("hidden");
  input.value = "";
  input.focus();

  const render = () => {
    const query = input.value.trim().toLowerCase();
    const matches = COMMANDS.filter(
      (entry) =>
        !query ||
        entry.title.toLowerCase().includes(query) ||
        entry.key.toLowerCase().includes(query),
    );
    list.innerHTML = "";
    matches.forEach((entry) => {
      const item = document.createElement("li");
      item.innerHTML = `<span>${entry.title}</span><kbd>${entry.key}</kbd>`;
      item.addEventListener("click", () => runCommand(entry.command, entry.title));
      list.appendChild(item);
    });
    if (matches.length === 0) {
      const item = document.createElement("li");
      item.textContent = "No matching commands";
      list.appendChild(item);
    }
  };

  input.oninput = render;
  input.onkeydown = (event) => {
    if (event.key === "Escape") {
      overlay.classList.add("hidden");
      event.preventDefault();
    } else if (event.key === "Enter") {
      const first = list.querySelector("li");
      if (first) first.click();
      overlay.classList.add("hidden");
      event.preventDefault();
    }
  };
  render();
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
