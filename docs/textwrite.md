# TextWrite

**TextWrite** is Fast Text's scripting and automation language: a small, complete,
dependency-free interpreter written in Rust (crate `textwrite`, binaries `textwrite` and `tw`).

It reads like Rust-style braces with word operators (`is`, `is not`, `and`, `or`,
`not`, `repeat N times`, `for x in ...`).

## Quick example

```textwrite
let name = "world"

fn greet(who) {
    print "Hello, {who}!"
}

greet(name)

for n in range(1, 6) {
    if n % 2 is 0 {
        print "{n} is even"
    }
}

repeat 3 times {
    print "again"
}
```

## Language features

- **Values:** ints, floats, strings with `{interpolation}`, booleans, `none`, ordered lists, insertion-ordered maps.
- **Functions & closures:** `fn name(a, b) { ... }` and anonymous `fn(x) { ... }`; closures capture their environment.
- **Control flow:** `if` / `else if` / `else`, `while`, `for x in ...`, `repeat N times`, `break`, `continue`, `return`.
- **Modules:** `import "path"` merges exports into scope; `import "path" as name` namespaces them. Built-in packages like `fast-text/automator` and `.tw` files both work.
- **Operators:** `+ - * / % **`, comparisons, `+= -= *= /= %=`, `is` / `is not` / `and` / `or` / `not`, and command-call style (`print "hi"`).
- **Standard library:** `len`, `type`, `str`/`int`/`float`, `range`, list/map helpers (`push`, `pop`, `sort`, `keys`, `values`, `get`, `has`), string helpers (`split`, `join`, `upper`, `trim`, `replace`, ...), math (`abs`, `floor`, `sqrt`, `min`, `max`, ...), higher-order (`map`, `filter`, `reduce`, `each`, `any`, `all`), `assert`/`error`, and native file I/O. Many are also methods: `"hi".upper()`, `[3, 1, 2].sort()`.

## CLI

```sh
cargo install --path crates/textwrite   # installs `textwrite` and `tw`
tw                                      # REPL
tw script.tw                            # run a script
tw eval '1 + 2 * 3'                     # evaluate expression
```

## Editor integration

Inside Fast Text, TextWrite is wired through `fast-text-textwrite`, which implements the TextWrite [`Host`] hook and turns script calls into `EditorAction`s.

- `.tw` files in `plugins/` load at startup and appear in `Ctrl+Shift+C`.
- `Ctrl+E` evaluates a TextWrite snippet against the current buffer.
- `Alt+R` runs a `.tw` automation file against the live editor.

For the editor bridge (context globals, host functions, hooks, status line, errors), see [scripting.md](scripting.md).

## Automations

An automation is a `.tw` file run against the live editor with **`Alt+R`**. Steps run in order; `wait(ms)` pauses between steps.

```textwrite
import "fast-text/automator"

cursor_demo(250)
cursor("bar")
```

| Helper | Effect |
| --- | --- |
| `cursor(style)` | Set cursor shape (`bar`, `block`, `underline`) |
| `type_text(text)` | Insert text at cursor |
| `pause(ms)` / `wait(ms)` | Wait before next step |
| `move(dir, extend)` | Move caret (`word_left`, `line_end`, ...) |
| `animate_cursor(styles, ms)` | Cycle cursor styles |
| `sequence(steps, ms)` | Run zero-arg functions with pauses |
| `cursor_demo(ms)` | Bar → block → underline demo |

Runnable demos: `automations/cursor-demo.tw`, `automations/type-demo.tw`.

## Imports

```textwrite
import "fast-text/automator"           # merge exports into scope
import "fast-text/automator" as auto   # namespace

auto.cursor("block")
```

A module is a `.tw` file (resolved relative to the importing file or `plugins/`) or a built-in package. Top-level `fn`s and `let`s are exports. Modules run once; circular imports are errors.

## More

- `crates/textwrite/README.md` - full language reference
- `crates/textwrite/examples/` - runnable examples
- [scripting.md](scripting.md) - editor scripting bridge

[`Host`]: ../crates/textwrite/src/host.rs
