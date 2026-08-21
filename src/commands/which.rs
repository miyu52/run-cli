//! The `which` command: resolve a tool name without running it.

use crate::EXIT_ERROR;
use crate::commands::{Context, locate_tool};
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
pub fn run(args: Args, context: &Context) -> Result<i32, Error> {
    let tool_path = locate_tool(&context.toolbox, &args.tool)?;
    let absolute = std::path::absolute(&tool_path).map_err(|e| {
        Error::formatted(
            EXIT_ERROR,
            format!(
                "failed to resolve absolute path for {}: {e}",
                tool_path.display()
            ),
        )
    })?;
    println!("{}", absolute.display());
    Ok(0)
}
