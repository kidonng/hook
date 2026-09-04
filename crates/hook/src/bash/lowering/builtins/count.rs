use crate::bash::ir::LoweredCommand;
use crate::bash::lowering::helpers::{WordsBuilder, extract_single_variable};
use crate::bash::lowering::{Scope, lower_redirection, lower_word};
use crate::words;
use fish_parser::ast::{Command, RedirectMode};

pub fn lower_count(c: &Command, scope: &Scope) -> Option<LoweredCommand> {
    let has_dev_null_redir = c.redirections.iter().any(|r| {
        matches!(r.mode, RedirectMode::Output | RedirectMode::OutputAndErr)
            && r.target.as_single_literal() == Some("/dev/null")
    });

    if c.args.len() == 2 {
        if let Some(var) = extract_single_variable(&c.args[1]) {
            let expr = if var == "argv" {
                "\"$#\"".to_string()
            } else {
                format!("\"${{#{}[@]}}\"", var)
            };

            return if has_dev_null_redir {
                Some(LoweredCommand {
                    assignments: vec![],
                    args: words!["[", expr, "-gt", "0", "]"],
                    redirections: vec![],
                })
            } else {
                Some(LoweredCommand {
                    assignments: vec![],
                    args: words!["printf", "'%s\\n'", expr],
                    redirections: c
                        .redirections
                        .iter()
                        .map(|r| lower_redirection(r, scope))
                        .collect(),
                })
            };
        }
    } else if c.args.len() == 1 {
        return if has_dev_null_redir {
            Some(LoweredCommand {
                assignments: vec![],
                args: words!["[", "0", "-gt", "0", "]"],
                redirections: vec![],
            })
        } else {
            Some(LoweredCommand {
                assignments: vec![],
                args: words!["printf", "'%s\\n'", "0"],
                redirections: c
                    .redirections
                    .iter()
                    .map(|r| lower_redirection(r, scope))
                    .collect(),
            })
        };
    }

    if has_dev_null_redir {
        let count_str = (c.args.len() - 1).to_string();
        Some(LoweredCommand {
            assignments: vec![],
            args: words!["[", count_str, "-gt", "0", "]"],
            redirections: vec![],
        })
    } else {
        let mut sub_args = WordsBuilder::new();
        sub_args.push_words("(set --");
        sub_args.extend(c.args.iter().skip(1).map(|arg| lower_word(arg, scope)));
        sub_args.push_words("; printf '%s\\n' \"$#\")");

        Some(LoweredCommand {
            assignments: vec![],
            args: sub_args.into_vec(),
            redirections: c
                .redirections
                .iter()
                .map(|r| lower_redirection(r, scope))
                .collect(),
        })
    }
}
