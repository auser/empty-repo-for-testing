use std::{fs, io, path::{Path, PathBuf}};

use pire_builtins::{PromptCommandPlugin, SidecarManifest, SidecarPlugin};
use pire_core::{Plugin, Registry, ResourceUri};
use walkdir::WalkDir;

use crate::config::{PluginsConfig, ResourcesConfig, user_config_path};

pub fn system_prompt(
    workspace: &Path,
    trusted: bool,
    config: &ResourcesConfig,
) -> io::Result<String> {
    let mut sections = vec![
        "You are Pire, a careful coding agent. Inspect before editing, keep changes scoped, use hash preconditions for writes, report failures precisely, and never claim an operation succeeded unless its tool result confirms it.".to_owned(),
    ];
    let mut used = sections[0].len();

    if config.include_user_instructions
        && let Some(path) = user_config_path()
            .and_then(|path| path.parent().map(|parent| parent.join("PIRE.md")))
    {
        append_external(&mut sections, &mut used, &path, config.max_total_bytes)?;
    }

    if trusted {
        for relative in [
            "PIRE.md",
            "AGENTS.md",
            "AGENTS.override.md",
            "CLAUDE.md",
            ".pire/PROMPT.md",
        ] {
            append_workspace(
                &mut sections,
                &mut used,
                workspace,
                relative,
                config.max_total_bytes,
            )?;
        }
        if !config.load_skills_on_demand {
            let skills = workspace.join(".pire/skills");
            if skills.is_dir() {
                for entry in WalkDir::new(skills).follow_links(false) {
                    let entry = entry.map_err(io::Error::other)?;
                    if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
                        append_external(
                            &mut sections,
                            &mut used,
                            entry.path(),
                            config.max_total_bytes,
                        )?;
                    }
                }
            }
        }
    }

    Ok(sections.join("\n\n---\n\n"))
}

pub fn expand_input(
    registry: &Registry,
    args: &[String],
    stdin: Option<String>,
    max_bytes: usize,
) -> io::Result<String> {
    let mut sections = Vec::new();
    let instruction = args
        .iter()
        .filter(|value| !value.starts_with('@'))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if !instruction.is_empty() {
        sections.push(instruction);
    }
    for argument in args.iter().filter(|value| value.starts_with('@')) {
        let path = argument.trim_start_matches('@');
        if path.is_empty() {
            continue;
        }
        let uri = ResourceUri::parse(format!("file://{path}"))
            .map_err(io::Error::other)?;
        let resolver = registry
            .resolve_resource(uri.scheme())
            .ok_or_else(|| io::Error::other("file resource resolver is unavailable"))?;
        let resource = resolver.read(&uri, max_bytes).map_err(io::Error::other)?;
        sections.push(format!(
            "Workspace resource {}:\n```text\n{}\n```",
            resource.uri, resource.content
        ));
    }
    if let Some(stdin) = stdin {
        sections.push(format!("Piped input:\n```text\n{stdin}\n```"));
    }
    let combined = sections.join("\n\n");
    if combined.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("combined input exceeds {max_bytes} bytes"),
        ));
    }
    Ok(combined)
}

pub fn prompt_plugins(workspace: &Path, trusted: bool) -> io::Result<Vec<Box<dyn Plugin>>> {
    if !trusted {
        return Ok(Vec::new());
    }
    let directory = workspace.join(".pire/prompts");
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();
    if !directory.is_dir() {
        return Ok(plugins);
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let template = fs::read_to_string(&path)?;
        let description = template
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Project prompt")
            .trim_start_matches('#')
            .trim()
            .to_owned();
        plugins.push(Box::new(PromptCommandPlugin::new(
            name,
            name,
            description,
            template,
        )));
    }
    Ok(plugins)
}

pub fn skill_plugins(workspace: &Path, trusted: bool) -> io::Result<Vec<Box<dyn Plugin>>> {
    if !trusted {
        return Ok(Vec::new());
    }
    let directory = workspace.join(".pire/skills");
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();
    if !directory.is_dir() {
        return Ok(plugins);
    }
    for entry in WalkDir::new(directory).min_depth(2).max_depth(2).follow_links(false) {
        let entry = entry.map_err(io::Error::other)?;
        if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
            continue;
        }
        let Some(name) = entry
            .path()
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        let template = fs::read_to_string(entry.path())?;
        plugins.push(Box::new(PromptCommandPlugin::new(
            format!("skill-{name}"),
            format!("skill:{name}"),
            format!("Activate the `{name}` skill"),
            template,
        )));
    }
    Ok(plugins)
}

pub fn sidecar_plugins(
    workspace: &Path,
    trusted: bool,
    config: &PluginsConfig,
) -> io::Result<Vec<Box<dyn Plugin>>> {
    if !config.enabled {
        return Ok(Vec::new());
    }
    let mut directories = config.directories.clone();
    if let Some(path) = user_config_path().and_then(|path| path.parent().map(|parent| parent.join("plugins"))) {
        directories.push(path);
    }
    if trusted {
        directories.push(workspace.join(".pire/plugins"));
    }
    let mut plugins: Vec<Box<dyn Plugin>> = Vec::new();
    for directory in directories {
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path)?;
            let mut manifest: SidecarManifest =
                serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            manifest.timeout_secs = manifest.timeout_secs.min(config.timeout_secs);
            manifest.max_message_bytes = manifest
                .max_message_bytes
                .min(config.max_message_bytes);
            let plugin = SidecarPlugin::new(manifest).map_err(io::Error::other)?;
            plugins.push(Box::new(plugin));
        }
    }
    Ok(plugins)
}

fn append_workspace(
    sections: &mut Vec<String>,
    used: &mut usize,
    workspace: &Path,
    relative: impl AsRef<Path>,
    limit: usize,
) -> io::Result<()> {
    let path = workspace.join(relative);
    append_external(sections, used, &path, limit)
}

fn append_external(
    sections: &mut Vec<String>,
    used: &mut usize,
    path: &Path,
    limit: usize,
) -> io::Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let remaining = limit.saturating_sub(*used);
    if remaining == 0 {
        return Ok(());
    }
    let bytes = fs::read(path)?;
    if bytes.len() > remaining {
        return Ok(());
    }
    let content = String::from_utf8(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    *used = used.saturating_add(content.len());
    sections.push(format!("Instructions from {}:\n{content}", path.display()));
    Ok(())
}

#[allow(dead_code)]
fn _path_marker(_path: &PathBuf) {}
