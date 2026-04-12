//! The standalone `lua` interpreter binary.
//!
//! Port of `lua.c`. Accepts a script file path, compiles via
//! lparser, executes via lvm with the full stdlib available.
//!
//! Usage:
//!   lua script.lua [args...]
//!   lua -e "code"
//!   lua -l lib                   -- load module (TODO)
//!   lua -i                       -- REPL after script
//!   lua                          -- REPL
//!   lua -v                       -- print version and exit

use spike::contract::{LuaError, LuaState, TValue, ThreadStatus};
use spike::lbaselib::{reset_print_buffer, take_print_buffer};
use spike::linit::open_libs;
use spike::lparser::parse_with_env;
use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = env::args().collect();
    if argv.len() < 2 {
        return repl();
    }
    let mut i = 1;
    let mut script: Option<String> = None;
    let mut inline: Option<String> = None;
    let mut interactive = false;
    let mut show_version = false;
    while i < argv.len() {
        let a = &argv[i];
        match a.as_str() {
            "-v" | "--version" => show_version = true,
            "-e" => {
                i += 1;
                if i >= argv.len() {
                    eprintln!("lua: '-e' needs argument");
                    return ExitCode::from(1);
                }
                inline = Some(argv[i].clone());
            }
            "-i" => interactive = true,
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            _ if a.starts_with('-') => {
                eprintln!("lua: unknown option '{}'", a);
                return ExitCode::from(1);
            }
            _ => {
                script = Some(a.clone());
                break;
            }
        }
        i += 1;
    }

    if show_version {
        println!("Lua 5.4 (Rust port, noricum)");
        if script.is_none() && inline.is_none() && !interactive {
            return ExitCode::SUCCESS;
        }
    }

    let mut state = LuaState::new(0);
    let globals = open_libs(&mut state);

    if let Some(code) = inline {
        match run_source(&mut state, globals, code.as_bytes(), "=(command line)") {
            Ok(()) => {}
            Err(msg) => {
                eprintln!("lua: {}", msg);
                return ExitCode::from(1);
            }
        }
    }

    if let Some(path) = script {
        let content = match fs::read(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("lua: cannot open '{}': {}", path, e);
                return ExitCode::from(1);
            }
        };
        let source_name = format!("@{}", path);
        match run_source(&mut state, globals, &content, &source_name) {
            Ok(()) => {}
            Err(msg) => {
                eprintln!("lua: {}", msg);
                return ExitCode::from(1);
            }
        }
    }

    if interactive {
        return run_repl(&mut state, globals);
    }

    ExitCode::SUCCESS
}

fn run_source(
    state: &mut LuaState,
    globals: spike::contract::TableHandle,
    source: &[u8],
    name: &str,
) -> Result<(), String> {
    // Compile.
    let closure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parse_with_env(state, source, name.as_bytes(), globals)
    }));
    let closure = match closure {
        Ok(c) => c,
        Err(e) => {
            let msg = panic_string(e);
            return Err(format!("compile: {}", msg));
        }
    };
    state.push_tvalue_helper(TValue::LuaClosure(closure));
    let status = state.pcall(0, 0, -1);
    match status {
        ThreadStatus::Ok => Ok(()),
        _ => {
            let err_val = state.current_thread().stack[0];
            Err(format_tv(state, err_val))
        }
    }
}

fn format_tv(state: &LuaState, v: TValue) -> String {
    match v {
        TValue::Nil => "nil".to_string(),
        TValue::ShortString(h) | TValue::LongString(h) => {
            String::from_utf8_lossy(&state.global.heap.string(h).bytes).to_string()
        }
        TValue::Integer(i) => i.to_string(),
        TValue::Number(n) => format!("{}", n),
        other => format!("{:?}", other),
    }
}

fn panic_string(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic".to_string()
    }
}

fn repl() -> ExitCode {
    let mut state = LuaState::new(0);
    let globals = open_libs(&mut state);
    run_repl(&mut state, globals)
}

fn run_repl(
    state: &mut LuaState,
    globals: spike::contract::TableHandle,
) -> ExitCode {
    println!("Lua 5.4 (Rust port, noricum). Type Ctrl-D to exit.");
    let stdin = io::stdin();
    let mut line = String::new();
    loop {
        print!("> ");
        let _ = io::stdout().flush();
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                return ExitCode::SUCCESS;
            }
            Ok(_) => {}
            Err(_) => return ExitCode::from(1),
        }
        let code = line.trim_end().as_bytes();
        if code.is_empty() {
            continue;
        }
        // Try `return <expr>` first so expressions print.
        let as_expr = format!("return {}", String::from_utf8_lossy(code));
        reset_print_buffer();
        if run_source(state, globals, as_expr.as_bytes(), "=stdin").is_err() {
            // Fall back to statement.
            let _ = run_source(state, globals, code, "=stdin");
        }
        for line in take_print_buffer() {
            println!("{}", line);
        }
    }
}

fn print_help() {
    println!("usage: lua [options] [script [args...]]");
    println!("Options:");
    println!("  -e stat     execute string 'stat'");
    println!("  -i          enter interactive mode after executing 'script'");
    println!("  -v          show version information");
    println!("  -h, --help  show this help");
}
