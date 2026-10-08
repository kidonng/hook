# Fish Language Alignment (Phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Phase 2 of Fish language alignment: dynamic variable slice subscripts (`$var[$idx]`), array slice mutation and deletion (`set var[i] val`, `set -e var[i]`), indirect variable dereferencing (`$$var`), and multi/reverse slicing.

**Architecture:** Extend `SliceIndex` and `VariableRef` in `crates/fish-parser` to capture dynamic indices and dereferencing faithfully; in `crates/hook`, lower 1-based dynamic indices into Bash 3.2 0-based arithmetic offsets and array mutations.

**Tech Stack:** Rust (2024 edition), `rust-peg`, `serde`, Bash 3.2+

**Spec:** `specs/2026-09-05-fish-language-alignment-phase2-design.md`

## Global Constraints

- Must target Bash 3.2+ compatibility (no Bash 4+ features).
- Maintain strict architectural separation: `fish-parser` produces pure AST; `hook` performs all semantic lowering and offset calculation.
- Zero compiler warnings (`cargo clippy`) and clean formatting (`cargo fmt --check`).
- All emitted scripts must validate with `bash -n`.

---

### Task 1: Parser Support for Dynamic Variable Subscript Indices

**Files:**
- Modify: `crates/fish-parser/src/ast.rs`
- Modify: `crates/fish-parser/src/grammar.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`

**Interfaces:**
- `SliceIndex` gains `Variable(VariableRef)`.
- `grammar.rs` parses variable references inside `slice_index`.

- [x] **Step 1: Write failing test in `parser_test.rs`**

```rust
#[test]
fn test_parse_dynamic_variable_slice_index() {
    let program = parse("echo $letters[$index]\necho $letters[$start..$end]\n").unwrap();
    if let Statement::Pipeline(p) = &program.statements[0] {
        let arg = &p.commands[0].args[1];
        if let WordPart::Variable(v) = &arg.parts[0] {
            assert_eq!(v.name, "letters");
            assert_eq!(v.slices.len(), 1);
            assert!(matches!(v.slices[0], Slice::Index(SliceIndex::Variable(_))));
        } else {
            panic!("expected variable");
        }
    }
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test test_parse_dynamic_variable_slice_index`
Expected: FAIL (SliceIndex::Variable does not exist).

- [x] **Step 3: Update `ast.rs` and `grammar.rs`**

In `crates/fish-parser/src/ast.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SliceIndex {
    Pos(usize),
    Neg(usize),
    Variable(VariableRef),
}
```

In `crates/fish-parser/src/grammar.rs`:
```rust
rule slice_index() -> SliceIndex
    = "-" n:$(['0'..='9']+) { SliceIndex::Neg(n.parse::<usize>().unwrap()) }
    / n:$(['0'..='9']+) { SliceIndex::Pos(n.parse::<usize>().unwrap()) }
    / v:variable_ref() {
        if let WordPart::Variable(vref) = v {
            SliceIndex::Variable(vref)
        } else {
            unreachable!()
        }
    }
```

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p fish-parser --test parser_test test_parse_dynamic_variable_slice_index`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/fish-parser/src/ast.rs crates/fish-parser/src/grammar.rs crates/fish-parser/tests/parser_test.rs
git commit -m "feat(fish-parser): support dynamic variable references in slice indices"
```

---

### Task 2: Lower Dynamic Variable Subscripts to Bash 3.2 0-Based Offsets

**Files:**
- Modify: `crates/hook/src/bash/ir.rs`
- Modify: `crates/hook/src/bash/lowering.rs`
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `BashSubscript` gains `DynamicVariable(String)` and `DynamicRange { start: String, end: String }`.
- `lower_variable_ref` maps `SliceIndex::Variable(v)` to `BashSubscript::DynamicVariable`.
- `emit_variable_ref` emits `${var[$((idx - 1))]}` in Bash.

- [x] **Step 1: Write failing test in `transpile_test.rs`**

```rust
#[test]
fn test_transpile_dynamic_variable_subscript() {
    let bash = transpile("echo $letters[$index]\n");
    assert_eq!(bash, "echo \"${letters[$((index - 1))]}\"\n");
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test transpile_test test_transpile_dynamic_variable_subscript`
Expected: FAIL.

- [x] **Step 3: Update `ir.rs`, `lowering.rs`, and `emitter.rs`**

In `crates/hook/src/bash/ir.rs`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BashSubscript {
    ZeroBasedIndex(usize),
    NegativeOffsetFromLength(usize),
    Range { offset: usize, length: usize },
    OpenRange { offset: usize },
    DynamicVariable(String),
    All,
}
```

In `crates/hook/src/bash/lowering.rs`:
Handle `SliceIndex::Variable` in `lower_variable_ref`.

In `crates/hook/src/bash/emitter.rs`:
In `emit_variable_ref`:
```rust
Some(BashSubscript::DynamicVariable(var_name)) => {
    out.push_str(&format!("\"${{{}[$(({} - 1))]}}\"", name, var_name));
}
```

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p hook --test transpile_test test_transpile_dynamic_variable_subscript`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/hook/src/bash/ir.rs crates/hook/src/bash/lowering.rs crates/hook/src/bash/emitter.rs crates/hook/tests/transpile_test.rs
git commit -m "feat(hook): lower dynamic variable slice subscripts to Bash 3.2 0-based offsets"
```

---

### Task 3: Array Slice Assignment and Deletion Lowering (`set var[i] val`, `set -e var[i]`)

**Files:**
- Modify: `crates/hook/src/bash/ir.rs`
- Modify: `crates/hook/src/bash/lowering.rs`
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `AssignmentIR` gains `SliceAssign { name: String, index: SliceIndexIR, value: LoweredWord }` and `SliceErase { name: String, index: SliceIndexIR }`.
- `lower_set_command` detects `name[index]` syntax and translates 1-based Fish indices to Bash 0-based indexing.

- [x] **Step 1: Write failing tests in `transpile_test.rs`**

```rust
#[test]
fn test_transpile_slice_assignment_and_erase() {
    let bash_assign = transpile("set fruit[2] evil\n");
    assert_eq!(bash_assign, "fruit[1]=\"evil\"\n");

    let bash_erase = transpile("set -e fruit[1]\n");
    assert_eq!(bash_erase, "unset 'fruit[0]'\n");
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test transpile_test test_transpile_slice_assignment_and_erase`
Expected: FAIL (currently emits `fruit[2]="evil"` and `unset fruit[1]`).

- [x] **Step 3: Implement Slice Assignment in `lowering.rs` and `emitter.rs`**

Parse `var[i]` in `lower_set_command`:
- Subtract 1 for positive literal index.
- Convert negative literal index `-k` to `$((${#var[@]}-k))`.
Emit appropriate Bash assignment or `unset`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test -p hook --test transpile_test test_transpile_slice_assignment_and_erase`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/hook/src/bash/ir.rs crates/hook/src/bash/lowering.rs crates/hook/src/bash/emitter.rs crates/hook/tests/transpile_test.rs
git commit -m "feat(hook): lower set slice assignments and deletions with 1-based to 0-based conversion"
```

---

### Task 4: Indirect Variable Dereferencing AST (`$$var`) and Lowering

**Files:**
- Modify: `crates/fish-parser/src/ast.rs`
- Modify: `crates/fish-parser/src/grammar.rs`
- Modify: `crates/hook/src/bash/ir.rs`
- Modify: `crates/hook/src/bash/lowering.rs`
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- In AST: `VariableTarget::Named(String)` vs `VariableTarget::Indirect(Box<VariableRef>)`.
- In `hook`: Lower scalar indirect variable references to `${!var}` in Bash 3.2.

- [x] **Step 1: Write failing tests**

```rust
#[test]
fn test_parse_and_transpile_indirect_variable() {
    let program = parse("echo $$var\n").unwrap();
    assert_eq!(program.statements.len(), 1);

    let bash = transpile("set var name\necho $$var\n");
    assert!(bash.contains("${!var}"));
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test`
Expected: FAIL.

- [x] **Step 3: Update `ast.rs`, `grammar.rs`, `lowering.rs`, and `emitter.rs`**

Support recursive `$` in `grammar.rs`.
Lower to `${!var}` in `lowering.rs` and `emitter.rs`.

- [x] **Step 4: Run test to verify it passes**

Run: `cargo test`
Expected: PASS

- [x] **Step 5: Commit**

```bash
git add crates/fish-parser crates/hook
git commit -m "feat: support indirect variable dereferencing ($$var)"
```

---

### Task 5: Phase 2 Integration Tests and bash -n Validation

**Files:**
- Test: `crates/hook/tests/transpile_test.rs`

- [x] **Step 1: Add comprehensive Phase 2 integration test**

Validate combined dynamic slicing, slice assignment, and indirect expansion.

- [x] **Step 2: Run fmt, clippy, and full tests**

```bash
nix develop --command cargo fmt --check
nix develop --command cargo clippy --all-targets -- -D warnings
cargo test
```
Expected: PASS with 0 warnings.

- [x] **Step 3: Commit**

```bash
git add crates/hook/tests/transpile_test.rs
git commit -m "test: add comprehensive integration test for fish language alignment phase 2"
```
