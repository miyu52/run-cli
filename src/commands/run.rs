//! The `run` command: resolve a tool and execute it.

use std::path::PathBuf;

use crate::commands::{Context, locate_tool};
use crate::error::Error;
use crate::messages;
use crate::runner::{self, Invocation, RunOptions};

/// Arguments for `run-cli run`.
///
/// `--cwd` / `--env` are run-cli's own options and are consumed wherever they
/// appear before the first unrecognized argument; use `--` before them to
/// pass them through to the tool instead.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Name or path of the tool to run.
    #[arg(value_name = "TOOL", help = messages::ARG_TOOL_HELP)]
    pub tool: String,
    /// Run the tool in this working directory.
    #[arg(long, value_name = "DIR", help = messages::OPT_CWD_HELP)]
    pub cwd: Option<PathBuf>,
    /// Set an environment variable for the tool, as KEY=VALUE; repeatable.
    #[arg(long, value_name = "KEY=VALUE", help = messages::OPT_ENV_HELP)]
    pub env: Vec<String>,
    /// Arguments passed through to the tool.
    #[arg(value_name = "ARGS", help = messages::ARG_PASSTHROUGH_HELP, trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

/// Resolve and run the tool, returning its exit code.
pub fn run(args: Args, context: &Context) -> Result<i32, Error> {
    let tool_path = locate_tool(&context.toolbox, &args.tool)?;
    let invocation = Invocation::from_path(tool_path);
    let options = RunOptions {
        cwd: args.cwd,
        env: runner::parse_env_pairs(&args.env)?,
    };
    Ok(runner::execute(&invocation, &args.args, &options)?)
}
