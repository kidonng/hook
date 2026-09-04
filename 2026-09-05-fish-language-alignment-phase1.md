# Fish Language Alignment (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align `hook` and `fish-parser` with Fish shell scripting language documentation (`language.rst`) Phase 1: double-quote command substitution, merged stdout/stderr pipes (`&|` / `|&`), block-level redirections on loops and conditionals, permissive function options, and built-in process variables.

**Architecture:** Extend PEG grammar and AST in `crates/fish-parser`, adjust Lowering and IR in `crates/hook` to map Fish idioms onto Bash 3.2+ compatible constructs, and emit formatted Bash code verified with `bash -n`.

**Tech Stack:** Rust (2024 edition), `rust-peg`, `serde`, Bash 3.2+

**Spec:** `2026-09-05-fish-language-alignment-phase1-design.md`

## Global Constraints

- Must target Bash 3.2+ compatibility (no Bash 4+ only features like `|&` or `readarray`).
- All tests must pass with `cargo test`.
- All emitted Bash scripts must validate cleanly with `bash -n`.
- Zero compiler warnings (`cargo clippy`) and clean formatting (`cargo fmt --check`).

---

### Task 1: Double-Quoted Command Substitution Semantics

**Files:**
- Modify: `crates/fish-parser/src/grammar.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- Grammar rule `command_subst()` continues to parse `$(...)` and `(...)` in unquoted contexts.
- New grammar rule `dollar_command_subst()` only parses `$(...) [slices]*`.
- `double_quoted_part` uses `dollar_command_subst` so that bare `"(pwd)"` produces `WordPart::Literal("(pwd)")` instead of `CommandSubst`.

- [ ] **Step 1: Write the failing tests**

In `crates/fish-parser/tests/parser_test.rs`:
```rust
#[test]
fn test_parse_double_quoted_command_substitution_rules() {
    // $(cmd) inside double quotes MUST parse as CommandSubst
    let p1 = parse("echo \"$(pwd)\"\n").unwrap();
    let stmt1 = &p1.statements[0];
    if let Statement::Pipeline(pipe) = stmt1 {
        let arg = &pipe.commands[0].args[1];
        if let WordPart::DoubleQuoted(parts) = &arg.parts[0] {
            assert!(matches!(parts[0], WordPart::CommandSubst { .. }));
        } else {
            panic!("expected DoubleQuoted");
        }
    }

    // Bare (cmd) inside double quotes MUST NOT parse as CommandSubst
    let p2 = parse("echo \"(pwd)\"\n").unwrap();
    let stmt2 = &p2.statements[0];
    if let Statement::Pipeline(pipe) = stmt2 {
        let arg = &pipe.commands[0].args[1];
        if let WordPart::DoubleQuoted(parts) = &arg.parts[0] {
            assert_eq!(parts[0], WordPart::Literal("(pwd)".to_string()));
        } else {
            panic!("expected DoubleQuoted");
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test test_parse_double_quoted_command_substitution_rules`
Expected: FAIL on `assert_eq!(parts[0], WordPart::Literal("(pwd)".to_string()))`.

- [ ] **Step 3: Update `grammar.rs`**

In `crates/fish-parser/src/grammar.rs`:
```rust
rule double_quoted_part() -> WordPart
    = variable_ref()
    / dollar_command_subst()
    / s:$(( [^'\"' | '$' | '\\'] / ("\\" [_]) )+) {
        WordPart::Literal(unescape(s))
    }

rule dollar_command_subst() -> WordPart
    = "$(" _* stmts:statement_list() _* ")" slices:slice()* {
        WordPart::CommandSubst { statements: stmts, slices }
    }

rule command_subst() -> WordPart
    = dollar_command_subst()
    / "(" _* stmts:statement_list() _* ")" slices:slice()* {
        WordPart::CommandSubst { statements: stmts, slices }
    }
```
Note: remove `'('` from the exclusion character set of `double_quoted_part` literal matcher so `(` can be matched as literal inside double quotes.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fish-parser`
Expected: PASS

- [ ] **Step 5: Add transpiler end-to-end test**

In `crates/hook/tests/transpile_test.rs`:
```rust
#[test]
fn test_transpile_double_quoted_parens_literal() {
    let bash = transpile("echo \"(pwd)\"\necho \"$(pwd)\"\n");
    assert_eq!(bash, "echo \"(pwd)\"\necho \"$(pwd)\"\n");
}
```
Run: `cargo test -p hook`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/fish-parser/src/grammar.rs crates/fish-parser/tests/parser_test.rs crates/hook/tests/transpile_test.rs
git commit -m "fix: disallow bare paren command substitution inside double quotes"
```

---

### Task 2: Support Merged Output Pipes (`&|` and `|&`)

**Files:**
- Modify: `crates/fish-parser/src/grammar.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `pipe_sep` recognizes `&|` and `|&` as well as `|`.
- When separated by `&|` or `|&`, the preceding command automatically has `2>&1` appended as a redirection (`Redirection { fd: Some(2), mode: RedirectMode::DupOutput, target: Word::from_literal("1") }`).
- Existing `emitter.rs` automatically formats this as `cmd1 2>&1 | cmd2`, which is fully Bash 3.2 compatible without requiring Bash 4.0's `|&`.

- [ ] **Step 1: Write the failing tests**

In `crates/fish-parser/tests/parser_test.rs`:
```rust
#[test]
fn test_parse_merged_pipes() {
    for input in &["make &| less\n", "make |& less\n"] {
        let program = parse(input).unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Statement::Pipeline(pipe) = &program.statements[0] {
            assert_eq!(pipe.commands.len(), 2);
            let cmd1 = &pipe.commands[0];
            assert!(cmd1.redirections.iter().any(|r| r.fd == Some(2) && r.mode == RedirectMode::DupOutput));
        } else {
            panic!("expected pipeline");
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test test_parse_merged_pipes`
Expected: FAIL (syntax error on `&|` / `|&`).

- [ ] **Step 3: Update `grammar.rs`**

In `crates/fish-parser/src/grammar.rs`:
```rust
rule pipe_op() -> bool
    = ("&|" / "|&") { true }
    / "|" { false }

rule pipe_sep() -> bool
    = _* p:pipe_op() cont_space() { p }
```
Update `pipeline()` rule in `grammar.rs`:
Parse commands with their connecting pipe separator:
```rust
rule pipeline() -> Pipeline
    = !reserved_keyword() comb:combinator_prefix()? _* negate:negate()? head:command() tail:(sep:pipe_sep() cmd:command() { (sep, cmd) })* bg:(_* "&" !['&' | '>'])? {
        let mut commands = vec![head];
        for (is_merged, mut next_cmd) in tail {
            if is_merged {
                if let Some(prev) = commands.last_mut() {
                    prev.redirections.push(Redirection {
                        fd: Some(2),
                        mode: RedirectMode::DupOutput,
                        target: Word::from_literal("1"),
                    });
                }
            }
            commands.push(next_cmd);
        }
        if let Some(true) = negate.map(|_| true) {
            if let Some(first) = commands.first_mut() {
                first.negate = true;
            }
        }
        Pipeline {
            commands,
            combinator: comb.unwrap_or(Combinator::None),
            background: bg.is_some(),
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fish-parser --test parser_test test_parse_merged_pipes`
Expected: PASS

- [ ] **Step 5: Add transpiler end-to-end test**

In `crates/hook/tests/transpile_test.rs`:
```rust
#[test]
fn test_transpile_merged_pipes() {
    let bash1 = transpile("make &| less\n");
    assert_eq!(bash1, "make 2>&1 | less\n");

    let bash2 = transpile("make |& less\n");
    assert_eq!(bash2, "make 2>&1 | less\n");
}
```
Run: `cargo test -p hook`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/fish-parser/src/grammar.rs crates/fish-parser/tests/parser_test.rs crates/hook/tests/transpile_test.rs
git commit -m "feat: support &| and |& merged pipes transpiled to Bash 3.2 2>&1 |"
```

---

### Task 3: Block-Level Redirections on `while`, `for`, and `if` Statements

**Files:**
- Modify: `crates/fish-parser/src/ast.rs`
- Modify: `crates/fish-parser/src/grammar.rs`
- Modify: `crates/hook/src/bash/ir.rs`
- Modify: `crates/hook/src/bash/lowering.rs`
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `IfStatement`, `ForStatement`, `WhileStatement` AST structs gain `pub redirections: Vec<Redirection>`.
- `LoweredIf`, `LoweredFor`, `LoweredWhile` IR structs gain `pub redirections: Vec<LoweredRedirection>`.
- `lowering.rs` lowers the AST redirections to LoweredRedirection.
- `emitter.rs` outputs redirections on `fi` and `done`.

- [ ] **Step 1: Write the failing tests**

In `crates/fish-parser/tests/parser_test.rs`:
```rust
#[test]
fn test_parse_block_redirections() {
    let input = "while read -l line\n echo $line\n end < input.txt\n";
    let program = parse(input).unwrap();
    if let Statement::While(w) = &program.statements[0] {
        assert_eq!(w.redirections.len(), 1);
        assert_eq!(w.redirections[0].mode, RedirectMode::Input);
    } else {
        panic!("expected while statement");
    }

    let input_for = "for x in 1 2 3\n echo $x\n end > output.txt\n";
    let program_for = parse(input_for).unwrap();
    if let Statement::For(f) = &program_for.statements[0] {
        assert_eq!(f.redirections.len(), 1);
        assert_eq!(f.redirections[0].mode, RedirectMode::Output);
    } else {
        panic!("expected for statement");
    }

    let input_if = "if test -e file\n echo yes\n end 2>/dev/null\n";
    let program_if = parse(input_if).unwrap();
    if let Statement::If(i) = &program_if.statements[0] {
        assert_eq!(i.redirections.len(), 1);
        assert_eq!(i.redirections[0].fd, Some(2));
    } else {
        panic!("expected if statement");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test test_parse_block_redirections`
Expected: FAIL (AST fields missing & parse errors on `end < input.txt`).

- [ ] **Step 3: Update `crates/fish-parser/src/ast.rs` and `grammar.rs`**

In `crates/fish-parser/src/ast.rs`:
Add `pub redirections: Vec<Redirection>` to `IfStatement`, `ForStatement`, `WhileStatement`.

In `crates/fish-parser/src/grammar.rs`:
Update `if_stmt`:
```rust
rule if_stmt() -> Statement
    = "if" !keyword_char() _+ cond:pipeline_chain() statement_sep()
      then_body:statement_list()
      elifs:elif_branch()*
      else_body:else_branch()?
      "end" !keyword_char() redirs:(_* r:redirection() { r })* {
        Statement::If(IfStatement {
            condition: cond,
            then_body,
            elif_branches: elifs,
            else_body,
            redirections: redirs,
        })
    }
```
Update `for_stmt`:
```rust
rule for_stmt() -> Statement
    = "for" !keyword_char() _+ var:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) _+ "in" !keyword_char() _+ vals:(word() ++ (_+)) statement_sep()
      body:statement_list()
      "end" !keyword_char() redirs:(_* r:redirection() { r })* {
        Statement::For(ForStatement {
            variable: var.to_string(),
            values: vals,
            body,
            redirections: redirs,
        })
    }
```
Update `while_stmt`:
```rust
rule while_stmt() -> Statement
    = "while" !keyword_char() _+ cond:pipeline_chain() statement_sep()
      body:statement_list()
      "end" !keyword_char() redirs:(_* r:redirection() { r })* {
        Statement::While(WhileStatement {
            condition: cond,
            body,
            redirections: redirs,
        })
    }
```

- [ ] **Step 4: Update `crates/hook/src/bash/ir.rs`, `lowering.rs`, and `emitter.rs`**

In `crates/hook/src/bash/ir.rs`:
Add `pub redirections: Vec<LoweredRedirection>` to `LoweredIf`, `LoweredFor`, `LoweredWhile`.

In `crates/hook/src/bash/lowering.rs`:
In `lower_statement`:
```rust
Statement::If(i) => LoweredStatement::If(LoweredIf {
    condition: i.condition.iter().map(|p| lower_pipeline(p, scope)).collect(),
    then_body: lower_statements(&i.then_body, scope),
    elif_branches: i.elif_branches.iter().map(|(p, b)| (p.iter().map(|pl| lower_pipeline(pl, scope)).collect(), lower_statements(b, scope))).collect(),
    else_body: i.else_body.as_ref().map(|b| lower_statements(b, scope)),
    redirections: i.redirections.iter().map(|r| lower_redirection(r, scope)).collect(),
}),
Statement::For(f) => {
    let mut val_scope = *scope;
    val_scope.in_for_values = true;
    let values = f.values.iter().map(|w| lower_word(w, &val_scope)).collect();
    LoweredStatement::For(LoweredFor {
        variable: f.variable.clone(),
        values,
        body: lower_statements(&f.body, scope),
        redirections: f.redirections.iter().map(|r| lower_redirection(r, scope)).collect(),
    })
},
Statement::While(w) => LoweredStatement::While(LoweredWhile {
    condition: w.condition.iter().map(|p| lower_pipeline(p, scope)).collect(),
    body: lower_statements(&w.body, scope),
    redirections: w.redirections.iter().map(|r| lower_redirection(r, scope)).collect(),
}),
```

In `crates/hook/src/bash/emitter.rs`:
In `emit_statement`:
For `LoweredIf`:
```rust
out.push_str(&pad);
out.push_str("fi");
for redir in &i.redirections {
    out.push(' ');
    emit_redirection(redir, out);
}
out.push('\n');
```
For `LoweredFor`:
```rust
out.push_str(&pad);
out.push_str("done");
for redir in &f.redirections {
    out.push(' ');
    emit_redirection(redir, out);
}
out.push('\n');
```
For `LoweredWhile`:
```rust
out.push_str(&pad);
out.push_str("done");
for redir in &w.redirections {
    out.push(' ');
    emit_redirection(redir, out);
}
out.push('\n');
```

- [ ] **Step 5: Run tests and fix any compiler issues in tests**

Update any test AST matching in `crates/fish-parser/tests/` where `IfStatement`, `ForStatement`, `WhileStatement` are pattern matched or constructed without `..`.
Run: `cargo test`
Expected: PASS

- [ ] **Step 6: Add transpiler end-to-end tests**

In `crates/hook/tests/transpile_test.rs`:
```rust
#[test]
fn test_transpile_block_redirections() {
    let bash_while = transpile("while read -r line\n  echo \"$line\"\nend < input.txt\n");
    assert!(bash_while.contains("done < input.txt\n"));

    let bash_for = transpile("for x in 1 2 3\n  echo \"$x\"\nend > output.txt\n");
    assert!(bash_for.contains("done > output.txt\n"));

    let bash_if = transpile("if test -e file\n  echo yes\nend 2>/dev/null\n");
    assert!(bash_if.contains("fi 2> /dev/null\n"));
}
```
Run: `cargo test`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/fish-parser crates/hook
git commit -m "feat: support block-level redirections on while, for, and if statements"
```

---

### Task 4: Permissive Function Options

**Files:**
- Modify: `crates/fish-parser/src/grammar.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `func_opt` grammar rule extended to handle additional Fish function declaration options:
  - `-w` / `--wraps` `<word>`
  - `-V` / `--inherit-variable` `<ident>`
  - `-e` / `--on-event` `<ident>`
  - `-s` / `--on-signal` `<ident>`
  - `-v` / `--on-variable` `<ident>`
  - `-j` / `--on-job-exit` `<ident>`
  - `-S` / `--no-scope-shadowing`
- Extensible `FuncOpt::Ignored` variant in `grammar.rs` to allow parsing these attributes without failing.

- [ ] **Step 1: Write the failing tests**

In `crates/fish-parser/tests/parser_test.rs`:
```rust
#[test]
fn test_parse_function_with_extended_options() {
    let fish_code = r#"
function my_git_wrap --wraps git -d "Git wrapper"
    git $argv
end

function my_event_handler -e fish_prompt -S
    echo prompt
end

function my_inherit -V PWD -a name
    echo $name in $PWD
end
"#;
    let program = parse(fish_code).unwrap();
    assert_eq!(program.statements.len(), 3);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test test_parse_function_with_extended_options`
Expected: FAIL (syntax error on `--wraps`).

- [ ] **Step 3: Update `crates/fish-parser/src/grammar.rs`**

In `crates/fish-parser/src/grammar.rs`:
```rust
enum FuncOpt {
    Args(Vec<String>),
    Desc(String),
    Ignored,
}
```
In `grammar fish_grammar()`:
```rust
rule func_opt() -> FuncOpt
    = _+ ("-a" / "--argument-names") _+ names:(ident() ++ (_+)) {
        FuncOpt::Args(names)
    }
    / _+ ("-d" / "--description") _+ w:word() {
        let desc = match &w.parts[0] {
            WordPart::SingleQuoted(s) => s.clone(),
            WordPart::DoubleQuoted(parts) => {
                let mut s = String::new();
                for p in parts {
                    if let WordPart::Literal(lit) = p {
                        s.push_str(lit);
                    }
                }
                s
            }
            WordPart::Literal(s) => s.clone(),
            _ => "".to_string(),
        };
        FuncOpt::Desc(desc)
    }
    / _+ ("-w" / "--wraps") _+ word() { FuncOpt::Ignored }
    / _+ ("-V" / "--inherit-variable") _+ ident() { FuncOpt::Ignored }
    / _+ ("-e" / "--on-event") _+ ident() { FuncOpt::Ignored }
    / _+ ("-s" / "--on-signal") _+ ident() { FuncOpt::Ignored }
    / _+ ("-v" / "--on-variable") _+ ident() { FuncOpt::Ignored }
    / _+ ("-j" / "--on-job-exit") _+ ident() { FuncOpt::Ignored }
    / _+ ("-S" / "--no-scope-shadowing") { FuncOpt::Ignored }
```
In `function_stmt()`:
```rust
for opt in opts {
    match opt {
        FuncOpt::Args(mut a) => named_args.append(&mut a),
        FuncOpt::Desc(d) => description = Some(d),
        FuncOpt::Ignored => {}
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fish-parser --test parser_test test_parse_function_with_extended_options`
Expected: PASS

- [ ] **Step 5: Add transpiler end-to-end test**

In `crates/hook/tests/transpile_test.rs`:
```rust
#[test]
fn test_transpile_function_with_wraps_and_events() {
    let fish = "function g -w git -d 'git alias'\n  git $argv\nend\n";
    let bash = transpile(fish);
    assert_eq!(bash, "g() {\n  git \"$@\"\n}\n");
}
```
Run: `cargo test`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/fish-parser/src/grammar.rs crates/fish-parser/tests/parser_test.rs crates/hook/tests/transpile_test.rs
git commit -m "feat: support permissive fish function options (-w, -V, -e, -s, -v, -j, -S)"
```

---

### Task 5: Special Built-in Process Variables (`$fish_pid`, `$last_pid`)

**Files:**
- Modify: `crates/hook/src/bash/ir.rs`
- Modify: `crates/hook/src/bash/lowering.rs`
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/hook/tests/lowering_test.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `LoweredVariableRef` gains `FishPid` and `LastPid`.
- `lower_variable_ref()` maps `fish_pid` to `LoweredVariableRef::FishPid` and `last_pid` to `LoweredVariableRef::LastPid`.
- `emit_variable_ref()` maps `FishPid` to `"$$"` and `LastPid` to `"$!"`.
- `emit_variable_ref_inner()` maps `FishPid` to `$$` and `LastPid` to `$!`.

- [ ] **Step 1: Write the failing tests**

In `crates/hook/tests/transpile_test.rs`:
```rust
#[test]
fn test_transpile_process_id_variables() {
    let bash = transpile("echo $fish_pid\necho $last_pid\n");
    assert_eq!(bash, "echo \"$$\"\necho \"$!\"\n");

    let bash_quoted = transpile("echo \"PID: $fish_pid, Background: $last_pid\"\n");
    assert_eq!(bash_quoted, "echo \"PID: $$, Background: $!\"\n");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test transpile_test test_transpile_process_id_variables`
Expected: FAIL (currently emits `"$fish_pid"` and `"$last_pid"`).

- [ ] **Step 3: Update `crates/hook/src/bash/ir.rs`**

In `crates/hook/src/bash/ir.rs`:
Add variants to `LoweredVariableRef`:
```rust
pub enum LoweredVariableRef {
    Status,           // $status -> $?
    Pipestatus,       // $pipestatus -> ${PIPESTATUS[@]}
    FishPid,          // $fish_pid -> $$
    LastPid,          // $last_pid -> $!
    ArgvAll,          // $argv -> "$@"
    ArgvIndex(usize), // $argv[1] -> $1
    ...
```

- [ ] **Step 4: Update `crates/hook/src/bash/lowering.rs`**

In `crates/hook/src/bash/lowering.rs`:
In `lower_variable_ref`:
```rust
if v.name == "status" && v.slices.is_empty() {
    return LoweredVariableRef::Status;
}
if v.name == "pipestatus" && v.slices.is_empty() {
    return LoweredVariableRef::Pipestatus;
}
if v.name == "fish_pid" && v.slices.is_empty() {
    return LoweredVariableRef::FishPid;
}
if v.name == "last_pid" && v.slices.is_empty() {
    return LoweredVariableRef::LastPid;
}
```

- [ ] **Step 5: Update `crates/hook/src/bash/emitter.rs`**

In `crates/hook/src/bash/emitter.rs`:
In `emit_variable_ref_inner`:
```rust
LoweredVariableRef::FishPid => out.push_str("$$"),
LoweredVariableRef::LastPid => out.push_str("$!"),
```
In `emit_variable_ref`:
```rust
LoweredVariableRef::FishPid => out.push_str("\"$$\""),
LoweredVariableRef::LastPid => out.push_str("\"$!\""),
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/hook/src/bash/ir.rs crates/hook/src/bash/lowering.rs crates/hook/src/bash/emitter.rs crates/hook/tests/transpile_test.rs
git commit -m "feat: lower \$fish_pid to \$\$ and \$last_pid to \$!"
```

---

### Task 6: Full Verification and Regression Test Suite

**Files:**
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- Comprehensive end-to-end test validating all Phase 1 features together.
- Validates clean execution with `bash -n` and full `cargo clippy` and `cargo fmt`.

- [ ] **Step 1: Add comprehensive Phase 1 alignment test**

In `crates/hook/tests/transpile_test.rs`:
```rust
#[test]
fn test_phase1_alignment_combined() {
    let script = r#"
#!/usr/bin/env fish

function run_pipeline --wraps make -d "Run build pipeline"
    make &| tee build.log
end

function check_services -S
    while read -r service
        if test -n "$service"
            echo "Service running: $service (PID: $fish_pid)"
        end 2>/dev/null
    end < services.txt
end

echo "(pwd)"
echo "$(pwd)"
echo "Background PID: $last_pid"
"#;

    let bash = transpile(script);
    assert!(bash.starts_with("#!/usr/bin/env bash\n"));
    assert!(bash.contains("make 2>&1 | tee build.log"));
    assert!(bash.contains("while read -r service; do"));
    assert!(bash.contains("done < services.txt"));
    assert!(bash.contains("fi 2> /dev/null"));
    assert!(bash.contains("echo \"(pwd)\""));
    assert!(bash.contains("echo \"$(pwd)\""));
    assert!(bash.contains("echo \"Background PID: $!\""));
}
```

- [ ] **Step 2: Run all workspace tests, clippy, and fmt**

Run:
```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
Expected: All checks PASS with 0 warnings.

- [ ] **Step 3: Commit**

```bash
git add crates/hook/tests/transpile_test.rs
git commit -m "test: add comprehensive integration test for fish language alignment phase 1"
```
