# Modern Bash (5.0+) Target Architecture Design

## 1. Overview & Motivation

`hook` was initially implemented with a strict target constraint of legacy Bash 3.2 (primarily for compatibility with macOS's default `/bin/bash` without external dependencies). However, legacy Bash 3.2 forced significant compromises on transpilation fidelity and code readability:
- Negative array indices had to be lowered into convoluted dynamic length arithmetic (e.g. `${var[$((${#var[@]}-1))]}`).
- Merged standard error and standard output pipes had to inject synthetic `2>&1` redirections rather than native `|&`.
- Global variable assignments inside functions could not use `declare -g`.
- Merged redirections had to emit trailing `2>&1` rather than native `&>` and `&>>`.

To achieve the core objective of **maximizing 1:1 structural fidelity between Fish and Bash**, the transpilation engine is transitioning to target **modern Bash (5.0+)** by default. A modular target architecture (`Target` configuration) will be established to allow subsequent re-introduction of a legacy Bash 3.2 target via a dedicated desugaring pass.

---

## 2. Architecture & Separation of Concerns

```
[Fish Shell Script]
       │
       ▼ (crates/fish-parser)
 [Fish AST (Pure)]
       │
       ▼ (crates/hook::bash::lowering)
[Modern Bash IR (1:1 with Fish)]
       │
       ├─────────────────────────────────────────┐
       ▼ (Target::Bash5)                         ▼ (Target::Bash3_2 - Future)
[Modern Bash Emitter]                  [Desugaring Pass to 3.2]
       │                                         │
       ▼                                         ▼
[Clean Bash 5.0+ Script]               [Legacy 3.2 Emitter]
```

### Key Principles
1. **High Structural Fidelity**: Generated modern Bash code mirrors Fish constructs 1:1 wherever possible.
2. **Zero Synthetic Hacks in Modern Path**: No synthetic `2>&1` pipes or length arithmetic calculations in the default pipeline.
3. **Target Config Extensibility**: The `Target` enum and CLI flag `--target` provide clean configuration without cluttering code with version guards.

---

## 3. Detailed Component Specifications

### 3.1 CLI & Target Configuration (`crates/hook/src/target.rs` & `main.rs`)

#### Target Types
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    Bash5,
    Bash3_2,
}

#[derive(Debug, Clone, Default)]
pub struct TranspileConfig {
    pub target: Target,
}
```

#### CLI Interface
- Command syntax: `hook [--target <bash5|bash3.2>] [FILE]`
- Default: `Target::Bash5` when omitted.
- Target `bash3.2`: Returns informative notice that modern Bash 5.0+ is the primary target and 3.2 support will follow in a subsequent phase.
- Help text: Update description to `hook - Transpile Fish shell scripts to modern Bash (5.0+)`.

---

### 3.2 Intermediate Representation (IR) Modernization (`crates/hook/src/bash/ir.rs`)

1. **Pipeline & Pipe Operators**:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum PipeKind {
       Stdout,
       StdoutAndStderr,
   }

   #[derive(Debug, Clone, PartialEq)]
   pub struct LoweredPipeline {
       pub commands: Vec<LoweredCommand>,
       pub pipe_operators: Vec<PipeKind>,
       pub combinator: Combinator,
       pub background: bool,
   }
   ```

2. **Array Subscripts & Negative Indices**:
   ```rust
   #[derive(Debug, Clone, PartialEq)]
   pub enum SliceIndexIR {
       ZeroBased(usize),
       Negative(isize),       // -1, -2, etc. directly
       Dynamic(String),       // $idx
   }

   #[derive(Debug, Clone, PartialEq)]
   pub enum BashSubscript {
       Index(isize),          // Positive or negative offset: 0, 1, -1, -2
       Range { offset: isize, length: usize },
       OpenRange { offset: isize },
       DynamicVariable(String),
       DynamicRange { start: String, end: String },
       All,
   }
   ```

3. **Global Scope Elevation in Functions**:
   ```rust
   #[derive(Debug, Clone, PartialEq)]
   pub enum AssignmentIR {
       Local { name: String, values: Vec<LoweredWord> },
       Export { name: String, values: Vec<LoweredWord> },
       Global { name: String, values: Vec<LoweredWord>, in_function: bool },
       Append { name: String, values: Vec<LoweredWord> },
       Prepend { name: String, values: Vec<LoweredWord> },
       Erase { name: String },
       ArgvLast { value: LoweredWord },
       SliceAssign { name: String, index: SliceIndexIR, value: LoweredWord },
       SliceErase { name: String, index: SliceIndexIR },
   }
   ```

---

### 3.3 AST Lowering Updates (`crates/hook/src/bash/lowering.rs`)

1. **Pipelines**:
   - `lower_pipeline`: Preserve `PipeOperator::StdoutAndStderr` as `PipeKind::StdoutAndStderr`. Stop injecting synthetic `2>&1` redirection into the preceding command.
2. **Assignments & Slices**:
   - Negative index literal `var[-k]` in `set` commands: Lower directly to `SliceIndexIR::Negative(-k)`.
   - `set -g var val` inside function scope (`scope.in_function == true`): Set `in_function: true` on `AssignmentIR::Global`.
3. **Variable References**:
   - `$var[-k]` lowers to `BashSubscript::Index(-k)`.
   - Dynamic variable `$var[$idx]` lowers to `BashSubscript::DynamicVariable(idx)` where Bash 5 can evaluate arithmetic directly.

---

### 3.4 Emitter Modernization (`crates/hook/src/bash/emitter.rs`)

1. **Pipeline & Pipe Operators**:
   - Emit ` |& ` when `pipe_operator == PipeKind::StdoutAndStderr`.
   - Emit ` | ` when `pipe_operator == PipeKind::Stdout`.
2. **Merged Redirections**:
   - `RedirectMode::OutputAndErr` emits `&> ` (rather than `> ... 2>&1`).
   - `RedirectMode::AppendAndErr` emits `&>> ` (rather than `>> ... 2>&1`).
3. **Array Negative Subscripting**:
   - Read: `"${var[-1]}"`, `"${var[-2]}"` (native Bash 4.3+/5.0+ negative indexing).
   - Assignment: `var[-1]="val"`.
   - Erase: `unset 'var[-1]'`.
4. **Dynamic Subscripting**:
   - Read: `"${var[idx-1]}"`.
   - Assignment: `var[idx-1]="val"`.
   - Erase: `unset 'var[idx-1]'`.
5. **Global Declarations in Functions**:
   - In function: `declare -g var="val"` (scalar) or `declare -ga var=("...")` (array).
   - Top level: `var="val"` or `var=("...")`.

---

## 4. Testing & Validation Plan

1. **Syntax Validation**:
   - Continue running `bash -n` validation over all generated Bash code in integration tests.
   - All tests run against the modern Bash version in PATH (e.g. GNU Bash 5.3+).
2. **Snapshots**:
   - Update existing `insta` snapshots to reflect the modern, cleaner output.
   - Ensure negative index tests output `${items[-1]}` instead of `${items[$((${#items[@]}-1))]}`.
   - Ensure merged pipe tests output `make |& less` instead of `make 2>&1 | less`.
3. **CLI Flags**:
   - Test `--target bash5` and default behaviour without `--target`.
   - Test `--target bash3.2` placeholder feedback.
