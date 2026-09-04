use fish_parser::parse;
use hook::bash::lowering::lower_program;
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

#[test]
fn test_lower_builtin_vars_and_slices() {
    let input = "echo $status $pipestatus $argv $argv[1] $argv[2..-1] $argv[-1] $var[-1]\n";
    let prog = parse(input).unwrap();
    let lowered = lower_program(&prog);
    match &lowered.statements[0] {
        LoweredStatement::Pipeline(p) => {
            let args = &p.commands[0].args;
            assert_eq!(args.len(), 8);
            assert_eq!(args[1].parts[0], LoweredWordPart::Variable(LoweredVariableRef::Status));
            assert_eq!(args[2].parts[0], LoweredWordPart::Variable(LoweredVariableRef::Pipestatus));
            assert_eq!(args[3].parts[0], LoweredWordPart::Variable(LoweredVariableRef::ArgvAll));
            assert_eq!(args[4].parts[0], LoweredWordPart::Variable(LoweredVariableRef::ArgvIndex(1)));
            assert_eq!(args[5].parts[0], LoweredWordPart::Variable(LoweredVariableRef::ArgvSlice { start: 2, len: None }));
            assert_eq!(args[6].parts[0], LoweredWordPart::Variable(LoweredVariableRef::ArgvLast));
            assert_eq!(args[7].parts[0], LoweredWordPart::Variable(LoweredVariableRef::Custom {
                name: "var".to_string(),
                subscript: Some(BashSubscript::NegativeOffsetFromLength(1)),
            }));
        }
        _ => panic!("expected pipeline"),
    }
}
