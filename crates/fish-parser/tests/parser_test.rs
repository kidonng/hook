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
            assert_eq!(cmd.args[1].parts, vec![WordPart::SingleQuoted("hello world".to_string())]);
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
            assert_eq!(args[1].parts, vec![WordPart::Variable(VariableRef {
                name: "status".to_string(),
                slices: vec![],
            })]);
            // $argv[1]
            assert_eq!(args[2].parts, vec![WordPart::Variable(VariableRef {
                name: "argv".to_string(),
                slices: vec![Slice::Index(SliceIndex::Pos(1))],
            })]);
            // $var[1..3]
            assert_eq!(args[3].parts, vec![WordPart::Variable(VariableRef {
                name: "var".to_string(),
                slices: vec![Slice::Range {
                    start: Some(SliceIndex::Pos(1)),
                    end: Some(SliceIndex::Pos(3)),
                }],
            })]);
            // $var[-1]
            assert_eq!(args[4].parts, vec![WordPart::Variable(VariableRef {
                name: "var".to_string(),
                slices: vec![Slice::Index(SliceIndex::Neg(1))],
            })]);
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
                WordPart::CommandSubst(stmts) => {
                    assert_eq!(stmts.len(), 1);
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
            assert_eq!(f.name, "greet");
            assert_eq!(f.named_args, vec!["name".to_string(), "title".to_string()]);
            assert_eq!(f.description, Some("greets a person".to_string()));
            assert_eq!(f.body.len(), 1);
        }
        _ => panic!("expected function statement"),
    }
}
