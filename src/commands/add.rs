//! The `add` command: add a file or directory to the toolbox.

use std::path::PathBuf;

use crate::commands::Context;
use crate::error::Error;
use crate::messages;
use crate::toolbox::AddOutcome;

/// Arguments for `run-cli add`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// File or directory to add.
    #[arg(value_name = "PATH", help = messages::ARG_SOURCE_HELP)]
    pub path: PathBuf,
    /// Add the tool under this name instead of the source file name.
    #[arg(long, value_name = "NAME", help = messages::OPT_NAME_HELP)]
    pub name: Option<String>,
}

/// Add the source to the toolbox, creating a symlink when possible and
/// falling back to a copy.
pub fn run(args: Args, context: &Context) -> Result<i32, Error> {
    let (outcome, dest) = context.toolbox.add(&args.path, args.name.as_deref())?;
    match outcome {
        AddOutcome::Linked => println!("{}", messages::added_linked(&args.path, &dest)),
        AddOutcome::Copied => println!("{}", messages::added_copied(&args.path, &dest)),
    }
    Ok(0)
}
