//! The `remove` command: remove a tool from the toolbox.

use crate::commands::{Context, map_tool_error};
use crate::error::Error;
use crate::messages;

/// Arguments for `run-cli remove`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Name or path of the tool.
    #[arg(value_name = "TOOL", help = messages::ARG_TOOL_HELP)]
    pub tool: String,
    /// Remove directories and their contents recursively.
    #[arg(short = 'r', long = "recursive", help = messages::OPT_RECURSIVE_HELP)]
    pub recursive: bool,
}

/// Remove the tool from the toolbox.
pub fn execute(args: Args, context: &Context) -> Result<i32, Error> {
    let path = context
        .toolbox
        .remove(&args.tool, args.recursive)
        .map_err(|err| map_tool_error(&context.toolbox, &args.tool, err))?;
    println!("{}", messages::removed(&path));
    Ok(0)
}
