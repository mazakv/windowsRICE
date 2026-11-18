//! `windowsrice-diag`: read-only diagnostics.
//!
//! Prints the environment, hardware, display and capability report to
//! stdout as text or JSON. It writes no files, changes no settings, touches
//! no registry values (reads only) and never requests elevation. Logging goes
//! to stderr only.

#![forbid(unsafe_code)]

mod render;

use std::io::Write;
use std::process::ExitCode;

use windowsrice_core::logging::{LogLevel, init_stderr};

const USAGE: &str = "\
windowsrice-diag — read-only windowsRICE diagnostics

USAGE:
    windowsrice-diag [--json] [--verbose]

OPTIONS:
    --json       Print the report as JSON
    --verbose    Log debug details to stderr
    --version    Print version
    -h, --help   Print this help

This tool only reads system information. It writes no files and changes
no Windows settings.";

#[derive(Debug, Default)]
struct Options {
    json: bool,
    verbose: bool,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
    let mut o = Options::default();
    for a in args {
        match a.as_str() {
            "--json" => o.json = true,
            "--verbose" => o.verbose = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(None);
            }
            "--version" => {
                println!("windowsrice-diag {}", windowsrice_core::VERSION);
                return Ok(None);
            }
            other => return Err(format!("unknown argument: {other}\n\n{USAGE}")),
        }
    }
    Ok(Some(o))
}

fn main() -> ExitCode {
    let opts = match parse_args(std::env::args().skip(1)) {
        Ok(Some(o)) => o,
        Ok(None) => return ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    let level = if opts.verbose {
        LogLevel::Debug
    } else {
        LogLevel::Warn
    };
    if let Err(e) = init_stderr(level) {
        eprintln!("warning: {e}");
    }

    let report = windowsrice_windows::collect();
    let output = if opts.json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("failed to serialize report: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        render::text(&report)
    };

    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{output}").is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
