use fish_parser::parse;
use hook::bash::ir::*;
use hook::bash::lowering::lower_program;

#[test]
fn test_lower_set_scope_defense() {
    // At top level (in_function = false), `set -l foo bar` falls back to Global assignment (NOT local)
    let input = "set -l foo 'bar'\n";
    let prog = parse(input).unwrap();
    let lowered = lower_program(&prog);
    match &lowered.statements[0].kind {
        LoweredStatementKind::Assignment(AssignmentIR::Global { name, values, .. }) => {
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
    match &lowered.statements[0].kind {
        LoweredStatementKind::Function(f) => match &f.body[0].kind {
            LoweredStatementKind::Assignment(AssignmentIR::Local { name, .. }) => {
                assert_eq!(name, "foo");
            }
            _ => panic!("expected Local assignment inside function"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn test_lower_psub_detection() {
    let input = "diff (sort file1 | psub) (sort file2 | psub)\n";
    let prog = parse(input).unwrap();
    let lowered = lower_program(&prog);
    match &lowered.statements[0].kind {
        LoweredStatementKind::Pipeline(p) => {
            let cmds = p.commands();
            let arg1 = &cmds[0].args[1];
            match &arg1.parts[0] {
                LoweredWordPart::ProcessSubst(pipeline) => {
                    assert_eq!(pipeline.commands().len(), 1);
                    assert_eq!(pipeline.commands()[0].args[0].as_literal(), Some("sort"));
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
    match &lowered.statements[0].kind {
        LoweredStatementKind::Pipeline(p) => {
            let cmds = p.commands();
            let args = &cmds[0].args;
            assert_eq!(args.len(), 8);
            assert_eq!(
                args[1].parts[0],
                LoweredWordPart::Variable(LoweredVariableRef::Status)
            );
            assert_eq!(
                args[2].parts[0],
                LoweredWordPart::Variable(LoweredVariableRef::Pipestatus)
            );
            assert_eq!(
                args[3].parts[0],
                LoweredWordPart::Variable(LoweredVariableRef::ArgvAll)
            );
            assert_eq!(
                args[4].parts[0],
                LoweredWordPart::Variable(LoweredVariableRef::ArgvIndex(1))
            );
            assert_eq!(
                args[5].parts[0],
                LoweredWordPart::Variable(LoweredVariableRef::ArgvSlice {
                    start: 2,
                    len: None
                })
            );
            assert_eq!(
                args[6].parts[0],
                LoweredWordPart::Variable(LoweredVariableRef::ArgvLast)
            );
            assert_eq!(
                args[7].parts[0],
                LoweredWordPart::Variable(LoweredVariableRef::Custom {
                    name: "var".to_string(),
                    subscript: Some(BashSubscript::Index(-1)),
                })
            );
        }
        _ => panic!("expected pipeline"),
    }
}

#[test]
fn test_lower_merged_pipe_modern() {
    let fish = "cmd1 &| cmd2\n";
    let prog = parse(fish).unwrap();
    let lowered = lower_program(&prog);
    let stmt = &lowered.statements[0];
    if let LoweredStatementKind::Pipeline(p) = &stmt.kind {
        assert_eq!(p.pipe_operators, vec![PipeKind::StdoutAndStderr]);
        assert!(
            p.commands()[0].redirections.is_empty(),
            "should not synthesize 2>&1 redirection"
        );
    } else {
        panic!("expected pipeline");
    }
}

#[test]
fn test_lower_negative_slice_modern() {
    let fish = "echo $arr[-1]\n";
    let prog = parse(fish).unwrap();
    let lowered = lower_program(&prog);
    if let LoweredStatementKind::Pipeline(p) = &lowered.statements[0].kind {
        let arg = &p.commands()[0].args[1];
        if let LoweredWordPart::Variable(LoweredVariableRef::Custom { subscript, .. }) =
            &arg.parts[0]
        {
            assert_eq!(subscript, &Some(BashSubscript::Index(-1)));
        } else {
            panic!("expected custom variable with negative index");
        }
    } else {
        panic!("expected pipeline");
    }
}

#[test]
fn test_lower_set_global_in_function() {
    let fish = "function foo\n  set -g bar 1\nend\n";
    let prog = parse(fish).unwrap();
    let lowered = lower_program(&prog);
    if let LoweredStatementKind::Function(f) = &lowered.statements[0].kind {
        if let LoweredStatementKind::Assignment(AssignmentIR::Global { in_function, .. }) = &f.body[0].kind {
            assert!(in_function);
        } else {
            panic!("expected global assignment in function");
        }
    } else {
        panic!("expected function");
    }
}

#[test]
fn test_lower_slice_assign_and_erase_negative() {
    let fish = "set fruit[-1] evil\nset -e fruit[-2]\n";
    let prog = parse(fish).unwrap();
    let lowered = lower_program(&prog);
    if let LoweredStatementKind::Assignment(AssignmentIR::SliceAssign { name, index, .. }) = &lowered.statements[0].kind
    {
        assert_eq!(name, "fruit");
        assert_eq!(index, &SliceIndexIR::Negative(-1));
    } else {
        panic!("expected SliceAssign");
    }
    if let LoweredStatementKind::Assignment(AssignmentIR::SliceErase { name, index }) = &lowered.statements[1].kind
    {
        assert_eq!(name, "fruit");
        assert_eq!(index, &SliceIndexIR::Negative(-2));
    } else {
        panic!("expected SliceErase");
    }
}

#[test]
fn test_lowering_preserves_statement_spans() {
    let input = "set -l foo bar\n\necho $foo\n";
    let prog = fish_parser::parse(input).unwrap();
    let lowered = lower_program(&prog);
    assert_eq!(lowered.statements.len(), 2);
    assert_eq!(lowered.statements[0].span.start_line, 1);
    assert_eq!(lowered.statements[1].span.start_line, 3);
}
