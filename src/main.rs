use std::process::ExitCode;

use run_cli::cli;
use run_cli::messages;

fn main() -> ExitCode {
    match cli::run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            eprintln!("{}{err}", messages::error_prefix());
            ExitCode::from(err.code() as u8)
        }
    }
}
