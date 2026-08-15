use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("workspace I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("path must be relative to the workspace: {0}")]
    AbsolutePath(PathBuf),

    #[error("path traversal is not allowed: {0}")]
    Traversal(PathBuf),

    #[error("path escapes the workspace: {0}")]
    Escape(PathBuf),

    #[error("file exceeds the {limit}-byte limit: {path}")]
    TooLarge { path: PathBuf, limit: usize },

    #[error("file is not valid UTF-8: {0}")]
    InvalidUtf8(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = fs::canonicalize(root)?;
        Ok(Self { root })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve_existing(&self, relative: impl AsRef<Path>) -> Result<PathBuf, WorkspaceError> {
        let relative = validate_relative(relative.as_ref())?;
        let path = fs::canonicalize(self.root.join(relative))?;
        self.ensure_inside(&path)?;
        Ok(path)
    }

    pub fn resolve_for_write(&self, relative: impl AsRef<Path>) -> Result<PathBuf, WorkspaceError> {
        let relative = validate_relative(relative.as_ref())?;
        let candidate = self.root.join(relative);
        let mut ancestor = candidate.as_path();

        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| WorkspaceError::Escape(candidate.clone()))?;
        }

        let canonical_ancestor = fs::canonicalize(ancestor)?;
        self.ensure_inside(&canonical_ancestor)?;
        Ok(candidate)
    }

    pub fn read_text(
        &self,
        relative: impl AsRef<Path>,
        max_bytes: usize,
    ) -> Result<String, WorkspaceError> {
        let path = self.resolve_existing(relative)?;
        let metadata = fs::metadata(&path)?;
        if metadata.len() > max_bytes as u64 {
            return Err(WorkspaceError::TooLarge {
                path,
                limit: max_bytes,
            });
        }

        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(&path)?
            .take(max_bytes.saturating_add(1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(WorkspaceError::TooLarge {
                path,
                limit: max_bytes,
            });
        }

        String::from_utf8(bytes).map_err(|_| WorkspaceError::InvalidUtf8(path))
    }

    pub fn write_text(
        &self,
        relative: impl AsRef<Path>,
        content: &str,
        max_bytes: usize,
    ) -> Result<PathBuf, WorkspaceError> {
        let relative = relative.as_ref();
        if content.len() > max_bytes {
            return Err(WorkspaceError::TooLarge {
                path: relative.to_path_buf(),
                limit: max_bytes,
            });
        }

        let path = self.resolve_for_write(relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            let canonical_parent = fs::canonicalize(parent)?;
            self.ensure_inside(&canonical_parent)?;
        }
        fs::write(&path, content)?;
        Ok(path)
    }

    pub fn list_files(
        &self,
        relative: impl AsRef<Path>,
        max_entries: usize,
    ) -> Result<Vec<String>, WorkspaceError> {
        let path = self.resolve_existing(relative)?;
        let mut entries = Vec::new();
        for entry in WalkDir::new(path).follow_links(false) {
            let entry = entry.map_err(io::Error::other)?;
            if entry.file_type().is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(&self.root)
                    .map_err(io::Error::other)?;
                entries.push(relative.to_string_lossy().into_owned());
                if entries.len() >= max_entries {
                    break;
                }
            }
        }
        entries.sort();
        Ok(entries)
    }

    fn ensure_inside(&self, path: &Path) -> Result<(), WorkspaceError> {
        if path.starts_with(&self.root) {
            Ok(())
        } else {
            Err(WorkspaceError::Escape(path.to_path_buf()))
        }
    }
}

fn validate_relative(path: &Path) -> Result<&Path, WorkspaceError> {
    if path.is_absolute() {
        return Err(WorkspaceError::AbsolutePath(path.to_path_buf()));
    }

    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)) {
            return Err(WorkspaceError::Traversal(path.to_path_buf()));
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{Workspace, WorkspaceError};

    #[test]
    fn rejects_parent_traversal() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile_directory()?;
        let workspace = Workspace::open(&directory)?;
        assert!(matches!(
            workspace.resolve_for_write("../outside"),
            Err(WorkspaceError::Traversal(_))
        ));
        Ok(())
    }

    fn tempfile_directory() -> Result<std::path::PathBuf, std::io::Error> {
        let path = std::env::temp_dir().join(format!("pire-core-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path)?;
        Ok(path)
    }
}
