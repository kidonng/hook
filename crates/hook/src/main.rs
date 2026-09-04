use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process;

use fish_parser::parse;
use hook::bash::emitter::emit_bash;
use hook::bash::lowering::lower_program;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "-h" | "--help" => {
                print_help();
                process::exit(0);
            }
            "-V" | "-v" | "--version" => {
                println!("hook {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            _ => {}
        }
    }

    let (input, source_name) = if args.len() > 1 && args[1] != "-" {
        let path = &args[1];
        match fs::read_to_string(path) {
            Ok(content) => (content, path.clone()),
            Err(e) => {
                eprintln!("hook: error reading {}: {}", path, e);
                process::exit(2);
            }
        }
    } else {
        let mut buffer = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut buffer) {
            eprintln!("hook: error reading from stdin: {}", e);
            process::exit(2);
        }
        (buffer, "<stdin>".to_string())
    };

    let parsed = match parse(&input) {
        Ok(prog) => prog,
        Err(err) => {
            eprintln!(
                "hook: syntax error in {} at line {}, column {}: expected {}",
                source_name, err.location.line, err.location.column, err.expected
            );
            process::exit(1);
        }
    };

    let lowered = lower_program(&parsed);
    let emitted = emit_bash(&lowered);

    if let Err(e) = io::stdout().write_all(emitted.as_bytes()) {
        eprintln!("hook: error writing output: {}", e);
        process::exit(2);
    }
}

fn print_help() {
    println!(
        r#"hook - Transpile Fish shell scripts to Bash 3.2+

USAGE:
    hook [FILE]
    cat file.fish | hook

ARGS:
    <FILE>    Fish script to transpile (reads from stdin if omitted or '-')

FLAGS:
    -h, --help       Print help information
    -V, --version    Print version information
"#
    );
}
