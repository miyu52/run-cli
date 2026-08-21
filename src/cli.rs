//! Command line interface: clap argument definitions and command dispatch.
//!
//! This module only parses arguments and dispatches; the `Args` structs and
//! execution logic of each command live in [`crate::commands`]; all help text
//! comes from [`crate::messages`].

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::commands;
use crate::commands::Context;
use crate::error::Error;
use crate::messages;

/// Command line arguments for run-cli.
#[derive(Debug, Parser)]
#[command(
    name = "run-cli",
    version,
    about = messages::APP_ABOUT,
    long_about = None
)]
pub struct Cli {
    /// Custom toolbox directory; overrides RUN_CLI_BIN, defaults to ./bin.
    #[arg(short = 'b', long = "bin-dir", value_name = "PATH", help = messages::OPT_BIN_DIR_HELP)]
    pub bin_dir: Option<PathBuf>,
    /// Allow tool paths to resolve outside the toolbox directory.
    #[arg(long, help = messages::OPT_ALLOW_ESCAPE_HELP)]
    pub allow_escape: bool,
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The run-cli subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a tool from the toolbox; arguments after the tool name are passed
    /// through. `--help` / `-h` after the tool name are passed through too.
    #[command(about = messages::CMD_RUN_ABOUT, disable_help_flag = true, help_expected = true)]
    Run(commands::run::Args),
    /// List the contents of the toolbox directory.
    #[command(about = messages::CMD_LIST_ABOUT)]
    List(commands::list::Args),
    /// Print the absolute path a tool name resolves to, without running it.
    #[command(about = messages::CMD_WHICH_ABOUT)]
    Which(commands::which::Args),
    /// Add a file or directory to the toolbox.
    #[command(about = messages::CMD_ADD_ABOUT)]
    Add(commands::add::Args),
    /// Remove a tool from the toolbox.
    #[command(about = messages::CMD_REMOVE_ABOUT)]
    Remove(commands::remove::Args),
    /// Generate shell completion scripts.
    #[command(about = messages::CMD_COMPLETIONS_ABOUT)]
    Completions(commands::completions::Args),
}

/// Parse command line arguments and run the selected command.
///
/// Returns the process exit code to report (0 for success; `run` propagates
/// the tool's own exit code).
pub fn run() -> Result<i32, Error> {
    let cli = Cli::parse();
    let context = Context::new(cli.bin_dir.as_deref(), cli.allow_escape);
    match cli.command {
        Command::Run(args) => commands::run::run(args, &context),
        Command::List(args) => commands::list::run(args, &context),
        Command::Which(args) => commands::which::run(args, &context),
        Command::Add(args) => commands::add::run(args, &context),
        Command::Remove(args) => commands::remove::run(args, &context),
        Command::Completions(args) => commands::completions::run(args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("run-cli").chain(args.iter().copied()))
    }

    #[test]
    fn parses_list_subcommand() {
        let cli = parse(&["list"]).unwrap();
        assert!(matches!(cli.command, Command::List(_)));
        let Command::List(args) = cli.command else {
            panic!("expected List");
        };
        assert!(!args.json);
    }

    #[test]
    fn parses_list_json() {
        let cli = parse(&["list", "--json"]).unwrap();
        let Command::List(args) = cli.command else {
            panic!("expected List");
        };
        assert!(args.json);
    }

    #[test]
    fn parses_global_bin_dir_and_allow_escape() {
        let cli = parse(&["--bin-dir", "tools", "--allow-escape", "list"]).unwrap();
        assert_eq!(cli.bin_dir.as_deref().unwrap().as_os_str(), "tools");
        assert!(cli.allow_escape);
    }

    #[test]
    fn parses_short_bin_dir_flag() {
        let cli = parse(&["-b", "tools", "list"]).unwrap();
        assert_eq!(cli.bin_dir.as_deref().unwrap().as_os_str(), "tools");
    }

    #[test]
    fn parses_run_without_options() {
        let cli = parse(&["run", "example"]).unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run");
        };
        assert_eq!(args.tool, "example");
        assert!(args.cwd.is_none());
        assert!(args.env.is_empty());
        assert!(args.args.is_empty());
    }

    #[test]
    fn run_passes_through_hyphen_args() {
        let cli = parse(&["run", "example", "--help", "-v", "x"]).unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run");
        };
        assert_eq!(args.tool, "example");
        assert_eq!(args.args, ["--help", "-v", "x"]);
    }

    #[test]
    fn run_passes_through_double_dash() {
        let cli = parse(&["run", "example", "--", "--help"]).unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run");
        };
        assert_eq!(args.args, ["--help"]);
    }

    #[test]
    fn run_options_before_tool() {
        let cli = parse(&[
            "run", "--cwd", "work", "--env", "A=1", "--env", "B=2", "tool", "-q",
        ])
        .unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run");
        };
        assert_eq!(args.cwd.as_deref().unwrap().as_os_str(), "work");
        assert_eq!(args.env, ["A=1", "B=2"]);
        assert_eq!(args.tool, "tool");
        assert_eq!(args.args, ["-q"]);
    }

    #[test]
    fn run_known_options_are_consumed_after_tool() {
        let cli = parse(&["run", "tool", "--cwd", "x", "--env", "A=1"]).unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run");
        };
        assert_eq!(args.cwd.as_deref().unwrap().as_os_str(), "x");
        assert_eq!(args.env, ["A=1"]);
        assert!(args.args.is_empty());
    }

    #[test]
    fn run_double_dash_forces_passthrough_of_known_flags() {
        let cli = parse(&["run", "tool", "--", "--cwd", "x", "--env", "A=1"]).unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run");
        };
        assert!(args.cwd.is_none());
        assert!(args.env.is_empty());
        assert_eq!(args.args, ["--cwd", "x", "--env", "A=1"]);
    }

    #[test]
    fn run_unknown_flag_after_tool_switches_to_passthrough() {
        let cli = parse(&["run", "--cwd", "x", "tool", "-q", "--env", "A=1"]).unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run");
        };
        assert_eq!(args.cwd.as_deref().unwrap().as_os_str(), "x");
        assert!(args.env.is_empty());
        assert_eq!(args.args, ["-q", "--env", "A=1"]);
    }

    #[test]
    fn bin_dir_after_tool_is_passed_through() {
        let cli = parse(&["run", "example", "--bin-dir", "x"]).unwrap();
        let Command::Run(args) = cli.command else {
            panic!("expected Run");
        };
        assert!(cli.bin_dir.is_none());
        assert_eq!(args.args, ["--bin-dir", "x"]);
    }

    #[test]
    fn parses_which() {
        let cli = parse(&["which", "example"]).unwrap();
        let Command::Which(args) = cli.command else {
            panic!("expected Which");
        };
        assert_eq!(args.tool, "example");
    }

    #[test]
    fn parses_add() {
        let cli = parse(&["add", "C:\\tools\\x.exe"]).unwrap();
        let Command::Add(args) = cli.command else {
            panic!("expected Add");
        };
        assert_eq!(args.path, PathBuf::from("C:\\tools\\x.exe"));
        assert!(args.name.is_none());

        let cli = parse(&["add", "x.exe", "--name", "renamed.exe"]).unwrap();
        let Command::Add(args) = cli.command else {
            panic!("expected Add");
        };
        assert_eq!(args.name.as_deref(), Some("renamed.exe"));
    }

    #[test]
    fn parses_remove() {
        let cli = parse(&["remove", "example"]).unwrap();
        let Command::Remove(args) = cli.command else {
            panic!("expected Remove");
        };
        assert!(!args.recursive);

        let cli = parse(&["remove", "example", "-r"]).unwrap();
        let Command::Remove(args) = cli.command else {
            panic!("expected Remove");
        };
        assert!(args.recursive);
    }

    #[test]
    fn parses_completions_shells() {
        for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
            parse(&["completions", shell]).unwrap();
        }
        assert!(parse(&["completions", "tcsh"]).is_err());
    }

    #[test]
    fn rejects_unknown_commands_and_missing_args() {
        assert!(parse(&["frobnicate"]).is_err());
        assert!(parse(&["run"]).is_err());
        assert!(parse(&["which"]).is_err());
        assert!(parse(&["add"]).is_err());
    }
}
