//! The `remove` command: remove a tool from the toolbox.

use crate::commands::{Context, tool_not_found_error};
use crate::error::Error;
use crate::messages;
use crate::toolbox::ToolboxError;

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
pub fn run(args: Args, context: &Context) -> Result<i32, Error> {
    let path = match context.toolbox.remove(&args.tool, args.recursive) {
        Ok(path) => path,
        Err(ToolboxError::ToolNotFound(..)) => {
            return Err(tool_not_found_error(&context.toolbox, &args.tool));
        }
        Err(err) => return Err(err.into()),
    };
    println!("{}", messages::removed(&path));
    Ok(0)
}
