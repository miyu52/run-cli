//! Binary-level integration tests: drive the real `run-cli` executable.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

#[cfg(windows)]
const ECHO_TOOL: &str = "echo.bat";
#[cfg(not(windows))]
const ECHO_TOOL: &str = "echo";

#[cfg(windows)]
const EXIT_TOOL: &str = "exit42.bat";
#[cfg(not(windows))]
const EXIT_TOOL: &str = "exit42";

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
const OUTSIDE_BODY: &str = "@echo off\necho %*";
#[cfg(not(windows))]
const OUTSIDE_BODY: &str = "echo \"$*\"";

#[cfg(windows)]
const REL_ADD_TOOL: &str = "rel-add.bat";
#[cfg(not(windows))]
const REL_ADD_TOOL: &str = "rel-add";

#[cfg(windows)]
const REL_ADD_BODY: &str = "@echo off\necho added-rel";
#[cfg(not(windows))]
const REL_ADD_BODY: &str = "echo \"added-rel\"";

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
        #[cfg(windows)]
        write_tool(&dir.path().join("scripts"), "sub.bat", "@echo off\necho %*");
        #[cfg(not(windows))]
        write_tool(&dir.path().join("scripts"), "sub", "echo \"$*\"");
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

fn run_cli_with_cwd(cwd: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_run-cli"));
    cmd.args(args).env_remove("RUN_CLI_BIN").current_dir(cwd);
    cmd.output().expect("failed to spawn run-cli")
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
fn list_json_empty_toolbox_is_empty_array() {
    let dir = TempDir::new().unwrap();
    let output = run_cli(&["--bin-dir", dir.path().to_str().unwrap(), "list", "--json"]);
    assert!(output.status.success());
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(parsed, serde_json::json!([]));
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

#[cfg(windows)]
const DASH_TOOL: &str = "-weird.bat";
#[cfg(not(windows))]
const DASH_TOOL: &str = "-weird";

#[test]
fn run_double_dash_escapes_hyphen_prefixed_tool() {
    let toolbox = Toolbox::new();
    write_tool(toolbox.path(), DASH_TOOL, ECHO_BODY);

    let output = run_cli_in_toolbox(&toolbox, &["run", "--", DASH_TOOL, "hi"]);
    assert!(
        output.status.success(),
        "stderr: {}; stdout: {}",
        stderr(&output),
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("hi"),
        "stdout was: {}",
        stdout(&output)
    );
}

#[test]
fn run_help_flags_before_tool_are_rejected() {
    let toolbox = Toolbox::new();
    for args in [
        ["run", "--help"],
        ["run", "-h"],
        ["run", "--version"],
        ["run", "-V"],
    ] {
        let output = run_cli_in_toolbox(&toolbox, &args);
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
    }
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
fn run_with_cwd_and_relative_bin_dir() {
    // A relative `--bin-dir` combined with `--cwd` must resolve the tool
    // against the parent's working directory (regression: relative tool paths
    // used to be resolved against the child's `--cwd`).
    let dir = TempDir::new_in(std::env::current_dir().unwrap().join("target")).unwrap();
    write_tool(dir.path(), CWD_ENV_TOOL, CWD_ENV_BODY);
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    let rel = dir
        .path()
        .strip_prefix(std::env::current_dir().unwrap())
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let output = run_cli(&[
        "--bin-dir",
        &rel,
        "run",
        "--cwd",
        sub.to_str().unwrap(),
        CWD_ENV_TOOL,
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let out = stdout(&output);
    assert!(out.contains("sub"), "stdout was: {out}");
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
fn run_missing_bin_dir_is_runtime_error_not_127() {
    let missing = PathBuf::from("definitely-missing-run-dir");
    let output = run_cli(&["--bin-dir", missing.to_str().unwrap(), "run", "any-tool"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("bin directory not found"));
}

#[test]
fn run_outside_path_exits_127() {
    // Absolute paths and `..` that resolve outside the toolbox are rejected
    // as not-found (exit 127), not executed.
    let toolbox = Toolbox::new();
    write_tool(
        toolbox.dir.path().parent().unwrap(),
        "outside.bat",
        OUTSIDE_BODY,
    );
    let output = run_cli_in_toolbox(&toolbox, &["run", "../outside.bat", "hi"]);
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn run_absolute_path_inside_toolbox_works() {
    let toolbox = Toolbox::new();
    let abs = toolbox.path().join(ECHO_TOOL);
    let output = run_cli_in_toolbox(&toolbox, &["run", abs.to_str().unwrap(), "hi"]);
    assert!(
        output.status.success(),
        "stderr: {}; stdout: {}",
        stderr(&output),
        stdout(&output)
    );
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

#[test]
fn which_missing_bin_dir_is_runtime_error_not_127() {
    let missing = PathBuf::from("definitely-missing-which-dir");
    let output = run_cli(&["--bin-dir", missing.to_str().unwrap(), "which", "any-tool"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("bin directory not found"));
}

// --- add / remove ---

#[test]
fn add_then_run_tool() {
    let toolbox = Toolbox::new();
    // write_tool makes the source executable on Unix (shebang + 0o755);
    // fs::write alone would leave it unexecutable.
    let source = write_tool(
        toolbox.path().parent().unwrap(),
        "new-tool.bat",
        "@echo off\necho added",
    );

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

    // The added entry is a toolbox tool even when it is a symlink pointing
    // outside (Unix always links; Windows copies without developer mode):
    // both outcomes run without any extra flag.
    let run = run_cli(&[
        "--bin-dir",
        toolbox.path().to_str().unwrap(),
        "run",
        "new-tool.bat",
    ]);
    assert!(run.status.success(), "stderr was: {}", stderr(&run));
    assert!(stdout(&run).contains("added"));
}

#[test]
fn add_relative_source_creates_working_entry() {
    // Regression: the symlink target used to be the source path verbatim, so a
    // relative source resolved against the toolbox directory and dangled. The
    // source is now absolutized before linking/copying.
    let toolbox = Toolbox::new();
    let workdir = TempDir::new().unwrap();
    write_tool(workdir.path(), REL_ADD_TOOL, REL_ADD_BODY);

    let output = run_cli_with_cwd(
        workdir.path(),
        &[
            "--bin-dir",
            toolbox.path().to_str().unwrap(),
            "add",
            REL_ADD_TOOL,
        ],
    );
    assert!(output.status.success(), "stderr was: {}", stderr(&output));

    // Works for both outcomes: symlink with an absolute target and copy.
    let run = run_cli(&[
        "--bin-dir",
        toolbox.path().to_str().unwrap(),
        "run",
        REL_ADD_TOOL,
    ]);
    assert!(run.status.success(), "stderr was: {}", stderr(&run));
    assert!(stdout(&run).contains("added-rel"));
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

#[test]
fn remove_missing_bin_dir_is_runtime_error_not_127() {
    let missing = PathBuf::from("definitely-missing-remove-dir");
    let output = run_cli(&["--bin-dir", missing.to_str().unwrap(), "remove", "any-tool"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("bin directory not found"));
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
