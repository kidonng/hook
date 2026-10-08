# Modern Bash (5.0+) Target Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transition `hook` from transpiling to legacy Bash 3.2 to modern Bash (5.0+) by default, introducing a modular `Target` configuration and upgrading IR, Lowering, and Emitter to achieve 1:1 structural fidelity with Fish.

**Architecture:** Introduce `Target` configuration (`Bash5` default, `Bash3_2` placeholder) in `crates/hook`. Upgrade `LoweredPipeline` in IR to retain native `PipeKind::StdoutAndStderr` (`|&`), support negative subscripts (`${var[-1]}`), native merged redirects (`&>`, `&>>`), and scope elevation (`declare -g`). Update Lowering and Emitter passes to emit clean, modern Bash 5.0+ idioms, and refresh test suites and snapshots.

**Tech Stack:** Rust (2024 edition), `rust-peg`, `serde`, `insta`, Bash 5.0+

**Spec:** `specs/2026-09-05-modern-bash-5-target-design.md`

## Global Constraints

- Transpilation target defaults to modern Bash (5.0+).
- Clean separation of concerns: `crates/fish-parser` remains an untainted syntax parser. All target configurations and semantic interpretations live in `crates/hook`.
- Output scripts must be syntactically validated with `bash -n` (using the system's Bash 5.0+).
- Zero compiler warnings (`cargo clippy`) and clean formatting (`cargo fmt --check`).
- No emojis or character portrayals in engineering artifacts (code, commits, comments).

---

### Task 1: Target Configuration & CLI Flags

**Files:**
- Create: `crates/hook/src/target.rs`
- Modify: `crates/hook/src/lib.rs`
- Modify: `crates/hook/src/main.rs`
- Test: `crates/hook/tests/cli_test.rs`

**Interfaces:**
- Produces:
  - `pub enum Target { Bash5, Bash3_2 }`
  - `pub struct TranspileConfig { pub target: Target }`
  - CLI argument `--target <bash5|bash3.2>`

- [ ] **Step 1: Write failing CLI tests for `--target` flag**

In `crates/hook/tests/cli_test.rs`:
```rust
#[test]
fn test_cli_target_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_hook"))
        .arg("--target")
        .arg("bash5")
        .arg("--help")
        .output()
        .expect("failed to execute hook");
    assert!(output.status.success());

    let output_invalid = Command::new(env!("CARGO_BIN_EXE_hook"))
        .arg("--target")
        .arg("invalid-target")
        .output()
        .expect("failed to execute hook");
    assert_eq!(output_invalid.status.code(), Some(2));
}

#[test]
fn test_cli_target_bash32_placeholder() {
    let output = Command::new(env!("CARGO_BIN_EXE_hook"))
        .arg("--target")
        .arg("bash3.2")
        .output()
        .expect("failed to execute hook");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("target 'bash3.2' will be supported in an upcoming release"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test cli_test`
Expected: FAIL due to unrecognized `--target` flag or compilation error.

- [ ] **Step 3: Implement `Target` and update CLI**

Create `crates/hook/src/target.rs`:
```rust
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    Bash5,
    Bash3_2,
}

impl FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "bash5" | "5" | "bash-5" | "bash5.0" => Ok(Target::Bash5),
            "bash3.2" | "bash32" | "3.2" | "bash-3.2" => Ok(Target::Bash3_2),
            _ => Err(format!(
                "unsupported target: '{}', expected 'bash5' or 'bash3.2'",
                s
            )),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Bash5 => write!(f, "bash5"),
            Target::Bash3_2 => write!(f, "bash3.2"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TranspileConfig {
    pub target: Target,
}
```

In `crates/hook/src/lib.rs`:
```rust
pub mod bash;
pub mod target;

pub use bash::emit_bash;
pub use target::{Target, TranspileConfig};
```

In `crates/hook/src/main.rs`:
Parse `--target <target>` or `--target=<target>`. If `Target::Bash3_2`, output an informative error to stderr and exit with code 2. Update `print_help()` to reflect modern Bash 5.0+ and `--target` flag.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p hook --test cli_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/hook/src/target.rs crates/hook/src/lib.rs crates/hook/src/main.rs crates/hook/tests/cli_test.rs
git commit -m "feat(hook): add Target configuration and --target CLI flag"
```

---

### Task 2: Modernize Intermediate Representation (IR)

**Files:**
- Modify: `crates/hook/src/bash/ir.rs`
- Modify: `crates/hook/src/bash/emitter.rs` (adapt to match new IR fields)
- Modify: `crates/hook/src/bash/lowering.rs` (adapt to match new IR fields)

**Interfaces:**
- Consumes: `fish_parser::ast::{Combinator, RedirectMode, Slice}`
- Produces:
  - `PipeKind { Stdout, StdoutAndStderr }`
  - `LoweredPipeline.pipe_operators: Vec<PipeKind>`
  - `SliceIndexIR::Negative(isize)`
  - `BashSubscript::Index(isize)`
  - `AssignmentIR::Global { in_function: bool, ... }`

- [ ] **Step 1: Write failing IR unit test**

In `crates/hook/src/bash/ir.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modern_ir_pipe_and_subscripts() {
        let pipeline = LoweredPipeline {
            commands: vec![],
            pipe_operators: vec![PipeKind::StdoutAndStderr],
            combinator: Combinator::None,
            background: false,
        };
        assert_eq!(pipeline.pipe_operators[0], PipeKind::StdoutAndStderr);

        let sub = BashSubscript::Index(-1);
        assert_eq!(sub, BashSubscript::Index(-1));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook bash::ir::tests`
Expected: FAIL due to missing `PipeKind` and new enum variants.

- [ ] **Step 3: Update `crates/hook/src/bash/ir.rs`**

Add `PipeKind`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeKind {
    Stdout,
    StdoutAndStderr,
}
```
Update `LoweredPipeline`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct LoweredPipeline {
    pub commands: Vec<LoweredCommand>,
    pub pipe_operators: Vec<PipeKind>,
    pub combinator: Combinator,
    pub background: bool,
}
```
Update `SliceIndexIR`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SliceIndexIR {
    ZeroBased(usize),
    Negative(isize),
    Dynamic(String),
}
```
Update `BashSubscript`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BashSubscript {
    Index(isize),
    Range { offset: isize, length: usize },
    OpenRange { offset: isize },
    DynamicVariable(String),
    DynamicRange { start: String, end: String },
    All,
}
```
Update `AssignmentIR::Global`:
```rust
    Global {
        name: String,
        values: Vec<LoweredWord>,
        in_function: bool,
    },
```

Temporarily update references in `lowering.rs` and `emitter.rs` so the workspace compiles.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p hook bash::ir::tests`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/hook/src/bash/ir.rs crates/hook/src/bash/lowering.rs crates/hook/src/bash/emitter.rs
git commit -m "refactor(hook): modernize IR with PipeKind and native negative indexing"
```

---

### Task 3: Modernize AST Lowering Pass

**Files:**
- Modify: `crates/hook/src/bash/lowering.rs`
- Test: `crates/hook/tests/lowering_test.rs`

**Interfaces:**
- Consumes: `fish_parser::ast::*`, `crates/hook/src/bash/ir::*`
- Produces: `pub fn lower_program(program: &Program) -> LoweredProgram` emitting modern IR without Bash 3.2 synthetic redirects or length calculations.

- [ ] **Step 1: Write failing tests in `crates/hook/tests/lowering_test.rs`**

```rust
#[test]
fn test_lower_merged_pipe_modern() {
    let fish = "cmd1 &| cmd2\n";
    let prog = fish_parser::parse(fish).unwrap();
    let lowered = lower_program(&prog);
    let stmt = &lowered.statements[0];
    if let LoweredStatement::Pipeline(p) = stmt {
        assert_eq!(p.pipe_operators, vec![PipeKind::StdoutAndStderr]);
        assert!(p.commands[0].redirections.is_empty(), "should not synthesize 2>&1 redirection");
    } else {
        panic!("expected pipeline");
    }
}

#[test]
fn test_lower_negative_slice_modern() {
    let fish = "echo $arr[-1]\n";
    let prog = fish_parser::parse(fish).unwrap();
    let lowered = lower_program(&prog);
    if let LoweredStatement::Pipeline(p) = &lowered.statements[0] {
        let arg = &p.commands[0].args[1];
        if let LoweredWordPart::Variable(LoweredVariableRef::Custom { subscript, .. }) = &arg.parts[0] {
            assert_eq!(subscript, &Some(BashSubscript::Index(-1)));
        } else {
            panic!("expected custom variable with negative index");
        }
    }
}

#[test]
fn test_lower_set_global_in_function() {
    let fish = "function foo\n  set -g bar 1\nend\n";
    let prog = fish_parser::parse(fish).unwrap();
    let lowered = lower_program(&prog);
    if let LoweredStatement::Function(f) = &lowered.statements[0] {
        if let LoweredStatement::Assignment(AssignmentIR::Global { in_function, .. }) = &f.body[0] {
            assert!(in_function);
        } else {
            panic!("expected global assignment in function");
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test lowering_test`
Expected: FAIL

- [ ] **Step 3: Implement modern lowering in `crates/hook/src/bash/lowering.rs`**

- In `lower_pipeline`: map `p.pipe_operators` directly to `PipeKind::Stdout` or `PipeKind::StdoutAndStderr`. Do NOT append synthetic `Redirection { fd: 2, ... }` to `commands[idx]`.
- In `parse_slice_target`: when index is negative (e.g. `-num`), return `SliceIndexIR::Negative(num)`.
- In `lower_set_command`: when setting global (`-g`), record `in_function: scope.in_function`.
- In `lower_variable_ref`: when subscript is `SliceIndex::Neg(k)`, return `BashSubscript::Index(-(k as isize))`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p hook --test lowering_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/hook/src/bash/lowering.rs crates/hook/tests/lowering_test.rs
git commit -m "feat(hook): modernize lowering pass with native pipe operators and indices"
```

---

### Task 4: Modernize Code Generation Emitter

**Files:**
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/hook/tests/emitter_test.rs`

**Interfaces:**
- Consumes: `LoweredProgram`, `LoweredPipeline`, `AssignmentIR`, etc.
- Produces: `pub fn emit_bash(program: &LoweredProgram) -> String` generating native Bash 5.0+ syntax (`|&`, `&>`, `&>>`, `${var[-1]}`, `declare -g`).

- [ ] **Step 1: Write failing tests in `crates/hook/tests/emitter_test.rs`**

```rust
#[test]
fn test_emitter_modern_pipes_and_redirects() {
    let pipeline = LoweredPipeline {
        commands: vec![
            LoweredCommand {
                negate: false,
                args: vec![LoweredWord::from_literal("cmd1")],
                redirections: vec![],
            },
            LoweredCommand {
                negate: false,
                args: vec![LoweredWord::from_literal("cmd2")],
                redirections: vec![],
            },
        ],
        pipe_operators: vec![PipeKind::StdoutAndStderr],
        combinator: Combinator::None,
        background: false,
    };
    let program = LoweredProgram {
        shebang: None,
        statements: vec![LoweredStatement::Pipeline(pipeline)],
    };
    let output = emit_bash(&program);
    assert_eq!(output, "cmd1 |& cmd2\n");
}

#[test]
fn test_emitter_modern_negative_subscript() {
    let var = LoweredVariableRef::Custom {
        name: "items".to_string(),
        subscript: Some(BashSubscript::Index(-1)),
    };
    let mut out = String::new();
    emit_variable_ref(&var, &mut out);
    assert_eq!(out, "\"${items[-1]}\"");
}

#[test]
fn test_emitter_modern_global_declaration_in_function() {
    let assign = AssignmentIR::Global {
        name: "count".to_string(),
        values: vec![LoweredWord::from_literal("42")],
        in_function: true,
    };
    let program = LoweredProgram {
        shebang: None,
        statements: vec![LoweredStatement::Assignment(assign)],
    };
    let output = emit_bash(&program);
    assert_eq!(output, "declare -g count=\"42\"\n");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test emitter_test`
Expected: FAIL

- [ ] **Step 3: Update `crates/hook/src/bash/emitter.rs`**

- In `emit_pipeline_commands`: format pipe using `pipe_operators[idx]` -> ` |& ` for `PipeKind::StdoutAndStderr`, ` | ` for `PipeKind::Stdout`.
- In `emit_redirection`:
  - `RedirectMode::OutputAndErr` -> `&> `
  - `RedirectMode::AppendAndErr` -> `&>> `
  - Remove trailing ` 2>&1`.
- In `emit_assignment`:
  - `AssignmentIR::Global`: if `in_function`, emit `declare -g name="..."` (or `declare -ga name=(...)`).
  - `AssignmentIR::SliceAssign`: for `SliceIndexIR::Negative(k)`, emit `name[k]="value"`.
  - `AssignmentIR::SliceErase`: for `SliceIndexIR::Negative(k)`, emit `unset 'name[k]'`.
  - For dynamic index `SliceIndexIR::Dynamic(var)`: emit `name[var-1]="value"`.
- In `emit_variable_ref_inner` and `emit_variable_ref`:
  - `BashSubscript::Index(k)` -> `${name[k]}` (supports both positive and negative directly!).
  - `BashSubscript::DynamicVariable(var)` -> `${name[var-1]}`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p hook --test emitter_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/hook/src/bash/emitter.rs crates/hook/tests/emitter_test.rs
git commit -m "feat(hook): modernize emitter with |&, &>, declare -g, and native negative indices"
```

---

### Task 5: Snapshot Updates, Integration Tests & Documentation

**Files:**
- Modify: `crates/hook/tests/transpile_test.rs`
- Modify: `crates/hook/tests/snapshots/*.snap`
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `flake.nix`

**Interfaces:**
- Validate all snapshots and integration tests against Bash 5.0+ syntax with `bash -n`.
- Synchronize workspace documentation with modern Bash target.

- [ ] **Step 1: Run integration test and observe snapshot diffs**

Run: `cargo test -p hook --test transpile_test`
Observe failing snapshots due to cleaner syntax (`${items[-1]}` vs `${items[$((${#items[@]}-1))]}` and `make |& less` vs `make 2>&1 | less`).

- [ ] **Step 2: Update snapshot assertions and review updated snapshots**

Run: `INSTA_UPDATE=always cargo test -p hook --test transpile_test`
Verify that updated snapshot files contain clean, modern Bash 5.0+ code:
- `transpile_test__snapshot_set_and_arrays.snap`: `${items[-1]}` instead of dynamic length math.
- `test_transpile_merged_pipes`: `make |& less` instead of `make 2>&1 | less`.
- `test_phase2_alignment_combined`: native negative indices.

- [ ] **Step 3: Update documentation and description files**

- In `AGENTS.md`: Update target description from Bash 3.2+ to modern Bash (5.0+).
- In `README.md` (if present) and `flake.nix`: Update description to modern Bash transpiler.
- In `crates/hook/src/main.rs`: Ensure help string and version information reflect the new target.

- [ ] **Step 4: Run full workspace test suite, clippy, and fmt**

Run:
```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
Expected: All tests pass, zero warnings, clean formatting.

- [ ] **Step 5: Commit**

```bash
git add crates/hook/tests/ AGENTS.md flake.nix crates/hook/src/main.rs
git commit -m "test: update integration tests, snapshots, and documentation for modern bash 5 target"
```
