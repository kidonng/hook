use fish_parser::parse;
use hook::bash::emitter::emit_bash;
use hook::bash::lowering::lower_program;

#[test]
fn test_emit_shebang_rewrite() {
    let input = "#!/usr/bin/env fish\necho hello\n";
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.starts_with("#!/usr/bin/env bash\n"));
    assert!(bash.contains("echo hello"));
}

#[test]
fn test_emit_assignments_bash_3_2() {
    let input = r#"
set -x GREET "hi"
set -l ARR a b c
set -a ARR d
set -e UNWANTED
"#;
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    // At top level, set -l generates ARR=("a" "b" "c")
    assert!(bash.contains(r#"export GREET="hi""#));
    assert!(bash.contains(r#"ARR=("a" "b" "c")"#));
    assert!(bash.contains(r#"ARR+=("d")"#));
    assert!(bash.contains("unset UNWANTED"));
}

#[test]
fn test_emit_array_negative_subscript_modern() {
    let input = "echo $var[-1] $var[1..3]\n";
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.contains(r#""${var[-1]}""#));
    assert!(bash.contains(r#""${var[@]:0:3}""#));
}

#[test]
fn test_emitter_modern_pipes_and_redirects() {
    let input = "cmd1 &| cmd2\ncmd &> output.log\ncmd &>> append.log\n";
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.contains("cmd1 |& cmd2"));
    assert!(bash.contains("cmd &> output.log"));
    assert!(bash.contains("cmd &>> append.log"));
}

#[test]
fn test_emitter_modern_global_declaration_in_function() {
    let input = "function foo\n  set -g count 42\n  set -g items a b\nend\n";
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.contains(r#"declare -g count="42""#));
    assert!(bash.contains(r#"declare -ga items=("a" "b")"#));
}

#[test]
fn test_emitter_modern_slice_assign_and_erase() {
    let input = "set fruit[-1] evil\nset -e fruit[-1]\nset fruit[$idx] kiwi\nset -e fruit[$idx]\n";
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.contains("fruit[-1]=\"evil\""));
    assert!(bash.contains("unset 'fruit[-1]'"));
    assert!(bash.contains("fruit[idx-1]=\"kiwi\""));
    assert!(bash.contains("unset 'fruit[idx-1]'"));
}

#[test]
fn test_emit_control_structures() {
    let input = r#"
if test -f file
    echo ok
else
    echo fail
end
for x in a b
    echo $x
end
while test $count -gt 0
    echo $count
end
"#;
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.contains("if test -f file; then"));
    assert!(bash.contains("else"));
    assert!(bash.contains("fi"));
    assert!(bash.contains("for x in a b; do"));
    assert!(bash.contains("done"));
    assert!(bash.contains(r#"while test "$count" -gt 0; do"#));
}

#[test]
fn test_emit_function_and_process_subst() {
    let input = r#"
function greet -a name
    echo "Hello $name"
end
diff (sort a.txt | psub) (sort b.txt | psub)
"#;
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    assert!(bash.contains("greet() {"));
    assert!(bash.contains(r#"local name="$1""#));
    assert!(bash.contains("<(sort a.txt)"));
    assert!(bash.contains("<(sort b.txt)"));
}
