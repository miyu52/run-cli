//! Command line interface: clap argument definitions and command dispatch.
//!
//! This module only parses arguments and dispatches; the `Args` structs and
//! execution logic of each command live in [`crate::commands`]; all help text
//! comes from [`crate::messages`].
//!
//! The `run` subcommand uses uv-style passthrough: everything after the tool
//! name is passed through verbatim (including flags that overlap with
//! run-cli's own options). clap cannot express that with `trailing_var_arg`
//! (declared options are still consumed), so the raw argv is pre-split by
//! [`split_run_args`]: the passthrough segment never reaches clap and is
//! injected into the parsed arguments afterwards. **The option sets hardcoded
//! in [`split_run_args`] must be kept in sync with the clap definitions
//! below** (global: `-b`/`--bin-dir`/`--allow-escape`; run: `--cwd`/`--env`).

use std::env;
use std::ffi::{OsStr, OsString};
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
    let raw: Vec<OsString> = env::args_os().skip(1).collect();
    let split = split_run_args(&raw);
    let mut cli = match &split {
        Some(parts) => {
            // Rebuild the argv for clap without the passthrough segment; the
            // `--` separator forces the tool name to be read as a positional
            // value (escapes `-`-prefixed tool names) while unknown options
            // before it are still rejected by clap.
            Cli::try_parse_from(rebuild_argv(parts)).unwrap_or_else(|err| err.exit())
        }
        None => Cli::parse(),
    };
    if let Some(parts) = &split {
        apply_passthrough(&mut cli, parts);
    }
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

/// The result of splitting raw argv for the `run` subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunSplit {
    global: Vec<OsString>,
    run_opts: Vec<OsString>,
    tool: OsString,
    passthrough: Vec<OsString>,
}

/// Rebuild the argv for clap from a [`RunSplit`], without the passthrough
/// segment; the `--` separator forces the tool name to be read as a
/// positional value (escapes `-`-prefixed tool names).
fn rebuild_argv(parts: &RunSplit) -> Vec<OsString> {
    let mut rebuilt = vec![OsString::from("run-cli")];
    rebuilt.extend(parts.global.iter().cloned());
    rebuilt.push(OsString::from("run"));
    rebuilt.extend(parts.run_opts.iter().cloned());
    rebuilt.push(OsString::from("--"));
    rebuilt.push(parts.tool.clone());
    rebuilt
}

/// Inject the passthrough segment of a [`RunSplit`] into a parsed `run`
/// subcommand. Safe to call on any [`Cli`]: the splitter only produces
/// `Some` when the arguments target `run`.
fn apply_passthrough(cli: &mut Cli, parts: &RunSplit) {
    if let Command::Run(args) = &mut cli.command {
        args.args = parts.passthrough.clone();
    }
}

/// Split raw arguments targeting the `run` subcommand.
///
/// Global options (`-b`/`--bin-dir` with its value, `--allow-escape`,
/// `-h`/`--help`, `-V`/`--version`) are collected before the subcommand;
/// `--cwd`/`--env` (and their values) before the tool name; the first
/// non-option token becomes the tool name; everything after it is the
/// passthrough segment, kept verbatim (including `--` and flags overlapping
/// with run-cli's own options). A `--` before the tool name starts the tool
/// name at the next token (escaping `-`-prefixed tool names).
///
/// Returns `None` when the arguments do not target `run` with a tool name;
/// clap then parses the original argv unchanged (error reporting stays with
/// clap for missing values, unknown options or missing tool names).
fn split_run_args(raw: &[OsString]) -> Option<RunSplit> {
    #[derive(Debug, Clone, Copy)]
    enum Stage {
        Global,
        GlobalValue,
        RunOpts,
        RunOptValue,
        AfterDash,
        Passthrough,
    }

    fn is_global_opt(token: &OsStr) -> bool {
        token == OsStr::new("--allow-escape")
            || token == OsStr::new("-h")
            || token == OsStr::new("--help")
            || token == OsStr::new("-V")
            || token == OsStr::new("--version")
    }

    fn starts_with_hyphen(token: &OsStr) -> bool {
        token.to_str().is_some_and(|s| s.starts_with('-'))
    }

    let mut global = Vec::new();
    let mut run_opts = Vec::new();
    let mut passthrough = Vec::new();
    let mut tool: Option<OsString> = None;
    let mut stage = Stage::Global;

    for token in raw {
        match stage {
            Stage::Global => {
                if token == OsStr::new("-b") || token == OsStr::new("--bin-dir") {
                    global.push(token.clone());
                    stage = Stage::GlobalValue;
                } else if is_global_opt(token) {
                    global.push(token.clone());
                } else if token == OsStr::new("run") {
                    stage = Stage::RunOpts;
                } else if starts_with_hyphen(token) {
                    // Unknown global option: let clap reject it.
                    global.push(token.clone());
                } else {
                    // Another subcommand: no splitting needed.
                    return None;
                }
            }
            Stage::GlobalValue => {
                global.push(token.clone());
                stage = Stage::Global;
            }
            Stage::RunOpts => {
                if token == OsStr::new("--cwd") || token == OsStr::new("--env") {
                    run_opts.push(token.clone());
                    stage = Stage::RunOptValue;
                } else if token == OsStr::new("--") {
                    stage = Stage::AfterDash;
                } else if starts_with_hyphen(token) {
                    // Unknown option before the tool name: let clap reject it.
                    run_opts.push(token.clone());
                } else {
                    tool = Some(token.clone());
                    stage = Stage::Passthrough;
                }
            }
            Stage::RunOptValue => {
                run_opts.push(token.clone());
                stage = Stage::RunOpts;
            }
            Stage::AfterDash => {
                tool = Some(token.clone());
                stage = Stage::Passthrough;
            }
            Stage::Passthrough => {
                passthrough.push(token.clone());
            }
        }
    }

    tool.map(|tool| RunSplit {
        global,
        run_opts,
        tool,
        passthrough,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("run-cli").chain(args.iter().copied()))
    }

    /// Parse like [`run`]: split the argv, rebuild it without the passthrough
    /// segment and inject the passthrough args afterwards.
    fn parse_run(args: &[&str]) -> Result<Cli, clap::Error> {
        let raw: Vec<OsString> = args.iter().map(OsString::from).collect();
        let split = split_run_args(&raw);
        let mut cli = match &split {
            Some(parts) => Cli::try_parse_from(rebuild_argv(parts))?,
            None => parse(args)?,
        };
        if let Some(parts) = &split {
            apply_passthrough(&mut cli, parts);
        }
        Ok(cli)
    }

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn run_args_of(cli: &Cli) -> &commands::run::Args {
        let Command::Run(args) = &cli.command else {
            panic!("expected Run");
        };
        args
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
        let cli = parse_run(&["run", "example"]).unwrap();
        let args = run_args_of(&cli);
        assert_eq!(args.tool, "example");
        assert!(args.cwd.is_none());
        assert!(args.env.is_empty());
        assert!(args.args.is_empty());
    }

    #[test]
    fn run_passes_through_hyphen_args() {
        let cli = parse_run(&["run", "example", "--help", "-v", "x"]).unwrap();
        let args = run_args_of(&cli);
        assert_eq!(args.tool, "example");
        assert_eq!(args.args, os_args(&["--help", "-v", "x"]));
    }

    #[test]
    fn run_double_dash_after_tool_is_preserved() {
        let cli = parse_run(&["run", "example", "--", "--help"]).unwrap();
        let args = run_args_of(&cli);
        assert_eq!(args.args, os_args(&["--", "--help"]));
    }

    #[test]
    fn run_options_before_tool() {
        let cli = parse_run(&[
            "run", "--cwd", "work", "--env", "A=1", "--env", "B=2", "tool", "-q",
        ])
        .unwrap();
        let args = run_args_of(&cli);
        assert_eq!(args.cwd.as_deref().unwrap().as_os_str(), "work");
        assert_eq!(args.env, ["A=1", "B=2"]);
        assert_eq!(args.tool, "tool");
        assert_eq!(args.args, os_args(&["-q"]));
    }

    #[test]
    fn run_options_after_tool_pass_through_verbatim() {
        let cli = parse_run(&["run", "tool", "--cwd", "x", "--env", "A=1"]).unwrap();
        let args = run_args_of(&cli);
        assert!(args.cwd.is_none());
        assert!(args.env.is_empty());
        assert_eq!(args.args, os_args(&["--cwd", "x", "--env", "A=1"]));
    }

    #[test]
    fn run_options_before_tool_then_verbatim_passthrough() {
        let cli = parse_run(&["run", "--cwd", "x", "tool", "-q", "--env", "A=1"]).unwrap();
        let args = run_args_of(&cli);
        assert_eq!(args.cwd.as_deref().unwrap().as_os_str(), "x");
        assert!(args.env.is_empty());
        assert_eq!(args.args, os_args(&["-q", "--env", "A=1"]));
    }

    #[test]
    fn bin_dir_after_tool_is_passed_through() {
        let cli = parse_run(&["run", "example", "--bin-dir", "x"]).unwrap();
        let args = run_args_of(&cli);
        assert!(cli.bin_dir.is_none());
        assert_eq!(args.args, os_args(&["--bin-dir", "x"]));
    }

    #[test]
    fn run_without_tool_is_usage_error() {
        assert!(parse_run(&["run"]).is_err());
        assert!(parse_run(&["run", "--cwd", "x"]).is_err());
        assert!(parse_run(&["run", "--bogus", "tool"]).is_err());
        assert!(parse_run(&["run", "tool", "--bogus"]).is_ok());
    }

    #[test]
    fn split_returns_none_for_other_subcommands() {
        assert!(split_run_args(&["list".into()]).is_none());
        assert!(split_run_args(&["-b".into(), "tools".into(), "list".into()]).is_none());
        assert!(split_run_args(&["run".into()]).is_none());
    }

    #[test]
    fn split_collects_global_options() {
        let parts = split_run_args(&[
            "-b".into(),
            "tools".into(),
            "--allow-escape".into(),
            "run".into(),
            "tool".into(),
        ])
        .unwrap();
        assert_eq!(
            parts.global,
            [
                OsString::from("-b"),
                OsString::from("tools"),
                OsString::from("--allow-escape")
            ]
        );
        assert_eq!(parts.tool, OsString::from("tool"));
        assert!(parts.run_opts.is_empty());
        assert!(parts.passthrough.is_empty());
    }

    #[test]
    fn split_bin_dir_value_named_run_is_not_a_subcommand() {
        assert!(split_run_args(&["-b".into(), "run".into(), "list".into()]).is_none());
    }

    #[test]
    fn split_collects_run_options_before_tool() {
        let parts = split_run_args(&[
            "run".into(),
            "--cwd".into(),
            "work".into(),
            "--env".into(),
            "A=1".into(),
            "tool".into(),
        ])
        .unwrap();
        assert_eq!(
            parts.run_opts,
            [
                OsString::from("--cwd"),
                OsString::from("work"),
                OsString::from("--env"),
                OsString::from("A=1")
            ]
        );
        assert_eq!(parts.tool, OsString::from("tool"));
        assert!(parts.passthrough.is_empty());
    }

    #[test]
    fn split_passthrough_starts_at_tool() {
        let parts = split_run_args(&[
            "run".into(),
            "tool".into(),
            "--cwd".into(),
            "x".into(),
            "--".into(),
            "-v".into(),
        ])
        .unwrap();
        assert!(parts.run_opts.is_empty());
        assert_eq!(parts.tool, OsString::from("tool"));
        assert_eq!(
            parts.passthrough,
            [
                OsString::from("--cwd"),
                OsString::from("x"),
                OsString::from("--"),
                OsString::from("-v")
            ]
        );
    }

    #[test]
    fn split_double_dash_before_tool_escapes_tool_name() {
        let parts =
            split_run_args(&["run".into(), "--".into(), "-weird".into(), "x".into()]).unwrap();
        assert_eq!(parts.tool, OsString::from("-weird"));
        assert_eq!(parts.passthrough, [OsString::from("x")]);
    }

    #[test]
    fn split_unknown_option_before_tool_goes_to_clap() {
        let parts = split_run_args(&["run".into(), "--bogus".into(), "tool".into()]).unwrap();
        assert_eq!(parts.run_opts, [OsString::from("--bogus")]);
        assert_eq!(parts.tool, OsString::from("tool"));
    }

    #[test]
    fn split_returns_none_without_tool() {
        assert!(split_run_args(&["run".into(), "--cwd".into(), "x".into()]).is_none());
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
        assert!(parse_run(&["run"]).is_err());
        assert!(parse(&["which"]).is_err());
        assert!(parse(&["add"]).is_err());
    }
}
