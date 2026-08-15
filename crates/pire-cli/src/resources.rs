use std::{fs, io, path::Path};

use pire_core::Workspace;
use walkdir::WalkDir;

use crate::config::{ResourcesConfig, user_config_path};

pub fn system_prompt(workspace: &Workspace, config: &ResourcesConfig) -> io::Result<String> {
    let mut sections = vec![
        "You are Pire, a careful coding agent. Inspect before editing, keep changes scoped, explain failures precisely, and never claim an operation succeeded unless its tool result confirms it.".to_owned(),
    ];
    let mut used = sections[0].len();

    if config.include_user_instructions
        && let Some(path) =
            user_config_path().and_then(|path| path.parent().map(|parent| parent.join("PIRE.md")))
    {
        append_external(&mut sections, &mut used, &path, config.max_total_bytes)?;
    }

    for relative in ["PIRE.md", "AGENTS.md", ".pire/PROMPT.md"] {
        append_workspace(
            &mut sections,
            &mut used,
            workspace,
            relative,
            config.max_total_bytes,
        );
    }

    let skills = workspace.root().join(".pire/skills");
    if skills.is_dir() {
        for entry in WalkDir::new(skills).follow_links(false) {
            let entry = entry.map_err(io::Error::other)?;
            if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
                let relative = entry
                    .path()
                    .strip_prefix(workspace.root())
                    .map_err(io::Error::other)?;
                append_workspace(
                    &mut sections,
                    &mut used,
                    workspace,
                    relative,
                    config.max_total_bytes,
                );
            }
        }
    }

    Ok(sections.join("\n\n---\n\n"))
}

pub fn input(
    workspace: &Workspace,
    args: &[String],
    stdin: Option<String>,
    max_bytes: usize,
) -> Result<String, io::Error> {
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
        let relative = argument.trim_start_matches('@');
        if relative.is_empty() {
            continue;
        }
        let content = workspace
            .read_text(relative, max_bytes)
            .map_err(io::Error::other)?;
        sections.push(format!("File: {relative}\n```text\n{content}\n```"));
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

fn append_workspace(
    sections: &mut Vec<String>,
    used: &mut usize,
    workspace: &Workspace,
    relative: impl AsRef<Path>,
    limit: usize,
) {
    let relative = relative.as_ref();
    let remaining = limit.saturating_sub(*used);
    if remaining == 0 {
        return;
    }
    if let Ok(content) = workspace.read_text(relative, remaining) {
        *used = used.saturating_add(content.len());
        sections.push(format!("Instructions from {}:\n{content}", relative.display()));
    }
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
    sections.push(format!("User instructions from {}:\n{content}", path.display()));
    Ok(())
}
