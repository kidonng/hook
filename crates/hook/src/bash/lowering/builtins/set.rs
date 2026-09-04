use crate::bash::ir::{AssignmentIR, LoweredCommand, LoweredWord, SliceIndexIR};
use crate::bash::lowering::{Scope, lower_word};
use crate::words;
use fish_parser::ast::{Command, Word, WordPart};

pub fn parse_slice_target(word: &Word) -> Option<(String, SliceIndexIR)> {
    if let Some(lit) = word.as_single_literal() {
        if lit.ends_with(']') {
            if let Some((name, rest)) = lit.split_once('[') {
                if let Some(idx_str) = rest.strip_suffix(']') {
                    if let Some(stripped) = idx_str.strip_prefix('$') {
                        return Some((
                            name.to_string(),
                            SliceIndexIR::Dynamic(stripped.to_string()),
                        ));
                    }
                    if let Ok(num) = idx_str.parse::<isize>() {
                        if num < 0 {
                            return Some((name.to_string(), SliceIndexIR::Negative(num)));
                        } else if num > 0 {
                            return Some((
                                name.to_string(),
                                SliceIndexIR::ZeroBased((num - 1) as usize),
                            ));
                        } else {
                            return Some((name.to_string(), SliceIndexIR::ZeroBased(0)));
                        }
                    }
                }
            }
        }
    } else if word.parts.len() == 3 {
        if let WordPart::Literal(prefix) = &word.parts[0] {
            if let Some(name) = prefix.strip_suffix('[') {
                if let WordPart::Variable(vref) = &word.parts[1] {
                    if let Some(var_name) = vref.name() {
                        if let WordPart::Literal(suffix) = &word.parts[2] {
                            if suffix == "]" {
                                return Some((
                                    name.to_string(),
                                    SliceIndexIR::Dynamic(var_name.to_string()),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn lower_set_assignment(cmd: &Command, scope: &Scope) -> Option<AssignmentIR> {
    let mut is_local = false;
    let mut is_export = false;
    let mut is_append = false;
    let mut is_prepend = false;
    let mut is_erase = false;

    let mut var_word = None;
    let mut values = Vec::new();

    for arg in cmd.args.iter().skip(1) {
        if let Some(lit) = arg.as_single_literal() {
            if lit.starts_with('-') && var_word.is_none() {
                match lit {
                    "-q" | "--query" => return None,
                    "-l" | "--local" => is_local = true,
                    "-x" | "--export" => is_export = true,
                    "-gx" | "-xg" => is_export = true,
                    "-a" | "--append" => is_append = true,
                    "-p" | "--prepend" => is_prepend = true,
                    "-e" | "--erase" => is_erase = true,
                    "-g" | "--global" => {}
                    _ => {}
                }
                continue;
            }
        }
        if var_word.is_none() {
            var_word = Some(arg);
        } else {
            values.push(lower_word(arg, scope));
        }
    }

    let target = var_word?;

    if let Some((name, slice_idx)) = parse_slice_target(target) {
        if name == "argv" && slice_idx == SliceIndexIR::Negative(-1) && !values.is_empty() {
            return Some(AssignmentIR::ArgvLast {
                value: values.remove(0),
            });
        }
        if is_erase {
            return Some(AssignmentIR::SliceErase {
                name,
                index: slice_idx,
            });
        } else {
            let val = if !values.is_empty() {
                values.remove(0)
            } else {
                LoweredWord::from("")
            };
            return Some(AssignmentIR::SliceAssign {
                name,
                index: slice_idx,
                value: val,
            });
        }
    }

    let name = target.as_single_literal()?.to_string();

    if name == "argv[-1]" && !values.is_empty() {
        return Some(AssignmentIR::ArgvLast {
            value: values.remove(0),
        });
    }

    if is_erase {
        Some(AssignmentIR::Erase { name })
    } else if is_append {
        Some(AssignmentIR::Append { name, values })
    } else if is_prepend {
        Some(AssignmentIR::Prepend { name, values })
    } else if is_export {
        Some(AssignmentIR::Export { name, values })
    } else if is_local {
        if scope.in_function {
            Some(AssignmentIR::Local { name, values })
        } else {
            // Safety defense: top level falls back to Global
            Some(AssignmentIR::Global {
                name,
                values,
                in_function: false,
            })
        }
    } else {
        Some(AssignmentIR::Global {
            name,
            values,
            in_function: scope.in_function,
        })
    }
}

pub fn lower_set_query(cmd: &Command) -> Option<LoweredCommand> {
    let mut is_query = false;
    let mut query_var = None;

    for arg in cmd.args.iter().skip(1) {
        if let Some(lit) = arg.as_single_literal() {
            if lit == "-q" || lit == "--query" {
                is_query = true;
                continue;
            }
            if lit == "--" {
                continue;
            }
            if query_var.is_none() && !lit.starts_with('-') {
                query_var = Some(lit);
            }
        }
    }

    if !is_query {
        return None;
    }

    let var = query_var?;
    if var == "argv[-1]" || var == "argv" {
        Some(LoweredCommand {
            assignments: vec![],
            args: words!["[", "$#", "-gt", "0", "]"],
            redirections: vec![],
        })
    } else {
        Some(LoweredCommand {
            assignments: vec![],
            args: words!["[", "-n", format!("\"${{{}:-}}\"", var), "]"],
            redirections: vec![],
        })
    }
}
