use crate::bash::ir::*;
use fish_parser::ast::{Combinator, RedirectMode, Slice, SliceIndex};

pub fn emit_bash(program: &LoweredProgram) -> String {
    let mut out = String::new();
    if let Some(shebang) = &program.shebang {
        if shebang.contains("fish") {
            out.push_str("#!/usr/bin/env bash\n");
        } else {
            out.push_str(shebang);
            out.push('\n');
        }
    }

    emit_statements(&program.statements, 0, &mut out);
    out
}

pub fn emit_statements(stmts: &[LoweredStatement], indent: usize, out: &mut String) {
    for stmt in stmts {
        emit_statement(stmt, indent, out);
    }
}

fn emit_statement(stmt: &LoweredStatement, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    match stmt {
        LoweredStatement::Comment(c) => {
            out.push_str(&pad);
            out.push('#');
            out.push_str(c);
            out.push('\n');
        }
        LoweredStatement::Return(w) => {
            out.push_str(&pad);
            out.push_str("return");
            if let Some(w) = w {
                out.push(' ');
                emit_word(w, out);
            }
            out.push('\n');
        }
        LoweredStatement::Break => {
            out.push_str(&pad);
            out.push_str("break\n");
        }
        LoweredStatement::Continue => {
            out.push_str(&pad);
            out.push_str("continue\n");
        }
        LoweredStatement::Assignment(assign) => {
            out.push_str(&pad);
            emit_assignment(assign, out);
            out.push('\n');
        }
        LoweredStatement::Pipeline(p) => {
            if p.combinator != Combinator::None && out.ends_with('\n') {
                out.pop();
                match p.combinator {
                    Combinator::And => out.push_str(" && "),
                    Combinator::Or => out.push_str(" || "),
                    Combinator::None => {}
                }
                emit_pipeline_commands(p, out);
                out.push('\n');
            } else {
                out.push_str(&pad);
                emit_pipeline(p, out);
                out.push('\n');
            }
        }
        LoweredStatement::If(i) => {
            out.push_str(&pad);
            out.push_str("if ");
            emit_pipeline_chain(&i.condition, out);
            out.push_str("; then\n");
            emit_statements(&i.then_body, indent + 1, out);
            for (cond, body) in &i.elif_branches {
                out.push_str(&pad);
                out.push_str("elif ");
                emit_pipeline_chain(cond, out);
                out.push_str("; then\n");
                emit_statements(body, indent + 1, out);
            }
            if let Some(else_body) = &i.else_body {
                out.push_str(&pad);
                out.push_str("else\n");
                emit_statements(else_body, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str("fi");
            for redir in &i.redirections {
                out.push(' ');
                emit_redirection(redir, out);
            }
            out.push('\n');
        }
        LoweredStatement::Switch(s) => {
            out.push_str(&pad);
            out.push_str("case ");
            emit_word(&s.value, out);
            out.push_str(" in\n");
            for clause in &s.cases {
                out.push_str(&format!("{}  ", pad));
                for (idx, pat) in clause.patterns.iter().enumerate() {
                    if idx > 0 {
                        out.push('|');
                    }
                    emit_word(pat, out);
                }
                out.push_str(")\n");
                emit_statements(&clause.body, indent + 2, out);
                out.push_str(&format!("{}    ;;\n", pad));
            }
            out.push_str(&pad);
            out.push_str("esac\n");
        }
        LoweredStatement::For(f) => {
            out.push_str(&pad);
            out.push_str(&format!("for {} in ", f.variable));
            for (idx, val) in f.values.iter().enumerate() {
                if idx > 0 {
                    out.push(' ');
                }
                emit_word(val, out);
            }
            out.push_str("; do\n");
            emit_statements(&f.body, indent + 1, out);
            out.push_str(&pad);
            out.push_str("done");
            for redir in &f.redirections {
                out.push(' ');
                emit_redirection(redir, out);
            }
            out.push('\n');
        }
        LoweredStatement::While(w) => {
            out.push_str(&pad);
            out.push_str("while ");
            emit_pipeline_chain(&w.condition, out);
            out.push_str("; do\n");
            emit_statements(&w.body, indent + 1, out);
            out.push_str(&pad);
            out.push_str("done");
            for redir in &w.redirections {
                out.push(' ');
                emit_redirection(redir, out);
            }
            out.push('\n');
        }
        LoweredStatement::Function(f) => {
            out.push_str(&pad);
            out.push_str(&format!("{}() {{\n", f.name));
            for (idx, arg) in f.named_args.iter().enumerate() {
                out.push_str(&format!("{}  local {}=\"${}\"\n", pad, arg, idx + 1));
            }
            let has_executable = !f.named_args.is_empty()
                || f.body
                    .iter()
                    .any(|s| !matches!(s, LoweredStatement::Comment(_)));
            emit_statements(&f.body, indent + 1, out);
            if !has_executable {
                out.push_str(&format!("{}  :\n", pad));
            }
            out.push_str(&pad);
            out.push_str("}\n");
        }
        LoweredStatement::BeginBlock(b) => {
            if b.combinator != Combinator::None && out.ends_with('\n') {
                out.pop();
                match b.combinator {
                    Combinator::And => out.push_str(" && {\n"),
                    Combinator::Or => out.push_str(" || {\n"),
                    Combinator::None => out.push_str(" {\n"),
                }
            } else {
                out.push_str(&pad);
                match b.combinator {
                    Combinator::And => out.push_str("&& "),
                    Combinator::Or => out.push_str("|| "),
                    Combinator::None => {}
                }
                out.push_str("{\n");
            }
            let has_executable = b
                .body
                .iter()
                .any(|s| !matches!(s, LoweredStatement::Comment(_)));
            emit_statements(&b.body, indent + 1, out);
            if !has_executable {
                out.push_str(&format!("{}  :\n", pad));
            }
            out.push_str(&pad);
            out.push('}');
            for redir in &b.redirections {
                out.push(' ');
                emit_redirection(redir, out);
            }
            out.push('\n');
        }
    }
}

fn emit_assignment(assign: &AssignmentIR, out: &mut String) {
    match assign {
        AssignmentIR::Local { name, values } => {
            if values.len() <= 1 {
                out.push_str(&format!("local {}=\"", name));
                if let Some(v) = values.first() {
                    emit_word_inner(v, out);
                }
                out.push('\"');
            } else {
                out.push_str(&format!("local -a {}=(", name));
                emit_quoted_values(values, out);
                out.push(')');
            }
        }
        AssignmentIR::Export { name, values } => {
            out.push_str(&format!("export {}=\"", name));
            let sep = if name == "PATH" { ":" } else { " " };
            for (idx, v) in values.iter().enumerate() {
                if idx > 0 {
                    out.push_str(sep);
                }
                emit_word_inner(v, out);
            }
            out.push('\"');
        }
        AssignmentIR::Global { name, values } => {
            if values.len() <= 1 {
                out.push_str(&format!("{}=\"", name));
                if let Some(v) = values.first() {
                    emit_word_inner(v, out);
                }
                out.push('\"');
            } else {
                out.push_str(&format!("{}=(", name));
                emit_quoted_values(values, out);
                out.push(')');
            }
        }
        AssignmentIR::Append { name, values } => {
            out.push_str(&format!("{}+=(", name));
            emit_quoted_values(values, out);
            out.push(')');
        }
        AssignmentIR::Prepend { name, values } => {
            out.push_str(&format!("{}=(", name));
            emit_quoted_values(values, out);
            out.push_str(&format!(" \"${{{}[@]}}\")", name));
        }
        AssignmentIR::Erase { name } => {
            out.push_str(&format!("unset {}", name));
        }
        AssignmentIR::ArgvLast { value } => {
            out.push_str("set -- \"${@:1:$#-1}\" \"");
            emit_word_inner(value, out);
            out.push('\"');
        }
        AssignmentIR::SliceAssign { name, index, value } => {
            match index {
                SliceIndexIR::ZeroBased(idx) => {
                    out.push_str(&format!("{}[{}]=\"", name, idx));
                }
                SliceIndexIR::NegativeOffset(k) => {
                    out.push_str(&format!("{}[$((${{#{}[@]}}-{}))]=\"", name, name, k));
                }
                SliceIndexIR::Dynamic(var_name) => {
                    out.push_str(&format!("{}[$(({} - 1))]=\"", name, var_name));
                }
            }
            emit_word_inner(value, out);
            out.push('\"');
        }
        AssignmentIR::SliceErase { name, index } => match index {
            SliceIndexIR::ZeroBased(idx) => {
                out.push_str(&format!("unset '{}[{}]'", name, idx));
            }
            SliceIndexIR::NegativeOffset(k) => {
                out.push_str(&format!("unset \"{}[$((${{#{}[@]}}-{}))]\"", name, name, k));
            }
            SliceIndexIR::Dynamic(var_name) => {
                out.push_str(&format!("unset \"{}[$(({} - 1))]\"", name, var_name));
            }
        },
    }
}

fn emit_quoted_values(values: &[LoweredWord], out: &mut String) {
    for (idx, val) in values.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        out.push('\"');
        emit_word_inner(val, out);
        out.push('\"');
    }
}

fn emit_pipeline_chain(pipelines: &[LoweredPipeline], out: &mut String) {
    for (idx, p) in pipelines.iter().enumerate() {
        if idx > 0 {
            match p.combinator {
                Combinator::And => out.push_str(" && "),
                Combinator::Or => out.push_str(" || "),
                Combinator::None => out.push_str("; "),
            }
        }
        emit_pipeline_commands(p, out);
    }
}

fn emit_pipeline_commands(p: &LoweredPipeline, out: &mut String) {
    for (idx, cmd) in p.commands.iter().enumerate() {
        if idx > 0 {
            out.push_str(" | ");
        }
        emit_command(cmd, out);
    }
    if p.background {
        out.push_str(" &");
    }
}

fn emit_pipeline(p: &LoweredPipeline, out: &mut String) {
    match p.combinator {
        Combinator::And => out.push_str("&& "),
        Combinator::Or => out.push_str("|| "),
        Combinator::None => {}
    }
    emit_pipeline_commands(p, out);
}

fn emit_command_subst_slices(slices: &[Slice], out: &mut String) {
    for s in slices {
        match s {
            Slice::Index(SliceIndex::Pos(1)) => {
                out.push_str(" | head -n 1");
            }
            Slice::Index(SliceIndex::Pos(n)) => {
                out.push_str(&format!(" | sed -n '{}p'", n));
            }
            Slice::Index(SliceIndex::Neg(1)) => {
                out.push_str(" | tail -n 1");
            }
            Slice::Index(SliceIndex::Neg(n)) => {
                out.push_str(&format!(" | tail -n {}", n));
            }
            Slice::Range {
                start: Some(SliceIndex::Pos(s)),
                end: Some(SliceIndex::Pos(e)),
            } => {
                out.push_str(&format!(" | sed -n '{},{}p'", s, e));
            }
            Slice::Range { .. } => {}
            _ => {}
        }
    }
}

fn emit_command(cmd: &LoweredCommand, out: &mut String) {
    if cmd.negate {
        out.push_str("! ");
    }
    for (idx, arg) in cmd.args.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        emit_word(arg, out);
    }
    for redir in &cmd.redirections {
        out.push(' ');
        emit_redirection(redir, out);
    }
}

fn emit_redirection(r: &LoweredRedirection, out: &mut String) {
    if let Some(fd) = r.fd {
        out.push_str(&fd.to_string());
    }
    match r.mode {
        RedirectMode::Output => {
            out.push('>');
            out.push(' ');
        }
        RedirectMode::Append => {
            out.push_str(">>");
            out.push(' ');
        }
        RedirectMode::Input => {
            out.push('<');
            out.push(' ');
        }
        RedirectMode::OutputAndErr => {
            out.push('>');
            out.push(' ');
        }
        RedirectMode::AppendAndErr => {
            out.push_str(">>");
            out.push(' ');
        }
        RedirectMode::DupOutput => out.push_str(">&"),
        RedirectMode::DupInput => out.push_str("<&"),
    }
    emit_word(&r.target, out);
    if r.mode == RedirectMode::OutputAndErr || r.mode == RedirectMode::AppendAndErr {
        out.push_str(" 2>&1");
    }
}

pub fn emit_word(w: &LoweredWord, out: &mut String) {
    for part in &w.parts {
        emit_word_part(part, out);
    }
}

fn emit_word_inner(w: &LoweredWord, out: &mut String) {
    for part in &w.parts {
        emit_word_part_inner(part, out);
    }
}

fn emit_word_part_inner(part: &LoweredWordPart, out: &mut String) {
    match part {
        LoweredWordPart::Literal(s) => out.push_str(s),
        LoweredWordPart::SingleQuoted(s) => out.push_str(s),
        LoweredWordPart::DoubleQuoted(parts) => {
            for p in parts {
                emit_word_part_inner(p, out);
            }
        }
        LoweredWordPart::Variable(v) => emit_variable_ref_inner(v, out),
        LoweredWordPart::CommandSubst { stmts, slices, .. } => {
            out.push_str("$(");
            let mut inner = String::new();
            emit_statements(stmts, 0, &mut inner);
            out.push_str(inner.trim_end());
            emit_command_subst_slices(slices, out);
            out.push(')');
        }
        LoweredWordPart::ProcessSubst(pipeline) => {
            out.push_str("<(");
            emit_pipeline(pipeline, out);
            out.push(')');
        }
        LoweredWordPart::BraceExpansion(words) => {
            out.push('{');
            for (idx, w) in words.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                emit_word(w, out);
            }
            out.push('}');
        }
    }
}

pub fn emit_word_part(part: &LoweredWordPart, out: &mut String) {
    match part {
        LoweredWordPart::Literal(s) => out.push_str(s),
        LoweredWordPart::SingleQuoted(s) => {
            out.push('\'');
            out.push_str(s);
            out.push('\'');
        }
        LoweredWordPart::DoubleQuoted(parts) => {
            out.push('\"');
            for p in parts {
                emit_word_part_inner(p, out);
            }
            out.push('\"');
        }
        LoweredWordPart::Variable(v) => emit_variable_ref(v, out),
        LoweredWordPart::CommandSubst {
            stmts,
            slices,
            quoted,
        } => {
            if *quoted {
                out.push_str("\"$(");
            } else {
                out.push_str("$(");
            }
            let mut inner = String::new();
            emit_statements(stmts, 0, &mut inner);
            out.push_str(inner.trim_end());
            emit_command_subst_slices(slices, out);
            if *quoted {
                out.push_str(")\"");
            } else {
                out.push(')');
            }
        }
        LoweredWordPart::ProcessSubst(pipeline) => {
            out.push_str("<(");
            emit_pipeline(pipeline, out);
            out.push(')');
        }
        LoweredWordPart::BraceExpansion(words) => {
            out.push('{');
            for (idx, w) in words.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                emit_word(w, out);
            }
            out.push('}');
        }
    }
}

fn emit_variable_ref_inner(v: &LoweredVariableRef, out: &mut String) {
    match v {
        LoweredVariableRef::Status => out.push_str("$?"),
        LoweredVariableRef::Pipestatus => out.push_str("${PIPESTATUS[@]}"),
        LoweredVariableRef::FishPid => out.push_str("$$"),
        LoweredVariableRef::LastPid => out.push_str("$!"),
        LoweredVariableRef::ArgvAll => out.push_str("$@"),
        LoweredVariableRef::ArgvIndex(n) => out.push_str(&format!("${}", n)),
        LoweredVariableRef::ArgvSlice { start, len } => {
            if let Some(length) = len {
                out.push_str(&format!("${{@:{}:{}}}", start, length));
            } else {
                out.push_str(&format!("${{@:{}}}", start));
            }
        }
        LoweredVariableRef::ArgvLast => out.push_str("${@: -1:1}"),
        LoweredVariableRef::ArgvDynamic(idx) => {
            out.push_str(&format!("${{@:{}:1}}", idx));
        }
        LoweredVariableRef::ArgvDynamicRange { start, end } => {
            out.push_str(&format!("${{@:{}:$((({} - {}) + 1))}}", start, end, start));
        }
        LoweredVariableRef::Indirect { name } => {
            out.push_str(&format!("${{!{}}}", name));
        }
        LoweredVariableRef::Custom { name, subscript } => match subscript {
            None => out.push_str(&format!("${}", name)),
            Some(BashSubscript::All) => out.push_str(&format!("${{{}[@]}}", name)),
            Some(BashSubscript::ZeroBasedIndex(idx)) => {
                out.push_str(&format!("${{{}[{}]}}", name, idx))
            }
            Some(BashSubscript::NegativeOffsetFromLength(k)) => {
                out.push_str(&format!("${{{}[$((${{#{}[@]}}-{}))]}}", name, name, k));
            }
            Some(BashSubscript::Range { offset, length }) => {
                out.push_str(&format!("${{{}[@]:{}:{}}}", name, offset, length));
            }
            Some(BashSubscript::OpenRange { offset }) => {
                out.push_str(&format!("${{{}[@]:{}}}", name, offset));
            }
            Some(BashSubscript::DynamicVariable(var_name)) => {
                out.push_str(&format!("${{{}[$(({} - 1))]}}", name, var_name));
            }
            Some(BashSubscript::DynamicRange { start, end }) => {
                out.push_str(&format!(
                    "${{{}[@]:$(({} - 1)):$(({} - {} + 1))}}",
                    name, start, end, start
                ));
            }
        },
    }
}

pub fn emit_variable_ref(v: &LoweredVariableRef, out: &mut String) {
    match v {
        LoweredVariableRef::Status => out.push_str("$?"),
        LoweredVariableRef::Pipestatus => out.push_str("\"${PIPESTATUS[@]}\""),
        LoweredVariableRef::FishPid => out.push_str("\"$$\""),
        LoweredVariableRef::LastPid => out.push_str("\"$!\""),
        LoweredVariableRef::ArgvAll => out.push_str("\"$@\""),
        LoweredVariableRef::ArgvIndex(n) => out.push_str(&format!("\"${}\"", n)),
        LoweredVariableRef::ArgvSlice { start, len } => {
            if let Some(length) = len {
                out.push_str(&format!("\"${{@:{}:{}}}\"", start, length));
            } else {
                out.push_str(&format!("\"${{@:{}}}\"", start));
            }
        }
        LoweredVariableRef::ArgvLast => out.push_str("\"${@: -1:1}\""),
        LoweredVariableRef::ArgvDynamic(idx) => {
            out.push_str(&format!("\"${{@:{}:1}}\"", idx));
        }
        LoweredVariableRef::ArgvDynamicRange { start, end } => {
            out.push_str(&format!(
                "\"${{@:{}:$((({} - {}) + 1))}}\"",
                start, end, start
            ));
        }
        LoweredVariableRef::Indirect { name } => {
            out.push_str(&format!("\"${{!{}}}\"", name));
        }
        LoweredVariableRef::Custom { name, subscript } => {
            match subscript {
                None => out.push_str(&format!("\"${}\"", name)),
                Some(BashSubscript::All) => out.push_str(&format!("\"${{{}[@]}}\"", name)),
                Some(BashSubscript::ZeroBasedIndex(idx)) => {
                    out.push_str(&format!("\"${{{}[{}]}}\"", name, idx))
                }
                Some(BashSubscript::NegativeOffsetFromLength(k)) => {
                    // Bash 3.2 safe dynamic array length arithmetic
                    out.push_str(&format!("\"${{{}[$((${{#{}[@]}}-{}))]}}\"", name, name, k));
                }
                Some(BashSubscript::Range { offset, length }) => {
                    out.push_str(&format!("\"${{{}[@]:{}:{}}}\"", name, offset, length));
                }
                Some(BashSubscript::OpenRange { offset }) => {
                    out.push_str(&format!("\"${{{}[@]:{}}}\"", name, offset));
                }
                Some(BashSubscript::DynamicVariable(var_name)) => {
                    out.push_str(&format!("\"${{{}[$(({} - 1))]}}\"", name, var_name));
                }
                Some(BashSubscript::DynamicRange { start, end }) => {
                    out.push_str(&format!(
                        "\"${{{}[@]:$(({} - 1)):$(({} - {} + 1))}}\"",
                        name, start, end, start
                    ));
                }
            }
        }
    }
}
