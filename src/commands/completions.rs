//! The `completions` command: generate shell completion scripts.

use clap::CommandFactory;
use clap_complete::Shell;

use crate::error::Error;
use crate::messages;

/// Arguments for `run-cli completions`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The shell to generate completions for.
    #[arg(value_name = "SHELL", help = messages::OPT_SHELL_HELP)]
    pub shell: Shell,
}

/// Print the completion script for the requested shell to stdout.
pub fn execute(args: Args) -> Result<i32, Error> {
    let mut command = crate::cli::Cli::command();
    clap_complete::generate(args.shell, &mut command, "run-cli", &mut std::io::stdout());
    Ok(0)
}
