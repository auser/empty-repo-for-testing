use std::{collections::BTreeSet, fs, io, path::{Path, PathBuf}};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustDocument {
    #[serde(default)]
    workspaces: BTreeSet<String>,
}

#[derive(Debug, Error)]
pub enum TrustError {
    #[error("trust store I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("trust store JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct TrustStore {
    path: PathBuf,
    document: TrustDocument,
}

impl TrustStore {
    pub fn load(path: PathBuf) -> Result<Self, TrustError> {
        let document = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => TrustDocument::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Self { path, document })
    }

    #[must_use]
    pub fn contains(&self, workspace: &Path) -> bool {
        self.document
            .workspaces
            .contains(&workspace.to_string_lossy().into_owned())
    }

    pub fn grant(&mut self, workspace: &Path) -> Result<(), TrustError> {
        self.document
            .workspaces
            .insert(workspace.to_string_lossy().into_owned());
        self.save()
    }

    pub fn revoke(&mut self, workspace: &Path) -> Result<(), TrustError> {
        self.document
            .workspaces
            .remove(&workspace.to_string_lossy().into_owned());
        self.save()
    }

    fn save(&self) -> Result<(), TrustError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.document)?;
        fs::write(&self.path, bytes)?;
        Ok(())
    }
}
