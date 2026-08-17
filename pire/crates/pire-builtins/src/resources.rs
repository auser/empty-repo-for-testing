use std::{
    fs::OpenOptions,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use pire_core::{Plugin, PluginMetadata, Registry, Resource, ResourceResolver, ResourceUri};

pub struct FileResourcePlugin {
    workspace: PathBuf,
}

impl FileResourcePlugin {
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl Plugin for FileResourcePlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "pire.resources.file",
            env!("CARGO_PKG_VERSION"),
            "workspace-confined file:// resource resolver",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        registry
            .register_resource(Arc::new(FileResourceResolver {
                workspace: self.workspace.clone(),
            }))
            .map_err(|error| error.to_string())
    }
}

struct FileResourceResolver {
    workspace: PathBuf,
}

impl ResourceResolver for FileResourceResolver {
    fn id(&self) -> &str {
        "file"
    }

    fn schemes(&self) -> &[&str] {
        &["file"]
    }

    fn read(&self, uri: &ResourceUri, max_bytes: usize) -> Result<Resource, String> {
        if uri.scheme() != "file" {
            return Err(format!("unsupported resource scheme `{}`", uri.scheme()));
        }
        let path = secure_existing_path(&self.workspace, uri.path())?;
        let mut file = OpenOptions::new()
            .read(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        let limit = u64::try_from(max_bytes)
            .map_err(|_| "resource byte limit is too large".to_owned())?
            .saturating_add(1);
        let mut bytes = Vec::new();
        file.take(limit)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() > max_bytes {
            return Err("resource exceeds the configured byte limit".to_owned());
        }
        let content = String::from_utf8(bytes).map_err(|error| error.to_string())?;
        Ok(Resource {
            uri: uri.clone(),
            media_type: media_type(&path).to_owned(),
            content,
        })
    }
}

fn secure_existing_path(workspace: &Path, relative: &str) -> Result<PathBuf, String> {
    let workspace = workspace.canonicalize().map_err(|error| error.to_string())?;
    let path = workspace
        .join(relative.trim_start_matches('/'))
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !path.starts_with(&workspace) {
        return Err("resource path escapes the workspace".to_owned());
    }
    Ok(path)
}

fn media_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => "application/json",
        Some("toml") => "application/toml",
        Some("md") => "text/markdown",
        Some("rs") => "text/x-rust",
        _ => "text/plain",
    }
}
