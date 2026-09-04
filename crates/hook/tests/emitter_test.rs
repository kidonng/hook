use fish_parser::parse;
use hook::bash::lowering::lower_program;
use hook::bash::emitter::emit_bash;

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
fn test_emit_array_negative_subscript_defense() {
    let input = "echo $var[-1] $var[1..3]\n";
    let lowered = lower_program(&parse(input).unwrap());
    let bash = emit_bash(&lowered);
    // Defense against Bash 3.2 bad array subscript: must use dynamic length calculation
    assert!(bash.contains(r#""${var[$((${#var[@]}-1))]}""#));
    assert!(bash.contains(r#""${var[@]:0:3}""#));
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
