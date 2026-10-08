# Design: Preserve Newlines via Source Spans in AST and IR

- **Author**: kidonng
- **Date**: 2026-09-05
- **Status**: Draft
- **Target Components**: `crates/fish-parser`, `crates/hook`

## 1. Problem & Context

Currently, the Fish-to-Bash transpiler pipeline (`fish-parser` -> `hook` lowering -> `hook` emitter) discards all blank lines between statements:
1. `fish-parser`'s `statement_sep` rule `(_* ['\n' | ';'])+ _*` discards all consecutive newline and semicolon characters without recording their presence or count.
2. `Statement` AST nodes do not track source position, byte offsets, or line numbers.
3. `hook`'s emitter simply outputs every lowered statement on a single line separated by a single newline character `\n`.

As a result, comments are preserved, but logical paragraph separation (blank lines between functions, code blocks, or distinct logic segments) is completely lost in the transpiled modern Bash output.

## 2. Goals & Non-Goals

### Goals
- **Preserve paragraph blank lines**: Retain blank lines between statements at top-level and inside all nested block constructs (`function`, `if`/`elif`/`else`, `for`, `while`, `switch`, `begin`).
- **Collapse excess blank lines**: Collapse multiple consecutive blank lines into at most one blank line (conforming to modern code formatting standards like `rustfmt` and `gofmt`).
- **Clean block boundaries**: Strip blank lines at the immediate start and end of blocks (avoiding leading blank lines after block openers or trailing blank lines before `fi`, `esac`, `end`, etc.).
- **Reusable high-fidelity AST**: Store clean, non-invasive `SourceSpan` (line numbers) on AST nodes in `fish-parser` without polluting the AST with synthetic "empty line" statement kinds.
- **Maintain architectural boundaries**: Ensure `fish-parser` remains a pure, high-fidelity parser without Bash-specific assumptions, keeping spans useful for future tools (linters, formatters, source mapping, diagnostic reporting).

### Non-Goals
- Preserving mid-statement line continuations (e.g. commands split across lines with `\` or multiline argument layouts).
- Preserving full column-level whitespace / indentation trivia.

## 3. Architecture & Data Flow

### 3.1 Line Index & Position Tracking in `fish-parser`

We introduce a lightweight, zero-allocation `LineIndex` helper in `fish-parser`:
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

### 3.2 AST Refactoring (`crates/fish-parser/src/ast.rs`)

`SourceSpan` represents the 1-based source lines:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SourceSpan {
    pub start_line: usize,
    pub end_line: usize,
}

impl SourceSpan {
    pub fn new(start_line: usize, end_line: usize) -> Self {
        Self { start_line, end_line }
    }
}
```

The existing `enum Statement` is renamed to `enum StatementKind`, and a first-class `struct Statement` is introduced:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: SourceSpan,
}

impl Statement {
    pub fn new(kind: StatementKind, span: SourceSpan) -> Self {
        Self { kind, span }
    }
}
```

All container fields previously holding `Vec<Statement>` (`Program.statements`, `IfStatement.then_body`, `FunctionStatement.body`, etc.) remain `Vec<Statement>`.

### 3.3 Grammar Integration (`crates/fish-parser/src/grammar.rs`)

The `peg` grammar takes `&LineIndex` as parameter:
```rust
pub grammar fish_grammar(line_index: &LineIndex) for str { ... }
```
Using `#position`, statement parsing captures start and end offsets:
```rust
pub rule statement() -> Vec<Statement>
    = !block_terminator() start:#position s:inner_statement() end:#position {
        let span = SourceSpan::new(line_index.line_of(start), line_index.line_of(end));
        s.into_iter().map(|kind| Statement::new(kind, span)).collect()
    }
```
Comment statements also capture `#position` and receive accurate spans.

### 3.4 IR Refactoring (`crates/hook/src/bash/ir.rs`)

`LoweredStatement` mirrors the AST structure:
```rust
pub use fish_parser::ast::SourceSpan;

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredStatement {
    pub kind: LoweredStatementKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoweredStatementKind {
    Pipeline(LoweredPipeline),
    Assignment(AssignmentIR),
    If(LoweredIf),
    Switch(LoweredSwitch),
    For(LoweredFor),
    While(LoweredWhile),
    Function(LoweredFunction),
    BeginBlock(LoweredBeginBlock),
    Return(Option<LoweredWord>),
    Break,
    Continue,
    Comment(String),
}
```

### 3.5 Lowering Phase (`crates/hook/src/bash/lowering/mod.rs`)

`lower_statement` matches against `&stmt.kind` and passes through `stmt.span`:
```rust
pub fn lower_statement(stmt: &Statement, scope: &mut Scope) -> LoweredStatement {
    let kind = match &stmt.kind {
        StatementKind::Comment(c) => LoweredStatementKind::Comment(c.clone()),
        StatementKind::Return(w) => LoweredStatementKind::Return(w.as_ref().map(|w| lower_word(w, scope))),
        StatementKind::Break => LoweredStatementKind::Break,
        StatementKind::Continue => LoweredStatementKind::Continue,
        StatementKind::Pipeline(p) => { ... }
        StatementKind::If(i) => { ... }
        ...
    };
    LoweredStatement {
        kind,
        span: stmt.span,
    }
}
```

### 3.6 Emitter Logic (`crates/hook/src/bash/emitter.rs`)

`emit_statements` tracks the previous statement's `end_line`:
```rust
pub fn emit_statements(stmts: &[LoweredStatement], indent: usize, out: &mut String) {
    let mut prev_end_line: Option<usize> = None;

    for stmt in stmts {
        if let Some(prev_end) = prev_end_line {
            if stmt.span.start_line > prev_end + 1 && prev_end > 0 {
                out.push('\n');
            }
        }
        emit_statement(stmt, indent, out);
        if stmt.span.end_line > 0 {
            prev_end_line = Some(stmt.span.end_line);
        }
    }
}
```

## 4. Edge Cases & Handling

1. **Synthetic / Zero-span nodes**: For synthetic nodes or ASTs constructed in unit tests without span, `span: SourceSpan::default()` has `start_line: 0, end_line: 0`. The emitter checks `prev_end > 0` before inserting blank lines, gracefully handling zero-spans without inserting spurious blank lines.
2. **Same-line statements (`echo a; echo b`)**: `stmt.span.start_line == prev_end`, so `start_line > prev_end + 1` is false; only the normal statement newline is emitted.
3. **Multiline comments**: Span correctly marks the lines spanned by the comments; blank lines preceding comments are properly detected and preserved.
4. **Block start/end padding**: Block starts have `prev_end_line == None`, avoiding leading blank lines. Block endings don't emit trailing blank lines.

## 5. Testing Strategy

1. **Parser Tests (`crates/fish-parser/tests/`)**:
   - Verify `SourceSpan` accuracy on single-line and multi-line statements and comments.
   - Verify spans inside nested blocks (`function`, `if`, `for`).
2. **Emitter & Transpilation Tests (`crates/hook/tests/`)**:
   - Verify single blank lines are preserved between top-level commands and functions.
   - Verify multiple consecutive blank lines are collapsed to exactly one.
   - Verify blank lines inside `function`, `if`, `while` blocks are preserved.
   - Verify no spurious blank lines at start or end of blocks.
   - Run full workspace tests (`cargo test`) to ensure all existing lowering and emission assertions are satisfied.
