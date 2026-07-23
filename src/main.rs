use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use simiscript::cli::{Cli, CliCommand, CliError, format_raised_trace};
use simiscript::span::line_column;

fn main() -> ExitCode {
    match Cli::parse().command {
        CliCommand::Run { file } => run_file(file),
        CliCommand::Lsp => match simi_lsp::run_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("simi lsp: {error}");
                ExitCode::FAILURE
            }
        },
    }
}

fn run_file(file: PathBuf) -> ExitCode {
    match simiscript::cli::run(&file) {
        Ok(Ok(value)) => {
            println!("{}", value.render());
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
        Err(CliError::Simi(error)) => {
            let source = fs::read_to_string(&file).unwrap_or_default();
            let (line, column) = line_column(&source, error.span().start);
            eprintln!("{}:{}:{}: {}", file.display(), line, column, error);
            ExitCode::FAILURE
        }
    }
}
