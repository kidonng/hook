use std::process::{Command, Stdio};
use std::io::Write;
use fish_parser::parse;
use hook::bash::lowering::lower_program;
use hook::bash::emitter::emit_bash;

fn transpile(fish_code: &str) -> String {
    let parsed = parse(fish_code).expect("failed to parse fish code");
    let lowered = lower_program(&parsed);
    let bash_code = emit_bash(&lowered);

    // Verify syntax using bash -n
    let mut child = Command::new("bash")
        .arg("-n")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to invoke bash for syntax validation");

    {
        let stdin = child.stdin.as_mut().expect("failed to get stdin");
        stdin.write_all(bash_code.as_bytes()).expect("failed to write to bash stdin");
    }

    let output = child.wait_with_output().expect("failed to wait on bash");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("bash -n syntax validation failed for output:\n{}\nError:\n{}", bash_code, stderr);
    }

    bash_code
}

#[test]
fn test_snapshot_set_and_arrays() {
    let fish = r#"
#!/usr/bin/env fish
set -x PATH $PATH /opt/bin
set -l items apple banana cherry
set -a items date
echo $items[1]
echo $items[-1]
echo $items[2..3]
"#;
    let result = transpile(fish);
    insta::assert_snapshot!(result);
}

#[test]
fn test_snapshot_builtins_and_args() {
    let fish = r#"
function deploy -a env target
    if test $status -eq 0
        echo "deploying $argv[1] to $argv[2..-1]"
    end
    echo "exit status: $status"
end
"#;
    let result = transpile(fish);
    insta::assert_snapshot!(result);
}

#[test]
fn test_snapshot_process_and_command_subst() {
    let fish = r#"
diff (sort a.txt | psub) (sort b.txt | psub)
set files (ls -1)
for f in (find . -name "*.txt")
    echo $f
end
"#;
    let result = transpile(fish);
    insta::assert_snapshot!(result);
}

#[test]
fn test_snapshot_control_flow() {
    let fish = r#"
if test -f foo.txt
    echo "is file"
else if test -d foo.txt
    echo "is dir"
else
    echo "unknown"
end

switch $target
    case prod production
        echo "deploying prod"
    case staging
        echo "deploying staging"
    case '*'
        echo "default"
end

while test $count -gt 0
    echo $count
    set count 0
end
"#;
    let result = transpile(fish);
    insta::assert_snapshot!(result);
}
