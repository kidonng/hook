# Fish to Bash Transpiler (`hook`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `hook`, a Rust-based command-line transpiler that converts Fish shell scripts and snippets into idiomatic, compatible Bash 3.2+ shell scripts via a dedicated parsing crate (`fish-parser`) and code generation CLI crate (`hook`).

**Architecture:** A Cargo workspace separates AST parsing from code generation. `fish-parser` parses Fish source text into a strongly typed AST using `rust-peg` and `serde`. `hook` lowers the AST into specialized intermediate representations (resolving `set` flags, scope tracking, variable mapping, Bash 3.2 array indexing, and command/process substitutions) and emits Bash 3.2 scripts. Snapshot tests with `insta` and `bash -n` syntax validation ensure correctness.

**Tech Stack:** Rust (edition 2021), `rust-peg` (0.8), `serde` (1.0) with derive, `insta` (1.40).

**Spec:** `specs/2026-09-04-fish-to-bash-hook-design.md`

## Global Constraints

- **Compatibility Target**: Bash 3.2+ (macOS default `/bin/bash` compatible). Strictly no Bash 4.0+ features (`mapfile`, `declare -A`, negative array subscripts `${arr[-1]}`, `|&`, `&>>`, case modifications `${v,,}`).
- **CLI Interface**: Standard Unix filter (`hook [file]` or stdin pipe), writing to stdout, errors with line/column to stderr.
- **Dependency Boundary**: `fish-parser` has zero Bash dependencies and only parses Fish to AST. `hook` depends on `fish-parser`.
- **Engineering Safety**: No colloquialisms or character expressions in code, comments, commit messages, or filenames.

---

### Task 1: Workspace Scaffolding & Flake Configuration

**Files:**
- Create: `Cargo.toml`
- Create: `crates/fish-parser/Cargo.toml`
- Create: `crates/fish-parser/src/lib.rs`
- Create: `crates/hook/Cargo.toml`
- Create: `crates/hook/src/main.rs`
- Modify: `flake.nix`

**Interfaces:**
- Consumes: None
- Produces: Workspace root with `fish-parser` and `hook` crates compiling cleanly via `cargo check`.

- [ ] **Step 1: Create Cargo workspace root configuration**

Write `Cargo.toml`:
```toml
[workspace]
members = [
    "crates/fish-parser",
    "crates/hook",
]
resolver = "2"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
peg = "0.8"
insta = { version = "1.40", features = ["yaml"] }
```

- [ ] **Step 2: Create `crates/fish-parser` crate**

Write `crates/fish-parser/Cargo.toml`:
```toml
[package]
name = "fish-parser"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
peg = { workspace = true }

[dev-dependencies]
insta = { workspace = true }
```

Write initial `crates/fish-parser/src/lib.rs`:
```rust
pub mod ast;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), "0.1.0");
    }
}
```

- [ ] **Step 3: Create `crates/hook` crate**

Write `crates/hook/Cargo.toml`:
```toml
[package]
name = "hook"
version = "0.1.0"
edition = "2021"

[dependencies]
fish-parser = { path = "../fish-parser" }

[dev-dependencies]
insta = { workspace = true }
```

Write initial `crates/hook/src/main.rs`:
```rust
fn main() {
    println!("hook v{}", env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 4: Update `flake.nix` for Rust Cargo build**

Update `flake.nix` to build the Rust `hook` package instead of the Go placeholder:
```nix
{
  description = "hook: Fish shell to Bash 3.2 transpiler";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/DeterminateSystems/nixpkgs-weekly/0.1";
  };

  outputs = { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = f:
        nixpkgs.lib.genAttrs systems
          (system: f nixpkgs.legacyPackages.${system});

      devTools = pkgs: [
        pkgs.bash
        pkgs.cargo
        pkgs.fish
        pkgs.rustc
      ];
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "hook";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = devTools pkgs;
        };
      });
    };
}
```

- [ ] **Step 5: Verify workspace compilation**

Run: `cargo test`
Expected: 1 passed test (`test_version`).

- [ ] **Step 6: Commit workspace scaffolding**

```bash
git add Cargo.toml Cargo.lock crates/ flake.nix
git commit -m "chore: scaffold cargo workspace for fish-parser and hook"
```

---

### Task 2: Fish AST Definitions (`fish-parser::ast`)

**Files:**
- Create: `crates/fish-parser/src/ast.rs`
- Modify: `crates/fish-parser/src/lib.rs`
- Test: `crates/fish-parser/tests/ast_test.rs`

**Interfaces:**
- Consumes: `serde::{Serialize, Deserialize}`
- Produces: `Program`, `Statement`, `Pipeline`, `Combinator`, `Command`, `Word`, `WordPart`, `VariableRef`, `Slice`, `SliceIndex`, `Redirection`, `RedirectMode`, `IfStatement`, `SwitchStatement`, `CaseClause`, `ForStatement`, `WhileStatement`, `FunctionStatement`, `BeginBlock`.

- [ ] **Step 1: Write failing serialization/deserialization test for AST**

Write `crates/fish-parser/tests/ast_test.rs`:
```rust
use fish_parser::ast::*;

#[test]
fn test_ast_serialization() {
    let program = Program {
        shebang: Some("#!/usr/bin/env fish".to_string()),
        statements: vec![
            Statement::Pipeline(Pipeline {
                commands: vec![Command {
                    negate: false,
                    args: vec![
                        Word {
                            parts: vec![WordPart::Literal("echo".to_string())],
                        },
                        Word {
                            parts: vec![WordPart::Variable(VariableRef {
                                name: "status".to_string(),
                                slices: vec![],
                            })],
                        },
                    ],
                    redirections: vec![],
                }],
                combinator: Combinator::None,
                background: false,
            }),
        ],
    };

    let serialized = serde_json::to_string(&program).expect("serialization failed");
    let deserialized: Program = serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(program, deserialized);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p fish-parser --test ast_test`
Expected: FAIL with compilation error (cannot find `ast` module contents).

- [ ] **Step 3: Implement AST module**

Write `crates/fish-parser/src/ast.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub shebang: Option<String>,
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
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub negate: bool,
    pub args: Vec<Word>,
    pub redirections: Vec<Redirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub parts: Vec<WordPart>,
}

impl Word {
    pub fn from_literal(s: impl Into<String>) -> Self {
        Self {
            parts: vec![WordPart::Literal(s.into())],
        }
    }

    pub fn as_single_literal(&self) -> Option<&str> {
        if self.parts.len() == 1 {
            match &self.parts[0] {
                WordPart::Literal(s) => Some(s.as_str()),
                _ => None,
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WordPart {
    Literal(String),
    SingleQuoted(String),
    DoubleQuoted(Vec<WordPart>),
    Variable(VariableRef),
    CommandSubst(Vec<Statement>),
    BraceExpansion(Vec<Word>),
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
    Pos(usize),
    Neg(usize),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Redirection {
    pub fd: Option<u32>,
    pub mode: RedirectMode,
    pub target: Word,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RedirectMode {
    Output,
    Append,
    Input,
    OutputAndErr,
    AppendAndErr,
    DupOutput,
    DupInput,
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
    pub named_args: Vec<String>,
    pub description: Option<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeginBlock {
    pub body: Vec<Statement>,
    pub redirections: Vec<Redirection>,
}
```

Add `serde_json` to `crates/fish-parser/Cargo.toml` `[dev-dependencies]`:
```toml
serde_json = "1.0"
```

Export `pub mod ast;` in `crates/fish-parser/src/lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p fish-parser --test ast_test`
Expected: PASS.

- [ ] **Step 5: Commit AST definitions**

```bash
git add crates/fish-parser/
git commit -m "feat(fish-parser): add complete fish AST definitions"
```

---

### Task 3: PEG Grammar Implementation (`fish-parser::grammar`)

**Files:**
- Create: `crates/fish-parser/src/grammar.rs`
- Modify: `crates/fish-parser/src/lib.rs`
- Test: `crates/fish-parser/tests/parser_test.rs`

**Interfaces:**
- Consumes: `fish_parser::ast::*`, `peg::parser!`
- Produces: `pub fn parse(input: &str) -> Result<Program, ParseError>`

- [ ] **Step 1: Write failing parser tests**

Write `crates/fish-parser/tests/parser_test.rs`:
```rust
use fish_parser::ast::*;
use fish_parser::parse;

#[test]
fn test_parse_shebang_and_simple_command() {
    let input = "#!/usr/bin/env fish\necho 'hello world'\n";
    let program = parse(input).expect("parsing failed");
    assert_eq!(program.shebang, Some("#!/usr/bin/env fish".to_string()));
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0] {
        Statement::Pipeline(p) => {
            assert_eq!(p.commands.len(), 1);
            let cmd = &p.commands[0];
            assert_eq!(cmd.args[0].as_single_literal(), Some("echo"));
            assert_eq!(cmd.args[1].parts, vec![WordPart::SingleQuoted("hello world".to_string())]);
        }
        _ => panic!("expected pipeline statement"),
    }
}

#[test]
fn test_parse_variables_and_slices() {
    let input = "echo $status $argv[1] $var[1..3] $var[-1]\n";
    let program = parse(input).expect("parsing failed");
    match &program.statements[0] {
        Statement::Pipeline(p) => {
            let args = &p.commands[0].args;
            assert_eq!(args.len(), 5);
            // $status
            assert_eq!(args[1].parts, vec![WordPart::Variable(VariableRef {
                name: "status".to_string(),
                slices: vec![],
            })]);
            // $argv[1]
            assert_eq!(args[2].parts, vec![WordPart::Variable(VariableRef {
                name: "argv".to_string(),
                slices: vec![Slice::Index(SliceIndex::Pos(1))],
            })]);
            // $var[1..3]
            assert_eq!(args[3].parts, vec![WordPart::Variable(VariableRef {
                name: "var".to_string(),
                slices: vec![Slice::Range {
                    start: Some(SliceIndex::Pos(1)),
                    end: Some(SliceIndex::Pos(3)),
                }],
            })]);
            // $var[-1]
            assert_eq!(args[4].parts, vec![WordPart::Variable(VariableRef {
                name: "var".to_string(),
                slices: vec![Slice::Index(SliceIndex::Neg(1))],
            })]);
        }
        _ => panic!("expected pipeline"),
    }
}

#[test]
fn test_parse_command_substitution_and_psub() {
    let input = "diff (sort a | psub) (sort b | psub)\n";
    let program = parse(input).expect("parsing failed");
    match &program.statements[0] {
        Statement::Pipeline(p) => {
            let args = &p.commands[0].args;
            assert_eq!(args.len(), 3);
            match &args[1].parts[0] {
                WordPart::CommandSubst(stmts) => {
                    assert_eq!(stmts.len(), 1);
                }
                _ => panic!("expected CommandSubst"),
            }
        }
        _ => panic!("expected pipeline"),
    }
}

#[test]
fn test_parse_if_statement() {
    let input = r#"
if test -f foo
    echo yes
else if test -d foo
    echo dir
else
    echo no
end
"#;
    let program = parse(input).expect("parsing failed");
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0] {
        Statement::If(if_stmt) => {
            assert_eq!(if_stmt.then_body.len(), 1);
            assert_eq!(if_stmt.elif_branches.len(), 1);
            assert!(if_stmt.else_body.is_some());
        }
        _ => panic!("expected if statement"),
    }
}

#[test]
fn test_parse_function() {
    let input = r#"
function greet -a name title -d "greets a person"
    echo "Hello $title $name"
end
"#;
    let program = parse(input).expect("parsing failed");
    match &program.statements[0] {
        Statement::Function(f) => {
            assert_eq!(f.name, "greet");
            assert_eq!(f.named_args, vec!["name".to_string(), "title".to_string()]);
            assert_eq!(f.description, Some("greets a person".to_string()));
            assert_eq!(f.body.len(), 1);
        }
        _ => panic!("expected function statement"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p fish-parser --test parser_test`
Expected: FAIL with compilation error (`parse` function not found).

- [ ] **Step 3: Implement PEG grammar in `grammar.rs`**

Write `crates/fish-parser/src/grammar.rs`:
```rust
use crate::ast::*;

peg::parser! {
    pub grammar fish_grammar() for str {
        pub rule program() -> Program
            = shebang:shebang_line()? statements:statement_list() _* {
                Program { shebang, statements }
            }

        rule shebang_line() -> String
            = "#!" s:$( [^'\n']* ) ('\n' / ![_]) {
                format!("#!{}", s)
            }

        rule statement_list() -> Vec<Statement>
            = _* stmts:(s:statement() ** statement_sep()) statement_sep()? _* {
                stmts.into_iter().filter(|s| match s {
                    Statement::Comment(_) => true,
                    Statement::Pipeline(p) => !p.commands.is_empty(),
                    _ => true,
                }).collect()
            }

        rule statement_sep()
            = _* (['\n' | ';'] / (['\\'] '\n')) _*

        rule _() = [' ' | '\t']

        pub rule statement() -> Statement
            = comment()
            / return_stmt()
            / break_stmt()
            / continue_stmt()
            / if_stmt()
            / switch_stmt()
            / for_stmt()
            / while_stmt()
            / function_stmt()
            / begin_stmt()
            / pipeline_stmt()

        rule comment() -> Statement
            = "#" s:$((!['\n'][_])*) {
                Statement::Comment(s.to_string())
            }

        rule return_stmt() -> Statement
            = "return" _+ w:word() { Statement::Return(Some(w)) }
            / "return" { Statement::Return(None) }

        rule break_stmt() -> Statement
            = "break" { Statement::Break }

        rule continue_stmt() -> Statement
            = "continue" { Statement::Continue }

        rule pipeline_stmt() -> Statement
            = p:pipeline() { Statement::Pipeline(p) }

        rule pipeline() -> Pipeline
            = comb:combinator_prefix()? _* negate:("not" _+)? cmds:(command() ** (_* "|" _*)) bg:(_* "&")? {
                let mut commands = cmds;
                if let Some(true) = negate.map(|_| true) {
                    if let Some(first) = commands.first_mut() {
                        first.negate = true;
                    }
                }
                Pipeline {
                    commands,
                    combinator: comb.unwrap_or(Combinator::None),
                    background: bg.is_some(),
                }
            }

        rule combinator_prefix() -> Combinator
            = "and" _+ { Combinator::And }
            / "or" _+ { Combinator::Or }
            / "&&" _* { Combinator::And }
            / "||" _* { Combinator::Or }

        rule command() -> Command
            = items:(command_item() ** (_+)) {
                let mut args = Vec::new();
                let mut redirections = Vec::new();
                for item in items {
                    match item {
                        CommandItem::Arg(w) => args.push(w),
                        CommandItem::Redir(r) => redirections.push(r),
                    }
                }
                Command {
                    negate: false,
                    args,
                    redirections,
                }
            }

        enum CommandItem {
            Arg(Word),
            Redir(Redirection),
        }

        rule command_item() -> CommandItem
            = r:redirection() { CommandItem::Redir(r) }
            / w:word() { CommandItem::Arg(w) }

        rule redirection() -> Redirection
            = fd:(n:$(['0'..='9']+) { n.parse::<u32>().unwrap() })? mode:redirect_mode() _* target:word() {
                Redirection { fd, mode, target }
            }

        rule redirect_mode() -> RedirectMode
            = ">>" { RedirectMode::Append }
            / ">&" { RedirectMode::DupOutput }
            / "<&" { RedirectMode::DupInput }
            / ">" { RedirectMode::Output }
            / "<" { RedirectMode::Input }
            / "^^" { RedirectMode::AppendAndErr }
            / "^" { RedirectMode::OutputAndErr }
            / "&>>" { RedirectMode::AppendAndErr }
            / "&>" { RedirectMode::OutputAndErr }

        rule word() -> Word
            = parts:(word_part()+ ) {
                Word { parts }
            }

        rule word_part() -> WordPart
            = single_quoted()
            / double_quoted()
            / variable_ref()
            / command_subst()
            / brace_expansion()
            / literal()

        rule literal() -> WordPart
            = s:$( ( [^' ' | '\t' | '\n' | ';' | '|' | '&' | '<' | '>' | '^' | '(' | ')' | '{' | '}' | '$' | '\'' | '\"' | '#'] / ('\\' [_]) )+ ) {
                WordPart::Literal(unescape(s))
            }

        rule single_quoted() -> WordPart
            = "'" s:$(( [^'\''] / "\\'" )*) "'" {
                WordPart::SingleQuoted(s.replace("\\'", "'"))
            }

        rule double_quoted() -> WordPart
            = "\"" parts:double_quoted_part()* "\"" {
                WordPart::DoubleQuoted(parts)
            }

        rule double_quoted_part() -> WordPart
            = variable_ref()
            / command_subst()
            / s:$(( [^'\"' | '$' | '(' | '\\'] / ('\\' [_]) )+) {
                WordPart::Literal(unescape(s))
            }

        rule variable_ref() -> WordPart
            = "$" name:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) slices:slice()* {
                WordPart::Variable(VariableRef {
                    name: name.to_string(),
                    slices,
                })
            }

        rule slice() -> Slice
            = "[" _* start:slice_index()? _* ".." _* end:slice_index()? _* "]" {
                Slice::Range { start, end }
            }
            / "[" _* idx:slice_index() _* "]" {
                Slice::Index(idx)
            }

        rule slice_index() -> SliceIndex
            = "-" n:$(['0'..='9']+) { SliceIndex::Neg(n.parse::<usize>().unwrap()) }
            / n:$(['0'..='9']+) { SliceIndex::Pos(n.parse::<usize>().unwrap()) }

        rule command_subst() -> WordPart
            = "$(" _* stmts:statement_list() _* ")" {
                WordPart::CommandSubst(stmts)
            }
            / "(" _* stmts:statement_list() _* ")" {
                WordPart::CommandSubst(stmts)
            }

        rule brace_expansion() -> WordPart
            = "{" _* words:(word() ** (_* "," _*)) _* "}" {
                WordPart::BraceExpansion(words)
            }

        rule if_stmt() -> Statement
            = "if" _+ cond:pipeline() statement_sep()
              then_body:statement_list()
              elifs:elif_branch()*
              else_body:else_branch()?
              "end" {
                Statement::If(IfStatement {
                    condition: cond,
                    then_body,
                    elif_branches: elifs,
                    else_body,
                })
            }

        rule elif_branch() -> (Pipeline, Vec<Statement>)
            = "else" _+ "if" _+ cond:pipeline() statement_sep() body:statement_list() {
                (cond, body)
            }

        rule else_branch() -> Vec<Statement>
            = "else" statement_sep() body:statement_list() {
                body
            }

        rule switch_stmt() -> Statement
            = "switch" _+ val:word() statement_sep()
              cases:case_clause()*
              "end" {
                Statement::Switch(SwitchStatement {
                    value: val,
                    cases,
                })
            }

        rule case_clause() -> CaseClause
            = "case" _+ pats:(word() ** (_+)) statement_sep()
              body:statement_list() {
                CaseClause { patterns: pats, body }
            }

        rule for_stmt() -> Statement
            = "for" _+ var:$(['a'..='z' | 'A'..='Z' | '_']['a'..='z' | 'A'..='Z' | '0'..='9' | '_']*) _+ "in" _+ vals:(word() ** (_+)) statement_sep()
              body:statement_list()
              "end" {
                Statement::For(ForStatement {
                    variable: var.to_string(),
                    values: vals,
                    body,
                })
            }

        rule while_stmt() -> Statement
            = "while" _+ cond:pipeline() statement_sep()
              body:statement_list()
              "end" {
                Statement::While(WhileStatement {
                    condition: cond,
                    body,
                })
            }

        rule function_stmt() -> Statement
            = "function" _+ name:word() opts:func_opt()* statement_sep()
              body:statement_list()
              "end" {
                let mut named_args = Vec::new();
                let mut description = None;
                for opt in opts {
                    match opt {
                        FuncOpt::Args(mut a) => named_args.append(&mut a),
                        FuncOpt::Desc(d) => description = Some(d),
                    }
                }
                let func_name = name.as_single_literal().unwrap_or("").to_string();
                Statement::Function(FunctionStatement {
                    name: func_name,
                    named_args,
                    description,
                    body,
                })
            }

        enum FuncOpt {
            Args(Vec<String>),
            Desc(String),
        }

        rule func_opt() -> FuncOpt
            = _+ ("-a" / "--argument-names") _+ args:(w:word() ** (_+)) {
                let names = args.into_iter().filter_map(|w| w.as_single_literal().map(|s| s.to_string())).collect();
                FuncOpt::Args(names)
            }
            / _+ ("-d" / "--description") _+ w:word() {
                let desc = match &w.parts[0] {
                    WordPart::SingleQuoted(s) => s.clone(),
                    WordPart::Literal(s) => s.clone(),
                    _ => "".to_string(),
                };
                FuncOpt::Desc(desc)
            }

        rule begin_stmt() -> Statement
            = "begin" statement_sep()
              body:statement_list()
              "end" redirs:(_* r:redirection() { r })* {
                Statement::BeginBlock(BeginBlock {
                    body,
                    redirections: redirs,
                })
            }
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    _ => out.push(next),
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn parse(input: &str) -> Result<Program, peg::error::ParseError<peg::str::LineCol>> {
    fish_grammar::program(input.trim_start_matches('\u{feff}'))
}
```

Update `crates/fish-parser/src/lib.rs` to expose `parse`:
```rust
pub mod ast;
pub mod grammar;

pub use ast::*;
pub use grammar::parse;
pub use peg::error::ParseError;
```

- [ ] **Step 4: Run parser unit tests to verify they pass**

Run: `cargo test -p fish-parser`
Expected: PASS all tests.

- [ ] **Step 5: Commit PEG parser implementation**

```bash
git add crates/fish-parser/
git commit -m "feat(fish-parser): implement peg grammar for complete fish syntax"
```

---

### Task 4: IR & Lowering Pass (`hook::bash::ir` & `lowering`)

**Files:**
- Create: `crates/hook/src/bash/ir.rs`
- Create: `crates/hook/src/bash/lowering.rs`
- Create: `crates/hook/src/bash/mod.rs`
- Test: `crates/hook/tests/lowering_test.rs`

**Interfaces:**
- Consumes: `fish_parser::ast::*`
- Produces: `AssignmentIR`, `LoweredStatement`, `lowering::lower_program(&Program) -> LoweredProgram`, with scope-aware defense (`in_function`), built-in variable mappings, Bash 3.2 array indexing, and `(cmd | psub)` detection.

- [ ] **Step 1: Write failing tests for lowering pass**

Write `crates/hook/tests/lowering_test.rs`:
```rust
use fish_parser::parse;
use hook::bash::lowering::{lower_program, Scope};
use hook::bash::ir::*;

#[test]
fn test_lower_set_scope_defense() {
    // At top level (in_function = false), `set -l foo bar` falls back to Global assignment (NOT local)
    let input = "set -l foo 'bar'\n";
    let prog = parse(input).unwrap();
    let lowered = lower_program(&prog);
    match &lowered.statements[0] {
        LoweredStatement::Assignment(AssignmentIR::Global { name, values }) => {
            assert_eq!(name, "foo");
            assert_eq!(values.len(), 1);
        }
        _ => panic!("expected Global assignment at top level for set -l"),
    }
}

#[test]
fn test_lower_set_in_function() {
    // Inside function (in_function = true), `set -l foo bar` is Local
    let input = "function test_fn\nset -l foo 'bar'\nend\n";
    let prog = parse(input).unwrap();
    let lowered = lower_program(&prog);
    match &lowered.statements[0] {
        LoweredStatement::Function(f) => {
            match &f.body[0] {
                LoweredStatement::Assignment(AssignmentIR::Local { name, .. }) => {
                    assert_eq!(name, "foo");
                }
                _ => panic!("expected Local assignment inside function"),
            }
        }
        _ => panic!("expected function"),
    }
}

#[test]
fn test_lower_psub_detection() {
    let input = "diff (sort file1 | psub) (sort file2 | psub)\n";
    let prog = parse(input).unwrap();
    let lowered = lower_program(&prog);
    match &lowered.statements[0] {
        LoweredStatement::Pipeline(p) => {
            let arg1 = &p.commands[0].args[1];
            match &arg1.parts[0] {
                LoweredWordPart::ProcessSubst(pipeline) => {
                    assert_eq!(pipeline.commands.len(), 1);
                    assert_eq!(pipeline.commands[0].args[0].as_literal(), Some("sort"));
                }
                _ => panic!("expected ProcessSubst"),
            }
        }
        _ => panic!("expected pipeline"),
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p hook --test lowering_test`
Expected: FAIL with compilation error (module `bash` does not exist).

- [ ] **Step 3: Implement `ir.rs` and `lowering.rs`**

Write `crates/hook/src/bash/ir.rs`:
```rust
use fish_parser::ast::{Combinator, RedirectMode};

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredProgram {
    pub shebang: Option<String>,
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoweredStatement {
    Pipeline(LoweredPipeline),
    Assignment(AssignmentIR),
    If(LoweredIf),
    Switch(LoweredSwitch),
    For(LoweredFor),
    While(LoweredWhile),
    Function(LoweredFunction),
    BeginBlock(LoweredBeginBlock),
    Return(Option<LoweredWord>),
    Break,
    Continue,
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignmentIR {
    Local { name: String, values: Vec<LoweredWord> },
    Export { name: String, values: Vec<LoweredWord> },
    Global { name: String, values: Vec<LoweredWord> },
    Append { name: String, values: Vec<LoweredWord> },
    Prepend { name: String, values: Vec<LoweredWord> },
    Erase { name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredPipeline {
    pub commands: Vec<LoweredCommand>,
    pub combinator: Combinator,
    pub background: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredCommand {
    pub negate: bool,
    pub args: Vec<LoweredWord>,
    pub redirections: Vec<LoweredRedirection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredWord {
    pub parts: Vec<LoweredWordPart>,
}

impl LoweredWord {
    pub fn as_literal(&self) -> Option<&str> {
        if self.parts.len() == 1 {
            match &self.parts[0] {
                LoweredWordPart::Literal(s) => Some(s.as_str()),
                _ => None,
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoweredWordPart {
    Literal(String),
    SingleQuoted(String),
    DoubleQuoted(Vec<LoweredWordPart>),
    Variable(LoweredVariableRef),
    CommandSubst { stmts: Vec<LoweredStatement>, quoted: bool },
    ProcessSubst(LoweredPipeline),
    BraceExpansion(Vec<LoweredWord>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoweredVariableRef {
    Status,                              // $status -> $?
    Pipestatus,                          // $pipestatus -> ${PIPESTATUS[@]}
    ArgvAll,                             // $argv -> "$@"
    ArgvIndex(usize),                    // $argv[1] -> $1
    ArgvSlice { start: usize, len: Option<usize> }, // $argv[2..] -> ${@:2}
    ArgvLast,                            // $argv[-1] -> ${@: -1:1}
    Custom { name: String, subscript: Option<BashSubscript> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BashSubscript {
    ZeroBasedIndex(usize),               // $var[1] -> var[0]
    NegativeOffsetFromLength(usize),     // $var[-1] -> $((${#var[@]}-1))
    Range { offset: usize, length: usize }, // $var[1..3] -> :0:3
    OpenRange { offset: usize },         // $var[2..] -> :1
    All,                                 // $var -> "${var[@]}"
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredRedirection {
    pub fd: Option<u32>,
    pub mode: RedirectMode,
    pub target: LoweredWord,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredIf {
    pub condition: LoweredPipeline,
    pub then_body: Vec<LoweredStatement>,
    pub elif_branches: Vec<(LoweredPipeline, Vec<LoweredStatement>)>,
    pub else_body: Option<Vec<LoweredStatement>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredSwitch {
    pub value: LoweredWord,
    pub cases: Vec<LoweredCaseClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredCaseClause {
    pub patterns: Vec<LoweredWord>,
    pub body: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredFor {
    pub variable: String,
    pub values: Vec<LoweredWord>,
    pub body: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredWhile {
    pub condition: LoweredPipeline,
    pub body: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredFunction {
    pub name: String,
    pub named_args: Vec<String>,
    pub description: Option<String>,
    pub body: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredBeginBlock {
    pub body: Vec<LoweredStatement>,
    pub redirections: Vec<LoweredRedirection>,
}
```

Write `crates/hook/src/bash/lowering.rs`:
```rust
use fish_parser::ast::*;
use super::ir::*;

#[derive(Debug, Clone, Copy, Default)]
pub struct Scope {
    pub in_function: bool,
    pub in_for_values: bool,
}

pub fn lower_program(program: &Program) -> LoweredProgram {
    let mut scope = Scope::default();
    let statements = lower_statements(&program.statements, &mut scope);
    LoweredProgram {
        shebang: program.shebang.clone(),
        statements,
    }
}

pub fn lower_statements(stmts: &[Statement], scope: &mut Scope) -> Vec<LoweredStatement> {
    stmts.iter().map(|s| lower_statement(s, scope)).collect()
}

pub fn lower_statement(stmt: &Statement, scope: &mut Scope) -> LoweredStatement {
    match stmt {
        Statement::Comment(c) => LoweredStatement::Comment(c.clone()),
        Statement::Return(w) => LoweredStatement::Return(w.as_ref().map(|w| lower_word(w, scope))),
        Statement::Break => LoweredStatement::Break,
        Statement::Continue => LoweredStatement::Continue,
        Statement::Pipeline(p) => {
            // Check if this pipeline is a `set` command
            if p.commands.len() == 1 {
                let cmd = &p.commands[0];
                if let Some("set") = cmd.args.first().and_then(|w| w.as_single_literal()) {
                    if let Some(assign) = lower_set_command(cmd, scope) {
                        return LoweredStatement::Assignment(assign);
                    }
                }
            }
            LoweredStatement::Pipeline(lower_pipeline(p, scope))
        }
        Statement::If(i) => LoweredStatement::If(LoweredIf {
            condition: lower_pipeline(&i.condition, scope),
            then_body: lower_statements(&i.then_body, scope),
            elif_branches: i.elif_branches.iter().map(|(p, b)| (lower_pipeline(p, scope), lower_statements(b, scope))).collect(),
            else_body: i.else_body.as_ref().map(|b| lower_statements(b, scope)),
        }),
        Statement::Switch(s) => LoweredStatement::Switch(LoweredSwitch {
            value: lower_word(&s.value, scope),
            cases: s.cases.iter().map(|c| LoweredCaseClause {
                patterns: c.patterns.iter().map(|w| lower_word(w, scope)).collect(),
                body: lower_statements(&c.body, scope),
            }).collect(),
        }),
        Statement::For(f) => {
            let mut val_scope = *scope;
            val_scope.in_for_values = true;
            let values = f.values.iter().map(|w| lower_word(w, &mut val_scope)).collect();
            LoweredStatement::For(LoweredFor {
                variable: f.variable.clone(),
                values,
                body: lower_statements(&f.body, scope),
            })
        }
        Statement::While(w) => LoweredStatement::While(LoweredWhile {
            condition: lower_pipeline(&w.condition, scope),
            body: lower_statements(&w.body, scope),
        }),
        Statement::Function(f) => {
            let mut fn_scope = *scope;
            fn_scope.in_function = true;
            LoweredStatement::Function(LoweredFunction {
                name: f.name.clone(),
                named_args: f.named_args.clone(),
                description: f.description.clone(),
                body: lower_statements(&f.body, &mut fn_scope),
            })
        }
        Statement::BeginBlock(b) => LoweredStatement::BeginBlock(LoweredBeginBlock {
            body: lower_statements(&b.body, scope),
            redirections: b.redirections.iter().map(|r| lower_redirection(r, scope)).collect(),
        }),
    }
}

fn lower_set_command(cmd: &Command, scope: &Scope) -> Option<AssignmentIR> {
    let mut is_local = false;
    let mut is_export = false;
    let mut is_append = false;
    let mut is_prepend = false;
    let mut is_erase = false;

    let mut var_name = None;
    let mut values = Vec::new();

    let mut iter = cmd.args.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        if let Some(lit) = arg.as_single_literal() {
            if lit.starts_with('-') && var_name.is_none() {
                match lit {
                    "-l" | "--local" => is_local = true,
                    "-x" | "--export" => is_export = true,
                    "-gx" | "-xg" => is_export = true,
                    "-a" | "--append" => is_append = true,
                    "-p" | "--prepend" => is_prepend = true,
                    "-e" | "--erase" => is_erase = true,
                    "-g" | "--global" => {},
                    _ => {}
                }
                continue;
            }
        }
        if var_name.is_none() {
            if let Some(lit) = arg.as_single_literal() {
                var_name = Some(lit.to_string());
            }
        } else {
            values.push(lower_word(arg, scope));
        }
    }

    let name = var_name?;

    if is_erase {
        Some(AssignmentIR::Erase { name })
    } else if is_append {
        Some(AssignmentIR::Append { name, values })
    } else if is_prepend {
        Some(AssignmentIR::Prepend { name, values })
    } else if is_export {
        Some(AssignmentIR::Export { name, values })
    } else if is_local {
        if scope.in_function {
            Some(AssignmentIR::Local { name, values })
        } else {
            // Safety defense: top level falls back to Global
            Some(AssignmentIR::Global { name, values })
        }
    } else {
        Some(AssignmentIR::Global { name, values })
    }
}

pub fn lower_pipeline(p: &Pipeline, scope: &Scope) -> LoweredPipeline {
    LoweredPipeline {
        commands: p.commands.iter().map(|c| lower_command(c, scope)).collect(),
        combinator: p.combinator,
        background: p.background,
    }
}

pub fn lower_command(c: &Command, scope: &Scope) -> LoweredCommand {
    LoweredCommand {
        negate: c.negate,
        args: c.args.iter().map(|w| lower_word(w, scope)).collect(),
        redirections: c.redirections.iter().map(|r| lower_redirection(r, scope)).collect(),
    }
}

pub fn lower_redirection(r: &Redirection, scope: &Scope) -> LoweredRedirection {
    LoweredRedirection {
        fd: r.fd,
        mode: r.mode.clone(),
        target: lower_word(&r.target, scope),
    }
}

pub fn lower_word(w: &Word, scope: &Scope) -> LoweredWord {
    LoweredWord {
        parts: w.parts.iter().map(|p| lower_word_part(p, scope)).collect(),
    }
}

pub fn lower_word_part(part: &WordPart, scope: &Scope) -> LoweredWordPart {
    match part {
        WordPart::Literal(s) => LoweredWordPart::Literal(s.clone()),
        WordPart::SingleQuoted(s) => LoweredWordPart::SingleQuoted(s.clone()),
        WordPart::DoubleQuoted(parts) => LoweredWordPart::DoubleQuoted(
            parts.iter().map(|p| lower_word_part(p, scope)).collect()
        ),
        WordPart::Variable(v) => LoweredWordPart::Variable(lower_variable_ref(v)),
        WordPart::CommandSubst(stmts) => {
            // Check for process substitution: single pipeline ending in `psub`
            if stmts.len() == 1 {
                if let Statement::Pipeline(p) = &stmts[0] {
                    if let Some(last_cmd) = p.commands.last() {
                        if let Some("psub") = last_cmd.args.first().and_then(|w| w.as_single_literal()) {
                            // Strip terminal psub
                            let mut stripped_pipeline = p.clone();
                            stripped_pipeline.commands.pop();
                            if !stripped_pipeline.commands.is_empty() {
                                return LoweredWordPart::ProcessSubst(lower_pipeline(&stripped_pipeline, scope));
                            }
                        }
                    }
                }
            }
            // IFS defense: quote by default unless in `for ... in` values
            let quoted = !scope.in_for_values;
            let mut subst_scope = *scope;
            LoweredWordPart::CommandSubst {
                stmts: lower_statements(stmts, &mut subst_scope),
                quoted,
            }
        }
        WordPart::BraceExpansion(words) => LoweredWordPart::BraceExpansion(
            words.iter().map(|w| lower_word(w, scope)).collect()
        ),
    }
}

fn lower_variable_ref(v: &VariableRef) -> LoweredVariableRef {
    if v.name == "status" && v.slices.is_empty() {
        return LoweredVariableRef::Status;
    }
    if v.name == "pipestatus" && v.slices.is_empty() {
        return LoweredVariableRef::Pipestatus;
    }
    if v.name == "argv" {
        if v.slices.is_empty() {
            return LoweredVariableRef::ArgvAll;
        }
        if v.slices.len() == 1 {
            match &v.slices[0] {
                Slice::Index(SliceIndex::Pos(idx)) => return LoweredVariableRef::ArgvIndex(*idx),
                Slice::Index(SliceIndex::Neg(1)) => return LoweredVariableRef::ArgvLast,
                Slice::Range { start: Some(SliceIndex::Pos(s)), end: None } => {
                    return LoweredVariableRef::ArgvSlice { start: *s, len: None };
                }
                Slice::Range { start: Some(SliceIndex::Pos(s)), end: Some(SliceIndex::Neg(1)) } => {
                    return LoweredVariableRef::ArgvSlice { start: *s, len: None };
                }
                Slice::Range { start: Some(SliceIndex::Pos(s)), end: Some(SliceIndex::Pos(e)) } if e >= s => {
                    return LoweredVariableRef::ArgvSlice { start: *s, len: Some(e - s + 1) };
                }
                _ => {}
            }
        }
    }

    // Generic variable
    let subscript = if v.slices.is_empty() {
        None
    } else if v.slices.len() == 1 {
        match &v.slices[0] {
            Slice::Index(SliceIndex::Pos(n)) => {
                let zero_based = if *n > 0 { n - 1 } else { 0 };
                Some(BashSubscript::ZeroBasedIndex(zero_based))
            }
            Slice::Index(SliceIndex::Neg(k)) => {
                Some(BashSubscript::NegativeOffsetFromLength(*k))
            }
            Slice::Range { start: Some(SliceIndex::Pos(s)), end: Some(SliceIndex::Pos(e)) } if e >= s => {
                let offset = if *s > 0 { s - 1 } else { 0 };
                let length = e - s + 1;
                Some(BashSubscript::Range { offset, length })
            }
            Slice::Range { start: Some(SliceIndex::Pos(s)), end: None } => {
                let offset = if *s > 0 { s - 1 } else { 0 };
                Some(BashSubscript::OpenRange { offset })
            }
            _ => Some(BashSubscript::All),
        }
    } else {
        Some(BashSubscript::All)
    };

    LoweredVariableRef::Custom {
        name: v.name.clone(),
        subscript,
    }
}
```

Write `crates/hook/src/bash/mod.rs`:
```rust
pub mod ir;
pub mod lowering;
pub mod emitter;

pub use emitter::emit_bash;
```

Update `crates/hook/src/lib.rs` (create it if not present, and update `crates/hook/Cargo.toml` to have `[lib]` and `[[bin]]`):
Modify `crates/hook/Cargo.toml`:
```toml
[package]
name = "hook"
version = "0.1.0"
edition = "2021"

[lib]
name = "hook"
path = "src/lib.rs"

[[bin]]
name = "hook"
path = "src/main.rs"

[dependencies]
fish-parser = { path = "../fish-parser" }

[dev-dependencies]
insta = { workspace = true }
```

Write `crates/hook/src/lib.rs`:
```rust
pub mod bash;

pub use bash::emit_bash;
```

- [ ] **Step 4: Run lowering tests to verify they pass**

Run: `cargo test -p hook --test lowering_test`
Expected: PASS all tests.

- [ ] **Step 5: Commit IR and lowering pass**

```bash
git add crates/hook/
git commit -m "feat(hook): implement IR definitions and semantic lowering pass"
```

---

### Task 5: Bash 3.2 Code Generation Emitter (`hook::bash::emitter`)

**Files:**
- Create: `crates/hook/src/bash/emitter.rs`
- Modify: `crates/hook/src/bash/mod.rs`
- Test: `crates/hook/tests/emitter_test.rs`

**Interfaces:**
- Consumes: `LoweredProgram`, `LoweredStatement`, `ir::*`
- Produces: `pub fn emit_bash(program: &LoweredProgram) -> String` producing strictly Bash 3.2 compliant scripts.

- [ ] **Step 1: Write failing emitter tests**

Write `crates/hook/tests/emitter_test.rs`:
```rust
use fish_parser::parse;
use hook::bash::lowering::lower_program;
use hook::bash::emitter::emit_bash;

#[test]
fn test_emit_shebang_rewrite() {
    let input = "#!/usr/bin/env fish\necho hello\n";
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.starts_with("#!/usr/bin/env bash\n"));
    assert!(bash.contains("echo hello"));
}

#[test]
fn test_emit_assignments_bash_3_2() {
    let input = r#"
set -x GREET "hi"
set -l ARR a b c
set -a ARR d
set -e UNWANTED
"#;
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    // At top level, set -l generates ARR=("a" "b" "c")
    assert!(bash.contains(r#"export GREET="hi""#));
    assert!(bash.contains(r#"ARR=("a" "b" "c")"#));
    assert!(bash.contains(r#"ARR+=("d")"#));
    assert!(bash.contains("unset UNWANTED"));
}

#[test]
fn test_emit_array_negative_subscript_defense() {
    let input = "echo $var[-1] $var[1..3]\n";
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    // Defense against Bash 3.2 bad array subscript: must use dynamic length calculation
    assert!(bash.contains(r#""${var[$((${#var[@]}-1))]}""#));
    assert!(bash.contains(r#""${var[@]:0:3}""#));
}

#[test]
fn test_emit_control_structures() {
    let input = r#"
if test -f file
    echo ok
else
    echo fail
end
for x in a b
    echo $x
end
"#;
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.contains("if test -f file; then"));
    assert!(bash.contains("else"));
    assert!(bash.contains("fi"));
    assert!(bash.contains("for x in a b; do"));
    assert!(bash.contains("done"));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p hook --test emitter_test`
Expected: FAIL with compilation error (`emit_bash` not implemented).

- [ ] **Step 3: Implement `emitter.rs`**

Write `crates/hook/src/bash/emitter.rs`:
```rust
use super::ir::*;
use fish_parser::ast::{Combinator, RedirectMode};

pub fn emit_bash(program: &LoweredProgram) -> String {
    let mut out = String::new();
    if let Some(shebang) = &program.shebang {
        if shebang.contains("fish") {
            out.push_str("#!/usr/bin/env bash\n");
        } else {
            out.push_str(shebang);
            out.push('\n');
        }
    }

    emit_statements(&program.statements, 0, &mut out);
    out
}

fn emit_statements(stmts: &[LoweredStatement], indent: usize, out: &mut String) {
    for stmt in stmts {
        emit_statement(stmt, indent, out);
    }
}

fn emit_statement(stmt: &LoweredStatement, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    match stmt {
        LoweredStatement::Comment(c) => {
            out.push_str(&pad);
            out.push('#');
            out.push_str(c);
            out.push('\n');
        }
        LoweredStatement::Return(w) => {
            out.push_str(&pad);
            out.push_str("return");
            if let Some(w) = w {
                out.push(' ');
                emit_word(w, out);
            }
            out.push('\n');
        }
        LoweredStatement::Break => {
            out.push_str(&pad);
            out.push_str("break\n");
        }
        LoweredStatement::Continue => {
            out.push_str(&pad);
            out.push_str("continue\n");
        }
        LoweredStatement::Assignment(assign) => {
            out.push_str(&pad);
            emit_assignment(assign, out);
            out.push('\n');
        }
        LoweredStatement::Pipeline(p) => {
            out.push_str(&pad);
            emit_pipeline(p, out);
            out.push('\n');
        }
        LoweredStatement::If(i) => {
            out.push_str(&pad);
            out.push_str("if ");
            emit_pipeline(&i.condition, out);
            out.push_str("; then\n");
            emit_statements(&i.then_body, indent + 1, out);
            for (cond, body) in &i.elif_branches {
                out.push_str(&pad);
                out.push_str("elif ");
                emit_pipeline(cond, out);
                out.push_str("; then\n");
                emit_statements(body, indent + 1, out);
            }
            if let Some(else_body) = &i.else_body {
                out.push_str(&pad);
                out.push_str("else\n");
                emit_statements(else_body, indent + 1, out);
            }
            out.push_str(&pad);
            out.push_str("fi\n");
        }
        LoweredStatement::Switch(s) => {
            out.push_str(&pad);
            out.push_str("case ");
            emit_word(&s.value, out);
            out.push_str(" in\n");
            for clause in &s.cases {
                out.push_str(&format!("{}  ", pad));
                for (idx, pat) in clause.patterns.iter().enumerate() {
                    if idx > 0 {
                        out.push('|');
                    }
                    emit_word(pat, out);
                }
                out.push_str(")\n");
                emit_statements(&clause.body, indent + 2, out);
                out.push_str(&format!("{}    ;;\n", pad));
            }
            out.push_str(&pad);
            out.push_str("esac\n");
        }
        LoweredStatement::For(f) => {
            out.push_str(&pad);
            out.push_str(&format!("for {} in ", f.variable));
            for (idx, val) in f.values.iter().enumerate() {
                if idx > 0 {
                    out.push(' ');
                }
                emit_word(val, out);
            }
            out.push_str("; do\n");
            emit_statements(&f.body, indent + 1, out);
            out.push_str(&pad);
            out.push_str("done\n");
        }
        LoweredStatement::While(w) => {
            out.push_str(&pad);
            out.push_str("while ");
            emit_pipeline(&w.condition, out);
            out.push_str("; do\n");
            emit_statements(&w.body, indent + 1, out);
            out.push_str(&pad);
            out.push_str("done\n");
        }
        LoweredStatement::Function(f) => {
            out.push_str(&pad);
            out.push_str(&format!("{}() {{\n", f.name));
            for (idx, arg) in f.named_args.iter().enumerate() {
                out.push_str(&format!("{}  local {}=\"${}\"\n", pad, arg, idx + 1));
            }
            emit_statements(&f.body, indent + 1, out);
            out.push_str(&pad);
            out.push_str("}\n");
        }
        LoweredStatement::BeginBlock(b) => {
            out.push_str(&pad);
            out.push_str("{\n");
            emit_statements(&b.body, indent + 1, out);
            out.push_str(&pad);
            out.push('}');
            for redir in &b.redirections {
                out.push(' ');
                emit_redirection(redir, out);
            }
            out.push('\n');
        }
    }
}

fn emit_assignment(assign: &AssignmentIR, out: &mut String) {
    match assign {
        AssignmentIR::Local { name, values } => {
            if values.len() <= 1 {
                out.push_str(&format!("local {}=\"", name));
                if let Some(v) = values.first() {
                    emit_word(v, out);
                }
                out.push('\"');
            } else {
                out.push_str(&format!("local -a {}=(", name));
                emit_quoted_values(values, out);
                out.push(')');
            }
        }
        AssignmentIR::Export { name, values } => {
            out.push_str(&format!("export {}=\"", name));
            if let Some(v) = values.first() {
                emit_word(v, out);
            }
            out.push('\"');
        }
        AssignmentIR::Global { name, values } => {
            if values.len() <= 1 {
                out.push_str(&format!("{}=\"", name));
                if let Some(v) = values.first() {
                    emit_word(v, out);
                }
                out.push('\"');
            } else {
                out.push_str(&format!("{}=(", name));
                emit_quoted_values(values, out);
                out.push(')');
            }
        }
        AssignmentIR::Append { name, values } => {
            out.push_str(&format!("{}+=(", name));
            emit_quoted_values(values, out);
            out.push(')');
        }
        AssignmentIR::Prepend { name, values } => {
            out.push_str(&format!("{}=(", name));
            emit_quoted_values(values, out);
            out.push_str(&format!(" \"${{{}[@]}}\")", name));
        }
        AssignmentIR::Erase { name } => {
            out.push_str(&format!("unset {}", name));
        }
    }
}

fn emit_quoted_values(values: &[LoweredWord], out: &mut String) {
    for (idx, val) in values.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        out.push('\"');
        emit_word(val, out);
        out.push('\"');
    }
}

fn emit_pipeline(p: &LoweredPipeline, out: &mut String) {
    match p.combinator {
        Combinator::And => out.push_str("&& "),
        Combinator::Or => out.push_str("|| "),
        Combinator::None => {}
    }
    for (idx, cmd) in p.commands.iter().enumerate() {
        if idx > 0 {
            out.push_str(" | ");
        }
        emit_command(cmd, out);
    }
    if p.background {
        out.push_str(" &");
    }
}

fn emit_command(cmd: &LoweredCommand, out: &mut String) {
    if cmd.negate {
        out.push_str("! ");
    }
    for (idx, arg) in cmd.args.iter().enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        emit_word(arg, out);
    }
    for redir in &cmd.redirections {
        out.push(' ');
        emit_redirection(redir, out);
    }
}

fn emit_redirection(r: &LoweredRedirection, out: &mut String) {
    if let Some(fd) = r.fd {
        out.push_str(&fd.to_string());
    }
    match r.mode {
        RedirectMode::Output => out.push('>'),
        RedirectMode::Append => out.push_str(">>"),
        RedirectMode::Input => out.push('<'),
        RedirectMode::OutputAndErr => out.push_str(">"), // Bash 3.2: > target 2>&1
        RedirectMode::AppendAndErr => out.push_str(">>"),
        RedirectMode::DupOutput => out.push_str(">&"),
        RedirectMode::DupInput => out.push_str("<&"),
    }
    out.push(' ');
    emit_word(&r.target, out);
    if r.mode == RedirectMode::OutputAndErr {
        out.push_str(" 2>&1");
    } else if r.mode == RedirectMode::AppendAndErr {
        out.push_str(" 2>&1");
    }
}

fn emit_word(w: &LoweredWord, out: &mut String) {
    for part in &w.parts {
        emit_word_part(part, out);
    }
}

fn emit_word_part(part: &LoweredWordPart, out: &mut String) {
    match part {
        LoweredWordPart::Literal(s) => out.push_str(s),
        LoweredWordPart::SingleQuoted(s) => {
            out.push('\'');
            out.push_str(s);
            out.push('\'');
        }
        LoweredWordPart::DoubleQuoted(parts) => {
            out.push('\"');
            for p in parts {
                emit_word_part(p, out);
            }
            out.push('\"');
        }
        LoweredWordPart::Variable(v) => emit_variable_ref(v, out),
        LoweredWordPart::CommandSubst { stmts, quoted } => {
            if *quoted {
                out.push_str("\"$(");
            } else {
                out.push_str("$(");
            }
            let mut inner = String::new();
            emit_statements(stmts, 0, &mut inner);
            out.push_str(inner.trim_end());
            if *quoted {
                out.push_str(")\"");
            } else {
                out.push(')');
            }
        }
        LoweredWordPart::ProcessSubst(pipeline) => {
            out.push_str("<(");
            emit_pipeline(pipeline, out);
            out.push(')');
        }
        LoweredWordPart::BraceExpansion(words) => {
            out.push('{');
            for (idx, w) in words.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                emit_word(w, out);
            }
            out.push('}');
        }
    }
}

fn emit_variable_ref(v: &LoweredVariableRef, out: &mut String) {
    match v {
        LoweredVariableRef::Status => out.push_str("$?"),
        LoweredVariableRef::Pipestatus => out.push_str("\"${PIPESTATUS[@]}\""),
        LoweredVariableRef::ArgvAll => out.push_str("\"$@\""),
        LoweredVariableRef::ArgvIndex(n) => out.push_str(&format!("\"${}\"", n)),
        LoweredVariableRef::ArgvSlice { start, len } => {
            if let Some(length) = len {
                out.push_str(&format!("\"${{@:{}:{}}}\"", start, length));
            } else {
                out.push_str(&format!("\"${{@:{}}}\"", start));
            }
        }
        LoweredVariableRef::ArgvLast => out.push_str("\"${@: -1:1}\""),
        LoweredVariableRef::Custom { name, subscript } => {
            match subscript {
                None => out.push_str(&format!("\"${}\"", name)),
                Some(BashSubscript::All) => out.push_str(&format!("\"${{{}[@]}}\"", name)),
                Some(BashSubscript::ZeroBasedIndex(idx)) => out.push_str(&format!("\"${{{}[{}]}}\"", name, idx)),
                Some(BashSubscript::NegativeOffsetFromLength(k)) => {
                    // Bash 3.2 compatible negative index calculation
                    out.push_str(&format!("\"${{{}[$((${{#{}[@]}}-{}))]}}\"", name, name, k));
                }
                Some(BashSubscript::Range { offset, length }) => {
                    out.push_str(&format!("\"${{{}[@]:{}:{}}}\"", name, offset, length));
                }
                Some(BashSubscript::OpenRange { offset }) => {
                    out.push_str(&format!("\"${{{}[@]:{}}}\"", name, offset));
                }
            }
        }
    }
}
```

- [ ] **Step 4: Run emitter tests to verify they pass**

Run: `cargo test -p hook --test emitter_test`
Expected: PASS all tests.

- [ ] **Step 5: Commit emitter implementation**

```bash
git add crates/hook/
git commit -m "feat(hook): implement bash 3.2 code generation emitter"
```

---

### Task 6: CLI Implementation (`crates/hook/src/main.rs`)

**Files:**
- Modify: `crates/hook/src/main.rs`
- Test: `crates/hook/tests/cli_test.rs`

**Interfaces:**
- Consumes: `std::env::args`, `std::io::stdin`, `fish_parser::parse`, `hook::bash::lowering`, `hook::bash::emitter`
- Produces: Command-line binary supporting `hook [file]` or stdin pipe, with exit codes 0 (success), 1 (syntax error), 2 (IO error).

- [ ] **Step 1: Write failing CLI integration test**

Write `crates/hook/tests/cli_test.rs`:
```rust
use std::process::{Command, Stdio};
use std::io::Write;

#[test]
fn test_cli_stdin_pipe() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hook"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn hook binary");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        stdin.write_all(b"echo hello\n").expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to read output");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "echo hello");
}

#[test]
fn test_cli_syntax_error() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hook"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn hook binary");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        stdin.write_all(b"if ; echo\n").expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to read output");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("syntax error"));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p hook --test cli_test`
Expected: FAIL (binary outputs "hook v0.1.0" instead of reading stdin/transpiling).

- [ ] **Step 3: Implement `crates/hook/src/main.rs`**

Write `crates/hook/src/main.rs`:
```rust
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process;

use fish_parser::parse;
use hook::bash::lowering::lower_program;
use hook::bash::emitter::emit_bash;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "-h" | "--help" => {
                print_help();
                process::exit(0);
            }
            "-V" | "-v" | "--version" => {
                println!("hook {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            _ => {}
        }
    }

    let (input, source_name) = if args.len() > 1 && args[1] != "-" {
        let path = &args[1];
        match fs::read_to_string(path) {
            Ok(content) => (content, path.clone()),
            Err(e) => {
                eprintln!("hook: error reading {}: {}", path, e);
                process::exit(2);
            }
        }
    } else {
        let mut buffer = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut buffer) {
            eprintln!("hook: error reading from stdin: {}", e);
            process::exit(2);
        }
        (buffer, "<stdin>".to_string())
    };

    let parsed = match parse(&input) {
        Ok(prog) => prog,
        Err(err) => {
            eprintln!("hook: syntax error in {} at line {}, column {}: expected {}",
                source_name, err.location.line, err.location.column, err.expected);
            process::exit(1);
        }
    };

    let lowered = lower_program(&parsed);
    let emitted = emit_bash(&lowered);

    if let Err(e) = io::stdout().write_all(emitted.as_bytes()) {
        eprintln!("hook: error writing output: {}", e);
        process::exit(2);
    }
}

fn print_help() {
    println!(r#"hook - Transpile Fish shell scripts to Bash 3.2+

USAGE:
    hook [FILE]
    cat file.fish | hook

ARGS:
    <FILE>    Fish script to transpile (reads from stdin if omitted or '-')

FLAGS:
    -h, --help       Print help information
    -V, --version    Print version information
"#);
}
```

- [ ] **Step 4: Run CLI tests to verify they pass**

Run: `cargo test -p hook --test cli_test`
Expected: PASS.

- [ ] **Step 5: Commit CLI implementation**

```bash
git add crates/hook/
git commit -m "feat(hook): implement unix filter CLI for hook"
```

---

### Task 7: End-to-End Transpilation Snapshot Tests & `bash -n` Validation (`tests/transpile_test.rs`)

**Files:**
- Create: `tests/transpile_test.rs`
- Create: `tests/snapshots/`
- Modify: `Cargo.toml` (if needed for root integration test target)

**Interfaces:**
- Consumes: `hook::bash::*`, `fish_parser::*`, `insta::assert_snapshot!`, `std::process::Command` with `bash -n`
- Produces: Snapshot tests validating all 6 core categories against golden snapshots and confirming syntax validity with `bash -n`.

- [ ] **Step 1: Write integration snapshot tests with `bash -n` checks**

Write `tests/transpile_test.rs`:
```rust
use std::process::{Command, Stdio};
use std::io::Write;
use fish_parser::parse;
use hook::bash::lowering::lower_program;
use hook::bash::emitter::emit_bash;

fn transpile(fish_code: &str) -> String {
    let parsed = parse(fish_code).expect("failed to parse fish code");
    let lowered = lower_program(&parsed);
    let bash_code = emit_bash(&lowered);

    // Verify syntax using bash -n
    let mut child = Command::new("bash")
        .arg("-n")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to invoke bash for syntax validation");

    {
        let stdin = child.stdin.as_mut().expect("failed to get stdin");
        stdin.write_all(bash_code.as_bytes()).expect("failed to write to bash stdin");
    }

    let output = child.wait_with_output().expect("failed to wait on bash");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("bash -n syntax validation failed for output:\n{}\nError:\n{}", bash_code, stderr);
    }

    bash_code
}

#[test]
fn test_snapshot_set_and_arrays() {
    let fish = r#"
#!/usr/bin/env fish
set -x PATH $PATH /opt/bin
set -l items apple banana cherry
set -a items date
echo $items[1]
echo $items[-1]
echo $items[2..3]
"#;
    let result = transpile(fish);
    insta::assert_snapshot!(result);
}

#[test]
fn test_snapshot_builtins_and_args() {
    let fish = r#"
function deploy -a env target
    if test $status -eq 0
        echo "deploying $argv[1] to $argv[2..-1]"
    end
    echo "exit status: $status"
end
"#;
    let result = transpile(fish);
    insta::assert_snapshot!(result);
}

#[test]
fn test_snapshot_process_and_command_subst() {
    let fish = r#"
diff (sort a.txt | psub) (sort b.txt | psub)
set files (ls -1)
for f in (find . -name "*.txt")
    echo $f
end
"#;
    let result = transpile(fish);
    insta::assert_snapshot!(result);
}

#[test]
fn test_snapshot_control_flow() {
    let fish = r#"
if test -f foo.txt
    echo "is file"
else if test -d foo.txt
    echo "is dir"
else
    echo "unknown"
end

switch $target
    case prod production
        echo "deploying prod"
    case staging
        echo "deploying staging"
    case '*'
        echo "default"
end

while test $count -gt 0
    echo $count
    set count (math $count - 1)
end
"#;
    let result = transpile(fish);
    insta::assert_snapshot!(result);
}
```

Add integration test to root `Cargo.toml` if needed or add `insta` to workspace dev dependencies.
Update `Cargo.toml`:
```toml
[workspace]
members = [
    "crates/fish-parser",
    "crates/hook",
]
resolver = "2"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
peg = "0.8"
insta = { version = "1.40", features = ["yaml"] }

[dev-dependencies]
fish-parser = { path = "crates/fish-parser" }
hook = { path = "crates/hook" }
insta = { workspace = true }
```

- [ ] **Step 2: Generate initial snapshots**

Run: `INSTA_UPDATE=always cargo test --test transpile_test`
Expected: PASS and snapshots generated in `tests/snapshots/`.

- [ ] **Step 3: Run full test suite without updates to verify stability**

Run: `cargo test`
Expected: All unit tests, CLI tests, and integration snapshot tests PASS.

- [ ] **Step 4: Commit integration snapshot tests**

```bash
git add tests/ Cargo.toml
git commit -m "test: add e2e transpile snapshot tests with bash -n validation"
```
