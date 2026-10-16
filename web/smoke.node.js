// Runtime smoke test for the Fast Text WebAssembly build.
// Run after: wasm-pack build crates/fast-text-wasm --target nodejs --out-dir ../../target/wasm-node
const assert = require("node:assert");
const path = require("node:path");

const pkg = path.join(__dirname, "..", "target", "wasm-node", "fast_text_wasm.js");
const { WasmEditor, detectLanguage, languageNames, version } = require(pkg);

console.log("fast-text wasm version:", version());

const ed = new WasmEditor("fn main() {}", "src/main.rs");
assert.strictEqual(ed.languageName(), "Rust", "language detection");

// Editing + history through the engine.
ed.setCursor(ed.charCount());
ed.insert("\n// done");
assert.ok(ed.text().includes("// done"));
ed.undo();
assert.ok(!ed.text().includes("// done"), "undo");

// Search + replace.
const s = new WasmEditor("one two one three one", null);
assert.strictEqual(s.replaceAll("one", "1", true), 3);
assert.strictEqual(s.text(), "1 two 1 three 1");
assert.ok(s.findNext("three", true));

// Highlighting emits token classes.
const h = new WasmEditor("let x = 42; // hi", "x.rs");
const html = h.highlightHtml();
assert.ok(html.includes("tok-keyword"), "keyword class");
assert.ok(html.includes("tok-number"), "number class");
assert.ok(html.includes("tok-comment"), "comment class");
assert.strictEqual(detectLanguage("a.py"), "Python");

// The Helix-derived language registry.
const names = JSON.parse(languageNames());
assert.ok(names.length > 300, "expected 300+ languages");
for (const n of ["Rust", "C#", "C++", "Zig", "Kotlin", "Haskell", "YAML", "SQL"]) {
  assert.ok(names.includes(n), `language registry missing ${n}`);
}

// Imported languages highlight through the same engine.
const zig = new WasmEditor("const x = 1;", "main.zig");
assert.strictEqual(zig.languageName(), "Zig");
assert.ok(zig.highlightHtml().includes("tok-keyword"), "zig keyword");
const hs = new WasmEditor("module Main where\n-- c\n", "App.hs");
assert.strictEqual(hs.languageName(), "Haskell");
const hsHtml = hs.highlightHtml();
assert.ok(hsHtml.includes("tok-keyword"), "haskell keyword");
assert.ok(hsHtml.includes("tok-comment"), "haskell comment");

console.log("OK: WebAssembly engine runs, edits, searches, and highlights");
