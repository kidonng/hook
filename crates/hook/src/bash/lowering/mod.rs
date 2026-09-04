pub mod builtins;
pub mod helpers;

use super::ir::*;
use fish_parser::ast::*;
use helpers::extract_function_meta;

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
            // Check if this pipeline is a `set` assignment command
            if p.commands.len() == 1 {
                let cmd = &p.commands[0];
                if let Some("set") = cmd.args.first().and_then(|w| w.as_single_literal()) {
                    if let Some(assign) = builtins::set::lower_set_assignment(cmd, scope) {
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

pub fn lower_pipeline(p: &Pipeline, scope: &Scope) -> LoweredPipeline {
    let commands: Vec<LoweredCommand> =
        p.commands.iter().map(|c| lower_command(c, scope)).collect();
    let pipe_operators: Vec<PipeKind> = p
        .pipe_operators
        .iter()
        .map(|op| match op {
            PipeOperator::Stdout => PipeKind::Stdout,
            PipeOperator::StdoutAndStderr => PipeKind::StdoutAndStderr,
            PipeOperator::Fd(fd) => PipeKind::Fd(*fd),
        })
        .collect();
    LoweredPipeline {
        commands,
        pipe_operators,
        combinator: p.combinator,
        background: p.background,
    }
}

pub fn lower_command(c: &Command, scope: &Scope) -> LoweredCommand {
    if let Some(lowered) = builtins::lower_builtin(c, scope) {
        return lowered;
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
