//! All user-facing text: clap help, error messages and output formatting.
//!
//! Help text lives here as constants (referenced from derive attributes);
//! runtime messages and output formatting are plain functions. No other
//! module should contain user-visible string literals.

#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::toolbox::{Tool, ToolKind};

/// Serialized form of a toolbox entry for `list --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolJson {
    /// Entry name.
    pub name: String,
    /// Full path of the entry.
    pub path: PathBuf,
    /// Whether the entry is a file or a directory.
    #[serde(rename = "kind")]
    kind: &'static str,
    /// Size in bytes for files; omitted for directories.
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
}

impl From<&Tool> for ToolJson {
    fn from(tool: &Tool) -> Self {
        ToolJson {
            name: tool.name.clone(),
            path: tool.path.clone(),
            kind: match tool.kind {
                ToolKind::File => "file",
                ToolKind::Directory => "directory",
            },
            size: tool.size,
        }
    }
}

pub const APP_ABOUT: &str = "Manage and run tools from a toolbox directory";

pub const OPT_BIN_DIR_HELP: &str =
    "Custom toolbox directory; overrides RUN_CLI_BIN, defaults to ./bin";
pub const OPT_ALLOW_ESCAPE_HELP: &str = "Allow tool paths to resolve outside the toolbox directory";

pub const CMD_RUN_ABOUT: &str =
    "Run a tool from the toolbox; arguments after the tool name are passed through";
pub const CMD_LIST_ABOUT: &str = "List the contents of the toolbox directory";
pub const CMD_WHICH_ABOUT: &str =
    "Print the absolute path a tool name resolves to, without running it";
pub const CMD_ADD_ABOUT: &str =
    "Add a file or directory to the toolbox (symlink, or copy as fallback)";
pub const CMD_REMOVE_ABOUT: &str = "Remove a tool from the toolbox";
pub const CMD_COMPLETIONS_ABOUT: &str = "Generate shell completion scripts";

pub const OPT_CWD_HELP: &str = "Run the tool in this working directory";
pub const OPT_ENV_HELP: &str = "Set an environment variable for the tool, as KEY=VALUE; repeatable";
pub const OPT_JSON_HELP: &str = "Print the listing as JSON";
pub const OPT_NAME_HELP: &str = "Add the tool under this name instead of the source file name";
pub const OPT_RECURSIVE_HELP: &str = "Remove directories and their contents recursively";
pub const OPT_SHELL_HELP: &str = "The shell to generate completions for";

pub const ARG_TOOL_HELP: &str = "Name or path of the tool";
pub const ARG_SOURCE_HELP: &str = "File or directory to add";

/// Prefix printed before every error message.
pub fn error_prefix() -> &'static str {
    "run-cli: error: "
}

/// Header line of the human-readable `list` output.
pub fn list_header(dir: &Path, count: usize) -> String {
    format!("tools in {} ({count}):", dir.display())
}

/// Message printed when the toolbox directory contains nothing.
pub fn no_tools_found(dir: &Path) -> String {
    format!("no tools found in {}", dir.display())
}

/// Error message for an unresolvable tool name, with an optional spelling
/// suggestion and the list of available tools.
pub fn tool_not_found(
    tool: &str,
    dir: &Path,
    available: &[String],
    suggestion: Option<&str>,
) -> String {
    let mut message = format!("tool '{tool}' not found in {}", dir.display());
    if let Some(suggestion) = suggestion {
        message.push_str(&format!("\ndid you mean '{suggestion}'?"));
    }
    if !available.is_empty() {
        message.push_str("\navailable tools:");
        for name in available {
            message.push_str(&format!("\n  - {name}"));
        }
    }
    message
}

/// Confirmation printed after `add` created a symlink.
pub fn added_linked(source: &Path, dest: &Path) -> String {
    format!("linked {} -> {}", source.display(), dest.display())
}

/// Confirmation printed after `add` fell back to copying.
pub fn added_copied(source: &Path, dest: &Path) -> String {
    format!("copied {} to {}", source.display(), dest.display())
}

/// Confirmation printed after `remove` deleted a tool.
pub fn removed(path: &Path) -> String {
    format!("removed {}", path.display())
}
