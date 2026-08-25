//! The `add` command: register a tool in the config.

use std::path::PathBuf;

use crate::commands::Context;
use crate::config::{Config, ConfigError, ConfigTool, validate_name};
use crate::error::Error;
use crate::messages;

/// Arguments for `run-cli add`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// File to register.
    #[arg(value_name = "PATH", help = messages::ARG_SOURCE_HELP)]
    pub path: PathBuf,
    /// Register the tool under this name instead of the source file name.
    #[arg(long, value_name = "NAME", help = messages::OPT_NAME_HELP)]
    pub name: Option<String>,
    /// Overwrite an existing entry with the same name.
    #[arg(long, help = messages::OPT_FORCE_HELP)]
    pub force: bool,
}

/// Register the source in the config by name and absolute path. Nothing is
/// copied or linked; `remove` later only unregisters the entry.
pub fn execute(args: Args, context: &Context) -> Result<i32, Error> {
    if !args.path.exists() {
        return Err(ConfigError::SourceMissing(args.path.clone()).into());
    }
    let source = std::path::absolute(&args.path)
        .map_err(|e| ConfigError::AbsolutePathError(args.path.clone(), e))?;
    if !source.is_file() {
        return Err(ConfigError::SourceNotFile(source).into());
    }
    let name = match &args.name {
        Some(name) => name.clone(),
        None => source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ConfigError::InvalidToolName(source.display().to_string()))?
            .to_string(),
    };
    validate_name(&name)?;
    let mut config = Config::load(&context.config_path)?;
    if config.lookup(&name).is_some() && !args.force {
        return Err(ConfigError::DuplicateName(name).into());
    }
    config.upsert(ConfigTool {
        name: name.clone(),
        path: source.clone(),
    });
    config.save_atomic(&context.config_path)?;
    println!("{}", messages::added(&name, &source));
    Ok(0)
}
