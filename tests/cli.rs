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
const PLAIN_TOOL: &str = "plain.bat";
#[cfg(not(windows))]
const PLAIN_TOOL: &str = "plain";

#[cfg(windows)]
const PLAIN_BODY: &str = "@exit /b 0";
#[cfg(not(windows))]
const PLAIN_BODY: &str = "exit 0";

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

/// A toolbox directory inside a temp dir, with an isolated config file in a
/// separate temp dir (so the config file itself never pollutes the toolbox
/// listing, and the real default config location is never touched).
struct Toolbox {
    dir: TempDir,
    /// Keeps the config's temp directory alive; only the derived path is used.
    #[expect(dead_code)]
    config_dir: TempDir,
    config: PathBuf,
}

impl Toolbox {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        write_tool(dir.path(), ECHO_TOOL, ECHO_BODY);
        write_tool(dir.path(), EXIT_TOOL, EXIT_BODY);
        write_tool(dir.path(), PLAIN_TOOL, PLAIN_BODY);
        write_tool(dir.path(), CWD_ENV_TOOL, CWD_ENV_BODY);
        std::fs::create_dir(dir.path().join("scripts")).unwrap();
        #[cfg(windows)]
        write_tool(&dir.path().join("scripts"), "sub.bat", "@echo off\necho %*");
        #[cfg(not(windows))]
        write_tool(&dir.path().join("scripts"), "sub", "echo \"$*\"");
        let config_dir = TempDir::new().unwrap();
        Toolbox {
            config: config_dir.path().join("config.toml"),
            config_dir,
            dir,
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn run_cli_opt(
    bin_dir: Option<&Path>,
    config: Option<&Path>,
    cwd: Option<&Path>,
    env: &[(&str, &str)],
    args: &[&str],
) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_run-cli"));
    cmd.env_remove("RUN_CLI_BIN").env_remove("RUN_CLI_CONFIG");
    if let Some(dir) = bin_dir {
        cmd.arg("--bin-dir").arg(dir);
    }
    if let Some(path) = config {
        cmd.arg("--config").arg(path);
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.args(args);
    cmd.output().expect("failed to spawn run-cli")
}

/// Run with an isolated (empty) config so tests never read the real default
/// config file.
fn run_cli(args: &[&str]) -> Output {
    let dir = TempDir::new().unwrap();
    run_cli_opt(None, Some(&dir.path().join("config.toml")), None, &[], args)
}

fn run_cli_with_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let dir = TempDir::new().unwrap();
    run_cli_opt(None, Some(&dir.path().join("config.toml")), None, env, args)
}

fn run_cli_in_toolbox(toolbox: &Toolbox, args: &[&str]) -> Output {
    run_cli_opt(Some(toolbox.path()), Some(&toolbox.config), None, &[], args)
}

fn run_cli_with_config(config: &Path, args: &[&str]) -> Output {
    run_cli_opt(None, Some(config), None, &[], args)
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
    let output = run_cli_opt(
        Some(dir.path()),
        Some(&dir.path().join("config.toml")),
        None,
        &[],
        &["list"],
    );
    assert!(output.status.success());
    assert!(stdout(&output).contains("no tools"));
}

#[test]
fn list_json_empty_toolbox_is_empty_array() {
    let dir = TempDir::new().unwrap();
    let output = run_cli_opt(
        Some(dir.path()),
        Some(&dir.path().join("config.toml")),
        None,
        &[],
        &["list", "--json"],
    );
    assert!(output.status.success());
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(parsed, serde_json::json!([]));
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
    assert!(
        parsed.iter().all(|tool| tool["origin"] == "bin"),
        "toolbox-only listing should be origin=bin"
    );
}

#[test]
fn list_groups_entries_by_source() {
    let toolbox = Toolbox::new();
    let source = toolbox.path().parent().unwrap().join("from-config.bat");
    std::fs::write(&source, "@echo off\necho from-config").unwrap();
    let add = run_cli_opt(
        Some(toolbox.path()),
        Some(&toolbox.config),
        None,
        &[],
        &["add", source.to_str().unwrap()],
    );
    assert!(add.status.success(), "stderr: {}", stderr(&add));

    let output = run_cli_in_toolbox(&toolbox, &["list"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    let lines: Vec<&str> = out.lines().collect();
    // Config group first with its own header and entries, then the bin group
    // (the toolbox has 5 entries).
    assert!(
        lines[0].contains(&toolbox.config.display().to_string()) && lines[0].contains("(1):"),
        "config group header was: {}",
        lines[0]
    );
    assert_eq!(lines[1], "  - from-config.bat");
    assert!(
        lines[2].contains(&toolbox.path().display().to_string()) && lines[2].contains("(5):"),
        "bin group header was: {}",
        lines[2]
    );
    assert_eq!(lines.len(), 2 + 1 + 5, "2 headers + 6 entries");
}

#[test]
fn list_bin_only_has_single_group() {
    let toolbox = Toolbox::new();
    let output = run_cli_in_toolbox(&toolbox, &["list"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    let lines: Vec<&str> = out.lines().collect();
    // No config entries: only the bin group is shown, without a config header.
    assert!(
        lines[0].contains(&toolbox.path().display().to_string()),
        "first line should be the bin group header: {}",
        lines[0]
    );
    assert!(!out.contains(&toolbox.config.display().to_string()));
    assert_eq!(lines.len(), 1 + 5, "1 header + 5 entries");
}

#[test]
fn list_merges_config_and_bin_with_origins() {
    let toolbox = Toolbox::new();
    let source = toolbox.path().parent().unwrap().join("from-config.bat");
    std::fs::write(&source, "@echo off\necho from-config").unwrap();

    let add = run_cli_opt(
        Some(toolbox.path()),
        Some(&toolbox.config),
        None,
        &[],
        &["add", source.to_str().unwrap()],
    );
    assert!(add.status.success(), "stderr: {}", stderr(&add));

    let output = run_cli_in_toolbox(&toolbox, &["list", "--json"]);
    assert!(output.status.success());
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout(&output)).unwrap();
    let config_entry = parsed
        .iter()
        .find(|tool| tool["name"] == "from-config.bat")
        .expect("config entry should be listed");
    assert_eq!(config_entry["origin"], "config");
    let bin_entry = parsed
        .iter()
        .find(|tool| tool["name"] == ECHO_TOOL)
        .expect("bin entry should be listed");
    assert_eq!(bin_entry["origin"], "bin");
}

#[test]
fn list_config_shadows_bin_same_name() {
    let toolbox = Toolbox::new();
    let source = toolbox.path().parent().unwrap().join("shadow.bat");
    std::fs::write(&source, "@echo off\necho shadow").unwrap();
    let add = run_cli_opt(
        Some(toolbox.path()),
        Some(&toolbox.config),
        None,
        &[],
        &["add", source.to_str().unwrap(), "--name", ECHO_TOOL],
    );
    assert!(add.status.success(), "stderr: {}", stderr(&add));

    let output = run_cli_in_toolbox(&toolbox, &["list", "--json"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout(&output)).unwrap();
    let matches: Vec<&serde_json::Value> = parsed
        .iter()
        .filter(|tool| tool["name"] == ECHO_TOOL)
        .collect();
    // Both entries are shown; the toolbox one is marked as shadowed.
    assert_eq!(
        matches.len(),
        2,
        "both the config and the bin entry are listed"
    );
    let config_entry = matches
        .iter()
        .find(|tool| tool["origin"] == "config")
        .unwrap();
    assert!(
        config_entry.get("shadowed").is_none(),
        "config entries are never shadowed"
    );
    let bin_entry = matches.iter().find(|tool| tool["origin"] == "bin").unwrap();
    assert_eq!(bin_entry["shadowed"], true);
}

#[test]
fn list_marks_shadowed_bin_entry_human_readable() {
    let toolbox = Toolbox::new();
    let source = toolbox.path().parent().unwrap().join("shadow.bat");
    std::fs::write(&source, "@echo off\necho shadow").unwrap();
    let add = run_cli_opt(
        Some(toolbox.path()),
        Some(&toolbox.config),
        None,
        &[],
        &["add", source.to_str().unwrap(), "--name", ECHO_TOOL],
    );
    assert!(add.status.success(), "stderr: {}", stderr(&add));

    let output = run_cli_in_toolbox(&toolbox, &["list"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(
        out.contains(&format!("{ECHO_TOOL} (shadowed)")),
        "shadowed bin entry should be marked: {out}"
    );
    // A config entry of the same name is not marked.
    let config_lines: Vec<&str> = out
        .lines()
        .skip(1)
        .take_while(|line| !line.starts_with("tools in"))
        .collect();
    assert!(
        config_lines
            .iter()
            .any(|line| line.contains(ECHO_TOOL) && !line.contains("shadowed")),
        "config entry should not be marked: {out}"
    );
}

#[test]
fn list_skips_config_entry_with_missing_path() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let gone = dir.path().join("gone.bat");
    std::fs::write(&gone, "@echo off").unwrap();
    let alive = dir.path().join("alive.bat");
    std::fs::write(&alive, "@echo off").unwrap();
    let add_gone = run_cli_with_config(&config, &["add", gone.to_str().unwrap()]);
    assert!(add_gone.status.success(), "stderr: {}", stderr(&add_gone));
    let add_alive = run_cli_with_config(&config, &["add", alive.to_str().unwrap()]);
    assert!(add_alive.status.success());
    std::fs::remove_file(&gone).unwrap();

    let output = run_cli_with_config(&config, &["list", "--json"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout(&output)).unwrap();
    let names: Vec<&str> = parsed
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["alive.bat"], "missing config path is skipped");
}

#[test]
fn list_missing_bin_dir_shows_config_entries_only() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = dir.path().join("only-config.bat");
    std::fs::write(&source, "@echo off").unwrap();
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success(), "stderr: {}", stderr(&add));

    let missing = dir.path().join("definitely-missing-bin");
    let output = run_cli_opt(
        Some(&missing),
        Some(&config),
        None,
        &[],
        &["list", "--json"],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["name"], "only-config.bat");
    assert_eq!(parsed[0]["origin"], "config");
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

#[cfg(windows)]
#[test]
fn run_ps1_tool_through_powershell() {
    // .ps1 tools are launched via `powershell -File`; bare-name resolution
    // must find them through the extension candidates.
    let toolbox = Toolbox::new();
    write_tool(toolbox.path(), "greet.ps1", "Write-Output $args");
    let output = run_cli_in_toolbox(&toolbox, &["run", "greet", "hello", "--flag"]);
    assert!(
        output.status.success(),
        "stderr: {}; stdout: {}",
        stderr(&output),
        stdout(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("hello"), "stdout was: {out}");
    assert!(out.contains("--flag"), "stdout was: {out}");
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
    let scratch = TempDir::new().unwrap();
    // run-cli itself stays in the test working directory (the relative
    // `--bin-dir` resolves against it); only the tool runs with `--cwd`.
    let output = run_cli_opt(
        Some(Path::new(&rel)),
        Some(&scratch.path().join("config.toml")),
        None,
        &[],
        &["run", "--cwd", sub.to_str().unwrap(), CWD_ENV_TOOL],
    );
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
fn run_missing_tool_suggests_config_names() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "echo.bat", ECHO_BODY);
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    let output = run_cli_with_config(&config, &["run", "echoe"]);
    assert_eq!(output.status.code(), Some(127));
    let err = stderr(&output);
    assert!(err.contains("did you mean"), "stderr was: {err}");
    assert!(err.contains("echo.bat"), "stderr was: {err}");
}

#[test]
fn run_missing_tool_in_empty_toolbox() {
    let dir = TempDir::new().unwrap();
    let output = run_cli_opt(
        Some(dir.path()),
        Some(&dir.path().join("config.toml")),
        None,
        &[],
        &["run", "definitely-missing"],
    );
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

// --- run via config ---

#[test]
fn add_then_run_tool() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "new-tool.bat", "@echo off\necho added");

    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success(), "stderr was: {}", stderr(&add));
    assert!(
        stdout(&add).contains("added"),
        "stdout was: {}",
        stdout(&add)
    );
    assert!(config.is_file(), "config file should be written");

    let run = run_cli_with_config(&config, &["run", "new-tool.bat", "hi"]);
    assert!(run.status.success(), "stderr was: {}", stderr(&run));
    assert!(
        stdout(&run).contains("added"),
        "stdout was: {}",
        stdout(&run)
    );
}

#[test]
fn run_registered_tool_passes_through_args() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "echo.bat", ECHO_BODY);
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    let output = run_cli_with_config(&config, &["run", "echo.bat", "hello", "--help", "-v"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("hello"), "stdout was: {out}");
    assert!(out.contains("--help"), "stdout was: {out}");
    assert!(out.contains("-v"), "stdout was: {out}");
}

#[cfg(windows)]
#[test]
fn run_registered_ps1_tool_through_powershell() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "greet.ps1", "Write-Output $args");
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    let output = run_cli_with_config(&config, &["run", "greet", "hello", "--flag"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("hello"), "stdout was: {out}");
    assert!(out.contains("--flag"), "stdout was: {out}");
}

#[test]
fn config_priority_over_bin() {
    let toolbox = Toolbox::new();
    #[cfg(windows)]
    let body = "@echo off\necho config-ran";
    #[cfg(not(windows))]
    let body = "echo \"config-ran\"";
    let config_src = write_tool(toolbox.path().parent().unwrap(), "config-echo.bat", body);
    let add = run_cli_opt(
        Some(toolbox.path()),
        Some(&toolbox.config),
        None,
        &[],
        &["add", config_src.to_str().unwrap(), "--name", ECHO_TOOL],
    );
    assert!(add.status.success(), "stderr: {}", stderr(&add));

    // The same name exists in the toolbox, but the config entry wins.
    let output = run_cli_in_toolbox(&toolbox, &["run", ECHO_TOOL, "x"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("config-ran"),
        "stdout was: {}",
        stdout(&output)
    );
}

#[test]
fn run_registered_missing_path_exits_1() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "gone.bat", "@echo off");
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());
    std::fs::remove_file(&source).unwrap();

    let output = run_cli_with_config(&config, &["run", "gone.bat"]);
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert!(err.contains("gone"), "stderr was: {err}");
    assert!(err.contains("not found"), "stderr was: {err}");
    assert!(!err.contains("did you mean"), "stderr was: {err}");
}

#[test]
fn run_registered_missing_path_does_not_fallback_to_bin() {
    let toolbox = Toolbox::new();
    let source = toolbox.path().parent().unwrap().join("gone.bat");
    std::fs::write(&source, "@echo off").unwrap();
    let add = run_cli_opt(
        Some(toolbox.path()),
        Some(&toolbox.config),
        None,
        &[],
        &["add", source.to_str().unwrap(), "--name", ECHO_TOOL],
    );
    assert!(add.status.success());
    std::fs::remove_file(&source).unwrap();

    // The toolbox has a real `echo.bat`, but the broken config registration
    // shadows it and must be reported loudly, not silently ignored.
    let output = run_cli_in_toolbox(&toolbox, &["run", "echo"]);
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("registered tool"),
        "stderr was: {}",
        stderr(&output)
    );
}

#[test]
fn run_registered_path_is_directory_exits_1() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    let add = run_cli_with_config(&config, &["add", sub.to_str().unwrap()]);
    assert_eq!(
        add.status.code(),
        Some(1),
        "directories cannot be registered"
    );
    assert!(
        stderr(&add).contains("not a file"),
        "stderr was: {}",
        stderr(&add)
    );
}

#[cfg(windows)]
#[test]
fn run_bare_name_matches_config_extension() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "program.bat", "@echo off\necho ran-bat");
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    // A bare query resolves through the registered name with extension
    // completion; the exact name also works.
    let bare = run_cli_with_config(&config, &["run", "program", "x"]);
    assert!(bare.status.success(), "stderr: {}", stderr(&bare));
    assert!(stdout(&bare).contains("ran-bat"));

    let exact = run_cli_with_config(&config, &["run", "program.bat", "x"]);
    assert!(exact.status.success(), "stderr: {}", stderr(&exact));
}

#[cfg(windows)]
#[test]
fn which_bare_name_prefers_exe_in_config() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let bat = write_tool(dir.path(), "program.bat", "@echo off");
    let exe = dir.path().join("program.exe");
    std::fs::write(&exe, "not really an exe; only resolved, never run").unwrap();
    let add_bat = run_cli_with_config(&config, &["add", bat.to_str().unwrap()]);
    assert!(add_bat.status.success());
    let add_exe = run_cli_with_config(&config, &["add", exe.to_str().unwrap()]);
    assert!(add_exe.status.success());

    let output = run_cli_with_config(&config, &["which", "program"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let resolved = PathBuf::from(stdout(&output).trim());
    assert_eq!(resolved, exe, "the .exe candidate wins for a bare query");

    let exact = run_cli_with_config(&config, &["which", "program.bat"]);
    let resolved = PathBuf::from(stdout(&exact).trim());
    assert_eq!(resolved, bat, "an exact name wins over completion");
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
fn which_resolves_config_entry() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "which-me.bat", "@echo off");
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    let output = run_cli_with_config(&config, &["which", "which-me.bat"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let path = PathBuf::from(stdout(&output).trim());
    assert_eq!(path, std::path::absolute(&source).unwrap());
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

// --- add ---

#[test]
fn add_custom_name() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "src.bat", "@echo off\necho renamed");

    let output = run_cli_with_config(
        &config,
        &["add", source.to_str().unwrap(), "--name", "renamed.bat"],
    );
    assert!(output.status.success());

    let run = run_cli_with_config(&config, &["run", "renamed.bat"]);
    assert!(run.status.success(), "stderr: {}", stderr(&run));
    assert!(stdout(&run).contains("renamed"));
}

#[test]
fn add_relative_source_creates_working_entry() {
    // The source is stored as an absolute path, so the registration works
    // regardless of the working directory used later.
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    write_tool(dir.path(), REL_ADD_TOOL, REL_ADD_BODY);

    let output = run_cli_opt(
        None,
        Some(&config),
        Some(dir.path()),
        &[],
        &["add", REL_ADD_TOOL],
    );
    assert!(output.status.success(), "stderr was: {}", stderr(&output));

    let run = run_cli_with_config(&config, &["run", REL_ADD_TOOL]);
    assert!(run.status.success(), "stderr was: {}", stderr(&run));
    assert!(stdout(&run).contains("added-rel"));

    // The stored path is absolute: `which` works from any cwd.
    let other = TempDir::new().unwrap();
    let which = run_cli_opt(
        None,
        Some(&config),
        Some(other.path()),
        &[],
        &["which", REL_ADD_TOOL],
    );
    assert!(which.status.success(), "stderr: {}", stderr(&which));
    assert!(
        PathBuf::from(stdout(&which).trim()).is_absolute(),
        "stored path should be absolute"
    );
}

#[test]
fn add_relative_source_with_subdirectory() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    write_tool(
        &dir.path().join("sub"),
        "nested.bat",
        "@echo off\necho nested",
    );

    let output = run_cli_opt(
        None,
        Some(&config),
        Some(dir.path()),
        &[],
        &["add", "sub/nested.bat"],
    );
    assert!(output.status.success(), "stderr was: {}", stderr(&output));

    let run = run_cli_with_config(&config, &["run", "nested.bat"]);
    assert!(run.status.success(), "stderr was: {}", stderr(&run));
    assert!(stdout(&run).contains("nested"));
}

#[test]
fn add_missing_source_fails() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let output = run_cli_with_config(&config, &["add", "definitely-missing-source"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("source not found"));
}

#[test]
fn add_invalid_name_fails() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "tool.bat", "@echo off");
    let output = run_cli_with_config(
        &config,
        &["add", source.to_str().unwrap(), "--name", "a/b.exe"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("invalid tool name"));
    assert!(!config.exists(), "nothing should be written on error");
}

#[test]
fn add_duplicate_fails_then_force_overwrites() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let first = write_tool(dir.path(), "dup.bat", "@echo off\necho first");
    let second = write_tool(dir.path(), "other.bat", "@echo off\necho second");

    let add = run_cli_with_config(&config, &["add", first.to_str().unwrap()]);
    assert!(add.status.success());

    let dup = run_cli_with_config(
        &config,
        &["add", second.to_str().unwrap(), "--name", "dup.bat"],
    );
    assert_eq!(dup.status.code(), Some(1), "duplicate name must fail");
    assert!(
        stderr(&dup).contains("already registered"),
        "stderr was: {}",
        stderr(&dup)
    );

    let forced = run_cli_with_config(
        &config,
        &[
            "add",
            second.to_str().unwrap(),
            "--name",
            "dup.bat",
            "--force",
        ],
    );
    assert!(forced.status.success(), "stderr was: {}", stderr(&forced));

    let run = run_cli_with_config(&config, &["run", "dup.bat"]);
    assert!(run.status.success());
    assert!(
        stdout(&run).contains("second"),
        "stdout was: {}",
        stdout(&run)
    );
}

// --- remove ---

#[test]
fn remove_registered_tool() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "plain.bat", "@echo off\necho plain");
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    let output = run_cli_with_config(&config, &["remove", "plain.bat"]);
    assert!(output.status.success(), "stderr was: {}", stderr(&output));
    assert!(
        stdout(&output).contains("removed"),
        "stdout was: {}",
        stdout(&output)
    );

    let run = run_cli_with_config(&config, &["run", "plain.bat"]);
    assert_eq!(run.status.code(), Some(127), "registration should be gone");
}

#[test]
fn remove_not_registered_exits_127() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "echo.bat", ECHO_BODY);
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    let output = run_cli_with_config(&config, &["remove", "echoe"]);
    assert_eq!(output.status.code(), Some(127));
    let err = stderr(&output);
    assert!(err.contains("not registered"), "stderr was: {err}");
    assert!(err.contains("did you mean"), "stderr was: {err}");
    assert!(err.contains("echo.bat"), "stderr was: {err}");
}

#[test]
fn remove_does_not_delete_bin_tool() {
    let toolbox = Toolbox::new();
    // `echo.bat` lives in the toolbox but is not registered: remove reports
    // it as not registered and leaves the file alone.
    let output = run_cli_in_toolbox(&toolbox, &["remove", "echo"]);
    assert_eq!(
        output.status.code(),
        Some(127),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        toolbox.path().join(ECHO_TOOL).is_file(),
        "manually placed toolbox entries are never deleted"
    );
}

#[cfg(windows)]
#[test]
fn remove_resolves_like_run() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let bat = write_tool(dir.path(), "program.bat", "@echo off\necho bat-ran");
    let exe = dir.path().join("program.exe");
    std::fs::write(&exe, "not really an exe; only resolved, never run").unwrap();
    let add_bat = run_cli_with_config(&config, &["add", bat.to_str().unwrap()]);
    assert!(add_bat.status.success());
    let add_exe = run_cli_with_config(&config, &["add", exe.to_str().unwrap()]);
    assert!(add_exe.status.success());

    // A bare `remove program` removes the entry a bare `run program` would
    // resolve to (the .exe registration).
    let output = run_cli_with_config(&config, &["remove", "program"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let which = run_cli_with_config(&config, &["which", "program"]);
    let resolved = PathBuf::from(stdout(&which).trim());
    assert_eq!(resolved, bat, "the .bat registration should remain");
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

// --- usage / bin dir / config resolution ---

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

#[test]
fn env_var_config_is_used() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "env-reg.bat", "@echo off");
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    // No `--config` flag: the RUN_CLI_CONFIG variable must take effect (a
    // scratch config would override it, so none is injected).
    let output = run_cli_opt(
        None,
        None,
        None,
        &[("RUN_CLI_CONFIG", config.to_str().unwrap())],
        &["list", "--json"],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout(&output)).unwrap();
    assert!(parsed.iter().any(|tool| tool["name"] == "env-reg.bat"));
}

#[test]
fn config_flag_overrides_env_var() {
    let dir = TempDir::new().unwrap();
    let flag_config = dir.path().join("flag.toml");
    let env_config = dir.path().join("env.toml");
    let flag_src = write_tool(dir.path(), "flag-reg.bat", "@echo off");
    let env_src = write_tool(dir.path(), "env-reg.bat", "@echo off");
    let add_flag = run_cli_with_config(&flag_config, &["add", flag_src.to_str().unwrap()]);
    assert!(add_flag.status.success());
    let add_env = run_cli_with_config(&env_config, &["add", env_src.to_str().unwrap()]);
    assert!(add_env.status.success());

    let output = run_cli_opt(
        None,
        Some(&flag_config),
        None,
        &[("RUN_CLI_CONFIG", env_config.to_str().unwrap())],
        &["list", "--json"],
    );
    assert!(output.status.success());
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout(&output)).unwrap();
    assert!(parsed.iter().any(|tool| tool["name"] == "flag-reg.bat"));
    assert!(!parsed.iter().any(|tool| tool["name"] == "env-reg.bat"));
}

#[test]
fn corrupt_config_is_an_error() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "not [ valid toml").unwrap();

    let list = run_cli_with_config(&config, &["list"]);
    assert_eq!(list.status.code(), Some(1));
    assert!(
        stderr(&list).contains("parse"),
        "stderr was: {}",
        stderr(&list)
    );

    let run = run_cli_with_config(&config, &["run", "any-tool"]);
    assert_eq!(run.status.code(), Some(1));
    assert!(
        stderr(&run).contains("parse"),
        "stderr was: {}",
        stderr(&run)
    );
}

#[test]
fn list_does_not_create_bin_dir() {
    let dir = TempDir::new().unwrap();
    let bin = dir.path().join("bin");
    let config = dir.path().join("config.toml");
    let source = write_tool(dir.path(), "only.bat", "@echo off");
    let add = run_cli_with_config(&config, &["add", source.to_str().unwrap()]);
    assert!(add.status.success());

    let output = run_cli_opt(Some(&bin), Some(&config), None, &[], &["list", "--json"]);
    assert!(output.status.success());
    assert!(
        !bin.exists(),
        "listing must not create the toolbox directory"
    );
}
