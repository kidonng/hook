use fish_parser::ast::{Combinator, RedirectMode, Slice, SourceSpan};

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredProgram {
    pub shebang: Option<String>,
    pub statements: Vec<LoweredStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredStatement {
    pub kind: LoweredStatementKind,
    pub span: SourceSpan,
}

impl LoweredStatement {
    pub fn new(kind: LoweredStatementKind, span: SourceSpan) -> Self {
        Self { kind, span }
    }
}

impl From<LoweredStatementKind> for LoweredStatement {
    fn from(kind: LoweredStatementKind) -> Self {
        Self {
            kind,
            span: SourceSpan::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoweredStatementKind {
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
        in_function: bool,
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
    Negative(isize),
    Dynamic(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeKind {
    Stdout,
    StdoutAndStderr,
    Fd(u32),
}
#[derive(Debug, Clone, PartialEq)]
pub struct LoweredVariableAssignment {
    pub name: String,
    pub value: LoweredWord,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoweredPipelineElement {
    Command(LoweredCommand),
    Block(LoweredStatement),
}

impl From<LoweredCommand> for LoweredPipelineElement {
    fn from(cmd: LoweredCommand) -> Self {
        Self::Command(cmd)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredPipeline {
    pub negate: bool,
    pub elements: Vec<LoweredPipelineElement>,
    pub pipe_operators: Vec<PipeKind>,
    pub combinator: Combinator,
    pub background: bool,
}

impl LoweredPipeline {
    pub fn commands(&self) -> Vec<&LoweredCommand> {
        self.elements
            .iter()
            .filter_map(|el| match el {
                LoweredPipelineElement::Command(c) => Some(c),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoweredCommand {
    pub assignments: Vec<LoweredVariableAssignment>,
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

    pub fn from_words(text: &str) -> Vec<Self> {
        text.split_whitespace().map(Self::from_literal).collect()
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

impl From<&str> for LoweredWord {
    fn from(s: &str) -> Self {
        Self::from_literal(s)
    }
}

impl From<String> for LoweredWord {
    fn from(s: String) -> Self {
        Self::from_literal(s)
    }
}

#[macro_export]
macro_rules! words {
    ($($w:expr),* $(,)?) => {
        vec![$($crate::bash::ir::LoweredWord::from($w)),*]
    };
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
    Index(isize),
    Range { offset: isize, length: usize },
    OpenRange { offset: isize },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modern_ir_pipe_and_subscripts() {
        let pipeline = LoweredPipeline {
            negate: false,
            elements: vec![],
            pipe_operators: vec![PipeKind::StdoutAndStderr],
            combinator: Combinator::None,
            background: false,
        };
        assert_eq!(pipeline.pipe_operators[0], PipeKind::StdoutAndStderr);

        let sub = BashSubscript::Index(-1);
        assert_eq!(sub, BashSubscript::Index(-1));
    }
}
