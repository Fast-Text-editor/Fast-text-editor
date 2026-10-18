# TextWrite

**TextWrite** is the scripting language of [Fast Text](../README.md). It is a
small, complete, general-purpose language with a hand-written lexer, parser,
and tree-walking interpreter — **no third-party dependencies** — so it builds
anywhere Rust does, including WebAssembly.

It reads like a blend of Rust and English: Rust-style braces and expressions,
but word operators and phrasing (`is`, `is not`, `and`, `or`, `not`,
`repeat 3 times`, `for x in ...`).

This crate provides:

- the `textwrite` library (embed the language anywhere);
- the **`textwrite`** binary and the short alias **`tw`**.

## Quick start

```sh
# Run a script.
cargo run -p textwrite -- examples/hello.tw

# Start the REPL (no arguments).
cargo run -p textwrite

# Evaluate a snippet and print the result.
cargo run -p textwrite -- eval 'join(map([1,2,3], fn(x) { return x * x }), ", ")'
```

Install the binaries with:

```sh
cargo install --path crates/textwrite   # installs `textwrite` and `tw`
tw                                       # REPL
tw hello.tw                              # run a file
tw eval '1 + 2 * 3'
```

As a library:

```rust,no_run
use textwrite::Interpreter;

let mut tw = Interpreter::new();
tw.run("
fn greet(name) { print \"Hello, {name}!\" }
greet(\"world\")
").unwrap();

let value = tw.eval_expression("1 + 2 * 3").unwrap();
assert_eq!(value.to_display(), "7");
```

## A taste

```textwrite
# Comments start with `#`, `//`, or a /* block */.
let name = "world"

fn greet(who) {
    print "Hello, {who}!"
}

greet(name)

let numbers = range(1, 11)
let evens = filter(numbers, fn(n) { return n % 2 is 0 })
let total = reduce(evens, fn(sum, n) { return sum + n }, 0)
print "evens: {evens}  total: {total}"

for word in split("the quick fox", " ") {
    if len(word) > 3 {
        print upper(word)
    }
}

repeat 3 times {
    print "again"
}
```

## Examples

The `examples/` directory is a set of runnable programs — one per area of the
language — that double as documentation. Every file is executed by
`tests/language.rs`, so they cannot drift out of date (files that need the
outside world carry a `# requires:` marker and are parsed instead of run).

| File | What it teaches |
| --- | --- |
| `hello.tw` | The classic first program; strings and `{interpolation}`. |
| `basics.tw` | Comments, every literal kind, `let`/assignment/`set`, and printing. |
| `operators.tw` | Arithmetic and precedence, `**`, comparisons, and `and`/`or` short-circuiting. |
| `strings.tw` | Quoting, escapes, indexing, slicing, and every string builtin. |
| `lists.tw` | Building, changing, searching, sorting, and folding lists. |
| `maps.tw` | Insertion-ordered maps: access, mutation, iteration, and counting. |
| `control_flow.tw` | `if`/`else if`/`else`, `while`, `for`, `repeat N times`, `break`, `continue`. |
| `functions.tw` | Declarations, recursion, mutual recursion, closures, and higher-order functions. |
| `math_and_types.tw` | Integers vs floats, the math builtins, conversions, and truthiness. |
| `errors.tw` | `assert`, `error`, `exit`, and the errors the runtime raises for you. |
| `modules.tw` | `import` in both forms — merge and namespace — using `units.tw`. |
| `units.tw` | The module `modules.tw` imports (not meant to run on its own). |
| `cookbook.tw` | Nine small complete programs: word counts, palindromes, a Caesar cipher, a prime sieve, matrix transpose, run-length encoding, and more. |
| `fibonacci.tw` | Recursion plus first-class functions and higher-order builtins. |
| `fizzbuzz.tw` | Loops and branching on the classic puzzle. |
| `words.tw` | Text processing with `split`/`join`/`map`/`filter`. |
| `io.tw` | Reading stdin and reading/writing files (native builds). |

Start with `tw examples/basics.tw`, then read them in the order above.

## Syntax reference

### Comments

```textwrite
# hash comment
// slash comment
/* block
   comment */
```

### Values and literals

| Kind | Examples |
| --- | --- |
| Int | `0`, `42`, `1_000`, `0xFF` |
| Float | `3.14`, `1e9`, `2.5e-3` |
| String | `"double"`, `'single'` |
| Bool | `true`, `false` |
| None | `none` |
| List | `[1, 2, 3]`, `[]` |
| Map | `{ name: "Ada", age: 36 }` |

Double-quoted strings support escapes (`\n`, `\t`, `\r`, `\"`, `\'`, `\\`,
`\{`, `\}`) and **interpolation** with `{expression}`; write `{{` / `}}` for
literal braces. Single-quoted strings are literal (escapes only, no
interpolation).

```textwrite
let who = "Ada"
print "Hi {who}, {1 + 2} times!"   # Hi Ada, 3 times!
```

### Variables and assignment

`let` declares a variable; `=` and the compound operators assign to an existing
one (`set x = ...` is an accepted synonym for assignment).

```textwrite
let x = 1
x = x + 1
x += 10          # x is now 12
let cache = {}   # empty map
```

Assigning to an undeclared name is an error — declare it with `let` first.

### Operators

Highest to lowest precedence:

1. postfix — call `f()`, index `a[i]`, field `m.k`, method `s.upper()`
2. `**` (right associative)
3. unary `-`, `not`
4. `* / %`
5. `+ -`
6. comparisons: `== != < <= > >=`, plus English `is`, `is not`
6. `and`
7. `or`

Notes:

- `+` adds numbers, concatenates strings (any operand), and concatenates lists.
  `"n = " + 4` is `"n = 4"`.
2. `/` always produces a float (`6 / 3` is `2`, printed without a decimal).
   `%` on two ints is integer modulo; division by zero is a runtime error.
3. `and` / `or` short-circuit and return the deciding operand, so
   `name or "anonymous"` works as a default.
6. Truthiness: `none` and `false` are false; `0`, `0.0`, `""`, `[]`, `{}` are
   falsy; everything else is truthy.

Equality (`is` / `==`) is structural for lists and maps.

### Strings

```textwrite
let s = "Hello, World"
print len(s)              # 13 (in characters)
print lower(s)            # hello, world
print s[0]                # H
print slice(s, 0, 5)      # Hello
print split(s, ", ")      # ["Hello", "World"]
print "a-b-c".split("-")  # methods work too
```

Indexing a string yields a one-character string; negative indices count from
the end.

### Lists

```textwrite
let xs = [3, 1, 2]
push(xs, 4)          # [3, 1, 2, 4]   (mutates)
xs[0] = 9            # [9, 1, 2, 4]
print xs[-1]         # 4              (last element)
print sort(xs)       # [1, 2, 4, 9]   (returns a new list)
print len(xs)        # 4
print slice(xs, 1, 3) # [1, 2]
```

### Maps

Maps preserve insertion order and keys are strings.

```textwrite
let person = { name: "Ada", age: 36 }
person.age = 37                   # dot access
person["city"] = "London"         # index access
print person.name                 # Ada
print has(person, "city")         # true
print keys(person)                # ["name", "age", "city"]
print get(person, "zip", "n/a")   # n/a  (default when missing)
```

Reading a missing key via `.` or `[]` is an error; use `get` for a default.

### Control flow

```textwrite
if score >= 90 {
    print "A"
} else if score >= 80 {
    print "B"
} else {
    print "keep trying"
}

while n < 10 {
    n += 1
}

for item in [1, 2, 3] { print item }
for i in range(0, 10, 2) { print i }
for ch in "abc" { print ch }
for key in some_map { print key }    # iterating a map yields its keys

repeat 3 times { print "hi" }

for n in range(10) {
    if n is 5 { break }
    if n % 2 is 0 { continue }
    print n
}
```

### Functions and closures

Functions are values with lexical scoping; anonymous functions and closures
capture their environment.

```textwrite
fn add(a, b) {
    return a + b
}

fn make_counter() {
    let count = 0
    return fn() {
        count += 1
        return count
    }
}

let next = make_counter()
print next()   # 1
print next()   # 2

# Higher-order builtins take functions:
print map([1, 2, 3], fn(x) { return x * 10 })   # [10, 20, 30]
```

Returning without a value yields `none`. Recursion is supported (with a
call-depth guard against runaway recursion).

### Modules and imports

Share code across files and pull in packages with `import`:

```textwrite
import "mathx"                          # merge exports into this scope
import "fast-text/automator" as auto    # ...or keep them namespaced

print square(6)        # defined in mathx.tw
auto.cursor("block")   # from a built-in package
```

A module is any `.tw` file — resolved relative to the importing file (or the
directory set with `Interpreter::set_module_base`) — or a built-in package
registered with `Interpreter::register_module`. A module's **exports are its
top-level `fn`s and `let`s**; a module runs once and is cached, and import cycles
are reported as errors.

`Interpreter::compile(source)` parses a program and resolves every `import`
(transitively) without running anything — the "compile" half of Fast Text's
compile-then-play automations.

### Command-call syntax

A statement beginning with a bare name may pass arguments without parentheses —
handy for `print`:

```textwrite
print "hello"
print 1, 2, 3
greet "world"
```

`print("hello")` and `greet("world")` are equivalent.

### Truthiness and `and` / `or`

```textwrite
let port = get(config, "port") or 8080
```

## Standard library

| Group | Functions |
| --- | --- |
| Console / IO | `print`, `write`, `input` |
| Types | `len`, `type`, `str`, `int`, `float`, `bool` |
| Math | `abs`, `min`, `max`, `floor`, `ceil`, `round`, `sqrt`, `pow` |
| Sequences | `range`, `push`, `pop`, `insert`, `remove`, `first`, `last`, `slice`, `reverse`, `sort`, `sort_by`, `join`, `split`, `contains`, `index_of`, `keys`, `values`, `has`, `get` |
| Strings | `upper`, `lower`, `trim`, `replace`, `starts_with`, `ends_with`, `chars`, `lines`, `words` |
| Higher-order | `map`, `filter`, `reduce`, `each`, `any`, `all` |
| Errors | `assert`, `error`, `exit` |
| Files (native) | `read_file`, `write_file`, `append_file`, `exists` |

Most of these also work as methods, with the receiver as the first argument:
`"hi".upper()`, `[3, 1, 2].sort()`, `list.push(4)`, `"a,b".split(",")`.

`min`/`max` accept either a list or two-or-more arguments. `sort_by` takes a
comparator function returning a number (`> 0` means "swap").

## Running and the REPL

```sh
tw                     # interactive REPL (persistent environment)
tw script.tw           # run a file
tw run script.tw       # same
tw eval '1 + 2 * 3'    # evaluate and print a value
tw --help              # usage
tw --version           # version
```

In the REPL, declarations such as `fn f(x) { ... }` may span multiple lines; the
prompt changes from `tw> ` to `...> ` until the input is complete. Type `help`
for commands and `quit` (or Ctrl+D) to exit.

Errors are reported with a line, column, and a caret pointing at the failing
span:

```
script.tw: runtime error (line 3, column 12): division by zero
   3 | let ratio = total / count
     |                 ^^^^^
```

## Embedding and host functions

TextWrite is designed to be embedded. A host program implements the small
[`Host`](src/host.rs) trait to expose native functions, then installs it:

```rust
use std::cell::RefCell;
use std::rc::Rc;
use textwrite::{Host, Interpreter, Value};

struct MyHost;
impl Host for MyHost {
    fn functions(&self) -> &'static [&'static str] { &["reward"] }
    fn call(&mut self, name: &str, args: Vec<Value>, _span: textwrite::Span)
        -> textwrite::Result<Value>
    {
        assert_eq!(name, "reward");
        Ok(Value::None) // ...do something with `args`...
    }
}

let mut tw = Interpreter::new();
tw.set_host(Rc::new(RefCell::new(MyHost)));
tw.set_global("player", Value::str("Ada"));
tw.run("reward(player)").unwrap();
```

Host functions are called exactly like builtins (`reward(player)` or
`player.reward()`), shadow a builtin of the same name, and take precedence over
user functions. `set_global` injects values the script can read. This is how
Fast Text exposes its editor API to `.tw` scripts — see the
`fast-text-textwrite` crate.

## Project layout

```
crates/textwrite/
├── src/
│   ├── lexer.rs     # source text -> tokens
│   ├── ast.rs       # syntax tree types
│   ├── parser.rs    # tokens -> AST (recursive descent)
│   ├── value.rs     # runtime values (int, float, string, list, map, fn)
│   ├── interp.rs    # tree-walking interpreter + environments
│   ├── stdlib.rs    # built-in functions
│   ├── host.rs      # embedding hook (native functions)
│   ├── cli.rs       # file runner + REPL
│   ├── error.rs     # spans and diagnostics
│   └── lib.rs
├── examples/        # runnable .tw programs
└── tests/           # end-to-end language tests
```

There are **no dependencies** — the entire language is hand-written Rust.
