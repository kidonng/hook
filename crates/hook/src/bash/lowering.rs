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
            })
        }
        Statement::While(w) => LoweredStatement::While(LoweredWhile {
            condition: w
                .condition
                .iter()
                .map(|p| lower_pipeline(p, scope))
                .collect(),
            body: lower_statements(&w.body, scope),
        }),
        Statement::Function(f) => {
            let mut fn_scope = *scope;
            fn_scope.in_function = true;
            LoweredStatement::Function(LoweredFunction {
                name: f.name.clone(),
                named_args: f.named_args.clone(),
                description: f.description.clone(),
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

fn lower_set_command(cmd: &Command, scope: &Scope) -> Option<AssignmentIR> {
    let mut is_local = false;
    let mut is_export = false;
    let mut is_append = false;
    let mut is_prepend = false;
    let mut is_erase = false;

    let mut var_name = None;
    let mut values = Vec::new();

    for arg in cmd.args.iter().skip(1) {
        if let Some(lit) = arg.as_single_literal() {
            if lit.starts_with('-') && var_name.is_none() {
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
        if var_name.is_none() {
            if let Some(lit) = arg.as_single_literal() {
                var_name = Some(lit.to_string());
            }
        } else {
            values.push(lower_word(arg, scope));
        }
    }

    let name = var_name?;

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
            Some(AssignmentIR::Global { name, values })
        }
    } else {
        Some(AssignmentIR::Global { name, values })
    }
}

pub fn lower_pipeline(p: &Pipeline, scope: &Scope) -> LoweredPipeline {
    LoweredPipeline {
        commands: p.commands.iter().map(|c| lower_command(c, scope)).collect(),
        combinator: p.combinator,
        background: p.background,
    }
}

pub fn lower_command(c: &Command, scope: &Scope) -> LoweredCommand {
    if let Some(cmd_name) = c.args.first().and_then(|w| w.as_single_literal()) {
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
    if v.name == "status" && v.slices.is_empty() {
        return LoweredVariableRef::Status;
    }
    if v.name == "pipestatus" && v.slices.is_empty() {
        return LoweredVariableRef::Pipestatus;
    }
    if v.name == "argv" {
        if v.slices.is_empty() {
            return LoweredVariableRef::ArgvAll;
        }
        if v.slices.len() == 1 {
            match &v.slices[0] {
                Slice::Index(SliceIndex::Pos(idx)) => return LoweredVariableRef::ArgvIndex(*idx),
                Slice::Index(SliceIndex::Neg(1)) => return LoweredVariableRef::ArgvLast,
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
                let zero_based = if *n > 0 { n - 1 } else { 0 };
                Some(BashSubscript::ZeroBasedIndex(zero_based))
            }
            Slice::Index(SliceIndex::Neg(k)) => Some(BashSubscript::NegativeOffsetFromLength(*k)),
            Slice::Range {
                start: Some(SliceIndex::Pos(s)),
                end: Some(SliceIndex::Pos(e)),
            } if e >= s => {
                let offset = if *s > 0 { s - 1 } else { 0 };
                let length = e - s + 1;
                Some(BashSubscript::Range { offset, length })
            }
            Slice::Range {
                start: Some(SliceIndex::Pos(s)),
                end: None,
            } => {
                let offset = if *s > 0 { s - 1 } else { 0 };
                Some(BashSubscript::OpenRange { offset })
            }
            _ => Some(BashSubscript::All),
        }
    } else {
        Some(BashSubscript::All)
    };

    LoweredVariableRef::Custom {
        name: v.name.clone(),
        subscript,
    }
}
