use fish_parser::ast::{Combinator, RedirectMode, Slice};

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
    ArgvLast {
        value: LoweredWord,
    },
    SliceAssign {
        name: String,
        index: SliceIndexIR,
        value: LoweredWord,
    },
    SliceErase {
        name: String,
        index: SliceIndexIR,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SliceIndexIR {
    ZeroBased(usize),
    NegativeOffset(usize),
    Dynamic(String),
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
    pub fn from_literal(s: impl Into<String>) -> Self {
        Self {
            parts: vec![LoweredWordPart::Literal(s.into())],
        }
    }
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
        slices: Vec<Slice>,
        quoted: bool,
    },
    ProcessSubst(LoweredPipeline),
    BraceExpansion(Vec<LoweredWord>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoweredVariableRef {
    Status,           // $status -> $?
    Pipestatus,       // $pipestatus -> ${PIPESTATUS[@]}
    FishPid,          // $fish_pid -> $$
    LastPid,          // $last_pid -> $!
    ArgvAll,          // $argv -> "$@"
    ArgvIndex(usize), // $argv[1] -> $1
    ArgvSlice {
        start: usize,
        len: Option<usize>,
    }, // $argv[2..-1] -> ${@:2}
    ArgvLast,         // $argv[-1] -> ${@: -1:1}
    ArgvDynamic(String), // $argv[$idx] -> ${@:$idx:1}
    ArgvDynamicRange {
        start: String,
        end: String,
    }, // $argv[$start..$end] -> ${@:$start:$((end - start + 1))}
    Indirect {
        name: String,
    }, // $$var -> ${!var}
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
    DynamicVariable(String),
    DynamicRange { start: String, end: String },
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
    pub condition: Vec<LoweredPipeline>,
    pub then_body: Vec<LoweredStatement>,
    pub elif_branches: Vec<(Vec<LoweredPipeline>, Vec<LoweredStatement>)>,
    pub else_body: Option<Vec<LoweredStatement>>,
    pub redirections: Vec<LoweredRedirection>,
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
    pub redirections: Vec<LoweredRedirection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredWhile {
    pub condition: Vec<LoweredPipeline>,
    pub body: Vec<LoweredStatement>,
    pub redirections: Vec<LoweredRedirection>,
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
    pub combinator: Combinator,
    pub body: Vec<LoweredStatement>,
    pub redirections: Vec<LoweredRedirection>,
}
