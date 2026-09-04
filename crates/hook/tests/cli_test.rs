use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn test_cli_stdin_pipe() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hook"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn hook binary");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        stdin
            .write_all(b"echo hello\n")
            .expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to read output");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "echo hello");
}

#[test]
fn test_cli_syntax_error() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_hook"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn hook binary");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        stdin
            .write_all(b"if ; echo\n")
            .expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to read output");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("syntax error"));
}

#[test]
fn test_cli_help_and_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_hook"))
        .arg("--help")
        .output()
        .expect("failed to run hook --help");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("hook - Transpile Fish shell scripts to Bash 3.2+"));

    let output = Command::new(env!("CARGO_BIN_EXE_hook"))
        .arg("--version")
        .output()
        .expect("failed to run hook --version");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("hook "));
}
