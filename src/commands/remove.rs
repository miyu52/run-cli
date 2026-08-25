//! The `remove` command: unregister a tool from the config.

use crate::commands::{Context, tool_not_registered_error};
use crate::config::Config;
use crate::error::Error;
use crate::messages;

/// Arguments for `run-cli remove`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Name of the registered tool.
    #[arg(value_name = "TOOL", help = messages::ARG_TOOL_HELP)]
    pub tool: String,
}

/// Unregister the tool from the config. Never deletes any file: the source
/// of an `add` and manually placed toolbox entries are left untouched.
pub fn execute(args: Args, context: &Context) -> Result<i32, Error> {
    let mut config = Config::load(&context.config_path)?;
    match config.remove(&args.tool) {
        Some(tool) => {
            config.save_atomic(&context.config_path)?;
            println!("{}", messages::removed(&tool.name));
            Ok(0)
        }
        None => Err(tool_not_registered_error(context, &args.tool)),
    }
}
