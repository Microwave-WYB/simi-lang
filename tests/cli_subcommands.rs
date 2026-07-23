use std::io::Write;
use std::process::{Command, Stdio};

fn simi() -> Command {
    Command::new(env!("CARGO_BIN_EXE_simi"))
}

fn lsp_frame(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

#[test]
fn help_lists_run_and_lsp_subcommands() {
    let output = simi().arg("--help").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("run"));
    assert!(stdout.contains("lsp"));
}

#[test]
fn direct_file_form_is_rejected() {
    let output = simi().arg("demo.simi").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unrecognized subcommand 'demo.simi'"));
    assert!(stderr.contains("Usage: simi <COMMAND>"));
}

#[test]
fn lsp_subcommand_serves_stdio_protocol() {
    let mut child = simi()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let messages = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
        r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
    ]
    .into_iter()
    .map(lsp_frame)
    .collect::<String>();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(messages.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(r#""id":1"#));
    assert!(stdout.contains(r#""name":"simi-lsp""#));
    assert!(stdout.contains(r#""id":2"#));
}
