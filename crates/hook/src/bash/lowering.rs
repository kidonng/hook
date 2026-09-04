use super::ir::*;
use fish_parser::ast::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct Scope {
    pub in_function: bool,
    pub in_for_values: bool,
}

pub fn lower_program(program: &Program) -> LoweredProgram {
    let mut scope = Scope::default();
    let statements = lower_statements(&program.statements, &mut scope);
    LoweredProgram {
        shebang: program.shebang.clone(),
        statements,
    }
}

pub fn lower_statements(stmts: &[Statement], scope: &mut Scope) -> Vec<LoweredStatement> {
    stmts.iter().map(|s| lower_statement(s, scope)).collect()
}

pub fn lower_statement(stmt: &Statement, scope: &mut Scope) -> LoweredStatement {
    match stmt {
        Statement::Comment(c) => LoweredStatement::Comment(c.clone()),
        Statement::Return(w) => LoweredStatement::Return(w.as_ref().map(|w| lower_word(w, scope))),
        Statement::Break => LoweredStatement::Break,
        Statement::Continue => LoweredStatement::Continue,
        Statement::Pipeline(p) => {
            // Check if this pipeline is a `set` command
            if p.commands.len() == 1 {
                let cmd = &p.commands[0];
                if let Some("set") = cmd.args.first().and_then(|w| w.as_single_literal()) {
                    if let Some(assign) = lower_set_command(cmd, scope) {
                        return LoweredStatement::Assignment(assign);
                    }
                }
            }
            LoweredStatement::Pipeline(lower_pipeline(p, scope))
        }
        Statement::If(i) => LoweredStatement::If(LoweredIf {
            condition: i
                .condition
                .iter()
                .map(|p| lower_pipeline(p, scope))
                .collect(),
            then_body: lower_statements(&i.then_body, scope),
            elif_branches: i
                .elif_branches
                .iter()
                .map(|(p, b)| {
                    (
                        p.iter().map(|pl| lower_pipeline(pl, scope)).collect(),
                        lower_statements(b, scope),
                    )
                })
                .collect(),
            else_body: i.else_body.as_ref().map(|b| lower_statements(b, scope)),
            redirections: i
                .redirections
                .iter()
                .map(|r| lower_redirection(r, scope))
                .collect(),
        }),
        Statement::Switch(s) => LoweredStatement::Switch(LoweredSwitch {
            value: lower_word(&s.value, scope),
            cases: s
                .cases
                .iter()
                .map(|c| LoweredCaseClause {
                    patterns: c.patterns.iter().map(|w| lower_word(w, scope)).collect(),
                    body: lower_statements(&c.body, scope),
                })
                .collect(),
        }),
        Statement::For(f) => {
            let mut val_scope = *scope;
            val_scope.in_for_values = true;
            let values = f.values.iter().map(|w| lower_word(w, &val_scope)).collect();
            LoweredStatement::For(LoweredFor {
                variable: f.variable.clone(),
                values,
                body: lower_statements(&f.body, scope),
                redirections: f
                    .redirections
                    .iter()
                    .map(|r| lower_redirection(r, scope))
                    .collect(),
            })
        }
        Statement::While(w) => LoweredStatement::While(LoweredWhile {
            condition: w
                .condition
                .iter()
                .map(|p| lower_pipeline(p, scope))
                .collect(),
            body: lower_statements(&w.body, scope),
            redirections: w
                .redirections
                .iter()
                .map(|r| lower_redirection(r, scope))
                .collect(),
        }),
        Statement::Function(f) => {
            let mut fn_scope = *scope;
            fn_scope.in_function = true;
            let (named_args, description) = extract_function_meta(&f.options);
            let func_name = f.name.as_single_literal().unwrap_or("").to_string();
            LoweredStatement::Function(LoweredFunction {
                name: func_name,
                named_args,
                description,
                body: lower_statements(&f.body, &mut fn_scope),
            })
        }
        Statement::BeginBlock(b) => LoweredStatement::BeginBlock(LoweredBeginBlock {
            combinator: b.combinator,
            body: lower_statements(&b.body, scope),
            redirections: b
                .redirections
                .iter()
                .map(|r| lower_redirection(r, scope))
                .collect(),
        }),
    }
}

fn parse_slice_target(word: &Word) -> Option<(String, SliceIndexIR)> {
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

fn lower_set_command(cmd: &Command, scope: &Scope) -> Option<AssignmentIR> {
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
                LoweredWord::from_literal("")
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

pub fn lower_pipeline(p: &Pipeline, scope: &Scope) -> LoweredPipeline {
    let commands: Vec<LoweredCommand> =
        p.commands.iter().map(|c| lower_command(c, scope)).collect();
    let pipe_operators: Vec<PipeKind> = p
        .pipe_operators
        .iter()
        .map(|op| match op {
            PipeOperator::Stdout => PipeKind::Stdout,
            PipeOperator::StdoutAndStderr => PipeKind::StdoutAndStderr,
        })
        .collect();
    LoweredPipeline {
        commands,
        pipe_operators,
        combinator: p.combinator,
        background: p.background,
    }
}
fn extract_single_variable(word: &Word) -> Option<&str> {
    if word.parts.len() == 1 {
        match &word.parts[0] {
            WordPart::Variable(vref) if vref.slices.is_empty() => vref.name(),
            WordPart::DoubleQuoted(inner) if inner.len() == 1 => {
                if let WordPart::Variable(vref) = &inner[0] {
                    if vref.slices.is_empty() {
                        return vref.name();
                    }
                }
                None
            }
            _ => None,
        }
    } else {
        None
    }
}

pub fn lower_command(c: &Command, scope: &Scope) -> LoweredCommand {
    if let Some(cmd_name) = c.args.first().and_then(|w| w.as_single_literal()) {
        if cmd_name == "count" {
            let has_dev_null_redir = c.redirections.iter().any(|r| {
                matches!(r.mode, RedirectMode::Output | RedirectMode::OutputAndErr)
                    && r.target.as_single_literal() == Some("/dev/null")
            });

            if c.args.len() == 2 {
                if let Some(var) = extract_single_variable(&c.args[1]) {
                    if var == "argv" {
                        if has_dev_null_redir {
                            return LoweredCommand {
                                negate: c.negate,
                                args: vec![
                                    LoweredWord::from_literal("["),
                                    LoweredWord::from_literal("\"$#\""),
                                    LoweredWord::from_literal("-gt"),
                                    LoweredWord::from_literal("0"),
                                    LoweredWord::from_literal("]"),
                                ],
                                redirections: vec![],
                            };
                        } else {
                            return LoweredCommand {
                                negate: c.negate,
                                args: vec![
                                    LoweredWord::from_literal("printf"),
                                    LoweredWord::from_literal("'%s\\n'"),
                                    LoweredWord::from_literal("\"$#\""),
                                ],
                                redirections: c
                                    .redirections
                                    .iter()
                                    .map(|r| lower_redirection(r, scope))
                                    .collect(),
                            };
                        }
                    } else {
                        if has_dev_null_redir {
                            return LoweredCommand {
                                negate: c.negate,
                                args: vec![
                                    LoweredWord::from_literal("["),
                                    LoweredWord::from_literal(format!("\"${{#{}[@]}}\"", var)),
                                    LoweredWord::from_literal("-gt"),
                                    LoweredWord::from_literal("0"),
                                    LoweredWord::from_literal("]"),
                                ],
                                redirections: vec![],
                            };
                        } else {
                            return LoweredCommand {
                                negate: c.negate,
                                args: vec![
                                    LoweredWord::from_literal("printf"),
                                    LoweredWord::from_literal("'%s\\n'"),
                                    LoweredWord::from_literal(format!("\"${{#{}[@]}}\"", var)),
                                ],
                                redirections: c
                                    .redirections
                                    .iter()
                                    .map(|r| lower_redirection(r, scope))
                                    .collect(),
                            };
                        }
                    }
                }
            } else if c.args.len() == 1 {
                if has_dev_null_redir {
                    return LoweredCommand {
                        negate: c.negate,
                        args: vec![
                            LoweredWord::from_literal("["),
                            LoweredWord::from_literal("0"),
                            LoweredWord::from_literal("-gt"),
                            LoweredWord::from_literal("0"),
                            LoweredWord::from_literal("]"),
                        ],
                        redirections: vec![],
                    };
                } else {
                    return LoweredCommand {
                        negate: c.negate,
                        args: vec![
                            LoweredWord::from_literal("printf"),
                            LoweredWord::from_literal("'%s\\n'"),
                            LoweredWord::from_literal("0"),
                        ],
                        redirections: c
                            .redirections
                            .iter()
                            .map(|r| lower_redirection(r, scope))
                            .collect(),
                    };
                }
            } else if has_dev_null_redir {
                return LoweredCommand {
                    negate: c.negate,
                    args: vec![
                        LoweredWord::from_literal("["),
                        LoweredWord::from_literal(format!("{}", c.args.len() - 1)),
                        LoweredWord::from_literal("-gt"),
                        LoweredWord::from_literal("0"),
                        LoweredWord::from_literal("]"),
                    ],
                    redirections: vec![],
                };
            } else {
                let mut sub_args = vec![
                    LoweredWord::from_literal("(set"),
                    LoweredWord::from_literal("--"),
                ];
                for arg in c.args.iter().skip(1) {
                    sub_args.push(lower_word(arg, scope));
                }
                sub_args.push(LoweredWord::from_literal(";"));
                sub_args.push(LoweredWord::from_literal("printf"));
                sub_args.push(LoweredWord::from_literal("'%s\\n'"));
                sub_args.push(LoweredWord::from_literal("\"$#\")"));
                return LoweredCommand {
                    negate: c.negate,
                    args: sub_args,
                    redirections: c
                        .redirections
                        .iter()
                        .map(|r| lower_redirection(r, scope))
                        .collect(),
                };
            }
        }
        if cmd_name == "contains" {
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

            if let Some(needle) = needle_opt {
                let lowered_needle = if needle.parts.len() == 1 {
                    if let WordPart::Literal(s) = &needle.parts[0] {
                        LoweredWord::from_literal(format!("\"{}\"", s))
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
                            lowered_haystack.push(LoweredWord::from_literal("\"$@\""));
                        } else {
                            lowered_haystack
                                .push(LoweredWord::from_literal(format!("\"${{{}[@]}}\"", var)));
                        }
                    } else {
                        lowered_haystack.push(lower_word(h, scope));
                    }
                }

                let mut sub_args = Vec::new();
                if is_index {
                    sub_args.push(LoweredWord::from_literal("(__hook_i=1;"));
                    sub_args.push(LoweredWord::from_literal("for"));
                    sub_args.push(LoweredWord::from_literal("__hook_item"));
                    sub_args.push(LoweredWord::from_literal("in"));
                    for h in lowered_haystack {
                        sub_args.push(h);
                    }
                    sub_args.push(LoweredWord::from_literal(";"));
                    sub_args.push(LoweredWord::from_literal("do"));
                    sub_args.push(LoweredWord::from_literal("["));
                    sub_args.push(LoweredWord::from_literal("\"$__hook_item\""));
                    sub_args.push(LoweredWord::from_literal("="));
                    sub_args.push(lowered_needle);
                    sub_args.push(LoweredWord::from_literal("]"));
                    sub_args.push(LoweredWord::from_literal("&&"));
                    sub_args.push(LoweredWord::from_literal("{"));
                    sub_args.push(LoweredWord::from_literal("printf"));
                    sub_args.push(LoweredWord::from_literal("'%s\\n'"));
                    sub_args.push(LoweredWord::from_literal("\"$__hook_i\";"));
                    sub_args.push(LoweredWord::from_literal("exit"));
                    sub_args.push(LoweredWord::from_literal("0;"));
                    sub_args.push(LoweredWord::from_literal("};"));
                    sub_args.push(LoweredWord::from_literal("__hook_i=$((__hook_i"));
                    sub_args.push(LoweredWord::from_literal("+"));
                    sub_args.push(LoweredWord::from_literal("1));"));
                    sub_args.push(LoweredWord::from_literal("done;"));
                    sub_args.push(LoweredWord::from_literal("exit"));
                    sub_args.push(LoweredWord::from_literal("1)"));
                } else {
                    sub_args.push(LoweredWord::from_literal("(for"));
                    sub_args.push(LoweredWord::from_literal("__hook_item"));
                    sub_args.push(LoweredWord::from_literal("in"));
                    for h in lowered_haystack {
                        sub_args.push(h);
                    }
                    sub_args.push(LoweredWord::from_literal(";"));
                    sub_args.push(LoweredWord::from_literal("do"));
                    sub_args.push(LoweredWord::from_literal("["));
                    sub_args.push(LoweredWord::from_literal("\"$__hook_item\""));
                    sub_args.push(LoweredWord::from_literal("="));
                    sub_args.push(lowered_needle);
                    sub_args.push(LoweredWord::from_literal("]"));
                    sub_args.push(LoweredWord::from_literal("&&"));
                    sub_args.push(LoweredWord::from_literal("exit"));
                    sub_args.push(LoweredWord::from_literal("0;"));
                    sub_args.push(LoweredWord::from_literal("done;"));
                    sub_args.push(LoweredWord::from_literal("exit"));
                    sub_args.push(LoweredWord::from_literal("1)"));
                }

                return LoweredCommand {
                    negate: c.negate,
                    args: sub_args,
                    redirections: c
                        .redirections
                        .iter()
                        .map(|r| lower_redirection(r, scope))
                        .collect(),
                };
            } else {
                return LoweredCommand {
                    negate: c.negate,
                    args: vec![
                        LoweredWord::from_literal("(exit"),
                        LoweredWord::from_literal("1)"),
                    ],
                    redirections: vec![],
                };
            }
        }
        if cmd_name == "set" {
            let mut is_query = false;
            let mut query_var = None;
            for arg in c.args.iter().skip(1) {
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
            if is_query {
                if let Some(var) = query_var {
                    if var == "argv[-1]" || var == "argv" {
                        return LoweredCommand {
                            negate: c.negate,
                            args: vec![
                                LoweredWord::from_literal("["),
                                LoweredWord::from_literal("$#"),
                                LoweredWord::from_literal("-gt"),
                                LoweredWord::from_literal("0"),
                                LoweredWord::from_literal("]"),
                            ],
                            redirections: vec![],
                        };
                    } else {
                        return LoweredCommand {
                            negate: c.negate,
                            args: vec![
                                LoweredWord::from_literal("["),
                                LoweredWord::from_literal("-n"),
                                LoweredWord::from_literal(format!("\"${{{}:-}}\"", var)),
                                LoweredWord::from_literal("]"),
                            ],
                            redirections: vec![],
                        };
                    }
                }
            }
        }
    }
    LoweredCommand {
        negate: c.negate,
        args: c.args.iter().map(|w| lower_word(w, scope)).collect(),
        redirections: c
            .redirections
            .iter()
            .map(|r| lower_redirection(r, scope))
            .collect(),
    }
}

pub fn lower_redirection(r: &Redirection, scope: &Scope) -> LoweredRedirection {
    LoweredRedirection {
        fd: r.fd,
        mode: r.mode.clone(),
        target: lower_word(&r.target, scope),
    }
}

pub fn lower_word(w: &Word, scope: &Scope) -> LoweredWord {
    LoweredWord {
        parts: w.parts.iter().map(|p| lower_word_part(p, scope)).collect(),
    }
}

pub fn lower_word_part(part: &WordPart, scope: &Scope) -> LoweredWordPart {
    match part {
        WordPart::Literal(s) => LoweredWordPart::Literal(s.clone()),
        WordPart::SingleQuoted(s) => LoweredWordPart::SingleQuoted(s.clone()),
        WordPart::DoubleQuoted(parts) => {
            LoweredWordPart::DoubleQuoted(parts.iter().map(|p| lower_word_part(p, scope)).collect())
        }
        WordPart::Variable(v) => LoweredWordPart::Variable(lower_variable_ref(v)),
        WordPart::CommandSubst { statements, slices } => {
            // Check for process substitution: single pipeline ending in `psub` and no slices
            if slices.is_empty() && statements.len() == 1 {
                if let Statement::Pipeline(p) = &statements[0] {
                    if let Some(last_cmd) = p.commands.last() {
                        if let Some("psub") =
                            last_cmd.args.first().and_then(|w| w.as_single_literal())
                        {
                            // Strip terminal psub
                            let mut stripped_pipeline = p.clone();
                            stripped_pipeline.commands.pop();
                            if !stripped_pipeline.commands.is_empty() {
                                return LoweredWordPart::ProcessSubst(lower_pipeline(
                                    &stripped_pipeline,
                                    scope,
                                ));
                            }
                        }
                    }
                }
            }
            // IFS defense: quote by default unless in `for ... in` values
            let quoted = !scope.in_for_values;
            let mut subst_scope = *scope;
            LoweredWordPart::CommandSubst {
                stmts: lower_statements(statements, &mut subst_scope),
                slices: slices.clone(),
                quoted,
            }
        }
        WordPart::BraceExpansion(words) => {
            LoweredWordPart::BraceExpansion(words.iter().map(|w| lower_word(w, scope)).collect())
        }
    }
}

fn lower_variable_ref(v: &VariableRef) -> LoweredVariableRef {
    match &v.target {
        VariableTarget::Indirect(inner) => {
            let mut curr = inner.as_ref();
            while let VariableTarget::Indirect(next) = &curr.target {
                curr = next.as_ref();
            }
            let name = curr.name().unwrap_or("").to_string();
            LoweredVariableRef::Indirect { name }
        }
        VariableTarget::Named(name) => {
            if name == "status" && v.slices.is_empty() {
                return LoweredVariableRef::Status;
            }
            if name == "pipestatus" && v.slices.is_empty() {
                return LoweredVariableRef::Pipestatus;
            }
            if name == "fish_pid" && v.slices.is_empty() {
                return LoweredVariableRef::FishPid;
            }
            if name == "last_pid" && v.slices.is_empty() {
                return LoweredVariableRef::LastPid;
            }
            if name == "argv" {
                if v.slices.is_empty() {
                    return LoweredVariableRef::ArgvAll;
                }
                if v.slices.len() == 1 {
                    match &v.slices[0] {
                        Slice::Index(SliceIndex::Pos(idx)) => {
                            return LoweredVariableRef::ArgvIndex(*idx);
                        }
                        Slice::Index(SliceIndex::Neg(1)) => return LoweredVariableRef::ArgvLast,
                        Slice::Index(SliceIndex::Variable(vref)) => {
                            return LoweredVariableRef::ArgvDynamic(
                                vref.name().unwrap_or("").to_string(),
                            );
                        }
                        Slice::Range {
                            start: Some(SliceIndex::Pos(s)),
                            end: None,
                        } => {
                            return LoweredVariableRef::ArgvSlice {
                                start: *s,
                                len: None,
                            };
                        }
                        Slice::Range {
                            start: Some(SliceIndex::Pos(s)),
                            end: Some(SliceIndex::Neg(1)),
                        } => {
                            return LoweredVariableRef::ArgvSlice {
                                start: *s,
                                len: None,
                            };
                        }
                        Slice::Range {
                            start: Some(SliceIndex::Pos(s)),
                            end: Some(SliceIndex::Pos(e)),
                        } if e >= s => {
                            return LoweredVariableRef::ArgvSlice {
                                start: *s,
                                len: Some(e - s + 1),
                            };
                        }
                        Slice::Range {
                            start: Some(SliceIndex::Variable(s)),
                            end: Some(SliceIndex::Variable(e)),
                        } => {
                            return LoweredVariableRef::ArgvDynamicRange {
                                start: s.name().unwrap_or("").to_string(),
                                end: e.name().unwrap_or("").to_string(),
                            };
                        }
                        _ => {}
                    }
                }
            }

            // Generic variable
            let subscript = if v.slices.is_empty() {
                None
            } else if v.slices.len() == 1 {
                match &v.slices[0] {
                    Slice::Index(SliceIndex::Pos(n)) => {
                        let zero_based = if *n > 0 { (*n as isize) - 1 } else { 0 };
                        Some(BashSubscript::Index(zero_based))
                    }
                    Slice::Index(SliceIndex::Neg(k)) => Some(BashSubscript::Index(-(*k as isize))),
                    Slice::Index(SliceIndex::Variable(vref)) => Some(
                        BashSubscript::DynamicVariable(vref.name().unwrap_or("").to_string()),
                    ),
                    Slice::Range {
                        start: Some(SliceIndex::Pos(s)),
                        end: Some(SliceIndex::Pos(e)),
                    } if e >= s => {
                        let offset = if *s > 0 { (*s as isize) - 1 } else { 0 };
                        let length = e - s + 1;
                        Some(BashSubscript::Range { offset, length })
                    }
                    Slice::Range {
                        start: Some(SliceIndex::Pos(s)),
                        end: None,
                    } => {
                        let offset = if *s > 0 { (*s as isize) - 1 } else { 0 };
                        Some(BashSubscript::OpenRange { offset })
                    }
                    Slice::Range {
                        start: Some(SliceIndex::Variable(s)),
                        end: Some(SliceIndex::Variable(e)),
                    } => Some(BashSubscript::DynamicRange {
                        start: s.name().unwrap_or("").to_string(),
                        end: e.name().unwrap_or("").to_string(),
                    }),
                    _ => Some(BashSubscript::All),
                }
            } else {
                Some(BashSubscript::All)
            };

            LoweredVariableRef::Custom {
                name: name.clone(),
                subscript,
            }
        }
    }
}

fn extract_function_meta(options: &[Word]) -> (Vec<String>, Option<String>) {
    let mut named_args = Vec::new();
    let mut description = None;

    let mut i = 0;
    while i < options.len() {
        let opt = &options[i];
        if let Some(lit) = opt.as_single_literal() {
            match lit {
                "-a" | "--argument-names" => {
                    i += 1;
                    while i < options.len() {
                        let next = &options[i];
                        if let Some(name) = next.as_single_literal() {
                            if !name.starts_with('-') {
                                named_args.push(name.to_string());
                                i += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    continue;
                }
                "-d" | "--description" => {
                    i += 1;
                    if i < options.len() {
                        description = extract_word_string(&options[i]);
                        i += 1;
                    }
                    continue;
                }
                "-w" | "--wraps" | "-V" | "--inherit-variable" | "-e" | "--on-event" | "-s"
                | "--on-signal" | "-v" | "--on-variable" | "-j" | "--on-job-exit" => {
                    i += 1;
                    if i < options.len() {
                        i += 1;
                    }
                    continue;
                }
                _ => {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    (named_args, description)
}

fn extract_word_string(w: &Word) -> Option<String> {
    let mut s = String::new();
    for p in &w.parts {
        match p {
            WordPart::Literal(lit) => s.push_str(lit),
            WordPart::SingleQuoted(sq) => s.push_str(sq),
            WordPart::DoubleQuoted(parts) => {
                for dp in parts {
                    if let WordPart::Literal(lit) = dp {
                        s.push_str(lit);
                    }
                }
            }
            _ => return None,
        }
    }
    Some(s)
}
