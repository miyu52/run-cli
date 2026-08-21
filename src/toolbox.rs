//! The toolbox directory: location resolution, listing, tool resolution and
//! add/remove management.
//!
//! The toolbox is a directory holding runnable tools. Tool names are resolved
//! against it with platform extension completion (Windows: `.exe` -> `.bat`
//! -> `.cmd` -> `.ps1`, case-insensitive), including subdirectories. Absolute
//! paths are accepted as-is.
//!
//! Unless escape is allowed ([`Toolbox::resolve`] `allow_escape`), resolved
//! paths must stay inside the toolbox directory; paths that resolve outside
//! it (e.g. `..` components or absolute paths) are rejected with
//! [`ToolboxError::EscapeAttempted`].

use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

/// Default toolbox directory when neither the CLI flag nor the environment
/// variable is set.
pub const DEFAULT_BIN_DIR: &str = "bin";
/// Environment variable overriding the toolbox directory.
pub const ENV_BIN_DIR: &str = "RUN_CLI_BIN";

/// Tool extension candidates for bare-name resolution on Windows, in search
/// order.
#[cfg(windows)]
pub const TOOL_EXTENSIONS: &[&str] = &["exe", "bat", "cmd", "ps1"];

/// On other platforms bare names are matched exactly.
#[cfg(not(windows))]
pub const TOOL_EXTENSIONS: &[&str] = &[];

/// An entry of the toolbox directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tool {
    /// File name (or directory name) inside the toolbox.
    pub name: String,
    /// Full path of the entry.
    pub path: PathBuf,
    /// Whether the entry is a file or a directory.
    pub kind: ToolKind,
    /// Size in bytes for files; `None` for directories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// Whether a toolbox entry is a file or a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
}

/// How `add` materialized the tool inside the toolbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddOutcome {
    /// A symlink was created.
    Linked,
    /// A copy was made (symlinks unavailable).
    Copied,
}

/// Errors produced by toolbox operations.
#[derive(Debug, Error)]
pub enum ToolboxError {
    /// The toolbox directory does not exist.
    #[error("bin directory not found: {0}")]
    MissingDirectory(PathBuf),
    /// The toolbox path exists but is not a directory.
    #[error("bin path is not a directory: {0}")]
    NotADirectory(PathBuf),
    /// Reading the toolbox directory failed.
    #[error("failed to read bin directory {0}: {1}")]
    ReadError(PathBuf, #[source] std::io::Error),
    /// Canonicalizing a path failed.
    #[error("failed to canonicalize {0}: {1}")]
    CanonicalizeError(PathBuf, #[source] std::io::Error),
    /// The requested tool does not exist in the toolbox.
    #[error("tool not found in {0}: {1}")]
    ToolNotFound(PathBuf, String),
    /// An empty tool name was given.
    #[error("tool name must not be empty")]
    EmptyToolName,
    /// The resolved path lies outside the toolbox directory.
    #[error(
        "path '{0}' resolves outside the toolbox directory {1}; use --allow-escape to permit it"
    )]
    EscapeAttempted(String, PathBuf),
    /// An `add` name contains path separators or is otherwise invalid.
    #[error("invalid tool name '{0}': must be a single file name")]
    InvalidToolName(String),
    /// The source of an `add` does not exist.
    #[error("source not found: {0}")]
    AddSourceMissing(PathBuf),
    /// The destination of an `add` already exists.
    #[error("'{0}' already exists in the toolbox")]
    AlreadyExists(PathBuf),
    /// Creating the toolbox directory failed.
    #[error("failed to create bin directory {0}: {1}")]
    CreateBinDirError(PathBuf, #[source] std::io::Error),
    /// Copying a source into the toolbox failed.
    #[error("failed to copy {0} to {1}: {2}")]
    CopyError(PathBuf, PathBuf, #[source] std::io::Error),
    /// Removing an entry failed.
    #[error("failed to remove {0}: {1}")]
    RemoveError(PathBuf, #[source] std::io::Error),
    /// Removing a directory without `--recursive`.
    #[error("'{0}' is a directory; use --recursive to remove it")]
    RemoveDirectory(PathBuf),
}

/// A toolbox directory with its path-boundary policy.
#[derive(Debug, Clone)]
pub struct Toolbox {
    dir: PathBuf,
    allow_escape: bool,
}

/// Resolve the toolbox directory with priority: CLI arg > `RUN_CLI_BIN` >
/// `./bin`. An empty environment variable counts as unset.
pub fn resolve_bin_dir(cli_bin_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = cli_bin_dir {
        return dir.to_path_buf();
    }
    if let Some(dir) = env::var_os(ENV_BIN_DIR)
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    PathBuf::from(DEFAULT_BIN_DIR)
}

impl Toolbox {
    /// Resolve the toolbox directory and build a [`Toolbox`] around it.
    ///
    /// When `allow_escape` is true, resolved tool paths may point outside the
    /// toolbox directory; otherwise they must stay within it.
    pub fn resolve(cli_bin_dir: Option<&Path>, allow_escape: bool) -> Self {
        Toolbox {
            dir: resolve_bin_dir(cli_bin_dir),
            allow_escape,
        }
    }

    /// The toolbox directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether tool paths may resolve outside the toolbox directory.
    pub fn allow_escape(&self) -> bool {
        self.allow_escape
    }

    /// List the contents of the toolbox directory (files and directories,
    /// sorted by name). Symlinks are followed.
    pub fn list(&self) -> Result<Vec<Tool>, ToolboxError> {
        let entries = fs::read_dir(&self.dir).map_err(|e| match e.kind() {
            ErrorKind::NotFound => ToolboxError::MissingDirectory(self.dir.clone()),
            ErrorKind::NotADirectory => ToolboxError::NotADirectory(self.dir.clone()),
            _ => ToolboxError::ReadError(self.dir.clone(), e),
        })?;

        let mut tools: Vec<Tool> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                // fs::metadata follows symlinks (stat semantics), so tools
                // added as links still show up; DirEntry::metadata() does not
                // follow symlinks on Unix (lstat semantics).
                let path = entry.path();
                let metadata = fs::metadata(&path).ok()?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if metadata.is_file() {
                    Some(Tool {
                        name,
                        path,
                        kind: ToolKind::File,
                        size: Some(metadata.len()),
                    })
                } else if metadata.is_dir() {
                    Some(Tool {
                        name,
                        path,
                        kind: ToolKind::Directory,
                        size: None,
                    })
                } else {
                    None
                }
            })
            .collect();

        tools.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tools)
    }

    /// Names of all toolbox entries, in sorted order.
    pub fn list_names(&self) -> Result<Vec<String>, ToolboxError> {
        Ok(self.list()?.into_iter().map(|tool| tool.name).collect())
    }

    /// Resolve a tool name to its actual path.
    ///
    /// * Absolute paths are used as-is (subject to the containment check).
    /// * Names with an explicit extension resolve against the toolbox
    ///   directory, including subdirectories (`sub/tool.exe`).
    /// * Names without an extension are searched with the platform extension
    ///   candidates, falling back to the bare file name; subdirectories work
    ///   too (`sub/tool`).
    pub fn locate(&self, name: &str) -> Result<PathBuf, ToolboxError> {
        let candidate = self.find_candidate(name)?;
        self.ensure_within(&candidate)
    }

    /// Find the candidate path for `name` (extension completion included)
    /// without applying the containment check.
    fn find_candidate(&self, name: &str) -> Result<PathBuf, ToolboxError> {
        let tool = Path::new(name);
        if tool.as_os_str().is_empty() {
            return Err(ToolboxError::EmptyToolName);
        }

        if tool.is_absolute() || tool.extension().is_some() {
            let candidate = if tool.is_absolute() {
                tool.to_path_buf()
            } else {
                self.dir.join(tool)
            };
            if !candidate.is_file() {
                return Err(ToolboxError::ToolNotFound(
                    self.dir.clone(),
                    name.to_string(),
                ));
            }
            return Ok(candidate);
        }

        self.find_bare_candidate(tool, name)
    }

    fn find_bare_candidate(&self, relative: &Path, name: &str) -> Result<PathBuf, ToolboxError> {
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let file_name = relative.file_name().unwrap_or_default();

        for ext in TOOL_EXTENSIONS {
            let candidate = self.dir.join(parent).join(file_name).with_extension(ext);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }

        let bare = self.dir.join(relative);
        if bare.is_file() {
            return Ok(bare);
        }

        Err(ToolboxError::ToolNotFound(
            self.dir.clone(),
            name.to_string(),
        ))
    }

    fn ensure_within(&self, candidate: &Path) -> Result<PathBuf, ToolboxError> {
        if self.allow_escape {
            return Ok(candidate.to_path_buf());
        }
        let base = fs::canonicalize(&self.dir).map_err(|e| match e.kind() {
            ErrorKind::NotFound => ToolboxError::MissingDirectory(self.dir.clone()),
            _ => ToolboxError::CanonicalizeError(self.dir.clone(), e),
        })?;
        let canonical = fs::canonicalize(candidate)
            .map_err(|e| ToolboxError::CanonicalizeError(candidate.to_path_buf(), e))?;
        if canonical.starts_with(&base) {
            Ok(candidate.to_path_buf())
        } else {
            Err(ToolboxError::EscapeAttempted(
                display_path(&canonical),
                display_path(&base).into(),
            ))
        }
    }

    /// Add a file or directory to the toolbox.
    ///
    /// A symlink is created when possible; otherwise the source is copied
    /// (directories recursively). The toolbox directory is created when
    /// missing. `name` defaults to the source file name and must be a single
    /// file name (no path separators). Returns the outcome and the destination
    /// path.
    pub fn add(
        &self,
        source: &Path,
        name: Option<&str>,
    ) -> Result<(AddOutcome, PathBuf), ToolboxError> {
        if !source.exists() {
            return Err(ToolboxError::AddSourceMissing(source.to_path_buf()));
        }
        let name = match name {
            Some(name) => name.to_string(),
            None => source
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| ToolboxError::InvalidToolName(source.display().to_string()))?
                .to_string(),
        };
        validate_name(&name)?;

        let dest = self.dir.join(&name);
        if dest.exists() {
            return Err(ToolboxError::AlreadyExists(dest));
        }
        fs::create_dir_all(&self.dir)
            .map_err(|e| ToolboxError::CreateBinDirError(self.dir.clone(), e))?;

        match create_link(source, &dest) {
            Ok(()) => Ok((AddOutcome::Linked, dest)),
            Err(_) => copy_recursive(source, &dest)
                .map(|()| (AddOutcome::Copied, dest.clone()))
                .map_err(|e| ToolboxError::CopyError(source.to_path_buf(), dest, e)),
        }
    }

    /// Remove a tool from the toolbox.
    ///
    /// Files are resolved like [`Toolbox::locate`] (extension completion
    /// included). Directories are matched by exact name and require
    /// `recursive`. Paths may not escape the toolbox; a symlink entry whose
    /// target resolves outside is still removed as an entry (only the link is
    /// deleted, never its target).
    pub fn remove(&self, name: &str, recursive: bool) -> Result<PathBuf, ToolboxError> {
        match self.locate(name) {
            Ok(path) => {
                fs::remove_file(&path).map_err(|e| ToolboxError::RemoveError(path.clone(), e))?;
                return Ok(path);
            }
            Err(ToolboxError::EscapeAttempted(..)) => {
                // The entry exists inside the toolbox but its target resolves
                // outside; removing the entry itself never touches the target,
                // so delete the link. Paths not lexically inside the toolbox
                // (e.g. absolute outside paths) are still rejected.
                let candidate = self.find_candidate(name)?;
                if !self.is_lexically_within(&candidate) {
                    return Err(ToolboxError::EscapeAttempted(
                        display_path(&candidate),
                        display_path(&self.dir).into(),
                    ));
                }
                fs::remove_file(&candidate)
                    .map_err(|e| ToolboxError::RemoveError(candidate.clone(), e))?;
                return Ok(candidate);
            }
            Err(ToolboxError::ToolNotFound(..)) => {}
            Err(err) => return Err(err),
        }

        // Not a file: a directory (or directory symlink) by exact name.
        let relative = Path::new(name);
        if relative.is_absolute() {
            return Err(ToolboxError::ToolNotFound(
                self.dir.clone(),
                name.to_string(),
            ));
        }
        let candidate = self.dir.join(relative);
        let meta = match fs::symlink_metadata(&candidate) {
            Ok(meta) => meta,
            Err(_) => {
                return Err(ToolboxError::ToolNotFound(
                    self.dir.clone(),
                    name.to_string(),
                ));
            }
        };
        if meta.file_type().is_symlink() {
            if !recursive {
                return Err(ToolboxError::RemoveDirectory(candidate.clone()));
            }
            remove_link(&candidate).map_err(|e| ToolboxError::RemoveError(candidate.clone(), e))?;
            Ok(candidate)
        } else if meta.is_dir() {
            self.ensure_within(&candidate)?;
            if !recursive {
                return Err(ToolboxError::RemoveDirectory(candidate.clone()));
            }
            fs::remove_dir_all(&candidate)
                .map_err(|e| ToolboxError::RemoveError(candidate.clone(), e))?;
            Ok(candidate)
        } else {
            Err(ToolboxError::ToolNotFound(
                self.dir.clone(),
                name.to_string(),
            ))
        }
    }

    /// Whether `candidate` is lexically inside the toolbox directory (no
    /// parent traversal), without following symlinks.
    fn is_lexically_within(&self, candidate: &Path) -> bool {
        candidate.starts_with(&self.dir)
            && !candidate
                .components()
                .any(|c| matches!(c, Component::ParentDir))
    }
}

/// Display form of a canonicalized path; on Windows strips the `\\?\`
/// extended-length prefix `fs::canonicalize` produces.
fn display_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        text.strip_prefix(r"\\?\")
            .map(|stripped| stripped.to_string())
            .unwrap_or_else(|| text.into_owned())
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy().into_owned()
    }
}

fn validate_name(name: &str) -> Result<(), ToolboxError> {
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
        Err(ToolboxError::InvalidToolName(name.to_string()))
    }
}

#[cfg(unix)]
fn create_link(source: &Path, dest: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, dest)
}

#[cfg(windows)]
fn create_link(source: &Path, dest: &Path) -> std::io::Result<()> {
    let metadata = fs::metadata(source)?;
    if metadata.is_dir() {
        std::os::windows::fs::symlink_dir(source, dest)
    } else {
        std::os::windows::fs::symlink_file(source, dest)
    }
}

/// Remove a symlink entry without following it into its target.
#[cfg(unix)]
fn remove_link(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path)
}

#[cfg(windows)]
fn remove_link(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            // Directory symlinks require remove_dir; fall back for files.
            fs::remove_dir(path).or_else(|_| fs::remove_file(path))
        }
        _ => fs::remove_file(path),
    }
}

fn copy_recursive(source: &Path, dest: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(source, dest).map(|_| ())
    }
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

    fn temp_dir_with_tools(names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        for name in names {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, "").unwrap();
        }
        dir
    }

    fn toolbox(dir: &Path, allow_escape: bool) -> Toolbox {
        Toolbox::resolve(Some(dir), allow_escape)
    }

    #[test]
    fn bin_dir_priority_arg_over_env_and_default() {
        with_env(ENV_BIN_DIR, Some("env-dir"), || {
            let arg = PathBuf::from("arg-dir");
            assert_eq!(resolve_bin_dir(Some(&arg)), arg);
        });
    }

    #[test]
    fn bin_dir_priority_env_over_default() {
        with_env(ENV_BIN_DIR, Some("env-dir"), || {
            assert_eq!(resolve_bin_dir(None), PathBuf::from("env-dir"));
        });
    }

    #[test]
    fn bin_dir_falls_back_to_default() {
        with_env(ENV_BIN_DIR, None, || {
            assert_eq!(resolve_bin_dir(None), PathBuf::from(DEFAULT_BIN_DIR));
        });
    }

    #[test]
    fn bin_dir_empty_env_falls_back_to_default() {
        with_env(ENV_BIN_DIR, Some(""), || {
            assert_eq!(resolve_bin_dir(None), PathBuf::from(DEFAULT_BIN_DIR));
        });
    }

    #[test]
    fn list_returns_sorted_files_and_dirs() {
        let dir = temp_dir_with_tools(&["b.txt", "a.exe", "z.bat"]);
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir").join("nested.exe"), "").unwrap();

        let tools = toolbox(dir.path(), false).list().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["a.exe", "b.txt", "subdir", "z.bat"]);
        assert_eq!(tools[0].kind, ToolKind::File);
        assert_eq!(tools[0].size, Some(0));
        assert_eq!(tools[2].kind, ToolKind::Directory);
        assert_eq!(tools[2].size, None);
    }

    #[test]
    fn list_returns_names() {
        let dir = temp_dir_with_tools(&["b.exe", "a.exe"]);
        assert_eq!(
            toolbox(dir.path(), false).list_names().unwrap(),
            ["a.exe", "b.exe"]
        );
    }

    #[test]
    fn list_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(toolbox(dir.path(), false).list().unwrap().is_empty());
    }

    #[test]
    fn list_missing_dir() {
        let missing = PathBuf::from("does-not-exist");
        assert!(matches!(
            toolbox(&missing, false).list(),
            Err(ToolboxError::MissingDirectory(_))
        ));
    }

    #[test]
    fn list_non_directory() {
        let file = temp_dir_with_tools(&["file.txt"]);
        let path = file.path().join("file.txt");
        assert!(matches!(
            toolbox(&path, false).list(),
            Err(ToolboxError::NotADirectory(_))
        ));
    }

    #[test]
    fn locate_empty_name() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(matches!(
            toolbox(dir.path(), false).locate(""),
            Err(ToolboxError::EmptyToolName)
        ));
    }

    #[test]
    fn locate_missing() {
        let dir = temp_dir_with_tools(&["other.exe"]);
        assert!(matches!(
            toolbox(dir.path(), false).locate("nope"),
            Err(ToolboxError::ToolNotFound(_, _))
        ));
    }

    #[test]
    fn locate_explicit_extension() {
        let dir = temp_dir_with_tools(&["example.bat", "example.exe"]);
        let resolved = toolbox(dir.path(), false).locate("example.bat").unwrap();
        assert_eq!(resolved, dir.path().join("example.bat"));
    }

    #[test]
    fn locate_absolute_path() {
        let dir = temp_dir_with_tools(&["abs.exe"]);
        let abs = dir.path().join("abs.exe");
        let resolved = toolbox(dir.path(), false)
            .locate(&abs.to_string_lossy())
            .unwrap();
        assert_eq!(resolved, abs);
    }

    #[test]
    fn locate_subdirectory() {
        let dir = temp_dir_with_tools(&["sub/tool.exe"]);
        let resolved = toolbox(dir.path(), false).locate("sub/tool.exe").unwrap();
        assert_eq!(resolved, dir.path().join("sub/tool.exe"));
    }

    /// Bare-name resolution with extension completion is a Windows feature.
    #[cfg(windows)]
    #[test]
    fn locate_relative_path_inside_bin() {
        let dir = temp_dir_with_tools(&["sub/tool.exe"]);
        let resolved = toolbox(dir.path(), false).locate("sub/tool").unwrap();
        assert_eq!(resolved, dir.path().join("sub/tool.exe"));
    }

    #[test]
    fn locate_parent_dir_traversal_is_rejected() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(root.path().join("tool.exe"), "").unwrap();

        assert!(matches!(
            toolbox(&bin, false).locate("../tool.exe"),
            Err(ToolboxError::EscapeAttempted(_, _))
        ));
    }

    #[test]
    fn locate_absolute_path_outside_is_rejected() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let outside = root.path().join("outside.exe");
        std::fs::write(&outside, "").unwrap();

        assert!(matches!(
            toolbox(&bin, false).locate(&outside.to_string_lossy()),
            Err(ToolboxError::EscapeAttempted(_, _))
        ));
    }

    #[test]
    fn locate_escape_allowed_with_flag() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let outside = root.path().join("outside.exe");
        std::fs::write(&outside, "").unwrap();

        let resolved = toolbox(&bin, true)
            .locate(&outside.to_string_lossy())
            .unwrap();
        assert_eq!(resolved, outside);
    }

    #[test]
    fn locate_parent_dir_traversal_allowed_with_flag() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(root.path().join("tool.exe"), "").unwrap();

        let resolved = toolbox(&bin, true).locate("../tool.exe").unwrap();
        assert!(resolved.is_file());
    }

    #[test]
    fn add_default_name_creates_bin_dir() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::write(root.path().join("tool.exe"), "").unwrap();

        let (outcome, dest) = toolbox(&bin, false)
            .add(&root.path().join("tool.exe"), None)
            .unwrap();
        assert!(matches!(outcome, AddOutcome::Linked | AddOutcome::Copied));
        assert!(bin.join("tool.exe").is_file());
        assert_eq!(dest, bin.join("tool.exe"));
    }

    #[test]
    fn add_custom_name() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(root.path().join("source.exe"), "").unwrap();
        let toolbox = toolbox(&bin, false);

        toolbox
            .add(&root.path().join("source.exe"), Some("renamed.exe"))
            .unwrap();
        assert!(bin.join("renamed.exe").is_file());
    }

    #[test]
    fn add_missing_source() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        assert!(matches!(
            toolbox(&bin, false).add(&root.path().join("nope"), None),
            Err(ToolboxError::AddSourceMissing(_))
        ));
    }

    #[test]
    fn add_invalid_name() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(root.path().join("tool.exe"), "").unwrap();
        let toolbox = toolbox(&bin, false);

        assert!(matches!(
            toolbox.add(&root.path().join("tool.exe"), Some("a/b.exe")),
            Err(ToolboxError::InvalidToolName(_))
        ));
        assert!(matches!(
            toolbox.add(&root.path().join("tool.exe"), Some("..")),
            Err(ToolboxError::InvalidToolName(_))
        ));
    }

    #[test]
    fn add_already_exists() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(root.path().join("tool.exe"), "").unwrap();
        std::fs::write(root.path().join("other.exe"), "").unwrap();
        let toolbox = toolbox(&bin, false);

        toolbox.add(&root.path().join("tool.exe"), None).unwrap();
        assert!(matches!(
            toolbox.add(&root.path().join("other.exe"), Some("tool.exe")),
            Err(ToolboxError::AlreadyExists(_))
        ));
    }

    #[test]
    fn add_directory() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let source = root.path().join("scripts");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("inner.exe"), "").unwrap();
        let toolbox = toolbox(&bin, false);

        let (outcome, dest) = toolbox.add(&source, Some("scripts")).unwrap();
        assert!(matches!(outcome, AddOutcome::Linked | AddOutcome::Copied));
        assert_eq!(dest, bin.join("scripts"));
        assert!(bin.join("scripts").is_dir());
        if matches!(outcome, AddOutcome::Copied) {
            assert!(bin.join("scripts").join("inner.exe").is_file());
        }
    }

    /// Bare-name resolution with extension completion is a Windows feature.
    #[cfg(windows)]
    #[test]
    fn remove_file() {
        let dir = temp_dir_with_tools(&["tool.exe", "other.bat"]);
        let toolbox = toolbox(dir.path(), false);

        let removed = toolbox.remove("tool", false).unwrap();
        assert_eq!(removed, dir.path().join("tool.exe"));
        assert!(!dir.path().join("tool.exe").exists());
        assert!(dir.path().join("other.bat").exists());
    }

    #[test]
    fn remove_missing() {
        let dir = temp_dir_with_tools(&["tool.exe"]);
        assert!(matches!(
            toolbox(dir.path(), false).remove("nope", false),
            Err(ToolboxError::ToolNotFound(_, _))
        ));
    }

    #[test]
    fn remove_directory_requires_recursive() {
        let dir = temp_dir_with_tools(&[]);
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir").join("t.exe"), "").unwrap();
        let toolbox = toolbox(dir.path(), false);

        assert!(matches!(
            toolbox.remove("subdir", false),
            Err(ToolboxError::RemoveDirectory(_))
        ));
        toolbox.remove("subdir", true).unwrap();
        assert!(!dir.path().join("subdir").exists());
    }

    #[test]
    fn remove_escaping_path_is_rejected() {
        let root = tempfile::TempDir::new().unwrap();
        let bin = root.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(root.path().join("tool.exe"), "").unwrap();

        assert!(matches!(
            toolbox(&bin, false).remove("../tool.exe", false),
            Err(ToolboxError::EscapeAttempted(_, _))
        ));
    }

    #[cfg(unix)]
    mod remove_symlinks {
        use super::*;

        #[test]
        fn remove_file_symlink_to_outside_removes_link() {
            let root = tempfile::TempDir::new().unwrap();
            let bin = root.path().join("bin");
            std::fs::create_dir(&bin).unwrap();
            let outside = root.path().join("outside.exe");
            std::fs::write(&outside, "").unwrap();
            std::os::unix::fs::symlink(&outside, bin.join("link.exe")).unwrap();
            let toolbox = toolbox(&bin, false);

            let removed = toolbox.remove("link.exe", false).unwrap();
            assert_eq!(removed, bin.join("link.exe"));
            assert!(!bin.join("link.exe").exists(), "link should be removed");
            assert!(outside.exists(), "target must not be removed");
        }

        #[test]
        fn remove_dir_symlink_to_outside_removes_link() {
            let root = tempfile::TempDir::new().unwrap();
            let bin = root.path().join("bin");
            std::fs::create_dir(&bin).unwrap();
            let outside = root.path().join("outside-dir");
            std::fs::create_dir(&outside).unwrap();
            std::fs::write(outside.join("inner.txt"), "").unwrap();
            std::os::unix::fs::symlink(&outside, bin.join("subdir")).unwrap();
            let toolbox = toolbox(&bin, false);

            assert!(matches!(
                toolbox.remove("subdir", false),
                Err(ToolboxError::RemoveDirectory(_))
            ));

            let removed = toolbox.remove("subdir", true).unwrap();
            assert_eq!(removed, bin.join("subdir"));
            assert!(!bin.join("subdir").exists(), "link should be removed");
            assert!(
                outside.join("inner.txt").exists(),
                "target must not be removed"
            );
        }

        #[test]
        fn remove_absolute_outside_symlink_is_rejected() {
            let root = tempfile::TempDir::new().unwrap();
            let bin = root.path().join("bin");
            std::fs::create_dir(&bin).unwrap();
            let outside = root.path().join("outside.exe");
            std::fs::write(&outside, "").unwrap();
            std::os::unix::fs::symlink(&outside, bin.join("link.exe")).unwrap();

            assert!(matches!(
                toolbox(&bin, false).remove(&outside.to_string_lossy(), false),
                Err(ToolboxError::EscapeAttempted(_, _))
            ));
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::*;

        #[test]
        fn locate_bare_name_prefers_exe() {
            let dir = temp_dir_with_tools(&["example.bat", "example.exe", "example.ps1"]);
            let resolved = toolbox(dir.path(), false).locate("example").unwrap();
            assert_eq!(resolved, dir.path().join("example.exe"));
        }

        #[test]
        fn locate_bare_name_cmd_order() {
            let dir = temp_dir_with_tools(&["tool.bat", "tool.ps1"]);
            assert_eq!(
                toolbox(dir.path(), false).locate("tool").unwrap(),
                dir.path().join("tool.bat")
            );
        }

        #[test]
        fn locate_bare_name_ps1() {
            let dir = temp_dir_with_tools(&["tool.ps1"]);
            assert_eq!(
                toolbox(dir.path(), false).locate("tool").unwrap(),
                dir.path().join("tool.ps1")
            );
        }

        #[test]
        fn locate_bare_name_without_extension() {
            let dir = temp_dir_with_tools(&["tool"]);
            assert_eq!(
                toolbox(dir.path(), false).locate("tool").unwrap(),
                dir.path().join("tool")
            );
        }

        #[test]
        fn locate_case_insensitive_input() {
            let dir = temp_dir_with_tools(&["Example.EXE"]);
            let resolved = toolbox(dir.path(), false).locate("example.exe").unwrap();
            assert!(resolved.is_file());
            let name = resolved.file_name().unwrap().to_string_lossy();
            assert!(
                name.eq_ignore_ascii_case("Example.EXE"),
                "resolved to wrong file: {name}"
            );
        }

        #[test]
        fn locate_case_insensitive_bare() {
            let dir = temp_dir_with_tools(&["TOOL.BAT"]);
            let resolved = toolbox(dir.path(), false).locate("tool").unwrap();
            assert!(resolved.is_file());
            let name = resolved.file_name().unwrap().to_string_lossy();
            assert!(
                name.eq_ignore_ascii_case("TOOL.BAT"),
                "resolved to wrong file: {name}"
            );
        }
    }

    #[cfg(not(windows))]
    mod unix {
        use super::*;

        #[test]
        fn locate_bare_name_no_extension_search() {
            let dir = temp_dir_with_tools(&["tool"]);
            assert_eq!(
                toolbox(dir.path(), false).locate("tool").unwrap(),
                dir.path().join("tool")
            );
        }

        #[test]
        fn locate_explicit_extension_missing() {
            let dir = temp_dir_with_tools(&["tool"]);
            assert!(matches!(
                toolbox(dir.path(), false).locate("tool.exe"),
                Err(ToolboxError::ToolNotFound(_, _))
            ));
        }

        #[test]
        fn locate_bare_name_in_subdirectory() {
            let dir = temp_dir_with_tools(&["sub/tool"]);
            let resolved = toolbox(dir.path(), false).locate("sub/tool").unwrap();
            assert_eq!(resolved, dir.path().join("sub/tool"));
        }

        #[test]
        fn remove_file_by_exact_name() {
            let dir = temp_dir_with_tools(&["tool.exe", "other.bat"]);
            let toolbox = toolbox(dir.path(), false);

            let removed = toolbox.remove("tool.exe", false).unwrap();
            assert_eq!(removed, dir.path().join("tool.exe"));
            assert!(!dir.path().join("tool.exe").exists());
            assert!(dir.path().join("other.bat").exists());
        }
    }
}
