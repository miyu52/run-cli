//! The `list` command: show config registrations and toolbox contents.

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

fn tool_from_path(name: &str, path: &std::path::Path) -> Option<Tool> {
    let meta = fs::metadata(path).ok()?;
    let (kind, size) = if meta.is_dir() {
        (ToolKind::Directory, None)
    } else if meta.is_file() {
        (ToolKind::File, Some(meta.len()))
    } else {
        return None;
    };
    Some(Tool {
        name: name.to_string(),
        path: path.to_path_buf(),
        kind,
        size,
    })
}

/// List config registrations and toolbox contents. The human-readable output
/// shows one group per source (config first, then the toolbox), each with its
/// own header line and entries; a group is only shown when it has entries.
/// Every entry is shown; a toolbox entry whose name is also registered in the
/// config is marked as shadowed. Config entries whose path no longer exists
/// are skipped (like dangling symlinks in the toolbox), and a missing toolbox
/// directory counts as empty. `--json` merges and sorts all entries with an
/// `origin` field and a `shadowed` flag instead.
pub fn execute(args: Args, context: &Context) -> Result<i32, Error> {
    let config = Config::load(&context.config_path)?;
    let mut config_tools: Vec<Tool> = config
        .tools
        .iter()
        .filter_map(|tool| tool_from_path(&tool.name, &tool.path))
        .collect();
    config_tools.sort_by(|a, b| a.name.cmp(&b.name));

    let bin_tools = match context.toolbox.list() {
        Ok(tools) => tools,
        // A missing toolbox directory is not an error for listing: the
        // config may be the only source of tools.
        Err(ToolboxError::MissingDirectory(_)) | Err(ToolboxError::NotADirectory(_)) => Vec::new(),
        Err(err) => return Err(err.into()),
    };

    if args.json {
        let mut entries: Vec<(Tool, Origin)> = config_tools
            .into_iter()
            .map(|tool| (tool, Origin::Config))
            .collect();
        entries.extend(bin_tools.into_iter().map(|tool| (tool, Origin::Bin)));
        entries.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        let json: Vec<messages::ToolJson> = entries
            .iter()
            .map(|(tool, origin)| {
                messages::ToolJson::new(
                    tool,
                    origin.as_str(),
                    *origin == Origin::Bin && config.lookup(&tool.name).is_some(),
                )
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(0);
    }

    let mut any = false;
    if !config_tools.is_empty() {
        println!(
            "{}",
            messages::list_header(&context.config_path, config_tools.len())
        );
        for tool in &config_tools {
            print_entry(tool, false);
        }
        any = true;
    }
    if !bin_tools.is_empty() {
        println!(
            "{}",
            messages::list_header(context.toolbox.dir(), bin_tools.len())
        );
        for tool in &bin_tools {
            print_entry(tool, config.lookup(&tool.name).is_some());
        }
        any = true;
    }
    if !any {
        println!(
            "{}",
            messages::no_tools_found(context.toolbox.dir(), &context.config_path)
        );
    }
    Ok(0)
}

fn print_entry(tool: &Tool, shadowed: bool) {
    let slash = if tool.kind == ToolKind::Directory {
        "/"
    } else {
        ""
    };
    let suffix = if shadowed {
        messages::shadowed_suffix()
    } else {
        ""
    };
    println!("  - {}{}{}", tool.name, slash, suffix);
}
