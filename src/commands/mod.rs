//! Command implementation modules.
//!
//! Each subcommand lives in its own module (`Args` argument definition +
//! `run` entry point), decoupled from the clap dispatch in [`crate::cli`]:
//! to add a command, add a module here and register the variant and dispatch
//! in the `Command` enum of `cli.rs`. Shared context and helpers live here.

pub mod add;
pub mod completions;
pub mod list;
pub mod remove;
pub mod run;
pub mod which;

use std::path::{Path, PathBuf};

use crate::EXIT_TOOL_NOT_FOUND;
use crate::error::Error;
use crate::messages;
use crate::suggest;
use crate::toolbox::{Toolbox, ToolboxError};

/// Shared state passed to every command: the resolved toolbox.
#[derive(Debug, Clone)]
pub struct Context {
    /// The toolbox directory with its path-boundary policy.
    pub toolbox: Toolbox,
}

impl Context {
    /// Build a [`Context`] from the CLI-level options.
    pub fn new(bin_dir: Option<&Path>, allow_escape: bool) -> Self {
        Context {
            toolbox: Toolbox::resolve(bin_dir, allow_escape),
        }
    }
}

/// Resolve a tool name like [`Toolbox::locate`], mapping "not found" to a
/// rich error message with a spelling suggestion and the list of available
/// tools.
pub fn locate_tool(toolbox: &Toolbox, name: &str) -> Result<PathBuf, Error> {
    match toolbox.locate(name) {
        Ok(path) => Ok(path),
        Err(ToolboxError::ToolNotFound(..)) => Err(tool_not_found_error(toolbox, name)),
        Err(err) => Err(err.into()),
    }
}

/// Build the exit-127 error message for an unresolvable tool name.
pub fn tool_not_found_error(toolbox: &Toolbox, name: &str) -> Error {
    let available = toolbox.list_names().unwrap_or_default();
    Error::formatted(
        EXIT_TOOL_NOT_FOUND,
        messages::tool_not_found(
            name,
            toolbox.dir(),
            &available,
            suggest::suggest(name, &available).as_deref(),
        ),
    )
}
