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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipeOperator {
    Stdout,
    StdoutAndStderr,
    Fd(u32),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableAssignment {
    pub name: String,
    pub value: Word,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PipelineElement {
    Command(Command),
    Block(Statement),
}

impl From<Command> for PipelineElement {
    fn from(cmd: Command) -> Self {
        Self::Command(cmd)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pipeline {
    pub negate: bool,
    pub elements: Vec<PipelineElement>,
    pub pipe_operators: Vec<PipeOperator>,
    pub combinator: Combinator,
    pub background: bool,
}

impl Pipeline {
    pub fn commands(&self) -> Vec<&Command> {
        self.elements
            .iter()
            .filter_map(|el| match el {
                PipelineElement::Command(c) => Some(c),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Combinator {
    None,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub assignments: Vec<VariableAssignment>,
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
    CommandSubst {
        statements: Vec<Statement>,
        slices: Vec<Slice>,
    },
    BraceExpansion(Vec<Word>),
}

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

impl VariableRef {
    pub fn new_named(name: impl Into<String>, slices: Vec<Slice>) -> Self {
        Self {
            target: VariableTarget::Named(name.into()),
            slices,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match &self.target {
            VariableTarget::Named(name) => Some(name.as_str()),
            VariableTarget::Indirect(_) => None,
        }
    }
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
    Variable(VariableRef),
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
    SafeInput,
    NoClobberOutput,
    NoClobberAppend,
    NoClobberOutputAndErr,
    NoClobberAppendAndErr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfStatement {
    pub condition: Vec<Pipeline>,
    pub then_body: Vec<Statement>,
    pub elif_branches: Vec<(Vec<Pipeline>, Vec<Statement>)>,
    pub else_body: Option<Vec<Statement>>,
    pub redirections: Vec<Redirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchStatement {
    pub value: Word,
    pub cases: Vec<CaseClause>,
    pub redirections: Vec<Redirection>,
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
    pub redirections: Vec<Redirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhileStatement {
    pub condition: Vec<Pipeline>,
    pub body: Vec<Statement>,
    pub redirections: Vec<Redirection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionStatement {
    pub name: Word,
    pub options: Vec<Word>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeginBlock {
    pub combinator: Combinator,
    pub body: Vec<Statement>,
    pub redirections: Vec<Redirection>,
}
