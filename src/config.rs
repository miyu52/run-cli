//! The toolbox config file: named registrations of external tools.
//!
//! A config entry maps a registered name to an absolute path. `add` records
//! entries here instead of materializing anything inside the toolbox
//! directory, and `remove` unregisters entries without touching any file.
//!
//! The config location is independent of the toolbox directory, resolved
//! with the priority CLI argument `--config` > environment variable
//! `RUN_CLI_CONFIG` > platform default (`config.toml` next to the run-cli
//! executable on Windows, `~/.run-cli/config.toml` on Unix). A missing config
//! file is an empty registry; a malformed one is a hard error for every
//! command that reads it.
//!
//! Name lookup mirrors the toolbox extension completion (see
//! [`crate::toolbox`]): a bare query is matched against the platform
//! extension candidates in order and then the bare name itself, while a query
//! with an extension matches exactly. Matching is case-insensitive on Windows
//! (NTFS), exact elsewhere.

use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::toolbox::TOOL_EXTENSIONS;

/// Environment variable overriding the config file location.
pub const ENV_CONFIG: &str = "RUN_CLI_CONFIG";
/// File name of the default config file.
pub const DEFAULT_CONFIG_FILE: &str = "config.toml";

/// A single registered tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigTool {
    /// Registered name, a single file name component.
    pub name: String,
    /// Absolute path the name resolves to.
    pub path: PathBuf,
}

/// The toolbox config file contents.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Registered tools, in file order.
    pub tools: Vec<ConfigTool>,
}

impl Config {
    /// Load the config file; a missing file is an empty registry.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| ConfigError::ParseError(path.to_path_buf(), Box::new(e))),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError::ReadError(path.to_path_buf(), e)),
        }
    }

    /// Persist the config atomically (write a `.tmp` sibling, then rename),
    /// creating the parent directory when missing.
    pub fn save_atomic(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .map_err(|e| ConfigError::WriteError(path.to_path_buf(), e))?;
        }
        let text = toml::to_string(self)
            .map_err(|e| ConfigError::SerializeError(path.to_path_buf(), Box::new(e)))?;
        let mut tmp = path.as_os_str().to_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, text).map_err(|e| ConfigError::WriteError(path.to_path_buf(), e))?;
        #[cfg(windows)]
        {
            // Windows `fs::rename` fails when the destination exists; removing
            // it first leaves a tiny non-atomic window, acceptable for a CLI
            // config file.
            let _ = fs::remove_file(path);
        }
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(ConfigError::WriteError(path.to_path_buf(), e));
        }
        Ok(())
    }

    /// Resolve a query to a registered tool, mirroring the toolbox
    /// extension completion (bare queries try the platform extension
    /// candidates in order, then the bare name; queries with an extension
    /// match exactly).
    pub fn lookup(&self, name: &str) -> Option<&ConfigTool> {
        self.index_of(name).map(|index| &self.tools[index])
    }

    /// Insert or replace the entry whose name `tool.name` resolves to.
    pub fn upsert(&mut self, tool: ConfigTool) {
        match self.index_of(&tool.name) {
            Some(index) => self.tools[index] = tool,
            None => self.tools.push(tool),
        }
    }

    /// Remove the entry the name resolves to, returning it.
    pub fn remove(&mut self, name: &str) -> Option<ConfigTool> {
        let index = self.index_of(name)?;
        Some(self.tools.remove(index))
    }

    /// All registered names, sorted.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.iter().map(|tool| tool.name.clone()).collect();
        names.sort();
        names
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        candidate_names(name).iter().find_map(|candidate| {
            self.tools
                .iter()
                .position(|tool| names_equal(&tool.name, candidate))
        })
    }
}

/// The candidate names a query resolves against, in lookup order: the
/// platform extension candidates when the query is bare, then the bare name
/// itself. Queries with an extension match exactly. Mirrors the toolbox
/// resolution order (see `toolbox::find_bare_candidate`).
fn candidate_names(query: &str) -> Vec<String> {
    if Path::new(query).extension().is_some() {
        return vec![query.to_string()];
    }
    #[cfg(windows)]
    {
        let mut names: Vec<String> = TOOL_EXTENSIONS
            .iter()
            .map(|ext| format!("{query}.{ext}"))
            .collect();
        names.push(query.to_string());
        names
    }
    #[cfg(not(windows))]
    {
        let _ = TOOL_EXTENSIONS;
        vec![query.to_string()]
    }
}

fn names_equal(a: &str, b: &str) -> bool {
    #[cfg(windows)]
    {
        a.eq_ignore_ascii_case(b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// Resolve the config file path with priority: CLI argument > `RUN_CLI_CONFIG`
/// > platform default. An empty environment variable counts as unset.
pub fn resolve_config_path(cli_config: Option<&Path>) -> PathBuf {
    if let Some(path) = cli_config {
        return path.to_path_buf();
    }
    if let Some(path) = env::var_os(ENV_CONFIG)
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    default_config_path()
}

/// The default config file path: `config.toml` next to the run-cli executable
/// on Windows, `~/.run-cli/config.toml` on Unix. Falls back to `./config.toml`
/// when the executable path or home directory cannot be determined.
fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        match env::current_exe() {
            Ok(exe) => exe
                .parent()
                .map(|dir| dir.join(DEFAULT_CONFIG_FILE))
                .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_FILE)),
            Err(_) => PathBuf::from(DEFAULT_CONFIG_FILE),
        }
    }
    #[cfg(not(windows))]
    {
        match env::var_os("HOME") {
            Some(home) if !home.is_empty() => PathBuf::from(home)
                .join(".run-cli")
                .join(DEFAULT_CONFIG_FILE),
            _ => PathBuf::from(DEFAULT_CONFIG_FILE),
        }
    }
}

/// Validate a registered name: a single file name component (no path
/// separators, no `.` / `..`).
pub fn validate_name(name: &str) -> Result<(), ConfigError> {
    let path = Path::new(name);
    let valid = !name.is_empty()
        && path.components().count() == 1
        && !matches!(
            path.components().next(),
            Some(Component::CurDir | Component::ParentDir)
        );
    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidToolName(name.to_string()))
    }
}

/// Errors produced by config operations.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Reading the config file failed.
    #[error("failed to read config {0}: {1}")]
    ReadError(PathBuf, #[source] std::io::Error),
    /// Writing the config file failed.
    #[error("failed to write config {0}: {1}")]
    WriteError(PathBuf, #[source] std::io::Error),
    /// The config file is not valid TOML.
    #[error("failed to parse config {0}: {1}")]
    ParseError(PathBuf, #[source] Box<toml::de::Error>),
    /// Serializing the config failed.
    #[error("failed to serialize config {0}: {1}")]
    SerializeError(PathBuf, #[source] Box<toml::ser::Error>),
    /// Making a path absolute failed.
    #[error("failed to resolve absolute path for {0}: {1}")]
    AbsolutePathError(PathBuf, #[source] std::io::Error),
    /// An add name is not a single file name.
    #[error("invalid tool name '{0}': must be a single file name")]
    InvalidToolName(String),
    /// The source of an add does not exist.
    #[error("source not found: {0}")]
    SourceMissing(PathBuf),
    /// The source of an add is not a file (directories are not supported).
    #[error("source is not a file: {0}")]
    SourceNotFile(PathBuf),
    /// The name is already registered and `--force` was not given.
    #[error("tool '{0}' is already registered; use --force to overwrite")]
    DuplicateName(String),
    /// A registered tool's path no longer exists.
    #[error("registered tool '{name}' path not found: {path}")]
    RegisteredPathMissing {
        /// The registered name.
        name: String,
        /// The stored path.
        path: PathBuf,
    },
    /// A registered tool's path exists but is not a file.
    #[error("registered tool '{name}' is not a file: {path}")]
    RegisteredPathNotFile {
        /// The registered name.
        name: String,
        /// The stored path.
        path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env<F>(key: &str, value: Option<&str>, f: F)
    where
        F: FnOnce(),
    {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe { env::remove_var(key) };
        if let Some(value) = value {
            unsafe { env::set_var(key, value) };
        }
        f();
    }

    fn config_with(entries: &[(&str, &str)]) -> Config {
        Config {
            tools: entries
                .iter()
                .map(|(name, path)| ConfigTool {
                    name: name.to_string(),
                    path: PathBuf::from(path),
                })
                .collect(),
        }
    }

    #[test]
    fn load_missing_file_is_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = Config::load(&dir.path().join("missing.toml")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn load_invalid_toml_is_parse_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not [ valid toml").unwrap();
        assert!(matches!(
            Config::load(&path),
            Err(ConfigError::ParseError(_, _))
        ));
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let config = config_with(&[("echo", "/usr/bin/echo"), ("clang", "C:\\LLVM\\clang.exe")]);
        config.save_atomic(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn save_creates_parent_directories() {
        let root = tempfile::TempDir::new().unwrap();
        let path = root.path().join("nested").join("dir").join("config.toml");
        let config = config_with(&[("echo", "/usr/bin/echo")]);
        config.save_atomic(&path).unwrap();
        assert!(Config::load(&path).unwrap().tools[0].name == "echo");
    }

    #[test]
    fn save_overwrites_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        config_with(&[("a", "/x")]).save_atomic(&path).unwrap();
        config_with(&[("b", "/y")]).save_atomic(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config_with(&[("b", "/y")]));
    }

    #[test]
    fn lookup_exact_name() {
        let config = config_with(&[("echo", "/usr/bin/echo"), ("echo.bat", "C:\\x\\echo.bat")]);
        // A bare query is exact on Unix, but prefers the extension candidates
        // over the bare name on Windows (mirroring the toolbox resolution).
        #[cfg(windows)]
        let expected = Some(Path::new("C:\\x\\echo.bat"));
        #[cfg(not(windows))]
        let expected = Some(Path::new("/usr/bin/echo"));
        assert_eq!(config.lookup("echo").map(|t| t.path.as_path()), expected);
    }

    #[test]
    fn lookup_query_with_extension_is_exact() {
        let config = config_with(&[("echo", "/usr/bin/echo"), ("echo.bat", "C:\\x\\echo.bat")]);
        assert_eq!(
            config.lookup("echo.bat").map(|t| t.path.as_path()),
            Some(Path::new("C:\\x\\echo.bat"))
        );
        assert_eq!(config.lookup("echo.exe"), None);
    }

    #[test]
    fn lookup_bare_name_falls_back_to_bare_entry() {
        let config = config_with(&[("tool", "/usr/bin/tool")]);
        assert_eq!(
            config.lookup("tool").map(|t| t.path.as_path()),
            Some(Path::new("/usr/bin/tool"))
        );
    }

    #[test]
    fn lookup_missing() {
        let config = config_with(&[("echo", "/usr/bin/echo")]);
        assert_eq!(config.lookup("nope"), None);
    }

    /// Extension completion in the config mirrors the Windows toolbox
    /// resolution; bare names never match on other platforms.
    #[cfg(windows)]
    mod windows {
        use super::*;

        #[test]
        fn lookup_bare_name_tries_extension_candidates() {
            let config = config_with(&[("program.bat", "C:\\x\\program.bat")]);
            assert_eq!(
                config.lookup("program").map(|t| t.path.as_path()),
                Some(Path::new("C:\\x\\program.bat"))
            );
        }

        #[test]
        fn lookup_bare_name_prefers_exe() {
            let config = config_with(&[
                ("program.bat", "C:\\x\\program.bat"),
                ("program.exe", "C:\\x\\program.exe"),
                ("program.ps1", "C:\\x\\program.ps1"),
            ]);
            assert_eq!(
                config.lookup("program").map(|t| t.path.as_path()),
                Some(Path::new("C:\\x\\program.exe"))
            );
        }

        #[test]
        fn lookup_is_case_insensitive() {
            let config = config_with(&[("PROGRAM.EXE", "C:\\x\\program.exe")]);
            assert_eq!(
                config.lookup("program.exe").map(|t| t.path.as_path()),
                Some(Path::new("C:\\x\\program.exe"))
            );
            assert_eq!(
                config.lookup("program").map(|t| t.path.as_path()),
                Some(Path::new("C:\\x\\program.exe"))
            );
        }

        #[test]
        fn remove_resolves_like_lookup() {
            let mut config = config_with(&[
                ("program.bat", "C:\\x\\program.bat"),
                ("program.exe", "C:\\x\\program.exe"),
            ]);
            let removed = config.remove("program").unwrap();
            assert_eq!(removed.name, "program.exe");
            assert_eq!(config.lookup("program.exe"), None);
            assert_eq!(
                config.lookup("program").map(|t| t.path.as_path()),
                Some(Path::new("C:\\x\\program.bat"))
            );
        }
    }

    #[test]
    fn upsert_adds_new_entry() {
        let mut config = Config::default();
        config.upsert(ConfigTool {
            name: "echo".to_string(),
            path: PathBuf::from("/usr/bin/echo"),
        });
        assert_eq!(config.tools.len(), 1);
        assert_eq!(
            config.lookup("echo").unwrap().path,
            PathBuf::from("/usr/bin/echo")
        );
    }

    #[test]
    fn upsert_replaces_matching_entry() {
        let mut config = config_with(&[("program", "/old")]);
        config.upsert(ConfigTool {
            name: "program".to_string(),
            path: PathBuf::from("/new"),
        });
        assert_eq!(config.tools.len(), 1);
        assert_eq!(
            config.lookup("program").unwrap().path,
            PathBuf::from("/new")
        );
    }

    #[test]
    fn names_are_sorted() {
        let config = config_with(&[("b", "/b"), ("a", "/a"), ("c", "/c")]);
        assert_eq!(config.names(), ["a", "b", "c"]);
    }

    #[test]
    fn validate_name_rejects_invalid() {
        assert!(validate_name("tool.exe").is_ok());
        assert!(validate_name("tool").is_ok());
        assert!(matches!(
            validate_name(""),
            Err(ConfigError::InvalidToolName(_))
        ));
        assert!(matches!(
            validate_name("a/b.exe"),
            Err(ConfigError::InvalidToolName(_))
        ));
        assert!(matches!(
            validate_name(".."),
            Err(ConfigError::InvalidToolName(_))
        ));
        assert!(matches!(
            validate_name("."),
            Err(ConfigError::InvalidToolName(_))
        ));
    }

    #[test]
    fn config_path_priority_arg_over_env_and_default() {
        with_env(ENV_CONFIG, Some("env.toml"), || {
            let arg = PathBuf::from("arg.toml");
            assert_eq!(resolve_config_path(Some(&arg)), arg);
        });
    }

    #[test]
    fn config_path_priority_env_over_default() {
        with_env(ENV_CONFIG, Some("env.toml"), || {
            assert_eq!(resolve_config_path(None), PathBuf::from("env.toml"));
        });
    }

    #[test]
    fn config_path_empty_env_falls_back_to_default() {
        with_env(ENV_CONFIG, Some(""), || {
            assert_eq!(resolve_config_path(None), default_config_path());
        });
    }

    /// The Windows default config lives next to the run-cli executable.
    #[cfg(windows)]
    #[test]
    fn default_config_path_is_next_to_executable() {
        let exe = env::current_exe().unwrap();
        let expected = exe.parent().unwrap().join(DEFAULT_CONFIG_FILE);
        assert_eq!(default_config_path(), expected);
    }

    /// The Unix default config lives under the user's home directory.
    #[cfg(not(windows))]
    #[test]
    fn default_config_path_is_under_home() {
        let home = env::var_os("HOME").expect("HOME is set in the test environment");
        let expected = PathBuf::from(home)
            .join(".run-cli")
            .join(DEFAULT_CONFIG_FILE);
        assert_eq!(default_config_path(), expected);
    }
}
