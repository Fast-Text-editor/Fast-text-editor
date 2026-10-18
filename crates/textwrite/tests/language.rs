//! End-to-end tests for the TextWrite language.

use std::cell::RefCell;
use std::rc::Rc;

use textwrite::{ErrorKind, Interpreter, Value};

/// A `std::io::Write` that appends to a shared buffer.
struct Sink(Rc<RefCell<String>>);

impl std::io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().push_str(&String::from_utf8_lossy(buf));
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Returns an interpreter plus a handle to its captured output.
fn capture_output() -> (Interpreter, Rc<RefCell<String>>) {
    let sink = Rc::new(RefCell::new(String::new()));
    let out = Box::new(Sink(sink.clone()));
    (Interpreter::with_output(out), sink)
}

fn eval(source: &str) -> Value {
    let mut tw = Interpreter::new();
    tw.eval_expression(source)
        .unwrap_or_else(|e| panic!("eval `{source}` failed: {e}"))
}

fn run(source: &str) -> String {
    let (mut tw, sink) = capture_output();
    tw.run(source)
        .unwrap_or_else(|e| panic!("run failed: {e}\n---\n{source}"));
    let out = sink.borrow().clone();
    out
}

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(eval("1 + 2 * 3").to_display(), "7");
    assert_eq!(eval("(1 + 2) * 3").to_display(), "9");
    assert_eq!(eval("2 ** 3 ** 2").to_display(), "512");
    assert_eq!(eval("-2 ** 2").to_display(), "-4");
    assert_eq!(eval("7 % 3").to_display(), "1");
    assert_eq!(eval("10 / 4").to_display(), "2.5");
    assert_eq!(eval("6 / 3").to_display(), "2");
}

#[test]
fn english_operators() {
    assert_eq!(eval("3 is 3").to_display(), "true");
    assert_eq!(eval("3 is not 4").to_display(), "true");
    assert_eq!(eval("true and false").to_display(), "false");
    assert_eq!(eval("true or false").to_display(), "true");
    assert_eq!(eval("not false").to_display(), "true");
}

#[test]
fn strings_and_interpolation() {
    assert_eq!(eval("\"ab\" + \"cd\"").to_display(), "abcd");
    assert_eq!(eval("\"n=\" + 4").to_display(), "n=4");

    let mut tw = Interpreter::new();
    tw.run("let name = \"Ada\"\nlet msg = \"Hi, {name}!\"\n")
        .unwrap();
    assert_eq!(tw.eval_expression("msg").unwrap().to_display(), "Hi, Ada!");

    // Escaped braces stay literal.
    assert_eq!(eval("\"{{x}}\"").to_display(), "{x}");
}

#[test]
fn variables_and_assignment() {
    let mut tw = Interpreter::new();
    tw.run("let x = 1\nx = x + 41\n").unwrap();
    assert_eq!(tw.eval_expression("x").unwrap().to_display(), "42");
}

#[test]
fn functions_and_recursion() {
    let source = "\
fn fact(n) {
    if n <= 1 { return 1 }
    return n * fact(n - 1)
}
";
    let mut tw = Interpreter::new();
    tw.run(source).unwrap();
    assert_eq!(tw.eval_expression("fact(5)").unwrap().to_display(), "120");
}

#[test]
fn closures_capture_environment() {
    let source = "\
fn make_counter() {
    let count = 0
    return fn() {
        count = count + 1
        return count
    }
}
let c = make_counter()
";
    let mut tw = Interpreter::new();
    tw.run(source).unwrap();
    assert_eq!(tw.eval_expression("c()").unwrap().to_display(), "1");
    assert_eq!(tw.eval_expression("c()").unwrap().to_display(), "2");
    assert_eq!(tw.eval_expression("c()").unwrap().to_display(), "3");
}

#[test]
fn control_flow() {
    let out = run("\
let total = 0
for n in range(1, 5) {
    if n is 3 { continue }
    total += n
}
print total
");
    assert_eq!(out, "7\n");

    let out = run("\
let i = 0
while i < 3 {
    print i
    i += 1
}
");
    assert_eq!(out, "0\n1\n2\n");

    let out = run("repeat 2 times { print \"x\" }\n");
    assert_eq!(out, "x\nx\n");
}

#[test]
fn lists_and_maps() {
    let mut tw = Interpreter::new();
    tw.run(
        "\
let nums = [3, 1, 2]
nums[0] = 9
let owner = { name: \"Ada\", age: 36 }
owner[\"age\"] = 37
",
    )
    .unwrap();
    assert_eq!(
        tw.eval_expression("nums").unwrap().to_display(),
        "[9, 1, 2]"
    );
    assert_eq!(tw.eval_expression("nums[1]").unwrap().to_display(), "1");
    assert_eq!(tw.eval_expression("nums[-1]").unwrap().to_display(), "2");
    assert_eq!(
        tw.eval_expression("owner.name").unwrap().to_display(),
        "Ada"
    );
    assert_eq!(
        tw.eval_expression("owner[\"age\"]").unwrap().to_display(),
        "37"
    );
}

#[test]
fn builtins() {
    assert_eq!(eval("len([1, 2, 3])").to_display(), "3");
    assert_eq!(eval("upper(\"abc\")").to_display(), "ABC");
    assert_eq!(eval("join([1, 2, 3], \"-\")").to_display(), "1-2-3");
    assert_eq!(
        eval("split(\"a,b,c\", \",\")").to_display(),
        "[\"a\", \"b\", \"c\"]"
    );
    assert_eq!(eval("sort([3, 1, 2])").to_display(), "[1, 2, 3]");
    assert_eq!(eval("contains([1, 2, 3], 2)").to_display(), "true");
    assert_eq!(eval("starts_with(\"hello\", \"he\")").to_display(), "true");
    assert_eq!(eval("slice([1, 2, 3, 4], 1, 3)").to_display(), "[2, 3]");
    assert_eq!(eval("int(\"42\") + 1").to_display(), "43");
}

#[test]
fn functional_builtins() {
    assert_eq!(
        eval("map([1, 2, 3], fn(x) { return x * 2 })").to_display(),
        "[2, 4, 6]"
    );
    assert_eq!(
        eval("filter([1, 2, 3, 4], fn(x) { return x % 2 is 0 })").to_display(),
        "[2, 4]"
    );
    assert_eq!(
        eval("reduce([1, 2, 3], fn(a, b) { return a + b }, 0)").to_display(),
        "6"
    );
    assert_eq!(eval("min([5, 2, 9])").to_display(), "2");
    assert_eq!(eval("max(3, 7)").to_display(), "7");
}

#[test]
fn methods_read_like_english() {
    assert_eq!(eval("\"hello\".upper()").to_display(), "HELLO");
    assert_eq!(eval("[3, 1, 2].sort()").to_display(), "[1, 2, 3]");
    assert_eq!(eval("[1, 2].len()").to_display(), "2");
}

#[test]
fn print_goes_to_output() {
    let out = run("print \"hello\"\nprint 1, 2, 3\n");
    assert_eq!(out, "hello\n1 2 3\n");
}

#[test]
fn errors_are_reported_with_positions() {
    let err = Interpreter::new().eval_expression("1 - \"a\"").unwrap_err();
    assert_eq!(err.kind, ErrorKind::Runtime);
    assert!(err.span.is_some());

    let err = Interpreter::new()
        .eval_expression("undefined_name")
        .unwrap_err();
    assert!(err.to_string().contains("undefined variable"));

    let err = Interpreter::new().eval_expression("1 / 0").unwrap_err();
    assert!(err.to_string().contains("division by zero"));
}

#[test]
fn parse_errors_are_incomplete_at_end_of_input() {
    let err = textwrite::parser::parse_expression("1 +").unwrap_err();
    assert!(
        err.is_incomplete(),
        "trailing operator should be incomplete"
    );

    let err = textwrite::parser::parse("fn f() {").unwrap_err();
    assert!(err.is_incomplete(), "unclosed block should be incomplete");
}

#[test]
fn host_functions_are_callable_and_shadow_builtins() {
    struct Host {
        log: Rc<RefCell<Vec<String>>>,
    }
    impl textwrite::Host for Host {
        fn functions(&self) -> &'static [&'static str] {
            &["emit", "print"]
        }
        fn call(
            &mut self,
            name: &str,
            args: Vec<Value>,
            _span: textwrite::Span,
        ) -> textwrite::Result<Value> {
            let text = args
                .iter()
                .map(Value::to_display)
                .collect::<Vec<_>>()
                .join(" ");
            self.log.borrow_mut().push(format!("{name}:{text}"));
            Ok(Value::None)
        }
    }

    let log = Rc::new(RefCell::new(Vec::new()));
    let mut tw = Interpreter::new();
    tw.set_host(Rc::new(RefCell::new(Host { log: log.clone() })));
    tw.set_global("who", Value::str("world"));

    tw.run("print \"hi\", who\nemit(\"a\", 1 + 1)").unwrap();
    assert_eq!(
        log.borrow().as_slice(),
        ["print:hi world".to_string(), "emit:a 2".to_string()]
    );
    // A host function can also be called in method position.
    tw.run("\"x\".emit(\"y\")").unwrap();
    assert_eq!(log.borrow().last().unwrap(), "emit:x y");
}

#[test]
fn examples_run() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");
    for entry in std::fs::read_dir(dir).expect("examples directory") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("tw") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let mut tw = Interpreter::new();
        // Relative `import`s resolve against the examples directory.
        tw.set_module_base(dir);
        // A few examples need the outside world (stdin, the filesystem). They
        // carry a `# requires:` marker and are parsed — with their imports
        // resolved — but not executed.
        if source
            .lines()
            .take(3)
            .any(|line| line.starts_with("# requires: "))
        {
            tw.compile(&source)
                .unwrap_or_else(|e| panic!("{} failed to compile: {e}", path.display()));
            continue;
        }
        tw.run(&source)
            .unwrap_or_else(|e| panic!("{} failed: {e}", path.display()));
    }
}

#[test]
fn builtin_modules_merge_and_namespace() {
    // Flat import: exports land in the importing scope.
    let mut tw = Interpreter::new();
    tw.register_module(
        "demo/util",
        "fn double(n) { return n * 2 }\nlet answer = 42\n",
    );
    tw.run("import \"demo/util\"\n").unwrap();
    assert_eq!(tw.eval_expression("double(21)").unwrap().to_display(), "42");
    assert_eq!(tw.eval_expression("answer").unwrap().to_display(), "42");

    // Namespaced import: exports live under the alias.
    let mut tw = Interpreter::new();
    tw.register_module("demo/util", "fn double(n) { return n * 2 }");
    tw.run("import \"demo/util\" as util\n").unwrap();
    assert!(tw.get_global("double").is_none());
    assert_eq!(
        tw.eval_expression("util.double(4)").unwrap().to_display(),
        "8"
    );
}

#[test]
fn module_functions_capture_their_module_scope() {
    let mut tw = Interpreter::new();
    tw.register_module(
        "demo/counter",
        "let start = 10\nfn bump(n) { return start + n }\n",
    );
    tw.run("import \"demo/counter\"\n").unwrap();
    assert_eq!(tw.eval_expression("bump(5)").unwrap().to_display(), "15");
}

#[test]
fn imports_modules_from_files() {
    let dir = std::env::temp_dir().join(format!("tw-mod-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mathx.tw"), "fn square(n) { return n * n }\n").unwrap();

    let mut tw = Interpreter::new();
    tw.set_module_base(&dir);
    // Both spellings resolve to the same file and share the cache.
    tw.run("import \"mathx\"\nimport \"./mathx.tw\"\n").unwrap();
    assert_eq!(tw.eval_expression("square(6)").unwrap().to_display(), "36");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn circular_and_missing_imports_are_errors() {
    let dir = std::env::temp_dir().join(format!("tw-cycle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.tw"), "import \"b\"\nfn a() { return 1 }\n").unwrap();
    std::fs::write(dir.join("b.tw"), "import \"a\"\nfn b() { return 2 }\n").unwrap();

    let mut tw = Interpreter::new();
    tw.set_module_base(&dir);
    let err = tw.run("import \"a\"\n").unwrap_err();
    assert!(
        format!("{err}").contains("circular"),
        "expected a circular import error, got: {err}"
    );

    let mut tw = Interpreter::new();
    let err = tw.run("import \"definitely/missing\"\n").unwrap_err();
    assert!(format!("{err}").contains("cannot find module"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_spec_must_be_a_string() {
    let mut tw = Interpreter::new();
    let err = tw.run("import 42\n").unwrap_err();
    assert!(matches!(err.kind, ErrorKind::Parse), "got {err}");
}

#[test]
fn compile_checks_imports_without_running() {
    let mut tw = Interpreter::new();
    tw.register_module("side/mod", "let touched = 1\nfn ok() { return 1 }\n");
    tw.compile("import \"side/mod\"\n").unwrap();
    assert!(
        tw.get_global("touched").is_none(),
        "compile must not execute the module"
    );

    let mut tw = Interpreter::new();
    let err = tw.compile("import \"missing/mod\"\n").unwrap_err();
    assert!(format!("{err}").contains("cannot find module"));

    // Syntax errors surface from compile too.
    let mut tw = Interpreter::new();
    assert!(tw.compile("fn (").is_err());
}

#[test]
fn word_and_line_builtins_split_text() {
    // `words` splits on any run of whitespace and drops the empties.
    assert_eq!(eval("len(words(\"  the quick   fox \"))").to_display(), "3");
    assert_eq!(
        eval("join(words(\"a\\tb\\nc\"), \"-\")").to_display(),
        "a-b-c"
    );
    assert_eq!(eval("len(words(\"   \"))").to_display(), "0");
    // Builtins are also methods.
    assert_eq!(
        eval("\"one two\".words()").to_display(),
        "[\"one\", \"two\"]"
    );
    // `lines` keeps its existing behaviour.
    assert_eq!(eval("len(lines(\"a\\nb\\n\"))").to_display(), "2");
}
