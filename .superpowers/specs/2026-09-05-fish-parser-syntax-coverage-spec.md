# Fish Parser Syntax Coverage Specification

## Overview

This specification documents the syntactic features of the Fish shell scripting language (as specified in `fish-shell/doc_src/language.rst` and the reference implementation in `fish-shell/src/`) that are currently unhandled or incorrectly handled in `crates/fish-parser`.

In accordance with `AGENTS.md`:
- `crates/fish-parser` is strictly a pure, high-fidelity syntax parser.
- The parser must faithfully reflect Fish syntax into a strongly typed AST without performing target-specific desugaring or semantic lowering.
- Target-specific lowering (e.g. into modern Bash 5.0+ constructs) belongs downstream in `crates/hook`.

---

## 1. Pipes and Redirections

### 1.1 Custom File Descriptor Pipes (`N>|` and `>|`)

#### Problem
In `fish-shell/doc_src/language.rst:234-239`:
> It is possible to pipe a different output file descriptor by prepending its FD number and the output redirect symbol to the pipe. For example:
> `make fish 2>| less`
> will attempt to build `fish`, and any errors will be shown using the `less` pager.

Currently, `PipeOperator` in `crates/fish-parser/src/ast.rs` only defines:
```rust
pub enum PipeOperator {
    Stdout,
    StdoutAndStderr,
}
```
`pipe_op` in `grammar.rs` only recognizes `|`, `&|`, and `|&`. Syntax like `2>|` or `>|` fails to parse (`ParseError`).

#### Specification & Design
1. **AST Update**:
   Update `PipeOperator` in `crates/fish-parser/src/ast.rs` to support explicit file descriptors:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
   pub enum PipeOperator {
       Stdout,
       StdoutAndStderr,
       Fd(u32),
   }
   ```
2. **Grammar Update**:
   In `crates/fish-parser/src/grammar.rs`:
   Update `pipe_op` rule to parse `N>|` and `>|`:
   ```rust
   rule pipe_op() -> PipeOperator
       = ("&|" / "|&") { PipeOperator::StdoutAndStderr }
       / fd:(n:$(['0'..='9']+) { n.parse::<u32>().unwrap() })? ">|" {
           PipeOperator::Fd(fd.unwrap_or(1))
       }
       / "|" { PipeOperator::Stdout }
   ```
   Update `pipe_sep` and `pipeline` to record `PipeOperator` directly.

---

### 1.2 Compound & Block Statements in Pipelines

#### Problem
In `fish-shell/doc_src/language.rst:581`:
> Input and output redirections (including pipes) can also be applied to loops:
> `while read -l line; echo line: $line; end < file`

In Fish, any compound statement or block (`begin ... end`, `while ... end`, `for ... end`, `if ... end`, `{ ... }`) can appear as any segment of a pipeline, e.g.:
```fish
begin; echo a; echo b; end | grep a
cat file | while read -l line; echo "got: $line"; end
```
Currently in `fish-parser`:
- `Pipeline` is defined as `pub commands: Vec<Command>`.
- In `grammar.rs`, `pipeline` matches `head:command() tail:(sep:pipe_sep() cmd:command())*`.
- `command` forbids `reserved_keyword()`, which immediately rejects `begin`, `while`, `for`, `if`, etc.

#### Specification & Design
1. **AST Representation**:
   Introduce `PipelineElement` (or generalize `Pipeline` segments) so that pipeline items can be either a simple command or a block statement:
   ```rust
   #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
   pub enum PipelineElement {
       Command(Command),
       Block(Statement),
   }
   ```
   Or allow `Pipeline` to hold `Vec<PipelineElement>`.
2. **Grammar Update**:
   Allow each stage of a pipeline to parse either a compound block (`begin`, `{ ... }`, `if`, `for`, `while`) or a regular command.

---

### 1.3 Extended Redirections (`&>?`, `&>>?`, `>>&`)

#### Problem
In `fish-shell/src/tokenizer.rs:1001-1061`:
- `&>?` and `&>>?`: Noclobber redirections with stderr merge.
- `>>&`: Appending file descriptor redirection (functionally equivalent to `>&`).

`RedirectMode` in `ast.rs` currently lacks representation for noclobber append with stderr merge and appending fd dup.

#### Specification & Design
1. **AST Update**:
   Extend `RedirectMode`:
   ```rust
   pub enum RedirectMode {
       Output,
       Append,
       Input,
       OutputAndErr,
       AppendAndErr,
       DupOutput,
       DupInput,
       SafeInput,
       NoClobberOutput,
       NoClobberAppend,
       NoClobberOutputAndErr,  // &>?
       NoClobberAppendAndErr,  // &>>?
   }
   ```
2. **Grammar Update**:
   In `redirect_mode()` rule:
   - Match `&>>?` -> `RedirectMode::NoClobberAppendAndErr`
   - Match `&>?` -> `RedirectMode::NoClobberOutputAndErr`
   - Match `>>&` -> `RedirectMode::DupOutput`

---

### 1.4 Redirections on `switch` Statements

#### Problem
In Fish, `switch` statements support trailing redirections just like any other block:
```fish
switch $x
    case a
        echo a
end >/dev/null
```
Currently, `SwitchStatement` in `ast.rs` has:
```rust
pub struct SwitchStatement {
    pub value: Word,
    pub cases: Vec<CaseClause>,
}
```
Notice `redirections` is missing entirely.

#### Specification & Design
1. **AST Update**:
   Add `pub redirections: Vec<Redirection>` to `SwitchStatement`.
2. **Grammar Update**:
   In `switch_stmt()`, parse `redirs:(_* r:redirection() { r })*` after `"end" !keyword_char()` and populate `redirections`.

---

## 2. Parameter Expansion & Slices

### 2.1 Multi-Index & Multi-Range Slices (Space-Separated)

#### Problem
In `fish-shell/doc_src/language.rst:1011-1030`:
> Multiple ranges are also possible, separated with a space.
> `echo (seq 10)[2..5 1..3]`
> `echo (seq 10)[1 2 3]`
> `set l a b c d e f; echo $l[1 3..5 2]`

Currently, `Slice` in `ast.rs` is:
```rust
pub enum Slice {
    Index(SliceIndex),
    Range {
        start: Option<SliceIndex>,
        end: Option<SliceIndex>,
    },
}
```
And `slice()` in `grammar.rs` only parses a single `slice_index` or `start..end` inside `[...]`.
If a bracket contains multiple space-separated indices/ranges (e.g. `[1 3..5 2]`), parsing fails immediately.

#### Specification & Design
1. **AST Representation**:
   Distinguish between a slice bracket `[...]` and the elements inside it:
   ```rust
   #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
   pub struct Slice {
       pub elements: Vec<SliceElement>,
   }

   #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
   pub enum SliceElement {
       Index(SliceIndex),
       Range {
           start: Option<SliceIndex>,
           end: Option<SliceIndex>,
       },
   }
   ```
2. **Grammar Update**:
   In `grammar.rs`:
   - `slice_element`: parses `start..end` or `slice_index`.
   - `slice`: `"[" _* elements:(slice_element() ++ (_+)) _* "]"` -> `Slice { elements }`.

---

## 3. Variables & Identifiers

### 3.1 Leading Digits in Variable Names

#### Problem
In `fish-shell/doc_src/language.rst:1135` and `1988`:
> A variable name cannot be empty and can contain only letters, digits, and underscores. It may begin and end with any of those characters.

In Fish, `set 1 foo; echo $1` is completely valid.
Currently in `fish-parser/src/grammar.rs`:
```rust
rule variable_ref() -> WordPart
    = ...
    / "$" name:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) slices:slice()*
```
The first character is restricted to `[a-zA-Z_]`. Variable `$1` fails to parse as a variable reference.

#### Specification & Design
1. **Grammar Update**:
   Allow variable names to start with digits:
   ```rust
   rule var_name() -> String
       = s:$(['a'..='z' | 'A'..='Z' | '0'..='9' | '_']+) { s.to_string() }
   ```
   Ensure `variable_ref` uses `var_name()`.

---

### 3.2 Literal Braces vs. Brace Expansion

#### Problem
In `fish-shell/doc_src/language.rst:896-905`:
> If there is no "," or variable expansion between the curly braces, they will not be expanded:
> ```fish
> # This {} isn't special
> > echo foo-{}
> foo-{}
> # This passes "HEAD@{2}" to git
> > git reset --hard HEAD@{2}
> ```

Currently in `fish-parser`:
`brace_expansion()` matches any `"{" ... "}"` and produces `WordPart::BraceExpansion`.
`HEAD@{2}` is parsed as `Literal("HEAD@")` + `BraceExpansion([Word("2")])`.
`foo-{}` is parsed as `Literal("foo-")` + `BraceExpansion([])`.

#### Specification & Design
1. **Grammar Rule**:
   A `{...}` construct is only a `WordPart::BraceExpansion` if:
   - It contains at least one comma `,` (e.g. `{a,b}`, `{a,}`, `{,}`), OR
   - It contains at least one variable expansion (e.g. `{$foo}`).
   Otherwise, `{...}` must be treated as a literal `WordPart::Literal("{...}")`.

---

### 3.3 Command-Scoped Environment Overrides (`VAR=VAL cmd`)

#### Problem
In `fish-shell/doc_src/language.rst:1284-1313`:
> If you want to override a variable for a single command, you can use "var=val" statements before the command:
> `GIT_DIR=somerepo git status`
> `PATH={/usr,}/{s,}bin bash`

In `fish-shell/src/ast.rs`, the AST explicitly models this:
```rust
pub struct JobPipeline {
    pub variables: VariableAssignmentList,
    pub statement: Statement,
    ...
}
```
Currently in `fish-parser`, `Command` only contains `args: Vec<Word>` and `redirections: Vec<Redirection>`. Leading `VAR=VAL` tokens are indistinguishable from regular arguments in the AST.

#### Specification & Design
1. **AST Update**:
   Define `VariableAssignment`:
   ```rust
   #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
   pub struct VariableAssignment {
       pub name: String,
       pub value: Word,
   }
   ```
   Add `pub assignments: Vec<VariableAssignment>` to `Command` (or `PipelineElement`).
2. **Grammar Update**:
   Before matching command arguments, parse zero or more `ident = word` expressions where the identifier is immediately followed by `=`.

---

## 4. Quotes and Escapes

### 4.1 Single Quote Backslash Escaping (`\\`)

#### Problem
In `fish-shell/doc_src/language.rst:77`:
> The only meaningful escape sequences in single quotes are `\'`, which escapes a single quote and `\\`, which escapes the backslash symbol.

Testing in Fish confirms: `echo 'a\\b'` outputs `a\b`.
Currently in `fish-parser/src/grammar.rs`:
```rust
rule single_quoted() -> WordPart
    = "'" s:$(( [^'\''] / "\\'" )*) "'" {
        WordPart::SingleQuoted(s.replace("\\'", "'"))
    }
```
`\\` is not treated as an escape for `\`.

#### Specification & Design
Update single-quoted parser to unescape `\\` into `\` and `\'` into `'`.

---

### 4.2 Preserving Literal Escape Sequences Inside Double Quotes

#### Problem
In `fish-shell/doc_src/language.rst:75-78`:
> Between double quotes, fish only performs variable expansion and command substitution in the `$(command)`. No other kind of expansion (including brace expansion or parameter expansion) is performed, and escape sequences (for example, `\n`) are ignored.
> The only meaningful escapes in double quotes are `\"`, which escapes a double quote, `\$`, which escapes a dollar character, `\` followed by a newline, which deletes the backslash and the newline, and `\\`, which escapes the backslash symbol.

In Fish: `echo "a\nb"` prints literal `a\nb`.
Currently in `fish-parser/src/grammar.rs`:
`double_quoted_part` invokes `unescape(s)`, converting `\n`, `\t`, `\r` into literal control characters.

#### Specification & Design
Inside double quotes:
- Only unescape `\"` -> `"`, `\$` -> `$`, `\\` -> `\`, and line continuations `\` + `\n`.
- Do NOT unescape `\n`, `\t`, `\r`, `\e`, or other escape sequences; preserve them verbatim.

---

### 4.3 Comprehensive Unquoted Escape Sequences

#### Problem
In `fish-shell/doc_src/language.rst:98-114`:
Fish supports:
- `\a`: alert (BEL, `\x07`)
- `\e`: escape (ESC, `\x1b`)
- `\f`: form feed (`\x0c`)
- `\n`: newline (`\x0a`)
- `\r`: carriage return (`\x0d`)
- `\t`: tab (`\x09`)
- `\v`: vertical tab (`\x0b`)
- `\xHH` / `\XHH`: hex byte
- `\ooo`: octal
- `\uXXXX`: 16-bit unicode
- `\UXXXXXXXX`: 32-bit unicode
- `\cX`: control sequence

Currently, `unescape()` in `grammar.rs` only handles `\n`, `\t`, and `\r`. Sequences like `\e` (frequently used for terminal colors) degenerate into the literal character `'e'`, producing invalid output.

#### Specification & Design
Implement full Fish unescape logic for unquoted literals according to the table in `language.rst:98-114`.

---

## 5. Statements & Control Flow

### 5.1 Pipeline-Level Negation Scope

#### Problem
In `fish-shell/doc_src/language.rst:457`:
`not` inverts the exit status of the entire pipeline, not merely the first command:
```fish
not true | false   # $status is 0
not false | true   # $status is 1
```
Currently, `negate: bool` is placed inside `Command`, and `pipeline` sets `commands[0].negate = true`. This is structurally inaccurate and misrepresents pipeline-level negation.

#### Specification & Design
1. **AST Update**:
   Move `negate: bool` from `Command` to `Pipeline`.
2. **Grammar Update**:
   Set `negate` on the resulting `Pipeline` struct.

---

### 5.2 Empty Iteration List in `for` Loops

#### Problem
In `fish-shell/doc_src/language.rst:560-572`:
A `for` loop with no values (or an empty variable expansion) is valid syntax in Fish:
```fish
for x in; echo $x; end
```
Currently in `grammar.rs`:
`rule for_stmt() = ... "in" _+ vals:(word() ++ (_+)) statement_sep()`
The `++` operator requires at least 1 word, causing a syntax error when no words follow `in`.

#### Specification & Design
Change `vals:(word() ++ (_+))` to `vals:(word() ** (_+))`.

---

### 5.3 Context-Sensitive Background Operator (`&`)

#### Problem
In `fish-shell/doc_src/language.rst:299` and `2059` (`ampersand-nobg-in-token`):
> If the `&` character is followed by a non-separating character, it is not interpreted as background operator. Separating characters are whitespace and the characters `;<>&|`.

Currently in `grammar.rs`:
`bg:(_* "&" !['&' | '>'])?`
An ampersand inside a token (like in a query parameter or URL) can be prematurely matched as a background operator if whitespace precedes it.

#### Specification & Design
Ensure `&` is only matched as a background operator when followed by end of statement (newline, `;`), EOF, or separating characters.

---

## 6. Implementation Phasing Strategy

To manage risk and prevent regressions in downstream crates (`crates/hook`), implementation will proceed in distinct phases:

1. **Phase A: Grammar & Parser Correctness (Non-breaking AST changes)**
   - Fix single-quoted `\\` escape.
   - Fix double-quoted escape preservation.
   - Fix unquoted escape sequences (`\e`, `\a`, `\xHH`, `\uXXXX`).
   - Allow leading digits in variable names (`$1`).
   - Fix literal `{}` and `HEAD@{2}` brace expansion filtering.
   - Allow empty value list in `for` loops (`word() ** (_+)`).
   - Add redirections to `SwitchStatement`.

2. **Phase B: Redirections & Slices Extension**
   - Add `PipeOperator::Fd(u32)` and parse `2>|` / `>|`.
   - Add extended redirection modes (`&>?`, `&>>?`, `>>&`).
   - Refactor `Slice` to support multiple space-separated elements `[2..5 1..3]`.

3. **Phase C: Pipeline Architecture Refactor**
   - Move `negate` to `Pipeline`.
   - Add `VariableAssignment` to support `VAR=VAL cmd`.
   - Allow compound blocks in pipeline stages (`PipelineElement`).
   - Update `hook`'s lowering phase to consume the enhanced AST.
