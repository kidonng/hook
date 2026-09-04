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
    Local {
        name: String,
        values: Vec<LoweredWord>,
    },
    Export {
        name: String,
        values: Vec<LoweredWord>,
    },
    Global {
        name: String,
        values: Vec<LoweredWord>,
    },
    Append {
        name: String,
        values: Vec<LoweredWord>,
    },
    Prepend {
        name: String,
        values: Vec<LoweredWord>,
    },
    Erase {
        name: String,
    },
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
    CommandSubst {
        stmts: Vec<LoweredStatement>,
        quoted: bool,
    },
    ProcessSubst(LoweredPipeline),
    BraceExpansion(Vec<LoweredWord>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoweredVariableRef {
    Status,           // $status -> $?
    Pipestatus,       // $pipestatus -> ${PIPESTATUS[@]}
    ArgvAll,          // $argv -> "$@"
    ArgvIndex(usize), // $argv[1] -> $1
    ArgvSlice {
        start: usize,
        len: Option<usize>,
    }, // $argv[2..-1] -> ${@:2}
    ArgvLast,         // $argv[-1] -> ${@: -1:1}
    Custom {
        name: String,
        subscript: Option<BashSubscript>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BashSubscript {
    ZeroBasedIndex(usize),
    NegativeOffsetFromLength(usize),
    Range { offset: usize, length: usize },
    OpenRange { offset: usize },
    All,
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
