pub mod ast;
pub mod grammar;
pub mod unescape;
pub use ast::*;
pub use grammar::parse;
pub use peg::error::ParseError;

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
