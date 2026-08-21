//! Unified error type and exit code mapping.
//!
//! Every error surfaced by the CLI funnels through [`Error`]; its
//! [`Error::code`] decides the process exit code.

use thiserror::Error;

use crate::runner::RunError;
use crate::toolbox::ToolboxError;
use crate::{EXIT_ERROR, EXIT_TOOL_NOT_FOUND};

/// Unified error type for all commands.
#[derive(Debug, Error)]
pub enum Error {
    /// A toolbox operation failed.
    #[error("{0}")]
    Toolbox(#[from] ToolboxError),
    /// Spawning or waiting for a tool failed.
    #[error("{0}")]
    Run(#[from] RunError),
    /// Serializing structured output failed.
    #[error("{0}")]
    Serialize(#[from] serde_json::Error),
    /// A pre-formatted message carrying an explicit exit code.
    #[error("{message}")]
    Formatted {
        /// Exit code to report.
        code: i32,
        /// Message printed to stderr.
        message: String,
    },
}

impl Error {
    /// Build a [`Error::Formatted`] with an explicit exit code.
    pub fn formatted(code: i32, message: impl Into<String>) -> Self {
        Error::Formatted {
            code,
            message: message.into(),
        }
    }

    /// The process exit code this error should produce.
    pub fn code(&self) -> i32 {
        match self {
            Error::Toolbox(ToolboxError::ToolNotFound(..)) => EXIT_TOOL_NOT_FOUND,
            Error::Formatted { code, .. } => *code,
            _ => EXIT_ERROR,
        }
    }
}
