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
    pub fn new(bin_dir: Option<&Path>) -> Self {
        Context {
            toolbox: Toolbox::resolve(bin_dir),
        }
    }
}

/// Map a toolbox error from a tool lookup to the unified error, turning a
/// plain "not found" into the rich exit-127 message.
pub fn map_tool_error(toolbox: &Toolbox, name: &str, err: ToolboxError) -> Error {
    match err {
        ToolboxError::ToolNotFound(..) => map_tool_not_found(toolbox, name),
        err => err.into(),
    }
}

/// Resolve a tool name like [`Toolbox::locate`], mapping "not found" to a
/// rich error message with a spelling suggestion and the list of available
/// tools.
pub fn locate_tool(toolbox: &Toolbox, name: &str) -> Result<PathBuf, Error> {
    toolbox
        .locate(name)
        .map_err(|err| map_tool_error(toolbox, name, err))
}

/// Build the error for an unresolvable tool name: a missing toolbox
/// directory is a runtime error (exit 1), a path that exists but is not a
/// directory is also a runtime error, while a real "not found" produces
/// the rich exit-127 message.
pub fn map_tool_not_found(toolbox: &Toolbox, name: &str) -> Error {
    let dir = toolbox.dir();
    if !dir.exists() {
        return Error::Toolbox(ToolboxError::MissingDirectory(dir.to_path_buf()));
    }
    if !dir.is_dir() {
        return Error::Toolbox(ToolboxError::NotADirectory(dir.to_path_buf()));
    }
    tool_not_found_error(toolbox, name)
}

/// Build the exit-127 error message for an unresolvable tool name.
pub fn tool_not_found_error(toolbox: &Toolbox, name: &str) -> Error {
    let message = match toolbox.list_names() {
        Ok(available) => messages::tool_not_found(
            name,
            toolbox.dir(),
            &available,
            suggest::suggest(name, &available).as_deref(),
        ),
        Err(_) => messages::tool_not_found_list_failed(name, toolbox.dir()),
    };
    Error::RichToolNotFound { message }
}

/// Resolve a tool name to an absolute path, like [`locate_tool`] followed by
/// [`std::path::absolute`]; used by `run` and `which`.
pub fn resolve_absolute(toolbox: &Toolbox, name: &str) -> Result<PathBuf, Error> {
    let tool_path = locate_tool(toolbox, name)?;
    std::path::absolute(&tool_path)
        .map_err(|e| ToolboxError::AbsolutePathError(tool_path, e).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn missing_toolbox() -> Toolbox {
        Toolbox::resolve(Some(Path::new("definitely-missing-dir")))
    }

    #[test]
    fn map_tool_not_found_missing_dir_is_missing_directory() {
        assert!(matches!(
            map_tool_not_found(&missing_toolbox(), "x"),
            Error::Toolbox(ToolboxError::MissingDirectory(_))
        ));
    }

    #[test]
    fn map_tool_not_found_file_is_not_a_directory() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, "").unwrap();
        let toolbox = Toolbox::resolve(Some(&file));
        assert!(matches!(
            map_tool_not_found(&toolbox, "x"),
            Error::Toolbox(ToolboxError::NotADirectory(_))
        ));
    }

    #[test]
    fn map_tool_not_found_existing_dir_returns_rich_error() {
        let dir = TempDir::new().unwrap();
        let toolbox = Toolbox::resolve(Some(dir.path()));
        assert!(matches!(
            map_tool_not_found(&toolbox, "x"),
            Error::RichToolNotFound { .. }
        ));
    }
}
