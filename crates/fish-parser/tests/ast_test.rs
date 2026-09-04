use fish_parser::ast::*;

#[test]
fn test_ast_serialization() {
    let program = Program {
        shebang: Some("#!/usr/bin/env fish".to_string()),
        statements: vec![
            Statement::Pipeline(Pipeline {
                commands: vec![Command {
                    negate: false,
                    args: vec![
                        Word {
                            parts: vec![WordPart::Literal("echo".to_string())],
                        },
                        Word {
                            parts: vec![WordPart::Variable(VariableRef {
                                name: "status".to_string(),
                                slices: vec![],
                            })],
                        },
                    ],
                    redirections: vec![],
                }],
                combinator: Combinator::None,
                background: false,
            }),
        ],
    };

    let serialized = serde_json::to_string(&program).expect("serialization failed");
    let deserialized: Program = serde_json::from_str(&serialized).expect("deserialization failed");
    assert_eq!(program, deserialized);
}
