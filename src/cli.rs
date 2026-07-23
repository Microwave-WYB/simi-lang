use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::span::line_column;
use crate::{Engine, Raised, ScriptResult, SimiError};

#[derive(Debug, Parser)]
#[command(name = "simi")]
pub struct Cli {
    #[command(subcommand)]
    pub command: CliCommand,
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// Evaluate a Simi source file.
    Run { file: PathBuf },
    /// Run the Simi language server over standard input and output.
    Lsp,
}

#[derive(Debug)]
pub enum CliError {
    Io { path: PathBuf, source: io::Error },
    Simi(SimiError),
}

pub fn run(file: &Path) -> Result<ScriptResult, CliError> {
    let source = fs::read_to_string(file).map_err(|source| CliError::Io {
        path: file.to_path_buf(),
        source,
    })?;
    Engine::builder()
        .stdlib()
        .stdio()
        .build()
        .eval(&source)
        .map_err(CliError::Simi)
}

pub fn format_raised_trace(path: &Path, source: &str, raised: &Raised) -> String {
    let mut rendered = String::new();
    let mut context = Some(raised);

    while let Some(raised) = context {
        if !rendered.is_empty() {
            rendered.push_str("\ncaused by:\n");
        }

        let (line, column) = line_column(source, raised.origin.start);
        write!(rendered, "{}:{line}:{column}: {raised}", path.display())
            .expect("writing to a string cannot fail");

        for frame in &raised.frames {
            let (line, column) = line_column(source, frame.call_span.start);
            write!(
                rendered,
                "\n  at {} ({}:{line}:{column})",
                frame.function,
                path.display()
            )
            .expect("writing to a string cannot fail");
        }

        context = raised.cause.as_deref();
    }

    rendered
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Simi(error) => error.fmt(formatter),
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Simi(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::span::Span;
    use crate::{TraceFrame, Value};

    use super::*;

    #[test]
    fn parses_run_and_lsp_subcommands_and_rejects_direct_files() {
        let run = Cli::try_parse_from(["simi", "run", "demo.simi"]).unwrap();
        assert!(matches!(
            run.command,
            CliCommand::Run { file } if file == Path::new("demo.simi")
        ));

        let lsp = Cli::try_parse_from(["simi", "lsp"]).unwrap();
        assert!(matches!(lsp.command, CliCommand::Lsp));
        assert!(Cli::try_parse_from(["simi", "demo.simi"]).is_err());
    }

    #[test]
    fn reports_the_path_for_missing_files() {
        let path = PathBuf::from("this-file-does-not-exist.simi");
        let error = match run(&path) {
            Ok(_) => panic!("missing file should fail"),
            Err(error) => error,
        };
        assert!(matches!(error, CliError::Io { path: error_path, .. } if error_path == path));
    }

    #[test]
    fn cli_registers_standard_stream_modules() {
        let path = std::env::temp_dir().join(format!("simi-stdio-{}.simi", std::process::id()));
        fs::write(
            &path,
            r#"
            let stdout = require("std/io/stdout")
            let stderr = require("std/io/stderr")
            [type(stdout.println), type(stderr.println)]
            "#,
        )
        .unwrap();
        let result = run(&path).unwrap().unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(result.render(), "[\"function\", \"function\"]");
    }

    #[test]
    fn formats_single_raises_with_unicode_columns_and_innermost_first_frames() {
        let source = "é fn_call()\nraise \"boom\"";
        let raised = Raised {
            value: Value::String("boom".to_owned()),
            origin: Span::new(source.find("raise").unwrap(), source.len()),
            frames: vec![
                TraceFrame {
                    function: "leaf".to_owned(),
                    call_span: Span::new(source.find("fn_call").unwrap(), 12),
                },
                TraceFrame {
                    function: "outer".to_owned(),
                    call_span: Span::new(0, 2),
                },
            ],
            cause: None,
        };

        assert_eq!(
            format_raised_trace(Path::new("demo.simi"), source, &raised),
            concat!(
                "demo.simi:2:1: raised \"boom\"\n",
                "  at leaf (demo.simi:1:3)\n",
                "  at outer (demo.simi:1:1)"
            )
        );
    }

    #[test]
    fn formats_newest_raise_first_without_blank_lines() {
        let source = "raise \"old\"\nraise \"new\"";
        let raised = Raised {
            value: Value::String("new".to_owned()),
            origin: Span::new(source.rfind("raise").unwrap(), source.len()),
            frames: Vec::new(),
            cause: Some(Box::new(Raised {
                value: Value::String("old".to_owned()),
                origin: Span::new(0, 11),
                frames: vec![TraceFrame {
                    function: "load".to_owned(),
                    call_span: Span::new(6, 11),
                }],
                cause: None,
            })),
        };

        assert_eq!(
            format_raised_trace(Path::new("errors.simi"), source, &raised),
            concat!(
                "errors.simi:2:1: raised \"new\"\n",
                "caused by:\n",
                "errors.simi:1:1: raised \"old\"\n",
                "  at load (errors.simi:1:7)"
            )
        );
    }
}
