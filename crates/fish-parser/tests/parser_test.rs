use fish_parser::ast::*;
use fish_parser::parse;

#[test]
fn test_parse_shebang_and_simple_command() {
    let input = "#!/usr/bin/env fish\necho 'hello world'\n";
    let program = parse(input).expect("parsing failed");
    assert_eq!(program.shebang, Some("#!/usr/bin/env fish".to_string()));
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0] {
        Statement::Pipeline(p) => {
            assert_eq!(p.commands.len(), 1);
            let cmd = &p.commands[0];
            assert_eq!(cmd.args[0].as_single_literal(), Some("echo"));
            assert_eq!(
                cmd.args[1].parts,
                vec![WordPart::SingleQuoted("hello world".to_string())]
            );
        }
        _ => panic!("expected pipeline statement"),
    }
}

#[test]
fn test_parse_variables_and_slices() {
    let input = "echo $status $argv[1] $var[1..3] $var[-1]\n";
    let program = parse(input).expect("parsing failed");
    match &program.statements[0] {
        Statement::Pipeline(p) => {
            let args = &p.commands[0].args;
            assert_eq!(args.len(), 5);
            // $status
            assert_eq!(
                args[1].parts,
                vec![WordPart::Variable(VariableRef {
                    target: VariableTarget::Named("status".to_string()),
                    slices: vec![],
                })]
            );
            // $argv[1]
            assert_eq!(
                args[2].parts,
                vec![WordPart::Variable(VariableRef {
                    target: VariableTarget::Named("argv".to_string()),
                    slices: vec![Slice::Index(SliceIndex::Pos(1))],
                })]
            );
            // $var[1..3]
            assert_eq!(
                args[3].parts,
                vec![WordPart::Variable(VariableRef {
                    target: VariableTarget::Named("var".to_string()),
                    slices: vec![Slice::Range {
                        start: Some(SliceIndex::Pos(1)),
                        end: Some(SliceIndex::Pos(3)),
                    }],
                })]
            );
            // $var[-1]
            assert_eq!(
                args[4].parts,
                vec![WordPart::Variable(VariableRef {
                    target: VariableTarget::Named("var".to_string()),
                    slices: vec![Slice::Index(SliceIndex::Neg(1))],
                })]
            );
        }
        _ => panic!("expected pipeline"),
    }
}

#[test]
fn test_parse_command_substitution_and_psub() {
    let input = "diff (sort a | psub) (sort b | psub)\n";
    let program = parse(input).expect("parsing failed");
    match &program.statements[0] {
        Statement::Pipeline(p) => {
            let args = &p.commands[0].args;
            assert_eq!(args.len(), 3);
            match &args[1].parts[0] {
                WordPart::CommandSubst { statements, .. } => {
                    assert_eq!(statements.len(), 1);
                }
                _ => panic!("expected CommandSubst"),
            }
        }
        _ => panic!("expected pipeline"),
    }
}

#[test]
fn test_parse_if_statement() {
    let input = r#"
if test -f foo
    echo yes
else if test -d foo
    echo dir
else
    echo no
end
"#;
    let program = parse(input).expect("parsing failed");
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0] {
        Statement::If(if_stmt) => {
            assert_eq!(if_stmt.then_body.len(), 1);
            assert_eq!(if_stmt.elif_branches.len(), 1);
            assert!(if_stmt.else_body.is_some());
        }
        _ => panic!("expected if statement"),
    }
}

#[test]
fn test_parse_function() {
    let input = r#"
function greet -a name title -d "greets a person"
    echo "Hello $title $name"
end
"#;
    let program = parse(input).expect("parsing failed");
    match &program.statements[0] {
        Statement::Function(f) => {
            assert_eq!(f.name.as_single_literal(), Some("greet"));
            assert_eq!(f.options.len(), 5);
            assert_eq!(f.options[0].as_single_literal(), Some("-a"));
            assert_eq!(f.options[1].as_single_literal(), Some("name"));
            assert_eq!(f.options[2].as_single_literal(), Some("title"));
            assert_eq!(f.options[3].as_single_literal(), Some("-d"));
            assert_eq!(f.body.len(), 1);
        }
        _ => panic!("expected function statement"),
    }
}

#[test]
fn test_parse_multiline_pipe_with_comments() {
    let input = r#"
echo $argv |
    string replace 's#' 'nix shell nixpkgs#' |
    # comment
    string replace , ' nixpkgs#'
"#;
    let program = parse(input).expect("parsing multiline pipe failed");
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0] {
        Statement::Pipeline(p) => {
            assert_eq!(p.commands.len(), 3);
        }
        _ => panic!("expected pipeline"),
    }
}

#[test]
fn test_parse_compound_pipeline_and_negation() {
    let input = r#"
if set --query argv[-1] && ! string match --quiet -- $argv[-1]
    echo ok
end
"#;
    let program = parse(input).expect("parsing compound pipeline failed");
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_command_subst_slice() {
    let input = r#"
set --local resolved (
    dig +short $argv[-1]
)[1]
"#;
    let program = parse(input).expect("parsing command subst slice failed");
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_and_begin_block() {
    let input = r#"
status is-login; and begin
    # comment
end
"#;
    let program = parse(input).expect("parsing and begin failed");
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn test_parse_multiline_command_arguments_with_comments() {
    let input = r#"
set --local roots \
    ~/.nix-profile \
    /opt/homebrew \
    # comment
    (string match '/nix/store/*' $PATH)
"#;
    let program = parse(input).expect("parsing multiline args failed");
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn test_parse_double_quoted_command_substitution_rules() {
    // $(cmd) inside double quotes MUST parse as CommandSubst
    let p1 = parse("echo \"$(pwd)\"\n").unwrap();
    let stmt1 = &p1.statements[0];
    if let Statement::Pipeline(pipe) = stmt1 {
        let arg = &pipe.commands[0].args[1];
        if let WordPart::DoubleQuoted(parts) = &arg.parts[0] {
            assert!(matches!(parts[0], WordPart::CommandSubst { .. }));
        } else {
            panic!("expected DoubleQuoted");
        }
    } else {
        panic!("expected pipeline");
    }

    // Bare (cmd) inside double quotes MUST NOT parse as CommandSubst
    let p2 = parse("echo \"(pwd)\"\n").unwrap();
    let stmt2 = &p2.statements[0];
    if let Statement::Pipeline(pipe) = stmt2 {
        let arg = &pipe.commands[0].args[1];
        if let WordPart::DoubleQuoted(parts) = &arg.parts[0] {
            assert_eq!(parts[0], WordPart::Literal("(pwd)".to_string()));
        } else {
            panic!("expected DoubleQuoted");
        }
    } else {
        panic!("expected pipeline");
    }
}

#[test]
fn test_parse_merged_pipes() {
    for input in &["make &| less\n", "make |& less\n"] {
        let program = parse(input).unwrap();
        assert_eq!(program.statements.len(), 1);
        if let Statement::Pipeline(pipe) = &program.statements[0] {
            assert_eq!(pipe.commands.len(), 2);
            assert_eq!(pipe.pipe_operators, vec![PipeOperator::StdoutAndStderr]);
        } else {
            panic!("expected pipeline");
        }
    }
}

#[test]
fn test_parse_block_redirections() {
    let input_while = "while read -l line\n echo $line\n end < input.txt\n";
    let program_while = parse(input_while).unwrap();
    if let Statement::While(w) = &program_while.statements[0] {
        assert_eq!(w.redirections.len(), 1);
        assert_eq!(w.redirections[0].mode, RedirectMode::Input);
    } else {
        panic!("expected while statement");
    }

    let input_for = "for x in 1 2 3\n echo $x\n end > output.txt\n";
    let program_for = parse(input_for).unwrap();
    if let Statement::For(f) = &program_for.statements[0] {
        assert_eq!(f.redirections.len(), 1);
        assert_eq!(f.redirections[0].mode, RedirectMode::Output);
    } else {
        panic!("expected for statement");
    }

    let input_if = "if test -e file\n echo yes\n end 2>/dev/null\n";
    let program_if = parse(input_if).unwrap();
    if let Statement::If(i) = &program_if.statements[0] {
        assert_eq!(i.redirections.len(), 1);
        assert_eq!(i.redirections[0].fd, Some(2));
    } else {
        panic!("expected if statement");
    }
}

#[test]
fn test_parse_function_with_extended_options() {
    let fish_code = r#"
function my_git_wrap --wraps git -d "Git wrapper"
    git $argv
end

function my_event_handler -e fish_prompt -S
    echo prompt
end

function my_inherit -V PWD -a name
    echo $name in $PWD
end
"#;
    let program = parse(fish_code).unwrap();
    assert_eq!(program.statements.len(), 3);
    if let Statement::Function(f) = &program.statements[0] {
        assert_eq!(f.name.as_single_literal(), Some("my_git_wrap"));
        assert_eq!(f.options.len(), 4);
        assert_eq!(f.options[0].as_single_literal(), Some("--wraps"));
    } else {
        panic!("expected function");
    }
}

#[test]
fn test_parse_dynamic_variable_slice_index() {
    let program = parse("echo $letters[$index]\necho $letters[$start..$end]\n").unwrap();
    if let Statement::Pipeline(p) = &program.statements[0] {
        let arg = &p.commands[0].args[1];
        if let WordPart::Variable(v) = &arg.parts[0] {
            assert_eq!(v.name(), Some("letters"));
            assert_eq!(v.slices.len(), 1);
            assert!(matches!(v.slices[0], Slice::Index(SliceIndex::Variable(_))));
        } else {
            panic!("expected variable");
        }
    }
    if let Statement::Pipeline(p) = &program.statements[1] {
        let arg = &p.commands[0].args[1];
        if let WordPart::Variable(v) = &arg.parts[0] {
            assert_eq!(v.name(), Some("letters"));
            assert_eq!(v.slices.len(), 1);
            assert!(matches!(
                v.slices[0],
                Slice::Range {
                    start: Some(SliceIndex::Variable(_)),
                    end: Some(SliceIndex::Variable(_))
                }
            ));
        } else {
            panic!("expected variable");
        }
    }
}

#[test]
fn test_parse_indirect_variable() {
    let program = parse("echo $$var\n").unwrap();
    assert_eq!(program.statements.len(), 1);
    if let Statement::Pipeline(p) = &program.statements[0] {
        let arg = &p.commands[0].args[1];
        if let WordPart::Variable(v) = &arg.parts[0] {
            assert!(matches!(v.target, VariableTarget::Indirect(_)));
        } else {
            panic!("expected variable");
        }
    }
}

#[test]
fn test_parse_safe_and_noclobber_redirections() {
    let p1 = parse("cat <?input.txt\n").unwrap();
    if let Statement::Pipeline(pipe) = &p1.statements[0] {
        assert_eq!(pipe.commands[0].redirections[0].mode, RedirectMode::SafeInput);
    }

    let p2 = parse("echo hello >?output.txt\n").unwrap();
    if let Statement::Pipeline(pipe) = &p2.statements[0] {
        assert_eq!(pipe.commands[0].redirections[0].mode, RedirectMode::NoClobberOutput);
    }

    let p3 = parse("echo err 2>?err.txt\n").unwrap();
    if let Statement::Pipeline(pipe) = &p3.statements[0] {
        assert_eq!(pipe.commands[0].redirections[0].fd, Some(2));
        assert_eq!(pipe.commands[0].redirections[0].mode, RedirectMode::NoClobberOutput);
    }
}

#[test]
fn test_parse_compound_statement_block() {
    let p = parse("{ echo hello; and echo world; }\n").unwrap();
    assert!(matches!(p.statements[0], Statement::BeginBlock(_)));
}
