# Fish Language Alignment (Phase 2) Design: Advanced Variables, Slicing & Array Assignments

## Overview

This specification details Phase 2 of aligning `hook` and `fish-parser` with the Fish shell scripting language documentation (`language.rst`).

Phase 2 focuses on advanced variable referencing, variable index slicing, list element mutations, dereferencing, and standard special environment variables:
1. **Dynamic Variable Subscript Expressions (`$var[$idx]`, `$var[$start..$end]`)**: Allowing variable references as slice indices for arrays.
2. **Array Slice Assignment and Deletion (`set var[i] val`, `set -e var[i]`)**: Proper 1-based to 0-based index translation and negative index handling for Bash 3.2.
3. **Multiple and Reverse Range Slices (`$var[1 2 3]`, `$var[-1..1]`)**: Slicing with multiple indices and reverse ordering.
4. **Indirect Variable Dereferencing (`$$var`, `$$var[1]`)**: Recursive variable expansion.
5. **Standard Special Read-Only Variables**: Mapping `$PWD`, `$HOME`, `$USER`, `$EUID`, `$IFS`, `$SHLVL`, `$_`.

---

## 1. Dynamic Variable Subscript Expressions (`$var[$idx]`, `$var[$start..$end]`)

### Problem
In `fish-shell/doc_src/language.rst:1076`:
```fish
set index 2
set letters a b c d
echo $letters[$index] # returns 'b'
```
Currently in `fish-parser/src/ast.rs`, `SliceIndex` is defined as:
```rust
pub enum SliceIndex {
    Pos(usize),
    Neg(usize),
}
```
And `grammar.rs` only parses literal digits for slice indices. If a variable `$index` is inside the bracket, it fails or parses as literal text outside the variable.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  Extend `SliceIndex` in AST to represent dynamic indices faithfully:
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub enum SliceIndex {
      Pos(usize),
      Neg(usize),
      Variable(VariableRef),
  }
  ```
  In `grammar.rs`, update `slice_index()` to parse a `variable_ref()` in addition to literal positive and negative integers.
- **`hook` Responsibility**:
  In `hook/src/bash/ir.rs`:
  Add `VariableIndex(LoweredVariableRef)` and `DynamicRange { start, end }` to `BashSubscript`.
  In `hook/src/bash/lowering.rs`:
  Translate Fish 1-based dynamic index to Bash 3.2 0-based arithmetic index:
  `${var[$((idx - 1))]}`.
  For dynamic range `$start..$end`:
  `${var[@]:$((start - 1)):$((end - start + 1))}`.

---

## 2. Array Slice Assignment and Deletion (`set var[i] val`, `set -e var[i]`)

### Problem
In `fish-shell/doc_src/language.rst:1380`:
```fish
set smurf blue small
set smurf[2] evil  # changes 2nd element
set -e smurf[1]    # erases 1st element
```
Currently, `hook` treats `smurf[2]` as a literal string variable name, outputting `smurf[2]="evil"` (which in Bash mutates index 2, the 3rd element!). `set -e smurf[1]` emits `unset smurf[1]`, unsetting the 2nd element.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  `set smurf[2] evil` continues to parse `set` as a command whose argument words are `["set", "smurf[2]", "evil"]`. The parser remains completely agnostic to `set` commands.
- **`hook` Responsibility**:
  In `hook/src/bash/lowering.rs`:
  When `lower_set_command` inspects variable targets, parse optional slice annotations on the variable name:
  - If `var[N]` (where `N` is positive integer literal):
    Target index is `N - 1`.
    Assignment emits `var[N-1]="val"`.
    Erase (`-e`) emits `unset 'var[N-1]'`.
  - If `var[-K]` (negative index literal):
    Target index in Bash 3.2 is `${#var[@]}-K`.
    Assignment emits `var[$((${#var[@]}-K))]="val"`.
    Erase emits `unset "var[$((${#var[@]}-K))]"`.
  - If `var[$idx]` (dynamic variable index):
    Target index in Bash 3.2 is `$((idx - 1))`.
    Assignment emits `var[$((idx - 1))]="val"`.
    Erase emits `unset 'var[$((idx - 1))]'`.

---

## 3. Multiple and Reverse Range Slices (`$var[1 2 3]`, `$var[-1..1]`)

### Problem
In `fish-shell/doc_src/language.rst:1010-1033`:
```fish
echo (seq 10)[2..5 1..3] # multiple ranges
echo (seq 10)[-1..1]     # reverse output
```
Currently `fish-parser` only allows a single index or single range inside `[...]`.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  Allow multiple slice elements separated by whitespace inside `[...]`:
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct SliceSequence {
      pub slices: Vec<Slice>,
  }
  ```
  Or update `VariableRef.slices` and `CommandSubst.slices` to accommodate multiple slice selectors in order.
- **`hook` Responsibility**:
  In `hook/src/bash/lowering.rs` & `emitter.rs`:
  - Multiple slices on array: emit `${var[0]} ${var[1]} ...` or loop expansion.
  - Reverse slice `[-1..1]`: for command substitution, emit `| tac` (or reverse sed/awk loop); for arrays in Bash 3.2, emit reverse indexed expansion.

---

## 4. Indirect Variable Dereferencing (`$$var`, `$$var[1]`)

### Problem
In `fish-shell/doc_src/language.rst:743`:
```fish
set foo a b c
set a 10; set b 20; set c 30
echo $$foo # prints 10 20 30
```
Currently, `fish-parser` requires a variable name to start immediately after `$` with an alphanumeric/underscore identifier, so `$$foo` fails to parse.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  Update `VariableRef` in `ast.rs` to support recursive / indirect dereferencing:
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub enum VariableTarget {
      Named(String),
      Indirect(Box<VariableRef>),
  }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct VariableRef {
      pub target: VariableTarget,
      pub slices: Vec<Slice>,
  }
  ```
  In `grammar.rs`, allow `$` to prefix another `variable_ref()`.
- **`hook` Responsibility**:
  In `hook/src/bash/lowering.rs` & `emitter.rs`:
  In Bash 3.2, indirect expansion is supported via `${!var_name}` for scalar variables. For array dereferencing, lower to an evaluation helper or safe dereference loop.

---

## 5. Standard Special Read-Only Variables

### Problem
In `fish-shell/doc_src/language.rst:1550-1650`:
Fish specifies common environment and status variables:
`$PWD`, `$HOME`, `$USER`, `$EUID`, `$IFS`, `$SHLVL`, `$_`.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  No changes needed. They parse as standard `VariableRef`.
- **`hook` Responsibility**:
  Ensure lowering maps them to Bash equivalents:
  - `$USER` -> `$USER`
  - `$EUID` -> `$EUID`
  - `$PWD` -> `$PWD`
  - `$HOME` -> `$HOME`
  - `$_` -> `$_`
  Ensure they are not incorrectly localized or altered by `hook`'s variable mangler.
