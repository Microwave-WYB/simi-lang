use std::fs;
use std::process::ExitCode;

use clap::Parser;
use simiscript::cli::{Cli, CliError, format_raised_trace};
use simiscript::span::line_column;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let file = cli.file.clone();

    match simiscript::cli::run(cli) {
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
