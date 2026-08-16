use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use pire_core::Workspace;
use walkdir::WalkDir;

use crate::config::{ResourcesConfig, user_config_path};

const DEFAULT_SYSTEM_PROMPT: &str = "You are Pire, a careful coding agent. Inspect before editing, keep changes scoped, explain failures precisely, and never claim an operation succeeded unless its tool result confirms it.";

#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ResourceCatalog {
    system_prompt: String,
    prompts: BTreeMap<String, PromptTemplate>,
    skills: BTreeMap<String, Skill>,
}

impl ResourceCatalog {
    pub fn load(
        workspace: &Workspace,
        config: &ResourcesConfig,
        trusted_project: bool,
    ) -> io::Result<Self> {
        let mut budget = Budget::new(config.max_total_bytes);
        let user_root = user_config_path().and_then(|path| path.parent().map(Path::to_path_buf));

        let mut system_prompt = DEFAULT_SYSTEM_PROMPT.to_owned();
        if let Some(root) = &user_root
            && let Some(content) = read_optional(&root.join("SYSTEM.md"), &mut budget)?
        {
            system_prompt = content;
        }
        if trusted_project
            && let Some(content) =
                read_workspace_optional(workspace, ".pire/SYSTEM.md", &mut budget)
        {
            system_prompt = content;
        }

        let mut sections = vec![system_prompt];
        if let Some(root) = &user_root {
            append_external(
                &mut sections,
                &root.join("APPEND_SYSTEM.md"),
                &mut budget,
            )?;
            if config.include_user_instructions {
                for name in ["AGENTS.md", "PIRE.md"] {
                    append_external(&mut sections, &root.join(name), &mut budget)?;
                }
            }
        }
        append_parent_context(&mut sections, workspace.root(), &mut budget)?;
        append_workspace(&mut sections, workspace, "PIRE.md", &mut budget);
        if trusted_project {
            append_workspace(&mut sections, workspace, ".pire/PROMPT.md", &mut budget);
            append_workspace(
                &mut sections,
                workspace,
                ".pire/APPEND_SYSTEM.md",
                &mut budget,
            );
        }

        let mut prompts = BTreeMap::new();
        let mut skills = BTreeMap::new();
        if let Some(root) = &user_root {
            discover_prompts(&root.join("prompts"), &mut prompts, &mut budget)?;
            discover_skills(&root.join("skills"), &mut skills, &mut budget)?;
        }
        if trusted_project {
            discover_prompts(
                &workspace.root().join(".pire/prompts"),
                &mut prompts,
                &mut budget,
            )?;
            discover_skills(
                &workspace.root().join(".pire/skills"),
                &mut skills,
                &mut budget,
            )?;
        }

        if config.load_skills_on_demand && !skills.is_empty() {
            let available = skills
                .values()
                .map(|skill| format!("- /skill:{} — {}", skill.name, skill.description))
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!(
                "Available on-demand skills. The user may activate one with its slash command:\n{available}"
            ));
        } else {
            for skill in skills.values() {
                sections.push(format!("Skill {}:\n{}", skill.name, skill.content));
            }
        }

        Ok(Self {
            system_prompt: sections.join("\n\n---\n\n"),
            prompts,
            skills,
        })
    }

    #[must_use]
    pub fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    #[must_use]
    pub fn prompts(&self) -> impl Iterator<Item = &PromptTemplate> {
        self.prompts.values()
    }

    #[must_use]
    pub fn skills(&self) -> impl Iterator<Item = &Skill> {
        self.skills.values()
    }

    #[must_use]
    pub fn dynamic_slash_commands(&self) -> Vec<(String, String)> {
        let mut commands = self
            .prompts
            .values()
            .map(|prompt| (format!("/{}", prompt.name), prompt.description.clone()))
            .collect::<Vec<_>>();
        commands.extend(
            self.skills
                .values()
                .map(|skill| (format!("/skill:{}", skill.name), skill.description.clone())),
        );
        commands.sort_by(|left, right| left.0.cmp(&right.0));
        commands
    }

    #[must_use]
    pub fn expand_slash(&self, input: &str) -> Option<String> {
        let input = input.trim();
        let (command, arguments) = input
            .split_once(char::is_whitespace)
            .map_or((input, ""), |(command, arguments)| (command, arguments.trim()));
        let name = command.strip_prefix('/')?;
        if let Some(skill_name) = name.strip_prefix("skill:") {
            let skill = self.skills.get(skill_name)?;
            return Some(format!(
                "Use the following skill for this task. Follow it where it does not conflict with higher-priority instructions.\n\n# Skill: {}\n\n{}\n\n# Task\n\n{}",
                skill.name,
                skill.content,
                if arguments.is_empty() {
                    "Apply this skill to the current task and conversation."
                } else {
                    arguments
                }
            ));
        }
        let template = self.prompts.get(name)?;
        let mut expanded = template
            .content
            .replace("{{args}}", arguments)
            .replace("$ARGUMENTS", arguments);
        if arguments.is_empty() {
            return Some(expanded);
        }
        if !template.content.contains("{{args}}") && !template.content.contains("$ARGUMENTS") {
            expanded.push_str("\n\n");
            expanded.push_str(arguments);
        }
        Some(expanded)
    }
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

pub fn file_completions(workspace: &Workspace, query: &str, limit: usize) -> Vec<String> {
    let query = query.to_ascii_lowercase();
    workspace
        .list_files(".", limit.saturating_mul(100).max(limit))
        .unwrap_or_default()
        .into_iter()
        .filter(|path| path.to_ascii_lowercase().contains(&query))
        .take(limit)
        .map(|path| format!("@{path}"))
        .collect()
}

fn append_parent_context(
    sections: &mut Vec<String>,
    workspace: &Path,
    budget: &mut Budget,
) -> io::Result<()> {
    let mut directories = workspace.ancestors().map(Path::to_path_buf).collect::<Vec<_>>();
    directories.reverse();
    for directory in directories {
        let override_path = directory.join("AGENTS.override.md");
        if override_path.is_file() {
            append_external(sections, &override_path, budget)?;
            continue;
        }
        let agents = directory.join("AGENTS.md");
        if agents.is_file() {
            append_external(sections, &agents, budget)?;
            continue;
        }
        append_external(sections, &directory.join("CLAUDE.md"), budget)?;
    }
    Ok(())
}

fn append_workspace(
    sections: &mut Vec<String>,
    workspace: &Workspace,
    relative: impl AsRef<Path>,
    budget: &mut Budget,
) {
    let relative = relative.as_ref();
    if let Some(content) = read_workspace_optional(workspace, relative, budget) {
        sections.push(format!("Instructions from {}:\n{content}", relative.display()));
    }
}

fn append_external(
    sections: &mut Vec<String>,
    path: &Path,
    budget: &mut Budget,
) -> io::Result<()> {
    if let Some(content) = read_optional(path, budget)? {
        sections.push(format!("Instructions from {}:\n{content}", path.display()));
    }
    Ok(())
}

fn read_workspace_optional(
    workspace: &Workspace,
    relative: impl AsRef<Path>,
    budget: &mut Budget,
) -> Option<String> {
    let remaining = budget.remaining();
    if remaining == 0 {
        return None;
    }
    let content = workspace.read_text(relative.as_ref(), remaining).ok()?;
    budget.consume(content.len());
    Some(content)
}

fn read_optional(path: &Path, budget: &mut Budget) -> io::Result<Option<String>> {
    if !path.is_file() || budget.remaining() == 0 {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    if bytes.len() > budget.remaining() {
        return Ok(None);
    }
    let content = String::from_utf8(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    budget.consume(content.len());
    Ok(Some(content))
}

fn discover_prompts(
    directory: &Path,
    prompts: &mut BTreeMap<String, PromptTemplate>,
    budget: &mut Budget,
) -> io::Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in WalkDir::new(directory).max_depth(2).follow_links(false) {
        let entry = entry.map_err(io::Error::other)?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("md")
        {
            continue;
        }
        let Some(name) = entry.path().file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some(content) = read_optional(entry.path(), budget)? else {
            continue;
        };
        prompts.insert(
            name.to_owned(),
            PromptTemplate {
                name: name.to_owned(),
                description: description(&content, "Prompt template"),
                content,
            },
        );
    }
    Ok(())
}

fn discover_skills(
    directory: &Path,
    skills: &mut BTreeMap<String, Skill>,
    budget: &mut Budget,
) -> io::Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in WalkDir::new(directory).max_depth(3).follow_links(false) {
        let entry = entry.map_err(io::Error::other)?;
        if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
            continue;
        }
        let Some(name) = entry
            .path()
            .parent()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
        else {
            continue;
        };
        let Some(content) = read_optional(entry.path(), budget)? else {
            continue;
        };
        skills.insert(
            name.to_owned(),
            Skill {
                name: name.to_owned(),
                description: description(&content, "Agent skill"),
                content,
            },
        );
    }
    Ok(())
}

fn description(content: &str, fallback: &str) -> String {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_owned())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

struct Budget {
    limit: usize,
    used: usize,
}

impl Budget {
    const fn new(limit: usize) -> Self {
        Self { limit, used: 0 }
    }

    const fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.used)
    }

    fn consume(&mut self, bytes: usize) {
        self.used = self.used.saturating_add(bytes).min(self.limit);
    }
}
