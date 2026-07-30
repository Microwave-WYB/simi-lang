use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::package::{ResolutionMode, lock_path, resolve_script};
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
    Run {
        /// Print the final value using canonical inspector rendering.
        #[arg(long)]
        inspect: bool,
        /// Require a matching lockfile and never fetch or rewrite it.
        #[arg(long, conflicts_with = "offline")]
        locked: bool,
        /// Require a matching lockfile and cached Git checkouts without network access.
        #[arg(long)]
        offline: bool,
        file: PathBuf,
    },
    /// Generate or refresh the lockfile for a Simi source file.
    Lock {
        /// Generate from local paths and already cached Git repositories without network access.
        #[arg(long)]
        offline: bool,
        file: PathBuf,
    },
    /// Run the Simi language server over standard input and output.
    Lsp,
}

#[derive(Debug)]
pub enum CliError {
    Io { path: PathBuf, source: io::Error },
    Package { path: PathBuf, message: String },
    Simi(SimiError),
}

pub fn run(file: &Path, mode: ResolutionMode) -> Result<ScriptResult, CliError> {
    let source = fs::read_to_string(file).map_err(|source| CliError::Io {
        path: file.to_path_buf(),
        source,
    })?;
    let resolved = resolve_script(file, mode).map_err(|message| CliError::Package {
        path: file.to_path_buf(),
        message,
    })?;
    if mode == ResolutionMode::Update {
        let path = lock_path(file);
        fs::write(&path, &resolved.lockfile).map_err(|source| CliError::Io { path, source })?;
    }
    Engine::builder()
        .prelude()
        .catalog(resolved.catalog)
        .stdio()
        .build()
        .eval(&source)
        .map_err(CliError::Simi)
}

pub fn lock(file: &Path, offline: bool) -> Result<PathBuf, CliError> {
    let mode = if offline {
        ResolutionMode::OfflineUpdate
    } else {
        ResolutionMode::Update
    };
    let resolved = resolve_script(file, mode).map_err(|message| CliError::Package {
        path: file.to_path_buf(),
        message,
    })?;
    let path = lock_path(file);
    fs::write(&path, resolved.lockfile).map_err(|source| CliError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
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
            Self::Package { path, message } => write!(formatter, "{}: {message}", path.display()),
            Self::Simi(error) => error.fmt(formatter),
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Package { .. } => None,
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
            CliCommand::Run { inspect: false, locked: false, offline: false, file } if file == Path::new("demo.simi")
        ));

        let inspected =
            Cli::try_parse_from(["simi", "run", "--inspect", "--locked", "demo.simi"]).unwrap();
        assert!(matches!(
            inspected.command,
            CliCommand::Run { inspect: true, locked: true, offline: false, file } if file == Path::new("demo.simi")
        ));
        assert!(
            Cli::try_parse_from(["simi", "run", "--locked", "--offline", "demo.simi"]).is_err()
        );

        let lock = Cli::try_parse_from(["simi", "lock", "--offline", "demo.simi"]).unwrap();
        assert!(
            matches!(lock.command, CliCommand::Lock { offline: true, file } if file == Path::new("demo.simi"))
        );

        let lsp = Cli::try_parse_from(["simi", "lsp"]).unwrap();
        assert!(matches!(lsp.command, CliCommand::Lsp));
        assert!(Cli::try_parse_from(["simi", "demo.simi"]).is_err());
    }

    #[test]
    fn reports_the_path_for_missing_files() {
        let path = PathBuf::from("this-file-does-not-exist.simi");
        let error = match run(&path, ResolutionMode::Update) {
            Ok(_) => panic!("missing file should fail"),
            Err(error) => error,
        };
        assert!(matches!(error, CliError::Io { path: error_path, .. } if error_path == path));
    }

    #[test]
    fn run_resolves_path_packages_before_engine_evaluation() {
        let directory = std::env::temp_dir().join(format!(
            "simi-cli-package-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let package = directory.join("deps/tools");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("simi.package.simi"),
            r#"{name = "tools", simi = "0.1", modules = ["tools"]}"#,
        )
        .unwrap();
        fs::write(package.join("tools.simi"), "{value = 42}").unwrap();
        let app = directory.join("app.simi");
        fs::write(
            &app,
            "requires {tools = {path = \"deps/tools\"}}\nlet tools = require(\"tools\")\ntools.value",
        )
        .unwrap();

        assert_eq!(
            run(&app, ResolutionMode::Update).unwrap().unwrap().render(),
            "42"
        );
        assert!(lock_path(&app).is_file());
        assert_eq!(
            run(&app, ResolutionMode::Locked).unwrap().unwrap().render(),
            "42"
        );
        assert_eq!(
            run(&app, ResolutionMode::Offline)
                .unwrap()
                .unwrap()
                .render(),
            "42"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn cli_registers_standard_stream_modules() {
        let path = std::env::temp_dir().join(format!("simi-stdio-{}.simi", std::process::id()));
        fs::write(
            &path,
            r#"
            let io = require("std/io")
            [type(io.println), type(io.eprintln)]
            "#,
        )
        .unwrap();
        let result = run(&path, ResolutionMode::Update).unwrap().unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(result.render(), "[\"function\", \"function\"]");
    }

    #[test]
    fn cli_provides_runtime_owned_standard_library_without_a_source_pin() {
        let path = std::env::temp_dir().join(format!("simi-stdlib-{}.simi", std::process::id()));
        fs::write(
            &path,
            "[iter.to_list(list.iter([4]))[0], number.to_string(5), string.upper(\"simi\"), bytes.length(#[1, 2]), type(require(\"std/iter\")), type(require(\"std/bytes\"))]",
        )
        .unwrap();
        let result = run(&path, ResolutionMode::Update).unwrap().unwrap();
        fs::remove_file(&path).unwrap();
        fs::remove_file(lock_path(&path)).unwrap();
        assert_eq!(result.render(), "[4, \"5\", \"SIMI\", 2, \"map\", \"map\"]");
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
