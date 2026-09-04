# AGENTS.md

## Project Overview

`hook` is a Rust-based command-line transpiler that converts Fish shell scripts and snippets into compatible Bash 3.2+ shell scripts. It operates as a standard Unix filter (accepting file paths or reading from standard input, and outputting to standard output).

### Workspace Structure

- **`crates/fish-parser`**: A standalone library parsing Fish shell syntax into a strongly typed AST using `rust-peg` and `serde`. It has no Bash or execution dependencies.
- **`crates/hook`**: The CLI binary and translation engine. It consumes the AST from `fish-parser`, lowers Fish idioms into Bash 3.2 compatible constructs, and emits Bash scripts.

## Development Environment

### Prerequisites

- **Rust**: 1.85+ (2024 edition)
- **Nix**: Flake support enabled with `nix develop` providing a hermetic toolchain (`rustc`, `cargo`, `clippy`, `rustfmt`, `bash`, `fish`).
- **Bash**: Bash 3.2+ for running integration test validations.

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
