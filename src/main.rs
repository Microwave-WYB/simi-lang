use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use simi::cli::{Cli, CliCommand, CliError, format_raised_trace};
use simi::package::ResolutionMode;
use simi::span::line_column;

fn main() -> ExitCode {
    match Cli::parse().command {
        CliCommand::Run {
            inspect,
            locked,
            offline,
            file,
        } => {
            let mode = if offline {
                ResolutionMode::Offline
            } else if locked {
                ResolutionMode::Locked
            } else {
                ResolutionMode::Update
            };
            run_file(file, inspect, mode)
        }
        CliCommand::Lock { offline, file } => match simi::cli::lock(&file, offline) {
            Ok(path) => {
                println!("{}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("simi lock: {error}");
                ExitCode::FAILURE
            }
        },
        CliCommand::Lsp => {
            let engine = simi::Engine::builder().stdlib().stdio().build();
            match simi_lsp::run_stdio_with_module_sources(engine.module_sources()) {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("simi lsp: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn run_file(file: PathBuf, inspect: bool, mode: ResolutionMode) -> ExitCode {
    match simi::cli::run(&file, mode) {
        Ok(Ok(value)) => {
            if inspect {
                println!("{}", value.render());
            }
            ExitCode::SUCCESS
        }
        Ok(Err(raised)) => {
            let source = fs::read_to_string(&file).unwrap_or_default();
            eprintln!("{}", format_raised_trace(&file, &source, &raised));
            ExitCode::FAILURE
        }
        Err(CliError::Io { path, source }) => {
            eprintln!("{}: {}", path.display(), source);
            ExitCode::FAILURE
        }
        Err(CliError::Package { path, message }) => {
            eprintln!("{}: {message}", path.display());
            ExitCode::FAILURE
        }
        Err(CliError::Simi(error)) => {
            let source = fs::read_to_string(&file).unwrap_or_default();
            let (line, column) = line_column(&source, error.span().start);
            eprintln!("{}:{}:{}: {}", file.display(), line, column, error);
            ExitCode::FAILURE
        }
    }
}
