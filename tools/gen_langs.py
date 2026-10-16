#!/usr/bin/env python3
"""Generate Fast Text's language registry from Helix's configuration.

This reads the Helix source tree (``languages.toml`` plus the tree-sitter
highlight queries under ``runtime/queries``) and emits
``crates/fast-text-syntax/src/generated.rs``: a data-driven set of language
definitions for everything Fast Text does not already hand-curate.

We cannot reuse Helix's tree-sitter grammars (Fast Text's highlighter is a
small, dependency-free, WASM-friendly lexer), but Helix's queries are still an
excellent source of *keyword/type/constant* word lists: highlight captures such
as ``[ "if" "else" ] @keyword.control`` enumerate exactly the reserved words.

Usage:
    python3 tools/gen_langs.py [path/to/helix] [-o out.rs]
"""

from __future__ import annotations

import argparse
import os
import re
import sys
import tomllib

# Languages already hand-curated in language.rs (by Helix name). These keep
# their richer, hand-written definitions and are not regenerated.
CURATED = {
    "plaintext",
    "rust",
    "javascript",
    "typescript",
    "python",
    "ruby",
    "java",
    "c-sharp",
    "json",
    "toml",
    "markdown",
    "bash",
    "c",
    "cpp",
    "go",
    "html",
    "css",
}

DISPLAY = {
    "c-sharp": "C#",
    "cpp": "C++",
    "c": "C",
    "javascript": "JavaScript",
    "typescript": "TypeScript",
    "html": "HTML",
    "css": "CSS",
    "json": "JSON",
    "toml": "TOML",
    "yaml": "YAML",
    "xml": "XML",
    "sql": "SQL",
    "ini": "INI",
    "php": "PHP",
    "bash": "Shell",
    "fsharp": "F#",
    "objective-c": "Objective-C",
    "graphql": "GraphQL",
    "hcl": "HCL",
    "http": "HTTP",
    "nix": "Nix",
    "solidity": "Solidity",
    "git-commit": "Git Commit",
    "git-rebase": "Git Rebase",
    "git-config": "Git Config",
}

# Capture prefixes (tree-sitter) mapped to Fast Text's coarse token buckets.
KEYWORD_PREFIXES = {
    "keyword",
    "storage",
    "conditional",
    "repeat",
    "exception",
    "include",
    "label",
    "namespace",
    "module",
    "operator",
    "return",
}
TYPE_PREFIXES = {"type"}
CONSTANT_PREFIXES = {"constant", "boolean"}

CAP_RE = re.compile(r"@([A-Za-z_][A-Za-z0-9_.]*)")
STR_RE = re.compile(r'"((?:[^"\\]|\\.)*)"')
WORD_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def rust_str(s: str) -> str:
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def rust_slice(items) -> str:
    if not items:
        return "&[]"
    return "&[" + ", ".join(rust_str(i) for i in items) + "]"


def rust_ident(name: str) -> str:
    out = []
    for ch in name:
        if ch.isalnum():
            out.append(ch.upper())
        elif ch in "+#":
            out.append("PLUS" if ch == "+" else "SHARP")
        else:
            out.append("_")
    ident = "LANG_" + re.sub(r"_+", "_", "".join(out)).strip("_")
    return ident


def extract_words(text: str, prefixes: set[str]) -> set[str]:
    """Words captured under any of `prefixes` in a tree-sitter query."""
    out: set[str] = set()
    for m in CAP_RE.finditer(text):
        base = m.group(1).split(".")[0]
        if base not in prefixes:
            continue
        i = m.start() - 1
        while i >= 0 and text[i] in " \t\r\n":
            i -= 1
        if i < 0:
            continue
        if text[i] == "]":
            depth = 0
            j = i
            while j >= 0:
                if text[j] == "]":
                    depth += 1
                elif text[j] == "[":
                    depth -= 1
                    if depth == 0:
                        break
                j -= 1
            seg = text[j : i + 1]
            for mm in STR_RE.finditer(seg):
                s = mm.group(1)
                if WORD_RE.match(s):
                    out.add(s)
        elif text[i] == '"':
            j = i - 1
            while j >= 0:
                if text[j] == '"' and (j == 0 or text[j - 1] != "\\"):
                    break
                j -= 1
            s = text[j + 1 : i]
            if WORD_RE.match(s):
                out.add(s)
    return out


class QueryIndex:
    def __init__(self, runtime: str):
        self.dir = os.path.join(runtime, "queries")
        self._cache: dict[str, set[str]] = {}

    def has(self, name: str) -> bool:
        return os.path.isfile(os.path.join(self.dir, name, "highlights.scm"))

    def words(self, name: str, prefixes: set[str], seen=None) -> set[str]:
        key = f"{name}|{','.join(sorted(prefixes))}"
        if key in self._cache:
            return self._cache[key]
        seen = seen or set()
        if name in seen or not self.has(name):
            return set()
        seen.add(name)
        text = open(os.path.join(self.dir, name, "highlights.scm"), encoding="utf-8").read()
        out = extract_words(text, prefixes)
        for line in text.splitlines():
            line = line.strip()
            if line.startswith(";") and "inherits:" in line:
                for parent in line.split("inherits:", 1)[1].split(","):
                    parent = parent.strip()
                    if parent:
                        out |= self.words(parent, prefixes, seen)
        self._cache[key] = out
        return out


def query_dir(idx: QueryIndex, lang: dict) -> list[str]:
    """Candidate query directories for a language, most specific first."""
    name = lang["name"]
    grammar = lang.get("grammar")
    cands = []
    for c in (grammar, name, "_" + grammar if grammar else None, "_" + name):
        if c and c not in cands:
            cands.append(c)
    return cands


def is_simple_ext(t) -> bool:
    return isinstance(t, str) and t and not any(c in t for c in "/.*?")


def display_name(helix_name: str) -> str:
    if helix_name in DISPLAY:
        return DISPLAY[helix_name]
    parts = re.split(r"[-_]", helix_name)
    return " ".join(p[:1].upper() + p[1:] for p in parts if p)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("helix", nargs="?", default="/home/kali/helix")
    ap.add_argument("-o", "--out", default=None)
    args = ap.parse_args()

    helix = os.path.abspath(args.helix)
    runtime = os.path.join(helix, "runtime")
    with open(os.path.join(helix, "languages.toml"), "rb") as fh:
        config = tomllib.load(fh)

    servers = {}
    for name, cfg in config.get("language-server", {}).items():
        cmd = cfg.get("command")
        if not cmd:
            continue
        sargs = [a for a in cfg.get("args", []) if isinstance(a, str)]
        servers[name] = (cmd, sargs)

    idx = QueryIndex(runtime)

    # Merge duplicate language names.
    merged: dict[str, dict] = {}
    for lang in config["language"]:
        merged.setdefault(lang["name"], {"entries": []})["entries"].append(lang)

    # Emit curated languages? No — they live in language.rs. Extras only.
    extras = []
    for name in sorted(merged):
        if name in CURATED:
            continue
        entries = merged[name]["entries"]
        lang = entries[0]

        exts, globs, lservers = set(), set(), []
        comments, blocks = set(), []
        for e in entries:
            for t in e.get("file-types", []):
                if is_simple_ext(t):
                    exts.add(t)
                elif isinstance(t, dict) and "glob" in t:
                    g = t["glob"]
                    if "/" not in g:
                        globs.add(g)
            ct = e.get("comment-token")
            if isinstance(ct, str):
                comments.add(ct)
            elif isinstance(ct, list):
                comments.update(c for c in ct if isinstance(c, str))
            for c in e.get("comment-tokens", []):
                if isinstance(c, str):
                    comments.add(c)
            for b in e.get("block-comment-tokens", []):
                if "start" in b and "end" in b:
                    blocks.append((b["start"], b["end"]))
            for s in e.get("language-servers", []):
                sname = s if isinstance(s, str) else s.get("name")
                if sname and sname not in lservers:
                    lservers.append(sname)

        kw, ty, co = set(), set(), set()
        for cand in query_dir(idx, lang):
            if idx.has(cand):
                kw |= idx.words(cand, KEYWORD_PREFIXES)
                ty |= idx.words(cand, TYPE_PREFIXES)
                co |= idx.words(cand, CONSTANT_PREFIXES)
                break

        extras.append(
            {
                "ident": rust_ident(name),
                "name": display_name(name),
                "language_id": lang.get("language-id") or name,
                "ext": sorted(exts),
                "globs": sorted(globs),
                "kw": sorted(kw),
                "ty": sorted(ty),
                "co": sorted(co),
                "comments": sorted(comments),
                "block": blocks[0] if blocks else None,
                "servers": lservers,
            }
        )

    out = []
    w = out.append
    w("// @generated by tools/gen_langs.py from the Helix configuration.")
    w("// Do not edit by hand; run `python3 tools/gen_langs.py` to refresh.")
    w("//")
    w("// FAST TEXT GENERATED FILE")
    w("")
    w("use crate::language::Language;")
    w("")

    for x in extras:
        block = (
            "None"
            if x["block"] is None
            else f'Some(({rust_str(x["block"][0])}, {rust_str(x["block"][1])}))'
        )
        w(f"static {x['ident']}: Language = Language {{")
        w(f"    name: {rust_str(x['name'])},")
        w(f"    language_id: {rust_str(x['language_id'])},")
        w(f"    extensions: {rust_slice(x['ext'])},")
        w(f"    keywords: {rust_slice(x['kw'])},")
        w(f"    types: {rust_slice(x['ty'])},")
        w(f"    constants: {rust_slice(x['co'])},")
        w(f"    line_comment: {rust_slice(x['comments'])},")
        w(f"    block_comment: {block},")
        w('    string_delims: &["\\""],')
        w("    triple_strings: &[],")
        w("    attribute_prefix: None,")
        w("    string_escapes: true,")
        w(f"    lsp_servers: {rust_slice(x['servers'])},")
        w("};")
        w("")

    w("/// Language definitions imported from Helix, in priority order.")
    w("pub static LANGUAGES: &[&Language] = &[")
    for x in extras:
        w(f"    &{x['ident']},")
    w("];")
    w("")

    w("/// Language servers: (name, command, args).")
    w("pub static LANGUAGE_SERVERS: &[(&str, &str, &[&str])] = &[")
    for name in sorted(servers):
        cmd, sargs = servers[name]
        w(f"    ({rust_str(name)}, {rust_str(cmd)}, {rust_slice(sargs)}),")
    w("];")
    w("")

    ext_index = {}
    for x in extras:
        for e in x["ext"]:
            ext_index.setdefault(e.lower(), x["ident"])
    name_index = {}
    for x in extras:
        name_index.setdefault(x["name"].lower(), x["ident"])

    w("pub fn language_for_extension(ext: &str) -> Option<&'static Language> {")
    w("    let ext = ext.to_ascii_lowercase();")
    w("    match ext.as_str() {")
    for e in sorted(ext_index):
        w(f"        {rust_str(e)} => Some(&{ext_index[e]}),")
    w("        _ => None,")
    w("    }")
    w("}")
    w("")

    w("pub fn language_for_name(name: &str) -> Option<&'static Language> {")
    w("    let name = name.trim().to_ascii_lowercase();")
    w("    match name.as_str() {")
    for n in sorted(name_index):
        w(f"        {rust_str(n)} => Some(&{name_index[n]}),")
    w("        _ => None,")
    w("    }")
    w("}")
    w("")

    # Basename glob matching (no path separators). Patterns may contain `*`/`?`.
    w("pub fn language_for_glob(base: &str) -> Option<&'static Language> {")
    w("    let lower = base.to_ascii_lowercase();")
    w("    for (pattern, lang) in GLOBS {")
    w("        if glob_match(pattern, &lower) {")
    w("            return Some(lang);")
    w("        }")
    w("    }")
    w("    None")
    w("}")
    w("")
    w("static GLOBS: &[(&str, &Language)] = &[")
    for x in extras:
        for g in x["globs"]:
            w(f"    ({rust_str(g.lower())}, &{x['ident']}),")
    w("];")
    w("")
    w("/// Matches a basename against a simple glob (`*` and `?` wildcards).")
    w("fn glob_match(pattern: &str, name: &str) -> bool {")
    w("    fn rec(p: &[u8], n: &[u8]) -> bool {")
    w("        if p.is_empty() {")
    w("            return n.is_empty();")
    w("        }")
    w("        match p[0] {")
    w("            b'*' => rec(&p[1..], n) || (!n.is_empty() && rec(p, &n[1..])),")
    w("            b'?' => !n.is_empty() && rec(&p[1..], &n[1..]),")
    w("            c => !n.is_empty() && n[0] == c && rec(&p[1..], &n[1..]),")
    w("        }")
    w("    }")
    w("    rec(pattern.as_bytes(), name.as_bytes())")
    w("}")
    w("")

    w("pub fn language_server(name: &str) -> Option<(&'static str, &'static [&'static str])> {")
    w("    LANGUAGE_SERVERS")
    w("        .iter()")
    w("        .find(|(n, _, _)| *n == name)")
    w("        .map(|(_, cmd, args)| (*cmd, *args))")
    w("}")
    w("")

    dest = args.out or os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "crates",
        "fast-text-syntax",
        "src",
        "generated.rs",
    )
    with open(dest, "w", encoding="utf-8") as fh:
        fh.write("\n".join(out) + "\n")

    with_kw = sum(1 for x in extras if x["kw"])
    print(f"wrote {dest}")
    print(f"  languages: {len(extras)}  with keywords: {with_kw}  servers: {len(servers)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
