//! Binary-level integration tests: drive the real `run-cli` executable.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

#[cfg(windows)]
const ECHO_TOOL: &str = "echo.bat";
#[cfg(not(windows))]
const ECHO_TOOL: &str = "echo.sh";

#[cfg(windows)]
const EXIT_TOOL: &str = "exit42.bat";
#[cfg(not(windows))]
const EXIT_TOOL: &str = "exit42.sh";

#[cfg(windows)]
const ECHO_BODY: &str = "@echo off\necho %*";
#[cfg(not(windows))]
const ECHO_BODY: &str = "echo \"$*\"";

#[cfg(windows)]
const EXIT_BODY: &str = "@exit /b 42";
#[cfg(not(windows))]
const EXIT_BODY: &str = "exit 42";

#[cfg(windows)]
const CWD_ENV_TOOL: &str = "cwd-env.bat";
#[cfg(not(windows))]
const CWD_ENV_TOOL: &str = "cwd-env.sh";

#[cfg(windows)]
const CWD_ENV_BODY: &str = "@echo off\necho %CD%\necho %FOO%";
#[cfg(not(windows))]
const CWD_ENV_BODY: &str = "echo \"$PWD\"\necho \"$FOO\"";

#[cfg(windows)]
fn write_tool(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body.replace('\n', "\r\n")).unwrap();
    path
}

#[cfg(not(windows))]
fn write_tool(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

/// A toolbox directory inside a temp dir.
struct Toolbox {
    dir: TempDir,
}

impl Toolbox {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        write_tool(dir.path(), ECHO_TOOL, ECHO_BODY);
        write_tool(dir.path(), EXIT_TOOL, EXIT_BODY);
        write_tool(dir.path(), "plain", "@exit /b 0");
        write_tool(dir.path(), CWD_ENV_TOOL, CWD_ENV_BODY);
        std::fs::create_dir(dir.path().join("scripts")).unwrap();
        write_tool(&dir.path().join("scripts"), "sub.bat", "@echo off\necho %*");
        Toolbox { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn run_cli_with_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_run-cli"));
    cmd.args(args).env_remove("RUN_CLI_BIN");
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.output().expect("failed to spawn run-cli")
}

fn run_cli(args: &[&str]) -> Output {
    run_cli_with_env(args, &[])
}

fn run_cli_in_toolbox(toolbox: &Toolbox, args: &[&str]) -> Output {
    let mut full = vec![
        "--bin-dir".to_string(),
        toolbox.path().to_string_lossy().into_owned(),
    ];
    full.extend(args.iter().map(|a| a.to_string()));
    let strings: Vec<&str> = full.iter().map(|s| s.as_str()).collect();
    run_cli(&strings)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

// --- list ---

#[test]
fn list_shows_tools_sorted_with_dirs() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["list"]);
    assert!(output.status.success());

    let out = stdout(&output);
    let echo_pos = out.find(ECHO_TOOL).unwrap();
    let scripts_pos = out.find("scripts/").unwrap();
    let exit_pos = out.find(EXIT_TOOL).unwrap();
    assert!(echo_pos < exit_pos, "tools should be sorted");
    assert!(
        scripts_pos > exit_pos,
        "dirs should be listed with a trailing slash"
    );
}

#[test]
fn list_empty_toolbox() {
    let dir = TempDir::new().unwrap();
    let output = run_cli(&["--bin-dir", dir.path().to_str().unwrap(), "list"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("no tools"));
}

#[test]
fn list_missing_bin_dir_fails() {
    let missing = PathBuf::from("definitely-missing-dir");
    let output = run_cli(&["--bin-dir", missing.to_str().unwrap(), "list"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains(&missing.display().to_string()));
}

#[test]
fn list_json_is_valid_and_contains_entries() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["list", "--json"]);
    assert!(output.status.success());

    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout(&output)).unwrap();
    assert!(!parsed.is_empty());
    let names: Vec<&str> = parsed
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&ECHO_TOOL));
    assert!(names.contains(&"scripts"));
    assert!(parsed.iter().any(|tool| tool["kind"] == "directory"));
    assert!(parsed.iter().any(|tool| tool["kind"] == "file"));
}

// --- run ---

#[test]
fn run_tool_passes_through_args() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["run", "echo", "hello", "--help", "-v"]);
    assert!(output.status.success());

    let out = stdout(&output);
    assert!(out.contains("hello"), "stdout was: {out}");
    assert!(out.contains("--help"), "stdout was: {out}");
    assert!(out.contains("-v"), "stdout was: {out}");
}

#[test]
fn run_tool_passes_through_run_cli_options_verbatim() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(
        &toolbox,
        &["run", "echo", "--cwd", "somewhere", "--env", "A=1"],
    );
    assert!(output.status.success());

    let out = stdout(&output);
    assert!(out.contains("--cwd"), "stdout was: {out}");
    assert!(out.contains("somewhere"), "stdout was: {out}");
    assert!(out.contains("--env"), "stdout was: {out}");
    assert!(out.contains("A=1"), "stdout was: {out}");
}

#[test]
fn run_tool_preserves_double_dash_after_tool() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["run", "echo", "--", "--help"]);
    assert!(output.status.success());

    let out = stdout(&output);
    assert!(out.contains("--"), "stdout was: {out}");
    assert!(out.contains("--help"), "stdout was: {out}");
}

#[test]
fn run_tool_passes_through_exit_code() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["run", "exit42"]);
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn run_tool_in_subdirectory() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["run", "scripts/sub", "hi"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("hi"));
}

#[test]
fn run_with_cwd_and_env() {
    let toolbox = Toolbox::new();
    let workdir = toolbox.path().join("scripts");
    let output = run_cli_in_toolbox(
        &toolbox,
        &[
            "run",
            "--cwd",
            workdir.to_str().unwrap(),
            "--env",
            "FOO=bar",
            CWD_ENV_TOOL,
        ],
    );
    assert!(output.status.success());

    let out = stdout(&output);
    assert!(out.contains("scripts"), "stdout was: {out}");
    assert!(out.contains("bar"), "stdout was: {out}");
}

#[test]
fn run_missing_tool_exits_127_with_suggestion_and_list() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["run", "echoe"]);
    assert_eq!(output.status.code(), Some(127));

    let err = stderr(&output);
    assert!(err.contains("echoe"));
    assert!(err.contains("did you mean"), "stderr was: {err}");
    assert!(err.contains("available tools"));
    assert!(err.contains(ECHO_TOOL));
}

#[test]
fn run_missing_tool_in_empty_toolbox() {
    let dir = TempDir::new().unwrap();
    let output = run_cli(&[
        "--bin-dir",
        dir.path().to_str().unwrap(),
        "run",
        "definitely-missing",
    ]);
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn run_escaping_path_is_rejected_by_default() {
    let toolbox = Toolbox::new();
    write_tool(
        toolbox.dir.path().parent().unwrap(),
        "outside.bat",
        "@echo off\necho %*",
    );
    let output = run_cli_in_toolbox(&toolbox, &["run", "../outside.bat", "hi"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("outside the toolbox"),
        "stderr was: {}",
        stderr(&output)
    );
}

#[test]
fn run_escaping_path_allowed_with_flag() {
    let toolbox = Toolbox::new();
    write_tool(
        toolbox.dir.path().parent().unwrap(),
        "outside.bat",
        "@echo off\necho %*",
    );
    let output = run_cli(&[
        "--bin-dir",
        toolbox.path().to_str().unwrap(),
        "--allow-escape",
        "run",
        "../outside.bat",
        "hi",
    ]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("hi"));
}

// --- which ---

#[test]
fn which_prints_absolute_path() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["which", "echo"]);
    assert!(output.status.success());

    let path = PathBuf::from(stdout(&output).trim());
    assert!(
        path.is_absolute(),
        "which should print an absolute path: {path:?}"
    );
    assert!(path.ends_with(ECHO_TOOL), "unexpected path: {path:?}");
}

#[test]
fn which_missing_tool_exits_127() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["which", "nope"]);
    assert_eq!(output.status.code(), Some(127));
}

// --- add / remove ---

#[test]
fn add_then_run_tool() {
    let toolbox = Toolbox::new();
    let source = toolbox.path().parent().unwrap().join("new-tool.bat");
    std::fs::write(&source, "@echo off\necho added").unwrap();

    let output = run_cli(&[
        "--bin-dir",
        toolbox.path().to_str().unwrap(),
        "add",
        source.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr was: {}", stderr(&output));
    let out = stdout(&output);
    assert!(
        out.contains("linked") || out.contains("copied"),
        "stdout was: {out}"
    );

    let run = run_cli_in_toolbox(&toolbox, &["run", "new-tool.bat"]);
    assert!(run.status.success());
    assert!(stdout(&run).contains("added"));
}

#[test]
fn add_custom_name() {
    let toolbox = Toolbox::new();
    let source = toolbox.path().parent().unwrap().join("src.bat");
    std::fs::write(&source, "@echo off\necho renamed").unwrap();

    let output = run_cli(&[
        "--bin-dir",
        toolbox.path().to_str().unwrap(),
        "add",
        source.to_str().unwrap(),
        "--name",
        "renamed.bat",
    ]);
    assert!(output.status.success());

    let list = run_cli_in_toolbox(&toolbox, &["list"]);
    assert!(stdout(&list).contains("renamed.bat"));
}

#[test]
fn add_duplicate_fails() {
    let toolbox = Toolbox::new();
    let source = toolbox.path().parent().unwrap().join("dup.bat");
    std::fs::write(&source, "@echo off").unwrap();

    let first = run_cli(&[
        "--bin-dir",
        toolbox.path().to_str().unwrap(),
        "add",
        source.to_str().unwrap(),
    ]);
    assert!(first.status.success());

    let second = run_cli(&[
        "--bin-dir",
        toolbox.path().to_str().unwrap(),
        "add",
        source.to_str().unwrap(),
    ]);
    assert_eq!(second.status.code(), Some(1));
    assert!(stderr(&second).contains("already exists"));
}

#[test]
fn remove_tool_and_directory() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["remove", "plain"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("plain"));

    let list = run_cli_in_toolbox(&toolbox, &["list"]);
    assert!(!stdout(&list).contains("plain"));

    let dir_no_recursive = run_cli_in_toolbox(&toolbox, &["remove", "scripts"]);
    assert_eq!(dir_no_recursive.status.code(), Some(1));
    assert!(stderr(&dir_no_recursive).contains("--recursive"));

    let dir_recursive = run_cli_in_toolbox(&toolbox, &["remove", "scripts", "-r"]);
    assert!(dir_recursive.status.success());

    let list = run_cli_in_toolbox(&toolbox, &["list"]);
    assert!(!stdout(&list).contains("scripts/"));
}

#[test]
fn remove_missing_tool_exits_127() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["remove", "nope"]);
    assert_eq!(output.status.code(), Some(127));
}

// --- completions ---

#[test]
fn completions_generates_script() {
    let output = run_cli(&["completions", "bash"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("run-cli"));

    let output = run_cli(&["completions", "powershell"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("run-cli"));
}

#[test]
fn completions_rejects_unknown_shell() {
    let output = run_cli(&["completions", "tcsh"]);
    assert_eq!(output.status.code(), Some(2));
}

// --- usage / bin dir resolution ---

#[test]
fn no_subcommand_is_usage_error() {
    let output = run_cli(&[]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn bin_dir_after_tool_is_passed_through() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["run", "echo", "--bin-dir", "x"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("--bin-dir"));
}

#[test]
fn env_var_bin_dir_is_used() {
    let toolbox = Toolbox::new();
    let output = run_cli_with_env(
        &["list"],
        &[("RUN_CLI_BIN", toolbox.path().to_str().unwrap())],
    );
    assert!(output.status.success());
    assert!(stdout(&output).contains(ECHO_TOOL));
}

#[test]
fn flag_overrides_env_var() {
    let flag_box = Toolbox::new();
    let env_box = Toolbox::new();
    write_tool(env_box.path(), "env-only-tool.bat", "@exit /b 0");

    let output = run_cli_with_env(
        &["--bin-dir", flag_box.path().to_str().unwrap(), "list"],
        &[("RUN_CLI_BIN", env_box.path().to_str().unwrap())],
    );
    assert!(output.status.success());

    let out = stdout(&output);
    assert!(out.contains(ECHO_TOOL));
    assert!(!out.contains("env-only-tool"));
}
