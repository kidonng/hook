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
    assert_eq!(bash1, "make |& less\n");

    let bash2 = transpile("make |& less\n");
    assert_eq!(bash2, "make |& less\n");
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

#[test]
fn test_transpile_function_with_wraps_and_events() {
    let fish = "function g -w git -d 'git alias'\n  git $argv\nend\n";
    let bash = transpile(fish);
    assert_eq!(bash, "g() {\n  git \"$@\"\n}\n");
}

#[test]
fn test_transpile_process_id_variables() {
    let bash = transpile("echo $fish_pid\necho $last_pid\n");
    assert_eq!(bash, "echo \"$$\"\necho \"$!\"\n");

    let bash_quoted = transpile("echo \"PID: $fish_pid, Background: $last_pid\"\n");
    assert_eq!(bash_quoted, "echo \"PID: $$, Background: $!\"\n");
}

#[test]
fn test_phase1_alignment_combined() {
    let script = r#"
#!/usr/bin/env fish

function run_pipeline --wraps make -d "Run build pipeline"
    make &| tee build.log
end

function check_services -S
    while read -r service
        if test -n "$service"
            echo "Service running: $service (PID: $fish_pid)"
        end 2>/dev/null
    end < services.txt
end

echo "(pwd)"
echo "$(pwd)"
echo "Background PID: $last_pid"
"#;

    let bash = transpile(script);
    assert!(bash.starts_with("#!/usr/bin/env bash\n"));
    assert!(bash.contains("make |& tee build.log"));
    assert!(bash.contains("while read -r service; do"));
    assert!(bash.contains("done < services.txt"));
    assert!(bash.contains("fi 2> /dev/null"));
    assert!(bash.contains("echo \"(pwd)\""));
    assert!(bash.contains("echo \"$(pwd)\""));
    assert!(bash.contains("echo \"Background PID: $!\""));
}

#[test]
fn test_transpile_dynamic_variable_subscript() {
    let bash = transpile("echo $letters[$index]\n");
    assert_eq!(bash, "echo \"${letters[index-1]}\"\n");
    let bash_range = transpile("echo $letters[$start..$end]\n");
    assert_eq!(
        bash_range,
        "echo \"${letters[@]:$((start - 1)):$((end - start + 1))}\"\n"
    );
}

#[test]
fn test_transpile_slice_assignment_and_erase() {
    let bash_assign = transpile("set fruit[2] evil\n");
    assert_eq!(bash_assign, "fruit[1]=\"evil\"\n");

    let bash_erase = transpile("set -e fruit[1]\n");
    assert_eq!(bash_erase, "unset 'fruit[0]'\n");

    let bash_assign_neg = transpile("set fruit[-1] evil\n");
    assert_eq!(bash_assign_neg, "fruit[-1]=\"evil\"\n");

    let bash_erase_neg = transpile("set -e fruit[-1]\n");
    assert_eq!(bash_erase_neg, "unset 'fruit[-1]'\n");

    let bash_assign_dyn = transpile("set fruit[$idx] evil\n");
    assert_eq!(bash_assign_dyn, "fruit[idx-1]=\"evil\"\n");

    let bash_erase_dyn = transpile("set -e fruit[$idx]\n");
    assert_eq!(bash_erase_dyn, "unset 'fruit[idx-1]'\n");
}

#[test]
fn test_transpile_indirect_variable() {
    let bash = transpile("set var name\necho $$var\n");
    assert!(bash.contains("${!var}"));
}

#[test]
fn test_phase2_alignment_combined() {
    let script = r#"
#!/usr/bin/env fish

set fruits apple banana cherry date
set idx 2
echo $fruits[$idx]

set start 1
set end 3
echo $fruits[$start..$end]

set fruits[1] apricot
set fruits[-1] elderberry
set fruits[$idx] blueberry

set -e fruits[1]
set -e fruits[-1]
set -e fruits[$idx]

set var fruits
echo $$var
"#;

    let bash = transpile(script);
    assert!(bash.starts_with("#!/usr/bin/env bash\n"));
    assert!(bash.contains("fruits=(\"apple\" \"banana\" \"cherry\" \"date\")"));
    assert!(bash.contains("echo \"${fruits[idx-1]}\""));
    assert!(bash.contains("echo \"${fruits[@]:$((start - 1)):$((end - start + 1))}\""));
    assert!(bash.contains("fruits[0]=\"apricot\""));
    assert!(bash.contains("fruits[-1]=\"elderberry\""));
    assert!(bash.contains("fruits[idx-1]=\"blueberry\""));
    assert!(bash.contains("unset 'fruits[0]'"));
    assert!(bash.contains("unset 'fruits[-1]'"));
    assert!(bash.contains("unset 'fruits[idx-1]'"));
}

#[test]
fn test_transpile_count_builtin() {
    let bash_var = transpile("count $foos\n");
    assert!(bash_var.contains("${#foos[@]}"));

    let bash_argv = transpile("count $argv\n");
    assert!(bash_argv.contains("$#"));

    let bash_if = transpile("if count $foos >/dev/null\n  echo yes\nend\n");
    assert!(bash_if.contains("[ \"${#foos[@]}\" -gt 0 ]"));
}

#[test]
fn test_transpile_contains_builtin() {
    let bash = transpile("if contains blue $smurf\n  echo found\nend\n");
    assert!(bash.contains("for __hook_item in"));
}

#[test]
fn test_transpile_safe_and_noclobber_redirections() {
    let bash_safe = transpile("cat <?input.txt\n");
    assert!(bash_safe.contains("< \"$([ -r input.txt ] && echo input.txt || echo /dev/null)\""));

    let bash_noclobber = transpile("echo hello >?output.txt\n");
    assert!(bash_noclobber.contains("> output.txt"));
}

#[test]
fn test_transpile_compound_statement_block() {
    let bash = transpile("{ echo hello; and echo world; }\n");
    assert!(bash.contains("{\n  echo hello && echo world\n}"));
}

#[test]
fn test_phase3_alignment_combined() {
    let script = r#"
#!/usr/bin/env fish

set colors red green blue yellow
set total (count $colors)

if count $colors >/dev/null
    echo "has colors"
end

if contains blue $colors
    echo "found blue"
end

set idx (contains -i green $colors)

{
    cat <?input.txt
    echo "phase 3 complete" >?output.txt
}
"#;

    let bash = transpile(script);
    assert!(bash.starts_with("#!/usr/bin/env bash\n"));
    assert!(bash.contains("colors=(\"red\" \"green\" \"blue\" \"yellow\")"));
    assert!(bash.contains("total=\"$(printf '%s\\n' \"${#colors[@]}\")\""));
    assert!(bash.contains("if [ \"${#colors[@]}\" -gt 0 ]; then"));
    assert!(bash.contains("for __hook_item in \"${colors[@]}\""));
    assert!(bash.contains("__hook_i=1; for __hook_item in \"${colors[@]}\""));
    assert!(bash.contains("< \"$([ -r input.txt ] && echo input.txt || echo /dev/null)\""));
    assert!(bash.contains("> output.txt"));
    assert!(bash.contains("{\n"));
    assert!(bash.contains("}\n"));
}

#[test]
fn test_transpile_pipeline_negate_and_assignments() {
    let script = "not true | false\nGIT_DIR=somerepo git status\n";
    let bash = transpile(script);
    assert!(bash.contains("! true | false"));
    assert!(bash.contains("GIT_DIR=somerepo git status"));
}

#[test]
fn test_transpile_block_in_pipeline() {
    let script = "begin; echo a; echo b; end | grep a\n";
    let bash = transpile(script);
    assert!(bash.contains("} | grep a"));
    assert!(bash.contains("echo a"));
    assert!(bash.contains("echo b"));
}

#[test]
fn test_preserve_blank_lines_top_level_and_collapsed() {
    let input = r#"
echo 1

echo 2



echo 3
"#;
    let output = transpile(input);
    let expected = "echo 1\n\necho 2\n\necho 3\n";
    assert_eq!(output.trim(), expected.trim());
}

#[test]
fn test_preserve_blank_lines_inside_blocks_without_boundary_padding() {
    let input = r#"
function my_fn

    echo start

    echo end

end
"#;
    let output = transpile(input);
    let expected = r#"my_fn() {
  echo start

  echo end
}"#;
    assert_eq!(output.trim(), expected.trim());
}
