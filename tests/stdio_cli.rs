use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn cli_routes_standard_output_and_error_to_distinct_streams() {
    let path = std::env::temp_dir().join(format!("simi-stdio-cli-{}.simi", std::process::id()));
    fs::write(
        &path,
        r#"
        let stdout = require("std/io/stdout")
        let stderr = require("std/io/stderr")
        stdout.println("hello")
        stderr.println("warning")
        42
        "#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_simi"))
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello\n42\n");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "warning\n");
}

#[test]
fn cli_stdin_reads_unicode_lines_and_returns_nil_at_eof() {
    let path = std::env::temp_dir().join(format!("simi-stdin-cli-{}.simi", std::process::id()));
    fs::write(
        &path,
        r#"
        let stdin = require("std/io/stdin")
        [stdin.read_line(), stdin.read_line()]
        "#,
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_simi"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all("héllo\n".as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    fs::remove_file(path).unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "[\"héllo\", nil]\n"
    );
    assert!(output.stderr.is_empty());
}
