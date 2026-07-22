use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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
fn cli_print_flushes_prompt_before_reading_stdin() {
    let path = std::env::temp_dir().join(format!("simi-prompt-cli-{}.simi", std::process::id()));
    fs::write(
        &path,
        r#"
        let stdin = require("std/io/stdin")
        let stdout = require("std/io/stdout")
        stdout.print("prompt: ")
        stdin.readline()
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
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (prompt_sender, prompt_receiver) = mpsc::channel();
    let stdout_reader = thread::spawn(move || {
        let mut prompt = [0; 8];
        let prompt_result = stdout.read_exact(&mut prompt).map(|()| prompt);
        prompt_sender.send(prompt_result).unwrap();

        let mut remaining = Vec::new();
        stdout.read_to_end(&mut remaining).unwrap();
        remaining
    });

    let prompt_result = prompt_receiver.recv_timeout(Duration::from_secs(5));
    stdin.write_all(b"answer\n").unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    let remaining_stdout = stdout_reader.join().unwrap();
    fs::remove_file(path).unwrap();

    let prompt = prompt_result
        .expect("prompt should be observable before stdin is supplied")
        .expect("prompt should be readable");
    assert_eq!(&prompt, b"prompt: ");
    assert!(output.status.success());
    assert_eq!(String::from_utf8(remaining_stdout).unwrap(), "\"answer\"\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn cli_stdin_reads_unicode_lines_and_returns_nil_at_eof() {
    let path = std::env::temp_dir().join(format!("simi-stdin-cli-{}.simi", std::process::id()));
    fs::write(
        &path,
        r#"
        let stdin = require("std/io/stdin")
        [stdin.readline(), stdin.readline()]
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
