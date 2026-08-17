from __future__ import annotations
import pathlib, sys


def replace_once(path: pathlib.Path, old: str, new: str) -> None:
    text = path.read_text(encoding='utf-8')
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'expected one match in {path}, got {count}: {old[:120]!r}')
    path.write_text(text.replace(old, new, 1), encoding='utf-8')


def replace_first(path: pathlib.Path, old: str, new: str) -> None:
    text = path.read_text(encoding='utf-8')
    if old not in text:
        raise SystemExit(f'missing match in {path}: {old[:120]!r}')
    path.write_text(text.replace(old, new, 1), encoding='utf-8')


def main(root: pathlib.Path) -> None:
    app = root / 'crates/pire-cli/src/app.rs'
    resources = root / 'crates/pire-cli/src/resources.rs'
    commands = root / 'crates/pire-cli/src/commands.rs'
    builtins = root / 'crates/pire-cli/src/builtins.rs'

    replace_once(app, 'PrivacyLevel, Provider, ResourceRegistry, Role,', 'PrivacyLevel, ResourceRegistry, Role,')
    replace_once(app, 'ProviderKind, ProviderSpec, RoutedProvider, RouterSettings, RoutingStrategy, build_router,', 'ProviderKind, ProviderSpec, RoutedProvider, RouterSettings, build_router,')

    replace_once(
        app,
        '''pub struct BootstrapOptions {
    pub workspace: PathBuf,
    pub explicit_config: Option<PathBuf>,
    pub overrides: CliOverrides,
    pub trust_project: bool,
    pub no_session: bool,
    pub session: Option<String>,
    pub provider_pin: Option<String>,
}

pub struct App {''',
        '''pub struct BootstrapOptions {
    pub workspace: PathBuf,
    pub explicit_config: Option<PathBuf>,
    pub overrides: CliOverrides,
    pub trust_project: bool,
    pub no_session: bool,
    pub session: Option<String>,
    pub provider_pin: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct PromptRunMode {
    json: bool,
    assume_yes: bool,
    task_override: Option<TaskKind>,
    privacy: PrivacyLevel,
    quiet: bool,
}

pub struct App {'''
    )

    replace_once(
        app,
        '''        self.run_prompt_with_profile_mode(
            &profile,
            prompt,
            json,
            assume_yes,
            None,
            PrivacyLevel::Workspace,
            false,
        )''',
        '''        self.run_prompt_with_profile_mode(
            &profile,
            prompt,
            PromptRunMode {
                json,
                assume_yes,
                task_override: None,
                privacy: PrivacyLevel::Workspace,
                quiet: false,
            },
        )'''
    )
    replace_once(
        app,
        '''        self.run_prompt_with_profile_mode(
            &profile,
            prompt,
            false,
            assume_yes,
            None,
            PrivacyLevel::Workspace,
            true,
        )''',
        '''        self.run_prompt_with_profile_mode(
            &profile,
            prompt,
            PromptRunMode {
                json: false,
                assume_yes,
                task_override: None,
                privacy: PrivacyLevel::Workspace,
                quiet: true,
            },
        )'''
    )
    replace_once(
        app,
        '''        self.run_prompt_with_profile_mode(
            profile_name,
            prompt,
            json,
            assume_yes,
            task_override,
            privacy,
            false,
        )''',
        '''        self.run_prompt_with_profile_mode(
            profile_name,
            prompt,
            PromptRunMode {
                json,
                assume_yes,
                task_override,
                privacy,
                quiet: false,
            },
        )'''
    )
    replace_once(
        app,
        '''    fn run_prompt_with_profile_mode(
        &mut self,
        profile_name: &str,
        prompt: String,
        json: bool,
        assume_yes: bool,
        task_override: Option<TaskKind>,
        privacy: PrivacyLevel,
        quiet: bool,
    ) -> PireResult<String> {''',
        '''    fn run_prompt_with_profile_mode(
        &mut self,
        profile_name: &str,
        prompt: String,
        mode: PromptRunMode,
    ) -> PireResult<String> {'''
    )
    replace_once(
        app,
        '''        let result = self.run_agent(
            &profile,
            prompt.clone(),
            json,
            assume_yes,
            task_override.unwrap_or(profile.purpose),
            privacy.max(self.config.agent.privacy),
            quiet,
        );''',
        '''        let result = self.run_agent(&profile, prompt.clone(), mode);'''
    )
    replace_once(
        app,
        '''    fn run_agent(
        &mut self,
        profile: &AgentProfile,
        prompt: String,
        json: bool,
        assume_yes: bool,
        task: TaskKind,
        privacy: PrivacyLevel,
        quiet: bool,
    ) -> PireResult<String> {''',
        '''    fn run_agent(
        &mut self,
        profile: &AgentProfile,
        prompt: String,
        mode: PromptRunMode,
    ) -> PireResult<String> {'''
    )
    replace_first(app, '            assume_yes,\n            self.trusted,', '            mode.assume_yes,\n            self.trusted,')
    replace_once(app, '        let observer = if quiet {', '        let observer = if mode.quiet {')
    replace_once(app, '                json,\n                self.config.stream_rules.clone(),', '                mode.json,\n                self.config.stream_rules.clone(),')
    replace_once(
        app,
        '''            task,
            privacy,
        })?;''',
        '''            task: mode.task_override.unwrap_or(profile.purpose),
            privacy: mode.privacy.max(self.config.agent.privacy),
        })?;'''
    )

    replace_once(
        resources,
        '''pub enum ContextKind {
    BuiltInPolicy,
    UserInstruction,
    ProjectInstruction,
    Prompt,
    Skill,
    UserMessage,
    PipedInput,
    WorkspaceFile,
}

#[derive(Debug, Clone)]''',
        '''pub enum ContextKind {
    BuiltInPolicy,
    UserInstruction,
    ProjectInstruction,
    Prompt,
    Skill,
    UserMessage,
    PipedInput,
    WorkspaceFile,
}

impl ContextKind {
    const fn label(&self) -> &'static str {
        match self {
            Self::BuiltInPolicy => "built-in policy",
            Self::UserInstruction => "user instruction",
            Self::ProjectInstruction => "project instruction",
            Self::Prompt => "prompt template",
            Self::Skill => "skill",
            Self::UserMessage => "user message",
            Self::PipedInput => "piped input",
            Self::WorkspaceFile => "workspace file",
        }
    }
}

#[derive(Debug, Clone)]'''
    )
    replace_once(
        resources,
        '.map(|segment| format!("Source: {}\\n{}", segment.source, segment.content))',
        '.map(|segment| {\n                format!(\n                    "Source: {} ({})\\n{}",\n                    segment.source,\n                    segment.kind.label(),\n                    segment.content\n                )\n            })'
    )
    replace_once(
        resources,
        '''        self.prompts.get(name).map(|prompt| {
            prompt
                .content
                .replace("{{args}}", arguments)
                .replace("$ARGUMENTS", arguments)
        })''',
        '''        self.prompts.get(name).map(|prompt| {
            let segment = ContextSegment {
                kind: ContextKind::Prompt,
                source: prompt.path.display().to_string(),
                content: prompt.content.clone(),
                privacy: PrivacyLevel::Sensitive,
            };
            segment
                .content
                .replace("{{args}}", arguments)
                .replace("$ARGUMENTS", arguments)
        })'''
    )
    replace_once(
        resources,
        '''        self.skills.get(name).map(|skill| {
            format!(
                "Apply the following skill for this request.\\n\\n{}\\n\\nRequest:\\n{}",
                skill.content, arguments
            )
        })''',
        '''        self.skills.get(name).map(|skill| {
            let segment = ContextSegment {
                kind: ContextKind::Skill,
                source: skill.path.display().to_string(),
                content: skill.content.clone(),
                privacy: PrivacyLevel::Sensitive,
            };
            format!(
                "Apply the following {} from {} for this request.\\n\\n{}\\n\\nRequest:\\n{}",
                segment.kind.label(),
                segment.source,
                segment.content,
                arguments
            )
        })'''
    )
    replace_once(
        commands,
        'description: prompt.description.clone(),',
        'description: format!("{} ({})", prompt.description, prompt.path.display()),'
    )

    replace_once(
        builtins,
        'use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};',
        'use std::{\n    collections::BTreeMap,\n    path::{Path, PathBuf},\n    sync::Arc,\n    time::Duration,\n};'
    )
    replace_once(builtins, '    path: &PathBuf,', '    path: &Path,')


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: apply-cli-clippy.py PROJECT_ROOT')
    main(pathlib.Path(sys.argv[1]))
