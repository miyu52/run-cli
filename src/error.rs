//! Unified error type and exit code mapping.
//!
//! Every error surfaced by the CLI funnels through [`Error`]; its
//! [`Error::code`] decides the process exit code.

use thiserror::Error;

use crate::config::ConfigError;
use crate::runner::RunError;
use crate::toolbox::ToolboxError;
use crate::{EXIT_ERROR, EXIT_TOOL_NOT_FOUND};

/// Unified error type for all commands.
#[derive(Debug, Error)]
pub enum Error {
    /// A toolbox operation failed.
    #[error("{0}")]
    Toolbox(#[from] ToolboxError),
    /// A config operation failed.
    #[error("{0}")]
    Config(#[from] ConfigError),
    /// Spawning or waiting for a tool failed.
    #[error("{0}")]
    Run(#[from] RunError),
    /// Serializing structured output failed.
    #[error("{0}")]
    Serialize(#[from] serde_json::Error),
    /// An unresolvable tool name, with a rich message (spelling suggestion and
    /// available tools). Distinct from [`ToolboxError::ToolNotFound`], which is
    /// the plain domain error; this variant carries the pre-formatted message.
    /// Also used for `remove` when the name is not registered in the config.
    #[error("{message}")]
    RichToolNotFound {
        /// Message printed to stderr.
        message: String,
    },
}

impl Error {
    /// The process exit code this error should produce.
    pub fn code(&self) -> i32 {
        match self {
            Error::Toolbox(ToolboxError::ToolNotFound(..)) => EXIT_TOOL_NOT_FOUND,
            Error::RichToolNotFound { .. } => EXIT_TOOL_NOT_FOUND,
            _ => EXIT_ERROR,
        }
    }
}
