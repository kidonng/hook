# Design Specification: Fish to Bash Transpiler (`hook`)

## 1. Overview and Goals

`hook` is a command-line utility written in Rust that translates fish shell scripts and snippets into idiomatic, compatible Bash 3.2+ shell scripts. It operates as a standard Unix filter (reading from a file argument or standard input, writing to standard output).

### Core Goals
- **Clean Separation of Concerns**: Partition the problem into an independent AST parsing library (`fish-parser`) and a code generation/CLI executable (`hook`).
- **Semantic Fidelity**: Accurately translate Fish language constructs (variables, arrays, slicing, functions, conditionals, loops, pipelines, command/process substitutions) to Bash equivalents.
- **Bash 3.2 Compatibility**: Strictly avoid Bash 4.0+ features (e.g., negative array subscripts, `mapfile`, `${var,,}`, `|&`, `declare -A`) to maintain compatibility with legacy environments such as macOS `/bin/bash`.
- **Snapshot-Driven Testing**: Use `insta` for end-to-end regression testing and snapshot validation of transpilation outputs alongside `bash -n` syntax validation.

---

## 2. Workspace Architecture

The project is structured as a Cargo workspace with two members:

```
.
├── Cargo.toml
├── crates/
│   ├── fish-parser/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ast.rs
│   │       └── grammar.rs
│   └── hook/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           └── bash/
│               ├── mod.rs
│               ├── ir.rs
│               ├── lowering.rs
│               └── emitter.rs
├── specs/
│   └── 2026-09-04-fish-to-bash-hook-design.md
└── tests/
    ├── snapshots/
    └── transpile_test.rs
```

### Dependencies
- **Workspace**:
  - `serde = { version = "1.0", features = ["derive"] }`
  - `peg = "0.8"`
  - `insta = { version = "1.40", features = ["yaml"] }`
- **`crates/fish-parser`**:
  - `serde`: Serialization and deserialization for all AST nodes.
  - `peg`: Parsing Expression Grammar definitions.
- **`crates/hook`**:
  - `fish-parser = { path = "../fish-parser" }`
  - `insta`: Dev-dependency for snapshot testing.

---

## 3. Abstract Syntax Tree (`fish-parser::ast`)

All AST nodes implement `Debug`, `Clone`, `PartialEq`, `Serialize`, and `Deserialize`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    Pipeline(Pipeline),
    If(IfStatement),
    Switch(SwitchStatement),
    For(ForStatement),
    While(WhileStatement),
    Function(FunctionStatement),
    BeginBlock(BeginBlock),
    Return(Option<Word>),
    Break,
    Continue,
    Comment(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pipeline {
    pub commands: Vec<Command>,
    pub combinator: Combinator,
    pub background: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Combinator {
    None,
    And, // && or leading `and`
    Or,  // || or leading `or`
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub negate: bool, // `not` prefix
    pub args: Vec<Word>,
    pub redirections: Vec<Redirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub parts: Vec<WordPart>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WordPart {
    Literal(String),
    SingleQuoted(String),
    DoubleQuoted(Vec<WordPart>),
    Variable(VariableRef),
    CommandSubst(Vec<Statement>), // (cmd) or $(cmd)
    BraceExpansion(Vec<Word>),    // {a,b,c}
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableRef {
    pub name: String,
    pub slices: Vec<Slice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Slice {
    Index(SliceIndex),
    Range {
        start: Option<SliceIndex>,
        end: Option<SliceIndex>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SliceIndex {
    Pos(usize), // 1-based positive index
    Neg(usize), // 1-based negative index (e.g. -1 is last)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Redirection {
    pub fd: Option<u32>,
    pub mode: RedirectMode,
    pub target: Word,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RedirectMode {
    Output,        // >
    Append,        // >>
    Input,         // <
    OutputAndErr,  // &> or ^
    AppendAndErr,  // &>>
    DupOutput,     // >&
    DupInput,      // <&
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfStatement {
    pub condition: Pipeline,
    pub then_body: Vec<Statement>,
    pub elif_branches: Vec<(Pipeline, Vec<Statement>)>,
    pub else_body: Option<Vec<Statement>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchStatement {
    pub value: Word,
    pub cases: Vec<CaseClause>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseClause {
    pub patterns: Vec<Word>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForStatement {
    pub variable: String,
    pub values: Vec<Word>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhileStatement {
    pub condition: Pipeline,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionStatement {
    pub name: String,
    pub named_args: Vec<String>, // parsed from -a/--argument-names
    pub description: Option<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeginBlock {
    pub body: Vec<Statement>,
    pub redirections: Vec<Redirection>,
}
```

---

## 4. PEG Grammar Specification (`fish-parser::grammar`)

The grammar is implemented using `rust-peg` in `fish-parser/src/grammar.rs`.

### Lexical & Formatting Rules
- **Whitespace & Separators**: Horizontal whitespace (spaces, tabs) separates words. Statements are separated by newlines `\n`, semicolons `;`, or EOF.
- **Comments**: `# ...` runs to the end of the line, preserved as `Statement::Comment`.
- **Escapes**: `\n`, `\t`, `\r`, `\ `, `\"`, `\'`, `\\`, etc., inside unquoted and double-quoted contexts.
- **Command Substitutions**: Unescaped `(cmd)` and `$(cmd)` map to `WordPart::CommandSubst`.
- **Blocks**: Statements enclosed by keywords `if`, `switch`, `for`, `while`, `function`, and `begin` all terminate with the keyword `end`.
- **`and` / `or` Statement Prefixes**:
  When a statement begins with keyword `and` or `or`, it is parsed as a Pipeline with `Combinator::And` or `Combinator::Or` attached to the preceding pipeline context.

---

## 5. Intermediate Representation & Lowering (`hook::bash`)

Before emitting Bash code, statements undergo a lowering pass to specialize Fish-specific idioms into explicit intermediate representations.

### 5.1 Specialization of `set` Statements
Fish commands beginning with `set` are analyzed and mapped to `AssignmentIR`:
```rust
pub enum AssignmentIR {
    Local { name: String, values: Vec<Word> },      // set -l
    Export { name: String, values: Vec<Word> },     // set -x / set -gx
    Global { name: String, values: Vec<Word> },     // set / set -g
    Append { name: String, values: Vec<Word> },     // set -a / --append
    Prepend { name: String, values: Vec<Word> },    // set -p / --prepend
    Erase { name: String },                         // set -e / --erase
}
```

#### Bash Code Generation for `AssignmentIR`:
- Single scalar value:
  - `Local`: `local var="value"`
  - `Export`: `export var="value"`
  - `Global`: `var="value"`
- Multiple values (Arrays):
  - `Local`: `local -a var=("val1" "val2")`
  - `Export`: `export var="val1"` (or serialized if array, noting Bash export restrictions)
  - `Global`: `var=("val1" "val2")`
- `Append`: `var+=("val1" "val2")`
- `Prepend`: `var=("val1" "val2" "${var[@]}")`
- `Erase`: `unset var`

### 5.2 Process Substitution Lowering `(cmd | psub)`
When a `WordPart::CommandSubst` contains a single pipeline whose terminal command is `psub` (e.g., `(sort file.txt | psub)` or `(cmd | psub -f)`):
- The terminal `| psub` is stripped.
- The construct is emitted as Bash process substitution: `<(sort file.txt)`.

### 5.3 Built-in Variables & Arguments Lowering
- **`$status`** $\to$ `$?`
- **`$pipestatus`** $\to$ `"${PIPESTATUS[@]}"`
- **`$argv` & Slicing**:
  - `$argv` $\to$ `"$@"`
  - `$argv[1]` $\to$ `"$1"`
  - `$argv[n]` $\to$ `"$n"` (for constant positive index `n`)
  - `$argv[start..end]` $\to$ `"${@:start:length}"` where `start` is 1-based offset.
  - `$argv[2..]` or `$argv[2..-1]` $\to$ `"${@:2}"`
  - `$argv[-1]` $\to$ `"${@: -1:1}"` (note leading space before `-1` to avoid `${var:-default}`)

### 5.4 Bash 3.2 Array Indexing & Slicing Compatibility
Fish uses 1-based indexing; Bash uses 0-based indexing.
- **Positive Index**: `$var[1]` $\to$ `"${var[0]}"`, `$var[n]` $\to$ `"${var[n-1]}"`.
- **Positive Range**: `$var[1..3]` $\to$ `"${var[@]:0:3}"` (`offset = start - 1`, `length = end - start + 1`).
- **Open Range**: `$var[2..]` $\to$ `"${var[@]:1}"`.
- **Negative Index**:
  Because Bash 3.2 generates `bad array subscript` on negative array indices such as `${var[-1]}`, negative indices must be lowered using dynamic array length arithmetic:
  - `$var[-1]` $\to$ `"${var[$((${#var[@]}-1))]}"`
  - Negative index `-k` $\to$ `"${var[$((${#var[@]}-k))]}"`

### 5.5 Control Structures Lowering
- **`if`**:
  ```bash
  if <condition>; then
      <then_body>
  elif <elif_condition>; then
      <elif_body>
  else
      <else_body>
  fi
  ```
- **`switch`**:
  ```bash
  case <value> in
      pattern1|pattern2)
          <body>
          ;;
      *)
          <default_body>
          ;;
  esac
  ```
- **`for`**:
  ```bash
  for var in <values>; do
      <body>
  done
  ```
- **`while`**:
  ```bash
  while <condition>; do
      <body>
  done
  ```
- **`function`**:
  ```bash
  func_name() {
      local arg1="$1"
      local arg2="$2"
      <body>
  }
  ```
- **`begin ... end`**:
  Grouped into curly brace compound command:
  ```bash
  {
      <body>
  } [redirections]
  ```

---

## 6. Command-Line Interface (`crates/hook/src/main.rs`)

### Invocations
```bash
# File argument
hook <input_file.fish>

# Stdin filter
cat <input_file.fish> | hook
hook < <input_file.fish>

# Flags
hook -h / --help
hook -V / --version
```

### Exit Codes
- `0`: Transpilation succeeded; generated Bash code written to `stdout`.
- `1`: Syntax / Parse error encountered; error message with line and column written to `stderr`.
- `2`: I/O error (e.g., file not found, permission denied); written to `stderr`.

---

## 7. Testing & Verification Strategy

### 7.1 Unit Snapshot Tests (`crates/fish-parser`)
Using `insta` to freeze parsed AST structures against regressions:
- Validates basic and complex syntax constructs.
- Verifies quote nesting and variable slice expressions.

### 7.2 End-to-End Transpilation Snapshot Tests (`tests/transpile_test.rs`)
Using `insta::assert_snapshot!` to match Fish inputs against expected Bash outputs:
1. `variables_and_arrays`: `set -l`, `set -gx`, `set -a`, `set -p`, `set -e`, `$var[1]`, `$var[-1]`, `$var[2..4]`.
2. `builtins_and_args`: `$status`, `$argv`, `$argv[1]`, `$argv[2..-1]`, `$pipestatus`.
3. `process_substitution`: `(sort f | psub)` $\to$ `<(sort f)`.
4. `control_flow`: `if/elif/else`, `switch/case`, `for`, `while`, `and`/`or` chains.
5. `functions`: `function test_fn -a foo bar; end`.

### 7.3 Bash Syntax Validation Check
Every integration test feeds the transpiled Bash script into `bash -n` via `std::process::Command` to guarantee that all emitted scripts are syntactically valid in Bash.
