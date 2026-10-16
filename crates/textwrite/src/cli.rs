//! The `textwrite` / `tw` command-line interface: run files, evaluate
//! snippets, and a small REPL.

use std::io::{BufRead, Write};

use crate::{parser, Error, Interpreter, Value, VERSION};

/// How the REPL should treat one chunk of input.
enum Outcome {
    /// The chunk was an expression and produced this value.
    Value(Value),
    /// The chunk was a program that ran successfully.
    Done,
    /// The chunk needs more input (unclosed brace, trailing operator, ...).
    Incomplete,
}

/// Entry point for both `textwrite` and `tw`. Returns a process exit code.
pub fn main() -> i32 {
    let program = std::env::args()
        .next()
        .unwrap_or_else(|| "textwrite".to_string());
    let args: Vec<String> = std::env::args().skip(1).collect();

    let Some(first) = args.first().map(String::as_str) else {
        return repl();
    };

    match first {
        "-h" | "--help" | "help" => {
            print_help(&program);
            0
        }
        "-v" | "--version" | "version" => {
            println!("TextWrite {VERSION}");
            0
        }
        "repl" => repl(),
        "eval" => {
            if args.len() < 2 {
                eprintln!("textwrite: `eval` needs an expression");
                return 2;
            }
            let code = args[1..].join(" ");
            run_source("<eval>", &code, true, None)
        }
        "run" => match args.get(1) {
            Some(path) => run_file(path),
            None => {
                eprintln!("textwrite: `run` needs a file path");
                2
            }
        },
        other if other.starts_with('-') => {
            eprintln!("textwrite: unknown option `{other}`");
            print_help(&program);
            2
        }
        path => run_file(path),
    }
}

fn print_help(program: &str) {
    println!(
        "TextWrite {VERSION} - the Fast Text scripting language\n\n\
         USAGE:\n  \
         {program} <file>        run a .tw file\n  \
         {program} run <file>    run a .tw file\n  \
         {program} eval <code>   evaluate a snippet and print its value\n  \
         {program} repl          start an interactive session\n  \
         {program} --help        show this help\n  \
         {program} --version     show the version\n\n\
         The short alias `tw` behaves identically."
    );
}

fn run_file(path: &str) -> i32 {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("textwrite: cannot read {path}: {e}");
            return 1;
        }
    };
    // Relative imports resolve against the file's own directory.
    let base = std::path::Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf());
    run_source(&display_name(path), &source, false, base)
}

/// Runs `source`, printing any error. When `print_value` is set and the source
/// is a single expression, its value is printed. `base` is the directory that
/// relative `import` paths resolve against.
fn run_source(
    name: &str,
    source: &str,
    print_value: bool,
    base: Option<std::path::PathBuf>,
) -> i32 {
    let mut tw = Interpreter::new();
    if let Some(dir) = base {
        tw.set_module_base(dir);
    }
    let result = if print_value {
        match parser::parse_expression(source) {
            Ok(expr) => tw.eval_parsed(&expr),
            Err(_) => tw.run(source).map(|()| Value::None),
        }
    } else {
        tw.run(source).map(|()| Value::None)
    };
    match result {
        Ok(value) => {
            if print_value && !matches!(value, Value::None) {
                println!("{}", value.repr());
            }
            0
        }
        Err(e) => {
            print_error(name, source, &e);
            1
        }
    }
}

fn print_error(name: &str, source: &str, err: &Error) {
    eprintln!("{name}: {err}");
    if let Some(span) = err.span {
        let line_no = span.start.line as usize;
        if let Some(text) = source.lines().nth(line_no.saturating_sub(1)) {
            eprintln!("{:>4} | {}", line_no, text);
            let pad = " ".repeat(span.start.col.saturating_sub(1) as usize);
            let width = span.end.col.saturating_sub(span.start.col).max(1) as usize;
            eprintln!("     | {}{}", pad, "^".repeat(width));
        }
    }
}

fn repl() -> i32 {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut tw = Interpreter::new();
    let mut buffer = String::new();
    let mut line = String::new();

    println!("TextWrite {VERSION} - type `help` for commands, Ctrl+D to exit.");
    loop {
        print!("{}", if buffer.is_empty() { "tw> " } else { "...> " });
        let _ = std::io::stdout().flush();

        line.clear();
        match input.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("input error: {e}");
                break;
            }
        }

        if buffer.is_empty() {
            match line.trim() {
                "help" => {
                    print_repl_help();
                    continue;
                }
                "quit" | "exit" => break,
                "" => continue,
                _ => {}
            }
        }

        buffer.push_str(&line);
        match evaluate_chunk(&mut tw, &buffer) {
            Ok(Outcome::Value(v)) => {
                if !matches!(v, Value::None) {
                    println!("{}", v.repr());
                }
                buffer.clear();
            }
            Ok(Outcome::Done) => buffer.clear(),
            Ok(Outcome::Incomplete) => {}
            Err(e) => {
                eprintln!("{e}");
                buffer.clear();
            }
        }
    }
    0
}

fn evaluate_chunk(tw: &mut Interpreter, source: &str) -> crate::Result<Outcome> {
    match parser::parse_expression(source) {
        Ok(expr) => Ok(Outcome::Value(tw.eval_parsed(&expr)?)),
        Err(e) if e.is_incomplete() => Ok(Outcome::Incomplete),
        Err(_) => match parser::parse(source) {
            Ok(stmts) => {
                tw.run_parsed(&stmts)?;
                Ok(Outcome::Done)
            }
            Err(e) if e.is_incomplete() => Ok(Outcome::Incomplete),
            Err(e) => Err(e),
        },
    }
}

fn print_repl_help() {
    println!(
        "  help   show this message\n  quit   leave the REPL\n\n\
         Anything else is evaluated as TextWrite code. Declarations such as\n\
         `fn f(x) {{ ... }}` may span several lines; the prompt becomes `...> `."
    );
}

fn display_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}
