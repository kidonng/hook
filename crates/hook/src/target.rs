use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    Bash5,
    Bash3_2,
}

impl FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "bash5" | "5" | "bash-5" | "bash5.0" => Ok(Target::Bash5),
            "bash3.2" | "bash32" | "3.2" | "bash-3.2" => Ok(Target::Bash3_2),
            _ => Err(format!(
                "unsupported target: '{}', expected 'bash5' or 'bash3.2'",
                s
            )),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Bash5 => write!(f, "bash5"),
            Target::Bash3_2 => write!(f, "bash3.2"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TranspileConfig {
    pub target: Target,
}
