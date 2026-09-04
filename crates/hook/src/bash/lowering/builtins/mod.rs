pub mod contains;
pub mod count;
pub mod set;

use crate::bash::ir::LoweredCommand;
use crate::bash::lowering::Scope;
use fish_parser::ast::Command;

pub fn lower_builtin(cmd: &Command, scope: &Scope) -> Option<LoweredCommand> {
    let cmd_name = cmd.args.first().and_then(|w| w.as_single_literal())?;
    match cmd_name {
        "count" => count::lower_count(cmd, scope),
        "contains" => contains::lower_contains(cmd, scope),
        "set" => set::lower_set_query(cmd),
        _ => None,
    }
}
