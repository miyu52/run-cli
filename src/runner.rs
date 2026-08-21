//! Process execution: how a resolved tool is launched and how its exit code
//! is propagated.
//!
//! Note: exit codes are propagated through `u8` (the platform limit for
//! process exit codes on most systems); codes outside that range are
//! truncated.

use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

/// How to launch a resolved tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Spawn the file directly.
    Direct(PathBuf),
    /// Run the file through PowerShell (Windows `.ps1` scripts cannot be
    /// spawned directly by the OS).
    #[cfg(windows)]
    PowerShell(PathBuf),
}

impl Invocation {
    /// Build an invocation from a resolved tool path.
    pub fn from_path(path: PathBuf) -> Self {
        #[cfg(windows)]
        {
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
            {
                return Invocation::PowerShell(path);
            }
        }
        Invocation::Direct(path)
    }

    /// The resolved tool path.
    pub fn path(&self) -> &Path {
        match self {
            Invocation::Direct(path) => path,
            #[cfg(windows)]
            Invocation::PowerShell(path) => path,
        }
    }

    fn build_command(&self, args: &[String]) -> Command {
        match self {
            Invocation::Direct(path) => {
                let mut cmd = Command::new(path);
                cmd.args(args);
                cmd
            }
            #[cfg(windows)]
            Invocation::PowerShell(path) => {
                let mut cmd = Command::new("powershell");
                cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
                cmd.arg(path);
                cmd.args(args);
                cmd
            }
        }
    }
}

/// Errors produced while launching or waiting for a tool.
#[derive(Debug, Error)]
pub enum RunError {
    /// The tool could not be spawned.
    #[error("failed to spawn {0}: {1}")]
    Spawn(PathBuf, #[source] std::io::Error),
    /// Waiting for the tool failed.
    #[error("failed to wait for {0}: {1}")]
    Wait(PathBuf, #[source] std::io::Error),
    /// A `--env` value is not of the form `KEY=VALUE`.
    #[error("invalid --env value '{0}'; expected KEY=VALUE")]
    InvalidEnv(String),
}

/// Options controlling how a tool is executed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// Working directory for the child process.
    pub cwd: Option<PathBuf>,
    /// Environment variables to set for the child process.
    pub env: Vec<(String, String)>,
}

/// Parse `--env` values of the form `KEY=VALUE`.
pub fn parse_env_pairs(values: &[String]) -> Result<Vec<(String, String)>, RunError> {
    values
        .iter()
        .map(|value| {
            value
                .split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .ok_or_else(|| RunError::InvalidEnv(value.clone()))
        })
        .collect()
}

/// Run the tool with the given arguments, inheriting stdio, and return its
/// exit code.
pub fn execute(
    invocation: &Invocation,
    args: &[String],
    options: &RunOptions,
) -> Result<i32, RunError> {
    let mut command = invocation.build_command(args);
    if let Some(cwd) = &options.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in &options.env {
        command.env(key, value);
    }
    let status = command
        .status()
        .map_err(|e| RunError::Spawn(invocation.path().to_path_buf(), e))?;
    Ok(exit_code(status))
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_path_direct_for_exe() {
        let invocation = Invocation::from_path(PathBuf::from("example.exe"));
        assert_eq!(invocation, Invocation::Direct(PathBuf::from("example.exe")));
    }

    #[test]
    fn from_path_direct_for_bat() {
        let invocation = Invocation::from_path(PathBuf::from("example.bat"));
        assert_eq!(invocation, Invocation::Direct(PathBuf::from("example.bat")));
    }

    #[cfg(windows)]
    #[test]
    fn from_path_powershell_for_ps1() {
        let invocation = Invocation::from_path(PathBuf::from("example.ps1"));
        assert_eq!(
            invocation,
            Invocation::PowerShell(PathBuf::from("example.ps1"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn from_path_powershell_for_ps1_case_insensitive() {
        let invocation = Invocation::from_path(PathBuf::from("example.PS1"));
        assert_eq!(
            invocation,
            Invocation::PowerShell(PathBuf::from("example.PS1"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn execute_passes_through_exit_code() {
        let invocation = Invocation::Direct(PathBuf::from("cmd.exe"));
        let code = execute(
            &invocation,
            &["/C".into(), "exit 42".into()],
            &RunOptions::default(),
        )
        .unwrap();
        assert_eq!(code, 42);
    }

    #[cfg(not(windows))]
    #[test]
    fn execute_passes_through_exit_code() {
        let invocation = Invocation::Direct(PathBuf::from("sh"));
        let code = execute(
            &invocation,
            &["-c".into(), "exit 42".into()],
            &RunOptions::default(),
        )
        .unwrap();
        assert_eq!(code, 42);
    }

    #[test]
    fn execute_spawn_error_for_missing_binary() {
        let invocation = Invocation::Direct(PathBuf::from("run-cli-definitely-missing-binary-xyz"));
        assert!(matches!(
            execute(&invocation, &[], &RunOptions::default()),
            Err(RunError::Spawn(_, _))
        ));
    }

    #[test]
    fn parse_env_pairs_ok() {
        let parsed = parse_env_pairs(&["A=1".into(), "B=x=y".into()]).unwrap();
        assert_eq!(
            parsed,
            [
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "x=y".to_string())
            ]
        );
    }

    #[test]
    fn parse_env_pairs_invalid() {
        assert!(matches!(
            parse_env_pairs(&["NO_EQUALS".into()]),
            Err(RunError::InvalidEnv(_))
        ));
    }
}
