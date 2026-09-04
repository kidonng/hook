use fish_parser::parse;
use hook::bash::emitter::emit_bash;
use hook::bash::lowering::lower_program;
use std::io::Write;
use std::process::{Command, Stdio};

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
        stdin
            .write_all(bash_code.as_bytes())
            .expect("failed to write to bash stdin");
    }

    let output = child.wait_with_output().expect("failed to wait on bash");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "bash -n syntax validation failed for output:\n{}\nError:\n{}",
            bash_code, stderr
        );
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

#[test]
fn test_transpile_empty_function() {
    let fish = "function prompt_login\nend\n";
    let result = transpile(fish);
    assert!(result.contains("prompt_login() {\n  :\n}"));
}

#[test]
fn test_transpile_abbr_function() {
    let fish = r#"
function abbr-nix-shell
    echo $argv |
        string replace 's#' 'nix shell nixpkgs#' |
        string replace , ' nixpkgs#'
    return 0
end
"#;
    let result = transpile(fish);
    assert!(result.contains("abbr-nix-shell()"));
}

#[test]
fn test_transpile_ping_function() {
    let fish = r#"
function ping
    if set --query argv[-1] && ! string match --regex '^(-.+|[\d.]+|[[:alnum:]:]+)$' --quiet -- $argv[-1]
        set --local resolved (
          dig +short $argv[-1] |
          # Exclude CNAMEs
          string match --invert '*.'
        )[1]

        if test -z "$resolved"
            echo "ping: cannot resolve $argv[-1]: Unknown host"
            return 1
        end

        set argv[-1] $resolved
    end

    command ping -b en0 $argv
end
"#;
    let result = transpile(fish);
    assert!(result.contains("ping()"));
}

#[test]
fn test_transpile_and_begin_empty() {
    let fish = r#"
status is-login; and begin
    # Login shell initialisation
end
"#;
    let result = transpile(fish);
    assert!(result.contains("status is-login && {"));
    assert!(result.contains(':'));
}

#[test]
fn test_transpile_multiline_set_with_comments() {
    let fish = r#"
set --local roots \
    ~/.nix-profile \
    /opt/homebrew \
    # Dynamic paths
    /opt/extra
"#;
    let result = transpile(fish);
    assert!(result.contains("roots="));
    assert!(!result.contains("~/.nix-profile \n"));
}

#[test]
fn test_transpile_double_quoted_parens_literal() {
    let bash = transpile("echo \"(pwd)\"\necho \"$(pwd)\"\n");
    assert_eq!(bash, "echo \"(pwd)\"\necho \"$(pwd)\"\n");
}

#[test]
fn test_transpile_merged_pipes() {
    let bash1 = transpile("make &| less\n");
    assert_eq!(bash1, "make 2>&1 | less\n");

    let bash2 = transpile("make |& less\n");
    assert_eq!(bash2, "make 2>&1 | less\n");
}

#[test]
fn test_transpile_block_redirections() {
    let bash_while = transpile("while read -r line\n  echo \"$line\"\nend < input.txt\n");
    assert!(bash_while.contains("done < input.txt\n"));

    let bash_for = transpile("for x in 1 2 3\n  echo \"$x\"\nend > output.txt\n");
    assert!(bash_for.contains("done > output.txt\n"));

    let bash_if = transpile("if test -e file\n  echo yes\nend 2>/dev/null\n");
    assert!(bash_if.contains("fi 2> /dev/null\n"));
}
