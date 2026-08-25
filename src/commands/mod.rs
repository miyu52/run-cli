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

use crate::config::{Config, ConfigError};
use crate::error::Error;
use crate::messages;
use crate::suggest;
use crate::toolbox::{Toolbox, ToolboxError};

/// Shared state passed to every command: the toolbox directory and the
/// resolved config file path.
#[derive(Debug, Clone)]
pub struct Context {
    /// The toolbox directory with its path-boundary policy.
    pub toolbox: Toolbox,
    /// The config file holding registered tools.
    pub config_path: PathBuf,
}

impl Context {
    /// Build a [`Context`] from the CLI-level options.
    pub fn new(bin_dir: Option<&Path>, config: Option<&Path>) -> Self {
        Context {
            toolbox: Toolbox::resolve(bin_dir),
            config_path: crate::config::resolve_config_path(config),
        }
    }
}

/// Map a toolbox error from a tool lookup to the unified error, turning a
/// plain "not found" into the rich exit-127 message.
pub fn map_tool_error(
    toolbox: &Toolbox,
    config_path: &Path,
    name: &str,
    err: ToolboxError,
) -> Error {
    match err {
        ToolboxError::ToolNotFound(..) => map_tool_not_found(toolbox, config_path, name),
        err => err.into(),
    }
}

/// Resolve a tool name like [`Toolbox::locate`], mapping "not found" to a
/// rich error message with a spelling suggestion and the list of available
/// tools (config entries and toolbox entries combined).
pub fn locate_tool(toolbox: &Toolbox, config_path: &Path, name: &str) -> Result<PathBuf, Error> {
    toolbox
        .locate(name)
        .map_err(|err| map_tool_error(toolbox, config_path, name, err))
}

/// Build the error for an unresolvable tool name: a missing toolbox
/// directory is a runtime error (exit 1) for a legacy bin-only setup (no
/// config file), a path that exists but is not a directory is also a runtime
/// error, while a real "not found" produces the rich exit-127 message. With a
/// config file present, the config is the primary source, so an absent or
/// non-directory toolbox counts as "no toolbox entries" and the name is
/// simply not found.
pub fn map_tool_not_found(toolbox: &Toolbox, config_path: &Path, name: &str) -> Error {
    let dir = toolbox.dir();
    if !dir.exists() {
        if config_path.is_file() {
            return tool_not_found_error(toolbox, config_path, name);
        }
        return Error::Toolbox(ToolboxError::MissingDirectory(dir.to_path_buf()));
    }
    if !dir.is_dir() {
        if config_path.is_file() {
            return tool_not_found_error(toolbox, config_path, name);
        }
        return Error::Toolbox(ToolboxError::NotADirectory(dir.to_path_buf()));
    }
    tool_not_found_error(toolbox, config_path, name)
}

/// Build the exit-127 error message for an unresolvable tool name, with
/// suggestions drawn from the union of config names and toolbox names.
pub fn tool_not_found_error(toolbox: &Toolbox, config_path: &Path, name: &str) -> Error {
    let (config_names, config_read) = match Config::load(config_path) {
        Ok(config) => (config.names(), true),
        Err(_) => (Vec::new(), false),
    };
    let bin_names = toolbox.list_names();
    let mut available = config_names;
    let bin_listed = match bin_names {
        Ok(names) => {
            available.extend(names);
            true
        }
        Err(_) => false,
    };
    available.sort();
    available.dedup();
    let message = if !config_read && !bin_listed {
        messages::tool_not_found_list_failed(name, toolbox.dir())
    } else {
        messages::tool_not_found(
            name,
            toolbox.dir(),
            &available,
            suggest::suggest(name, &available).as_deref(),
        )
    };
    Error::RichToolNotFound { message }
}

/// Build the exit-127 error message for `remove` when the name is not
/// registered in the config, with suggestions from registered names.
pub fn tool_not_registered_error(context: &Context, name: &str) -> Error {
    let message = match Config::load(&context.config_path) {
        Ok(config) => {
            let available = config.names();
            messages::tool_not_registered(
                name,
                &context.config_path,
                &available,
                suggest::suggest(name, &available).as_deref(),
            )
        }
        Err(_) => messages::tool_not_registered_no_list(name, &context.config_path),
    };
    Error::RichToolNotFound { message }
}

/// Resolve a tool name to an absolute path, like [`locate_tool`] followed by
/// [`std::path::absolute`]; used by `run` and `which` for the toolbox
/// fallback.
pub fn resolve_absolute(
    toolbox: &Toolbox,
    config_path: &Path,
    name: &str,
) -> Result<PathBuf, Error> {
    let tool_path = locate_tool(toolbox, config_path, name)?;
    std::path::absolute(&tool_path)
        .map_err(|e| ToolboxError::AbsolutePathError(tool_path, e).into())
}

/// Resolve a tool name with the config first: a registered name wins
/// wholesale over the toolbox (which is only consulted when the config does
/// not resolve the name at all). A registered name whose stored path is
/// missing or not a file is a runtime error (exit 1) and never falls back to
/// the toolbox, so a broken registration is surfaced loudly.
pub fn resolve_tool(context: &Context, name: &str) -> Result<PathBuf, Error> {
    let config = Config::load(&context.config_path)?;
    if let Some(tool) = config.lookup(name) {
        let path = std::path::absolute(&tool.path)
            .map_err(|e| ConfigError::AbsolutePathError(tool.path.clone(), e))?;
        if !path.exists() {
            return Err(ConfigError::RegisteredPathMissing {
                name: tool.name.clone(),
                path: tool.path.clone(),
            }
            .into());
        }
        if !path.is_file() {
            return Err(ConfigError::RegisteredPathNotFile {
                name: tool.name.clone(),
                path: tool.path.clone(),
            }
            .into());
        }
        return Ok(path);
    }
    resolve_absolute(&context.toolbox, &context.config_path, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigTool;
    use tempfile::TempDir;

    fn missing_toolbox() -> Toolbox {
        Toolbox::resolve(Some(Path::new("definitely-missing-dir")))
    }

    fn config_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    fn context_with(bin_dir: Option<&Path>, config_path: &Path) -> Context {
        Context {
            toolbox: Toolbox::resolve(bin_dir),
            config_path: config_path.to_path_buf(),
        }
    }

    #[test]
    fn map_tool_not_found_missing_dir_is_missing_directory() {
        let dir = config_dir();
        assert!(matches!(
            map_tool_not_found(&missing_toolbox(), dir.path().join("c.toml").as_path(), "x"),
            Error::Toolbox(ToolboxError::MissingDirectory(_))
        ));
    }

    #[test]
    fn map_tool_not_found_file_is_not_a_directory() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("not-a-dir");
        std::fs::write(&file, "").unwrap();
        let toolbox = Toolbox::resolve(Some(&file));
        let config = config_dir();
        assert!(matches!(
            map_tool_not_found(&toolbox, config.path().join("c.toml").as_path(), "x"),
            Error::Toolbox(ToolboxError::NotADirectory(_))
        ));
    }

    #[test]
    fn map_tool_not_found_existing_dir_returns_rich_error() {
        let dir = TempDir::new().unwrap();
        let toolbox = Toolbox::resolve(Some(dir.path()));
        let config = config_dir();
        assert!(matches!(
            map_tool_not_found(&toolbox, config.path().join("c.toml").as_path(), "x"),
            Error::RichToolNotFound { .. }
        ));
    }

    #[test]
    fn tool_not_found_error_merges_config_and_bin_names() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("other.exe"), "").unwrap();
        let toolbox = Toolbox::resolve(Some(dir.path()));
        let config_path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.upsert(ConfigTool {
            name: "echo".to_string(),
            path: PathBuf::from("/usr/bin/echo"),
        });
        config.save_atomic(&config_path).unwrap();

        let Error::RichToolNotFound { message } =
            tool_not_found_error(&toolbox, &config_path, "echoe")
        else {
            panic!("expected rich error");
        };
        assert!(message.contains("did you mean"), "message: {message}");
        assert!(message.contains("echo"), "message: {message}");
        assert!(message.contains("other.exe"), "message: {message}");
    }

    #[test]
    fn resolve_tool_uses_config_first() {
        let dir = TempDir::new().unwrap();
        let bin_tool = dir.path().join("tool.exe");
        std::fs::write(&bin_tool, "").unwrap();
        let config_path = dir.path().join("config.toml");
        let registered = dir.path().join("registered.exe");
        std::fs::write(&registered, "").unwrap();
        let mut config = Config::default();
        config.upsert(ConfigTool {
            name: "tool.exe".to_string(),
            path: registered.clone(),
        });
        config.save_atomic(&config_path).unwrap();

        let context = context_with(Some(dir.path()), &config_path);
        assert_eq!(resolve_tool(&context, "tool.exe").unwrap(), registered);
        // Bare-name completion is a Windows feature; elsewhere the bare query
        // resolves in neither the config nor the toolbox.
        #[cfg(windows)]
        assert_eq!(resolve_tool(&context, "tool").unwrap(), registered);
        #[cfg(not(windows))]
        assert!(matches!(
            resolve_tool(&context, "tool"),
            Err(Error::RichToolNotFound { .. })
        ));
    }

    #[test]
    fn resolve_tool_falls_back_to_toolbox() {
        let dir = TempDir::new().unwrap();
        let bin_tool = dir.path().join("tool.exe");
        std::fs::write(&bin_tool, "").unwrap();
        let config_path = dir.path().join("config.toml");

        let context = context_with(Some(dir.path()), &config_path);
        assert_eq!(resolve_tool(&context, "tool.exe").unwrap(), bin_tool);
    }

    #[test]
    fn resolve_tool_registered_missing_path_is_error() {
        let dir = TempDir::new().unwrap();
        let bin_tool = dir.path().join("tool.exe");
        std::fs::write(&bin_tool, "").unwrap();
        let config_path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.upsert(ConfigTool {
            name: "tool.exe".to_string(),
            path: dir.path().join("gone.exe"),
        });
        config.save_atomic(&config_path).unwrap();

        let context = context_with(Some(dir.path()), &config_path);
        // The registered entry resolves for the exact name on every platform
        // and (via extension completion) for the bare name on Windows.
        #[cfg(windows)]
        let query = "tool";
        #[cfg(not(windows))]
        let query = "tool.exe";
        assert!(matches!(
            resolve_tool(&context, query),
            Err(Error::Config(ConfigError::RegisteredPathMissing { .. }))
        ));
    }

    #[test]
    fn resolve_tool_registered_path_not_file_is_error() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.upsert(ConfigTool {
            name: "sub".to_string(),
            path: dir.path().to_path_buf(),
        });
        config.save_atomic(&config_path).unwrap();

        let context = context_with(None, &config_path);
        assert!(matches!(
            resolve_tool(&context, "sub"),
            Err(Error::Config(ConfigError::RegisteredPathNotFile { .. }))
        ));
    }

    #[test]
    fn tool_not_registered_error_suggests_config_names() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.upsert(ConfigTool {
            name: "echo".to_string(),
            path: PathBuf::from("/usr/bin/echo"),
        });
        config.save_atomic(&config_path).unwrap();

        let context = context_with(None, &config_path);
        let Error::RichToolNotFound { message } = tool_not_registered_error(&context, "echoe")
        else {
            panic!("expected rich error");
        };
        assert!(message.contains("did you mean"), "message: {message}");
        assert!(message.contains("echo"), "message: {message}");
    }
}
