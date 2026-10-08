# Fish Language Alignment (Phase 3) Design: Built-in Lowering & Advanced Parameter Expansions

## Overview

This specification details Phase 3 of aligning `hook` and `fish-parser` with the Fish shell scripting language documentation (`language.rst`).

Phase 3 focuses on high-level Fish built-in lowering, advanced parameter expansion rules, safe redirections, and compound statement blocks:
1. **Built-in `count` Lowering & Optimization**: Special-casing `count $var` and `count $argv` into native Bash array/parameter length operations (`${#var[@]}`, `$#`).
2. **Built-in `contains` Lowering**: Translating `contains <needle> <haystack...>` into Bash 3.2 search loops or helper functions.
3. **Cartesian Product & Empty List Cancellation**: Managing unquoted adjacent list expansions and empty list token cancellation.
4. **Process Substitution (`psub`) Generalization**: Supporting options like `-f` (FIFO).
5. **Safe Input Redirection (`<?`) & Noclobber Redirection (`>?`)**: Translating Fish-specific redirection modifiers into Bash 3.2 idioms.
6. **Compound Statement Blocks (`{ cmd; and cmd; }`)**: Parsing leading `{` as a compound statement block distinct from brace expansion.

---

## 1. Built-in `count` Lowering & Optimization

### Problem
In `fish-shell/doc_src/language.rst:1425`:
```fish
count $smurf
# prints 2
if count $foos >/dev/null
    ls $foos
end
```
Currently in `hook`, `count` is emitted as an external or undefined command call in Bash unless a custom user function exists.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  Parses `count $smurf` as a regular `Command` with arguments. The parser does not special-case built-ins.
- **`hook` Responsibility**:
  In `hook/src/bash/lowering.rs`:
  Inspect the command name:
  - If command is `count $argv`:
    Lower to `echo "$#"` (or `printf '%s\n' "$#"`).
    In condition contexts (like `if count $argv >/dev/null`): lower to `[ "$#" -gt 0 ]`.
  - If command is `count $var`:
    Lower to `echo "${#var[@]}"`.
    In condition contexts: lower to `[ "${#var[@]}" -gt 0 ]`.
  - General `count <args...>`:
    Lower to `set -- <args...>; echo "$#"`.

---

## 2. Built-in `contains` Lowering

### Problem
In `fish-shell/doc_src/language.rst:1428`:
```fish
contains blue $smurf # returns status 0 if found, 1 otherwise
contains -i blue $smurf # prints 1-based index if found
```
Bash has no native `contains` builtin.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  Parses `contains` as a standard `Command`.
- **`hook` Responsibility**:
  In `hook/src/bash/lowering.rs`:
  - Detect `contains` commands and emit a runtime helper function if required, or an inline `for item in "${arr[@]}"; do [ "$item" = "$needle" ] && ...; done`.
  - Provide a standardized helper preamble for Bash scripts utilizing `contains`.

---

## 3. Cartesian Product & Empty List Cancellation

### Problem
In `fish-shell/doc_src/language.rst:926-985`:
```fish
set -l foo x y z
echo 1$foo # 1x 1y 1z
set -l a x y z; set -l b 1 2 3
echo $a$b # x1 y1 z1 x2 y2 z2 x3 y3 z3
set -l c # empty list
echo {$c}word # outputs empty line (word disappears!)
```
In Fish, unquoted variables expand to multiple arguments. When concatenated with other strings or variables, Fish computes the Cartesian product across all elements. If an unquoted variable is an empty list, the attached token is completely cancelled out.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  Faithfully represents words with multiple concatenated `WordPart` items.
- **`hook` Responsibility**:
  In `hook/src/bash/lowering.rs`:
  When a word contains adjacent variable references or literals without quotes, emit a helper or expansion loop if static expansion is not possible.

---

## 4. Safe Input (`<?`) and Noclobber (`>?`) Redirections

### Problem
In `fish-shell/doc_src/language.rst:168, 173`:
```fish
string match '*foo*' <?myfile # read from file or /dev/null if not readable
echo hello >?output.txt       # noclobber: error if file already exists
```
Currently, `fish-parser` does not distinguish `<?` or `>?`.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  Add `SafeInput` and `NoClobber` variants to `RedirectMode` in `ast.rs`:
  ```rust
  pub enum RedirectMode {
      Output,
      Append,
      Input,
      SafeInput,        // <?
      NoClobberOutput,  // >?
      NoClobberAppend,  // 2>?
      ...
  }
  ```
  In `grammar.rs`, update `redirect_mode` to parse `<?`, `>?`, and `2>?`.
- **`hook` Responsibility**:
  In `hook/src/bash/emitter.rs`:
  - `SafeInput`: In Bash, lower to `< "$([ -r "$target" ] && echo "$target" || echo /dev/null)"` or equivalent Bash 3.2 idiom.
  - `NoClobber`: Lower to `>|` or handle with `set -o noclobber`.

---

## 5. Compound Statement Blocks (`{ ... }`)

### Problem
In `fish-shell/doc_src/language.rst:918`:
```fish
{echo hello, && echo world}
```
Fish 3.x allows `{ ... }` as an alternative syntax for `begin ... end`.

### Architectural Boundary Design
- **`fish-parser` Responsibility**:
  Distinguish leading `{` from brace expansion in `statement`:
  Add `compound_block_stmt` that parses `{` followed by `statement_list()` and `}` with optional redirections.
- **`hook` Responsibility**:
  Lowers compound statement block identically to `BeginBlock`, emitting `{ ...; }` in Bash.
