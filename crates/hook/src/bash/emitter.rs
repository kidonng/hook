use super::ir::*;
use fish_parser::ast::{Combinator, RedirectMode};

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
            out.push_str(&pad);
            emit_pipeline(p, out);
            out.push('\n');
        }
        LoweredStatement::If(i) => {
            out.push_str(&pad);
            out.push_str("if ");
            emit_pipeline(&i.condition, out);
            out.push_str("; then\n");
            emit_statements(&i.then_body, indent + 1, out);
            for (cond, body) in &i.elif_branches {
                out.push_str(&pad);
                out.push_str("elif ");
                emit_pipeline(cond, out);
                out.push_str("; then\n");
                emit_statements(body, indent + 1, out);
            }
            if let Some(else_body) = &i.else_body {
                out.push_str(&pad);
                out.push_str("else\n");
                emit_statements(else_body, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str("fi\n");
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
            out.push_str("done\n");
        }
        LoweredStatement::While(w) => {
            out.push_str(&pad);
            out.push_str("while ");
            emit_pipeline(&w.condition, out);
            out.push_str("; do\n");
            emit_statements(&w.body, indent + 1, out);
            out.push_str(&pad);
            out.push_str("done\n");
        }
        LoweredStatement::Function(f) => {
            out.push_str(&pad);
            out.push_str(&format!("{}() {{\n", f.name));
            for (idx, arg) in f.named_args.iter().enumerate() {
                out.push_str(&format!("{}  local {}=\"${}\"\n", pad, arg, idx + 1));
            }
            emit_statements(&f.body, indent + 1, out);
            out.push_str(&pad);
            out.push_str("}\n");
        }
        LoweredStatement::BeginBlock(b) => {
            out.push_str(&pad);
            out.push_str("{\n");
            emit_statements(&b.body, indent + 1, out);
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
            if let Some(v) = values.first() {
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

fn emit_pipeline(p: &LoweredPipeline, out: &mut String) {
    match p.combinator {
        Combinator::And => out.push_str("&& "),
        Combinator::Or => out.push_str("|| "),
        Combinator::None => {}
    }
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
        RedirectMode::Output => out.push('>'),
        RedirectMode::Append => out.push_str(">>"),
        RedirectMode::Input => out.push('<'),
        RedirectMode::OutputAndErr => out.push('>'), // Bash 3.2: > target 2>&1
        RedirectMode::AppendAndErr => out.push_str(">>"),
        RedirectMode::DupOutput => out.push_str(">&"),
        RedirectMode::DupInput => out.push_str("<&"),
    }
    out.push(' ');
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
        LoweredWordPart::CommandSubst { stmts, .. } => {
            out.push_str("$(");
            let mut inner = String::new();
            emit_statements(stmts, 0, &mut inner);
            out.push_str(inner.trim_end());
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
                emit_word_part(p, out);
            }
            out.push('\"');
        }
        LoweredWordPart::Variable(v) => emit_variable_ref(v, out),
        LoweredWordPart::CommandSubst { stmts, quoted } => {
            if *quoted {
                out.push_str("\"$(");
            } else {
                out.push_str("$(");
            }
            let mut inner = String::new();
            emit_statements(stmts, 0, &mut inner);
            out.push_str(inner.trim_end());
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
        LoweredVariableRef::Custom { name, subscript } => {
            match subscript {
                None => out.push_str(&format!("${}", name)),
                Some(BashSubscript::All) => out.push_str(&format!("${{{}[@]}}", name)),
                Some(BashSubscript::ZeroBasedIndex(idx)) => out.push_str(&format!("${{{}[{}]}}", name, idx)),
                Some(BashSubscript::NegativeOffsetFromLength(k)) => {
                    out.push_str(&format!("${{{}[$((${{#{}[@]}}-{}))]}}", name, name, k));
                }
                Some(BashSubscript::Range { offset, length }) => {
                    out.push_str(&format!("${{{}[@]:{}:{}}}", name, offset, length));
                }
                Some(BashSubscript::OpenRange { offset }) => {
                    out.push_str(&format!("${{{}[@]:{}}}", name, offset));
                }
            }
        }
    }
}

pub fn emit_variable_ref(v: &LoweredVariableRef, out: &mut String) {
    match v {
        LoweredVariableRef::Status => out.push_str("$?"),
        LoweredVariableRef::Pipestatus => out.push_str("\"${PIPESTATUS[@]}\""),
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
        LoweredVariableRef::Custom { name, subscript } => {
            match subscript {
                None => out.push_str(&format!("\"${}\"", name)),
                Some(BashSubscript::All) => out.push_str(&format!("\"${{{}[@]}}\"", name)),
                Some(BashSubscript::ZeroBasedIndex(idx)) => out.push_str(&format!("\"${{{}[{}]}}\"", name, idx)),
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
            }
        }
    }
}
