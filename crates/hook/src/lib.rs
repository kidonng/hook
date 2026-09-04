pub mod bash;
pub mod target;

pub use bash::emit_bash;
pub use target::{Target, TranspileConfig};
