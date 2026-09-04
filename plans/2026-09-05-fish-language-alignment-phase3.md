# Fish Language Alignment (Phase 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Phase 3 of Fish language alignment: built-in `count` and `contains` lowering, safe/noclobber redirections (`<?`, `>?`), compound statement blocks (`{ ... }`), and generalized process substitution.

**Architecture:** Extend PEG grammar for safe/noclobber redirections and compound blocks; enhance `crates/hook` lowering to recognize and optimize standard Fish built-ins into native Bash 3.2 idioms.

**Tech Stack:** Rust (2024 edition), `rust-peg`, `serde`, Bash 3.2+

**Spec:** `specs/2026-09-05-fish-language-alignment-phase3-design.md`

## Global Constraints

- Must target Bash 3.2+ compatibility.
- Adhere strictly to the architectural boundary: `fish-parser` only parses syntax into AST without semantic lowering; all lowering belongs in `hook`.
- All emitted Bash scripts must validate cleanly with `bash -n`.
- Zero compiler warnings (`cargo clippy`) and clean formatting (`cargo fmt --check`).

---

### Task 1: Builtin `count` Lowering to Native Bash Parameter Expansion

**Files:**
- Modify: `crates/hook/src/bash/lowering.rs`
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `lower_command` intercepts `count` calls:
  - `count $argv` -> `printf '%s\n' "$#"` or `$#`.
  - `count $var` -> `printf '%s\n' "${#var[@]}"`.
  - `if count $var >/dev/null` -> `if [ "${#var[@]}" -gt 0 ]; then`.

- [ ] **Step 1: Write failing test in `transpile_test.rs`**

```rust
#[test]
fn test_transpile_count_builtin() {
    let bash_var = transpile("count $foos\n");
    assert!(bash_var.contains("${#foos[@]}"));

    let bash_argv = transpile("count $argv\n");
    assert!(bash_argv.contains("$#"));

    let bash_if = transpile("if count $foos >/dev/null\n  echo yes\nend\n");
    assert!(bash_if.contains("[ \"${#foos[@]}\" -gt 0 ]"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test transpile_test test_transpile_count_builtin`
Expected: FAIL.

- [ ] **Step 3: Implement `count` lowering in `lowering.rs`**

Add pattern matching for `count` commands in `lower_command`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p hook --test transpile_test test_transpile_count_builtin`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/hook/src/bash/lowering.rs crates/hook/tests/transpile_test.rs
git commit -m "feat(hook): lower count builtin to native Bash array and parameter length expansions"
```

---

### Task 2: Builtin `contains` Lowering to Bash 3.2 Search Loop

**Files:**
- Modify: `crates/hook/src/bash/lowering.rs`
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `lower_command` detects `contains needle haystack...` and lowers to a Bash 3.2 compatible membership check.

- [ ] **Step 1: Write failing test in `transpile_test.rs`**

```rust
#[test]
fn test_transpile_contains_builtin() {
    let bash = transpile("if contains blue $smurf\n  echo found\nend\n");
    assert!(bash.contains("for __hook_item in"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p hook --test transpile_test test_transpile_contains_builtin`
Expected: FAIL.

- [ ] **Step 3: Implement `contains` lowering in `lowering.rs`**

Lower `contains` to an inline search loop or helper invocation.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p hook --test transpile_test test_transpile_contains_builtin`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/hook/src/bash/lowering.rs crates/hook/tests/transpile_test.rs
git commit -m "feat(hook): lower contains builtin to Bash 3.2 search loop"
```

---

### Task 3: Safe Input (`<?`) and Noclobber (`>?`) Redirection AST and Lowering

**Files:**
- Modify: `crates/fish-parser/src/ast.rs`
- Modify: `crates/fish-parser/src/grammar.rs`
- Modify: `crates/hook/src/bash/emitter.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- `RedirectMode` gains `SafeInput` (`<?`), `NoClobberOutput` (`>?`), and `NoClobberAppend` (`2>?`).
- `fish-parser` parses `<?`, `>?`, and `2>?`.
- `hook` emits safe input checks or noclobber flags.

- [ ] **Step 1: Write failing tests in `parser_test.rs`**

```rust
#[test]
fn test_parse_safe_and_noclobber_redirections() {
    let p1 = parse("cat <?input.txt\n").unwrap();
    if let Statement::Pipeline(pipe) = &p1.statements[0] {
        assert_eq!(pipe.commands[0].redirections[0].mode, RedirectMode::SafeInput);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test test_parse_safe_and_noclobber_redirections`
Expected: FAIL.

- [ ] **Step 3: Update `ast.rs`, `grammar.rs`, and `emitter.rs`**

In `crates/fish-parser/src/ast.rs`:
Add variants to `RedirectMode`.

In `crates/fish-parser/src/grammar.rs`:
Add `"<?", ">?", "2>?"` to `redirect_mode`.

In `crates/hook/src/bash/emitter.rs`:
Handle new redirection modes for Bash 3.2.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fish-parser crates/hook
git commit -m "feat: support safe input (<?) and noclobber (>?) redirections"
```

---

### Task 4: Compound Statement Block `{ ... }` Support

**Files:**
- Modify: `crates/fish-parser/src/grammar.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`
- Test: `crates/hook/tests/transpile_test.rs`

**Interfaces:**
- Parse leading `{` as `compound_block_stmt` yielding `Statement::BeginBlock`.
- Emitter formats `{ ...; }` in Bash.

- [ ] **Step 1: Write failing test in `parser_test.rs`**

```rust
#[test]
fn test_parse_compound_statement_block() {
    let p = parse("{ echo hello; and echo world; }\n").unwrap();
    assert!(matches!(p.statements[0], Statement::BeginBlock(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test parser_test test_parse_compound_statement_block`
Expected: FAIL.

- [ ] **Step 3: Implement compound block in `grammar.rs`**

Add rule for `{ statement_list() }` at statement head.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/fish-parser crates/hook
git commit -m "feat: support compound statement blocks ({ ... })"
```

---

### Task 5: Phase 3 Integration Tests and Bash 3.2 Validation

**Files:**
- Test: `crates/hook/tests/transpile_test.rs`

- [ ] **Step 1: Add comprehensive Phase 3 test**

Validate combined `count`, `contains`, safe redirections, and compound statement blocks.

- [ ] **Step 2: Run fmt, clippy, and full workspace tests**

```bash
nix develop --command cargo fmt --check
nix develop --command cargo clippy --all-targets -- -D warnings
cargo test
```
Expected: All checks PASS with 0 warnings.

- [ ] **Step 3: Commit**

```bash
git add crates/hook/tests/transpile_test.rs
git commit -m "test: add comprehensive integration test for fish language alignment phase 3"
```
