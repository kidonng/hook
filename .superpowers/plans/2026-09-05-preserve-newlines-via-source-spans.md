# Preserve Newlines via Source Spans Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve paragraph blank lines and code structure across the Fish-to-Bash transpilation pipeline by recording `SourceSpan` in AST/IR and emitting collapsed blank lines in the emitter.

**Architecture:** Refactor `fish-parser`'s `Statement` enum into `StatementKind` and a `Statement` struct containing `SourceSpan` computed via `LineIndex`. Propagate spans to `hook`'s `LoweredStatement`, where the Bash emitter checks line distances (`stmt.span.start_line > prev_end + 1`) to emit at most one blank line while stripping boundary blank lines.

**Tech Stack:** Rust 2024 edition, `rust-peg`, `serde`.

**Spec:** `specs/2026-09-05-preserve-newlines-via-source-spans-design.md`

## Global Constraints

- **Workspace Boundaries**: `fish-parser` must remain a pure, high-fidelity syntax parser with zero Bash dependencies or target assumptions.
- **Blank Line Collapsing**: Multiple consecutive blank lines must be collapsed into at most 1 blank line.
- **Block Boundary Cleanliness**: No blank lines emitted at the immediate start or end of blocks.
- **Span Defaults**: Synthetic or zero-span nodes (`start_line == 0`) must never trigger spurious blank lines.
- **Rust Edition**: Rust 2024 edition (`cargo clippy` and `cargo fmt --check` must pass cleanly).

---

### Task 1: Introduce `SourceSpan`, `LineIndex`, and Refactor `Statement` in `fish-parser`

**Files:**
- Create: `crates/fish-parser/src/line_index.rs`
- Modify: `crates/fish-parser/src/lib.rs`
- Modify: `crates/fish-parser/src/ast.rs`
- Modify: `crates/fish-parser/src/grammar.rs`
- Modify: `crates/fish-parser/tests/parser_test.rs`
- Modify: `crates/fish-parser/tests/ast_test.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
  pub struct SourceSpan {
      pub start_line: usize,
      pub end_line: usize,
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct Statement {
      pub kind: StatementKind,
      pub span: SourceSpan,
  }

  pub enum StatementKind { ... }
  ```

- [ ] **Step 1: Write unit tests for `LineIndex` and AST span tracking**

Add tests in `crates/fish-parser/tests/parser_test.rs`:
```rust
#[test]
fn test_statement_spans_and_blank_lines() {
    let input = r#"echo first

echo second


echo third
"#;
    let program = parse(input).expect("parse failed");
    assert_eq!(program.statements.len(), 3);
    assert_eq!(program.statements[0].span.start_line, 1);
    assert_eq!(program.statements[0].span.end_line, 1);
    assert_eq!(program.statements[1].span.start_line, 3);
    assert_eq!(program.statements[1].span.end_line, 3);
    assert_eq!(program.statements[2].span.start_line, 6);
    assert_eq!(program.statements[2].span.end_line, 6);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test test_statement_spans_and_blank_lines`
Expected: FAIL (compilation error: `Statement` has no field `span` or `StatementKind` not found).

- [ ] **Step 3: Implement `LineIndex`, `SourceSpan`, and refactor `Statement`**

1. Create `crates/fish-parser/src/line_index.rs`:
```rust
#[derive(Debug, Clone)]
pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn from_source(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self { line_starts }
    }

    pub fn line_of(&self, byte_offset: usize) -> usize {
        match self.line_starts.binary_search(&byte_offset) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        }
    }
}
```
2. Update `crates/fish-parser/src/ast.rs`:
Rename `enum Statement` to `enum StatementKind`, introduce `SourceSpan` and `struct Statement`.
3. Update `crates/fish-parser/src/grammar.rs`:
Parameterize `pub grammar fish_grammar(line_index: &LineIndex) for str`, capture `#position` at start and end of `statement`, and construct `Statement::new(kind, span)`.
4. Update `crates/fish-parser/src/lib.rs` to construct `LineIndex` and pass to `fish_grammar::program(input, &line_index)`.
5. Update pattern matches in `parser_test.rs` and `ast_test.rs` to access `stmt.kind` or `StatementKind`.

- [ ] **Step 4: Run `fish-parser` tests to verify they pass**

Run: `cargo test -p fish-parser`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/fish-parser
git commit -m "feat(parser): record source spans on statements using line index"
```

---

### Task 2: Propagate Spans in `hook` IR and Lowering Layer

**Files:**
- Modify: `crates/hook/src/bash/ir.rs`
- Modify: `crates/hook/src/bash/lowering/mod.rs`
- Modify: `crates/hook/tests/lowering_test.rs`

**Interfaces:**
- Consumes: `fish_parser::ast::{SourceSpan, Statement, StatementKind}`
- Produces:
  ```rust
  pub use fish_parser::ast::SourceSpan;

  #[derive(Debug, Clone, PartialEq)]
  pub struct LoweredStatement {
      pub kind: LoweredStatementKind,
      pub span: SourceSpan,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub enum LoweredStatementKind { ... }
  ```

- [ ] **Step 1: Write test verifying span preservation across lowering**

Add test in `crates/hook/tests/lowering_test.rs`:
```rust
#[test]
fn test_lowering_preserves_statement_spans() {
    let input = "set -l foo bar\n\necho $foo\n";
    let prog = fish_parser::parse(input).unwrap();
    let lowered = lower_program(&prog);
    assert_eq!(lowered.statements.len(), 2);
    assert_eq!(lowered.statements[0].span.start_line, 1);
    assert_eq!(lowered.statements[1].span.start_line, 3);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test lowering_test test_lowering_preserves_statement_spans`
Expected: FAIL (compilation error due to `LoweredStatementKind`).

- [ ] **Step 3: Update `ir.rs` and `lowering/mod.rs`**

1. In `ir.rs`, rename `enum LoweredStatement` to `enum LoweredStatementKind`, define `struct LoweredStatement { pub kind: LoweredStatementKind, pub span: SourceSpan }`.
2. In `lowering/mod.rs`:
   - In `lower_statement`, match on `&stmt.kind` and wrap returned `kind` in `LoweredStatement { kind, span: stmt.span }`.
   - Update references to `StatementKind` in `lowering/mod.rs` and child modules (e.g. `psub` handling).
3. Update `lowering_test.rs` assertions to inspect `&stmt.kind`.

- [ ] **Step 4: Run lowering tests to verify they pass**

Run: `cargo test -p hook --test lowering_test`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/hook/src/bash/ir.rs crates/hook/src/bash/lowering crates/hook/tests/lowering_test.rs
git commit -m "refactor(hook): propagate source spans in lowered statement ir"
```

---

### Task 3: Emit Preserved Blank Lines in Emitter and Validate

**Files:**
- Modify: `crates/hook/src/bash/emitter.rs`
- Modify: `crates/hook/tests/emitter_test.rs`
- Modify: `crates/hook/tests/transpile_test.rs`
- Modify: `crates/hook/tests/cli_test.rs`

**Interfaces:**
- Consumes: `LoweredStatement { kind, span }`
- Behavior: Emits blank line between statements if `stmt.span.start_line > prev_end + 1 && prev_end > 0`.

- [ ] **Step 1: Write integration tests for blank line preservation**

Add tests in `crates/hook/tests/transpile_test.rs`:
```rust
#[test]
fn test_preserve_blank_lines_top_level_and_collapsed() {
    let input = r#"
echo 1

echo 2



echo 3
"#;
    let output = transpile(input).unwrap();
    let expected = "echo 1\n\necho 2\n\necho 3\n";
    assert_eq!(output.trim(), expected.trim());
}

#[test]
fn test_preserve_blank_lines_inside_blocks_without_boundary_padding() {
    let input = r#"
function my_fn

    echo start

    echo end

end
"#;
    let output = transpile(input).unwrap();
    let expected = r#"my_fn() {
  echo start

  echo end
}"#;
    assert_eq!(output.trim(), expected.trim());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test transpile_test test_preserve_blank_lines_top_level_and_collapsed`
Expected: FAIL (output contains no blank lines).

- [ ] **Step 3: Implement blank line emission logic in `emitter.rs`**

1. Update `emit_statements(stmts: &[LoweredStatement], indent: usize, out: &mut String)`:
   - Track `prev_end_line: Option<usize>`.
   - Before `emit_statement`, if `let Some(prev_end) = prev_end_line`:
     - If `stmt.span.start_line > prev_end + 1 && prev_end > 0`: `out.push('\n')`.
   - Update `prev_end_line = Some(stmt.span.end_line)` if `stmt.span.end_line > 0`.
2. Update `emit_statement` to match on `&stmt.kind` (or match `stmt.kind`).
3. Update `has_executable` checks in `emit_statement` for functions/blocks (`matches!(s.kind, LoweredStatementKind::Comment(_))`).
4. Update `crates/hook/tests/emitter_test.rs` and `cli_test.rs` for AST/IR changes.

- [ ] **Step 4: Run all workspace tests**

Run: `cargo test`
Expected: All tests in `fish-parser` and `hook` pass.

- [ ] **Step 5: Run formatting and linter checks**

Run: `cargo fmt --check && cargo clippy`
Expected: Clean with 0 warnings/errors.

- [ ] **Step 6: Commit**

```bash
git add crates/hook/src/bash/emitter.rs crates/hook/tests
git commit -m "feat(hook): emit preserved blank lines with line-distance collapsing"
```
