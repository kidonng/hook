# Fish Language Alignment (Phase 1) Design

## Overview

This document specifies the design for Phase 1 of aligning `hook` and `fish-parser` with the Fish shell scripting language documentation (`language.rst`).

Phase 1 focuses on high-frequency syntax compatibility and semantic correctness:
1. **Double Quote Command Substitution**: In Fish, only `$(cmd)` expands inside double quotes; bare `(cmd)` is literal text.
2. **Merged Output Pipes**: `&|` and `|&` merge stdout and stderr into the next pipeline command, lowering to Bash 3.2 compatible `2>&1 |`.
3. **Block-level Redirections**: Redirections applied to `while`, `for`, and `if` blocks (e.g. `while read -l line; ...; end < file`), lowering to Bash `done < file` / `fi < file`.
4. **Permissive Function Options**: Parsing common Fish function attributes such as `-w`/`--wraps`, `-V`/`--inherit-variable`, `-e`/`--on-event`, `-s`/`--on-signal`, `-v`/`--on-variable`, `-j`/`--on-job-exit`.
5. **Special Built-in Variables**: Lowering `$fish_pid` to `$$` and `$last_pid` to `$!`.

---

## 1. Double Quote Command Substitution

### Problem
In `fish-shell/doc_src/language.rst:837`:
> fish also allows spelling command substitutions without the dollar, like `echo (pwd)`. This variant will not be expanded in double-quotes (`echo "(pwd)"` will print `(pwd)`).

Currently in `fish-parser/src/grammar.rs`, `double_quoted_part` matches `command_subst()`, which matches either `$(...)` or `(...)`. This causes `echo "(pwd)"` to be transpiled into `echo "$(pwd)"`, executing `pwd` unexpectedly.

### Solution
- In `fish-parser/src/grammar.rs`, separate command substitution into:
  - `dollar_command_subst`: parses `$(...) [slices]*`
  - `command_subst`: parses `$(...) [slices]*` or `(...) [slices]*`
- In `double_quoted_part`, only accept `dollar_command_subst`. Bare `(...)` remains matching the literal rule.
- Verify that `(cmd)` continues to parse as command substitution in unquoted word positions.

---

## 2. Merged Output Pipes (`&|` and `|&`)

### Problem
In `fish-shell/doc_src/language.rst:240`:
> As a convenience, the pipe `&|` (as well as the `|&` alias which is also supported by Bash) both redirect stdout and stderr to the same process.

Currently, `fish-parser` only parses `|` in `pipe_sep`. `&|` or `|&` causes a syntax error. Furthermore, `|&` is only supported in Bash 4.0+, while `hook` targets Bash 3.2+.

### Solution
- In `fish-parser/src/ast.rs`, represent the pipe connecting commands or attach a flag/mode to `Pipeline` or `Command`.
  Specifically, a pipeline in fish consists of commands separated by pipes. If a pipe is `&|` or `|&`, the preceding command's stderr is redirected into the pipe.
  In Bash 3.2: `cmd1 2>&1 | cmd2`.
- In `fish-parser/src/grammar.rs`:
  Recognize `|&` or `&|` as a pipe separator with stderr redirect. When parsing `cmd1 &| cmd2`, `cmd1` receives an implicit `2>&1` redirection (`RedirectMode::DupOutput` from fd 2 to 1), or `Pipeline` tracks `pipe_modes`.
  Attaching `Redirection { fd: Some(2), mode: RedirectMode::DupOutput, target: Word::from_literal("1") }` to the command preceding `&|`/`|&` cleanly leverages existing redirection lowering and avoids AST breaking changes for consumers of `Pipeline`.

---

## 3. Block-Level Redirections (`while`, `for`, `if`)

### Problem
In `fish-shell/doc_src/language.rst:581`:
> Input and output redirections (including pipes) can also be applied to loops:
> `while read -l line; echo line: $line; end < file`

Currently, `fish-parser` only allows `redirections` on `BeginBlock`. `while_stmt`, `for_stmt`, and `if_stmt` do not parse redirections after `end`.

### Solution
- In `fish-parser/src/ast.rs`:
  - Add `pub redirections: Vec<Redirection>` to `WhileStatement`, `ForStatement`, `IfStatement`.
- In `fish-parser/src/grammar.rs`:
  - `while_stmt`, `for_stmt`, `if_stmt`: after `end !keyword_char()`, parse `redirs:(_* r:redirection() { r })*`.
- In `hook/src/bash/ir.rs`:
  - Add `pub redirections: Vec<LoweredRedirection>` to `LoweredWhile`, `LoweredFor`, `LoweredIf`.
- In `hook/src/bash/lowering.rs`:
  - Lower redirections on `While`, `For`, `If`.
- In `hook/src/bash/emitter.rs`:
  - `LoweredWhile`: emit `done` followed by space-separated redirections.
  - `LoweredFor`: emit `done` followed by space-separated redirections.
  - `LoweredIf`: emit `fi` followed by space-separated redirections.

---

## 4. Permissive Function Options

### Problem
Fish functions commonly declare flags like:
- `-w / --wraps <cmd>`
- `-V / --inherit-variable <var>`
- `-e / --on-event <event>`
- `-s / --on-signal <signal>`
- `-v / --on-variable <var>`
- `-j / --on-job-exit <job>`
- `-S / --no-scope-shadowing`

Currently `fish-parser` only accepts `-a/--argument-names` and `-d/--description`. Any other option fails parsing immediately.

### Solution
- In `fish-parser/src/ast.rs`:
  - Keep `FunctionStatement` clean.
- In `fish-parser/src/grammar.rs`:
  - Update `func_opt` to accept:
    - `-a` / `--argument-names`
    - `-d` / `--description`
    - `-w` / `--wraps` followed by word
    - `-V` / `--inherit-variable` followed by ident
    - `-e` / `--on-event` followed by ident
    - `-s` / `--on-signal` followed by ident
    - `-v` / `--on-variable` followed by ident
    - `-j` / `--on-job-exit` followed by ident
    - `-S` / `--no-scope-shadowing` (flag without arg)
  - Ignored options are parsed and discarded during AST building, allowing valid Fish scripts to parse without error.

---

## 5. Special Built-in Variables (`$fish_pid`, `$last_pid`)

### Problem
In `fish-shell/doc_src/language.rst:1600,1625`:
- `fish_pid`: PID of the shell (Bash: `$$`)
- `last_pid`: PID of the last background process (Bash: `$!`)

Currently they are treated as generic variables (`"$fish_pid"` and `"$last_pid"`), which expand to empty strings in Bash.

### Solution
- In `hook/src/bash/ir.rs`:
  - Add `FishPid` and `LastPid` variants to `LoweredVariableRef`.
- In `hook/src/bash/lowering.rs`:
  - In `lower_variable_ref`:
    - `v.name == "fish_pid" && v.slices.is_empty()` -> `LoweredVariableRef::FishPid`
    - `v.name == "last_pid" && v.slices.is_empty()` -> `LoweredVariableRef::LastPid`
- In `hook/src/bash/emitter.rs`:
  - `FishPid` emits `$$` (or `"$$"`)
  - `LastPid` emits `$!` (or `"$!"`)

---

## Verification & Testing

1. **Unit tests in `fish-parser`**:
   - Verify double quote command substitution: `"(pwd)"` is literal, `"$(pwd)"` is CommandSubst.
   - Verify `&|` and `|&` parsing.
   - Verify `while ... end < file`, `for ... end > file`, `if ... end 2>/dev/null`.
   - Verify function parsing with `-w`, `-V`, `-e`, `-s`, `-v`, `-j`, `-S`.
2. **Integration tests in `hook`**:
   - Verify transpilation of all Phase 1 features.
   - Validate generated code with `bash -n` (Bash 3.2+ syntax validation).
   - Ensure all existing snapshots and tests pass without regressions.
