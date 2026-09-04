# AGENTS.md

## Project Overview

`hook` is a Rust-based command-line transpiler that converts Fish shell scripts and snippets into modern Bash (5.0+) shell scripts (with subsequent support planned for legacy Bash 3.2). It operates as a standard Unix filter (accepting file paths or reading from standard input, and outputting to standard output).

### Workspace Structure

- **`crates/fish-parser`**: A standalone library parsing Fish shell syntax into a strongly typed AST using `rust-peg` and `serde`. It has no Bash or execution dependencies.
- **`crates/hook`**: The CLI binary and translation engine. It consumes the AST from `fish-parser`, lowers Fish idioms into modern Bash (5.0+) constructs (and future target backends), and emits Bash scripts.

### Architectural Boundary & Separation of Concerns

- **`fish-parser` is strictly a pure, high-fidelity syntax parser**:
  - Its sole responsibility is parsing Fish syntax into a faithful, strongly typed AST.
  - NEVER perform semantic lowering, desugaring, or command-line option interpretation inside `fish-parser` (e.g. do not parse or discard function flags like `-a`/`-d`/`-w` inside the parser; preserve options as raw `Vec<Word>`).
  - NEVER inject synthetic AST nodes or target-specific workarounds (e.g. do not synthesize `2>&1` redirections into a command's AST when encountering `&|` pipes; represent `PipeOperator::StdoutAndStderr` explicitly in the AST).
- **All semantic interpretation, option parsing, and target-specific lowering belong in `hook`**:
  - Downstream consumers (such as `crates/hook`'s lowering phase) are solely responsible for inspecting AST options, desugaring constructs, and mapping Fish idioms to modern Bash 5.0+ (or other configured targets).
  - Keep `fish-parser` reusable for any consumer (linters, formatters, transpilers) without baking in `hook`-specific assumptions.

## Development Environment

### Prerequisites

- **Rust**: 1.85+ (2024 edition)
- **Nix**: Flake support enabled with `nix develop` providing a hermetic toolchain (`rustc`, `cargo`, `clippy`, `rustfmt`, `bash`, `fish`).
- **Bash**: Bash 5.0+ for running integration test validations.

### Common Commands

```bash
# Enter nix development shell
nix develop

# Build workspace binaries and libraries
cargo build
cargo build --release

# Run all workspace unit and integration tests
cargo test


# Format and lint code
cargo fmt --check
cargo clippy
# Build with Nix Flake
nix build

# Run the CLI directly
cargo run -p hook -- script.fish
cat script.fish | cargo run -p hook
```

## Documentation and Plans

- Place all design documents and specs in the root `specs/` directory (e.g. `specs/YYYY-MM-DD-<name>-design.md`).
- Place all implementation plans in the root `plans/` directory (e.g. `plans/YYYY-MM-DD-<name>.md`).
- Do NOT create or use `docs/superpowers/` or nested documentation directories for plans or specs.
