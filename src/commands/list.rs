//! The `list` command: show the contents of the toolbox directory.

use crate::commands::Context;
use crate::error::Error;
use crate::messages;
use crate::toolbox::ToolKind;

/// Arguments for `run-cli list`.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// Print the listing as JSON.
    #[arg(long, help = messages::OPT_JSON_HELP)]
    pub json: bool,
}

/// List the contents of the toolbox directory (files and directories,
/// sorted). With `--json`, prints a JSON array of entries instead.
pub fn execute(args: Args, context: &Context) -> Result<i32, Error> {
    let tools = context.toolbox.list()?;
    if args.json {
        let json: Vec<messages::ToolJson> = tools.iter().map(Into::into).collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else if tools.is_empty() {
        println!("{}", messages::no_tools_found(context.toolbox.dir()));
    } else {
        println!(
            "{}",
            messages::list_header(context.toolbox.dir(), tools.len())
        );
        for tool in &tools {
            match tool.kind {
                ToolKind::Directory => println!("  - {}/", tool.name),
                ToolKind::File => println!("  - {}", tool.name),
            }
        }
    }
    Ok(0)
}
