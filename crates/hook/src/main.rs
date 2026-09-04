use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process;
use std::str::FromStr;

use fish_parser::parse;
use hook::Target;
use hook::bash::emitter::emit_bash;
use hook::bash::lowering::lower_program;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut file_path = None;
    let mut target = Target::default();

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                process::exit(0);
            }
            "-V" | "-v" | "--version" => {
                println!("hook {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "--target" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("hook: error: --target requires an argument");
                    process::exit(2);
                }
                match Target::from_str(&args[i]) {
                    Ok(t) => target = t,
                    Err(e) => {
                        eprintln!("hook: error: {}", e);
                        process::exit(2);
                    }
                }
            }
            _ if arg.starts_with("--target=") => {
                let val = &arg["--target=".len()..];
                match Target::from_str(val) {
                    Ok(t) => target = t,
                    Err(e) => {
                        eprintln!("hook: error: {}", e);
                        process::exit(2);
                    }
                }
            }
            _ if arg.starts_with('-') && arg != "-" => {
                eprintln!("hook: unrecognized option '{}'", arg);
                process::exit(2);
            }
            _ => {
                if file_path.is_none() {
                    file_path = Some(arg.clone());
                }
            }
        }
        i += 1;
    }

    if target == Target::Bash3_2 {
        eprintln!("hook: target 'bash3.2' will be supported in an upcoming release");
        process::exit(2);
    }

    let (input, source_name) = if let Some(path) = &file_path {
        if path != "-" {
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
        r#"hook - Transpile Fish shell scripts to modern Bash (5.0+)

USAGE:
    hook [OPTIONS] [FILE]
    cat file.fish | hook

ARGS:
    <FILE>    Fish script to transpile (reads from stdin if omitted or '-')

OPTIONS:
    --target <TARGET>    Target Bash version: 'bash5' (default) or 'bash3.2'
    -h, --help           Print help information
    -V, --version        Print version information
"#
    );
}
