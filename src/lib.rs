//! run-cli: manage and run tools from a toolbox directory.
//!
//! Tools are looked up by name inside a toolbox directory resolved with the
//! priority CLI argument `--bin-dir` > environment variable `RUN_CLI_BIN` >
//! `./bin`. The `run` subcommand resolves a tool name (with platform
//! extension completion on Windows) and executes it, passing every argument
//! after the tool name through untouched.
//!
//! Tool paths must stay inside the toolbox directory: absolute paths and
//! `..` are judged by where they resolve, and anything pointing outside the
//! toolbox is rejected as not found. Symlink entries are judged by the link
//! itself, so a link inside the toolbox may point anywhere (the `add`
//! workflow links external tools into the toolbox).
//!
//! Exit codes: 0 success (the `run` command propagates the tool's exit code),
//! 1 runtime error, 2 usage error (clap), 127 tool not found.

#![warn(missing_docs)]
// Tests need `std::env::set_var` (unsafe in edition 2024); library code is
// pure safe Rust.
#![cfg_attr(not(test), forbid(unsafe_code))]

pub mod cli;
pub mod commands;
pub mod error;
pub mod messages;
pub mod runner;
pub mod suggest;
pub mod toolbox;

/// Generic runtime error (IO, validation, ...).
pub const EXIT_ERROR: i32 = 1;
/// Command line usage error (clap exits with this code itself).
pub const EXIT_USAGE: i32 = 2;
/// The requested tool could not be found.
pub const EXIT_TOOL_NOT_FOUND: i32 = 127;
