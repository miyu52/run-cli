//! The `run` command: resolve a tool and execute it.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::commands::{Context, resolve_absolute};
use crate::error::Error;
use crate::messages;
use crate::runner::{self, Invocation, RunError, RunOptions};

/// The tool to run and its arguments, captured verbatim by clap.
///
/// clap's `external_subcommand` mechanism treats the first token that is not
/// one of run-cli's own options as the tool name and collects everything after
/// it (including `--` and flags overlapping with run-cli's options) as raw
/// `OsString`s without any further option parsing.
#[derive(Debug, Clone, clap::Parser)]
pub enum ExternalCommand {
    /// The tool name followed by every argument passed through to it.
    #[command(external_subcommand)]
    Cmd(Vec<OsString>),
}

/// Arguments for `run-cli run`.
///
/// `--cwd` / `--env` are run-cli's own options and must appear before the
/// tool name; everything after the tool name is passed through verbatim
/// (uv-style, via clap's external subcommand).
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Run the tool in this working directory.
    #[arg(long, value_name = "DIR", help = messages::OPT_CWD_HELP)]
    pub cwd: Option<PathBuf>,
    /// Set an environment variable for the tool, as KEY=VALUE; repeatable.
    #[arg(long, value_name = "KEY=VALUE", help = messages::OPT_ENV_HELP)]
    pub env: Vec<String>,
    /// The tool to run; everything after the tool name is passed through.
    #[command(subcommand)]
    pub command: ExternalCommand,
}

/// Resolve and run the tool, returning its exit code.
pub fn execute(args: Args, context: &Context) -> Result<i32, Error> {
    let ExternalCommand::Cmd(command) = args.command;
    let (tool, passthrough) = command
        .split_first()
        .expect("required external command always carries a tool name");
    let tool = tool.to_str().ok_or(RunError::ToolNameNotUtf8)?;
    // Absolutize before spawning so that a relative tool path (e.g. from a
    // relative toolbox directory) is not resolved against the child's `--cwd`.
    let absolute = resolve_absolute(&context.toolbox, tool)?;
    let invocation = Invocation::from_path(absolute);
    let options = RunOptions {
        cwd: args.cwd,
        env: runner::parse_env_pairs(&args.env)?,
    };
    Ok(runner::execute(&invocation, passthrough, &options)?)
}
