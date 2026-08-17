use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustState {
    #[serde(default)]
    trusted_workspaces: BTreeSet<String>,
}

pub struct TrustStore {
    path: PathBuf,
    state: TrustState,
}

impl TrustStore {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let state = if path.is_file() {
            let bytes = fs::read(&path).map_err(|error| error.to_string())?;
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?
        } else {
            TrustState::default()
        };
        Ok(Self { path, state })
    }

    #[must_use]
    pub fn is_trusted(&self, workspace: &Path) -> bool {
        self.state
            .trusted_workspaces
            .contains(&workspace.to_string_lossy().into_owned())
    }

    pub fn grant(&mut self, workspace: &Path) -> Result<(), String> {
        self.state
            .trusted_workspaces
            .insert(workspace.to_string_lossy().into_owned());
        self.persist()
    }

    pub fn revoke(&mut self, workspace: &Path) -> Result<(), String> {
        self.state
            .trusted_workspaces
            .remove(&workspace.to_string_lossy().into_owned());
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&self.state).map_err(|error| error.to_string())?;
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|error| error.to_string())?;
        }
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}
