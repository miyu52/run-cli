//! The `which` command: resolve a tool name without running it.

use crate::commands::{Context, resolve_absolute};
use crate::error::Error;
use crate::messages;

/// Arguments for `run-cli which`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Name or path of the tool.
    #[arg(value_name = "TOOL", help = messages::ARG_TOOL_HELP)]
    pub tool: String,
}

/// Resolve the tool and print its absolute path.
pub fn execute(args: Args, context: &Context) -> Result<i32, Error> {
    let absolute = resolve_absolute(&context.toolbox, &args.tool)?;
    println!("{}", absolute.display());
    Ok(0)
}
