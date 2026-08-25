//! The `list` command: show config registrations and toolbox contents.

use std::collections::HashSet;
use std::fs;

use crate::commands::Context;
use crate::config::Config;
use crate::error::Error;
use crate::messages;
use crate::toolbox::{Tool, ToolKind, ToolboxError};

/// Arguments for `run-cli list`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Print the listing as JSON.
    #[arg(long, help = messages::OPT_JSON_HELP)]
    pub json: bool,
}

/// Where a listed entry comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// Registered in the config file.
    Config,
    /// Found in the toolbox directory.
    Bin,
}

impl Origin {
    fn as_str(self) -> &'static str {
        match self {
            Origin::Config => "config",
            Origin::Bin => "bin",
        }
    }
}

/// List config registrations and toolbox contents merged and sorted by name.
/// A name registered in the config shadows the same toolbox name; config
/// entries whose path no longer exists are skipped (like dangling symlinks in
/// the toolbox), and a missing toolbox directory counts as empty.
pub fn execute(args: Args, context: &Context) -> Result<i32, Error> {
    let config = Config::load(&context.config_path)?;
    let mut entries: Vec<(Tool, Origin)> = Vec::new();
    for tool in &config.tools {
        let Some(meta) = fs::metadata(&tool.path).ok() else {
            continue;
        };
        let (kind, size) = if meta.is_dir() {
            (ToolKind::Directory, None)
        } else if meta.is_file() {
            (ToolKind::File, Some(meta.len()))
        } else {
            continue;
        };
        entries.push((
            Tool {
                name: tool.name.clone(),
                path: tool.path.clone(),
                kind,
                size,
            },
            Origin::Config,
        ));
    }
    let bin_tools = match context.toolbox.list() {
        Ok(tools) => tools,
        // A missing toolbox directory is not an error for listing: the
        // config may be the only source of tools.
        Err(ToolboxError::MissingDirectory(_)) | Err(ToolboxError::NotADirectory(_)) => Vec::new(),
        Err(err) => return Err(err.into()),
    };
    let mut seen: HashSet<String> = entries.iter().map(|(tool, _)| tool.name.clone()).collect();
    for tool in bin_tools {
        if !seen.insert(tool.name.clone()) {
            continue;
        }
        entries.push((tool, Origin::Bin));
    }
    entries.sort_by(|a, b| a.0.name.cmp(&b.0.name));

    if args.json {
        let json: Vec<messages::ToolJson> = entries
            .iter()
            .map(|(tool, origin)| messages::ToolJson::new(tool, origin.as_str()))
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if entries.is_empty() {
        println!("{}", messages::no_tools_found(context.toolbox.dir()));
    } else {
        println!(
            "{}",
            messages::list_header(context.toolbox.dir(), entries.len())
        );
        for (tool, origin) in &entries {
            let slash = if tool.kind == ToolKind::Directory {
                "/"
            } else {
                ""
            };
            println!(
                "  - {}{}{}",
                tool.name,
                slash,
                messages::origin_suffix(origin.as_str())
            );
        }
    }
    Ok(0)
}
