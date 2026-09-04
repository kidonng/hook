use crate::bash::ir::{LoweredCommand, LoweredWord};
use crate::bash::lowering::helpers::{WordsBuilder, extract_single_variable};
use crate::bash::lowering::{Scope, lower_redirection, lower_word};
use crate::words;
use fish_parser::ast::{Command, Word, WordPart};

pub fn lower_contains(c: &Command, scope: &Scope) -> Option<LoweredCommand> {
    let mut is_index = false;
    let mut needle_opt: Option<&Word> = None;
    let mut haystack: Vec<&Word> = Vec::new();
    let mut stop_options = false;

    for arg in c.args.iter().skip(1) {
        if !stop_options {
            if let Some(lit) = arg.as_single_literal() {
                if lit == "--" {
                    stop_options = true;
                    continue;
                }
                if lit == "-i" || lit == "--index" {
                    is_index = true;
                    continue;
                }
                if lit == "-q" || lit == "--query" {
                    continue;
                }
            }
        }
        if needle_opt.is_none() {
            needle_opt = Some(arg);
        } else {
            haystack.push(arg);
        }
    }

    let needle = match needle_opt {
        Some(n) => n,
        None => {
            return Some(LoweredCommand {
                assignments: vec![],
                args: words!["(exit", "1)"],
                redirections: vec![],
            });
        }
    };

    let lowered_needle = if needle.parts.len() == 1 {
        if let WordPart::Literal(s) = &needle.parts[0] {
            LoweredWord::from(format!("\"{}\"", s))
        } else {
            lower_word(needle, scope)
        }
    } else {
        lower_word(needle, scope)
    };

    let mut lowered_haystack = Vec::new();
    for h in haystack {
        if let Some(var) = extract_single_variable(h) {
            if var == "argv" {
                lowered_haystack.push(LoweredWord::from("\"$@\""));
            } else {
                lowered_haystack.push(LoweredWord::from(format!("\"${{{}[@]}}\"", var)));
            }
        } else {
            lowered_haystack.push(lower_word(h, scope));
        }
    }

    let mut builder = WordsBuilder::new();
    if is_index {
        builder.push_words("(__hook_i=1; for __hook_item in");
        builder.extend(lowered_haystack);
        builder.push_words("; do [ \"$__hook_item\" =");
        builder.push(lowered_needle);
        builder.push_words(
            "] && { printf '%s\\n' \"$__hook_i\"; exit 0; }; __hook_i=$((__hook_i + 1)); done; exit 1)",
        );
    } else {
        builder.push_words("(for __hook_item in");
        builder.extend(lowered_haystack);
        builder.push_words("; do [ \"$__hook_item\" =");
        builder.push(lowered_needle);
        builder.push_words("] && exit 0; done; exit 1)");
    }

    Some(LoweredCommand {
        assignments: vec![],
        args: builder.into_vec(),
        redirections: c
            .redirections
            .iter()
            .map(|r| lower_redirection(r, scope))
            .collect(),
    })
}
