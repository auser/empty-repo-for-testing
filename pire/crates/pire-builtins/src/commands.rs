use std::sync::Arc;

use pire_core::{CommandAction, Plugin, PluginMetadata, Registry, SlashCommand};

pub struct CommandsPlugin;

impl Plugin for CommandsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "pire.commands.builtin",
            env!("CARGO_PKG_VERSION"),
            "Pi-style interactive slash commands",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        for command in builtin_commands() {
            registry
                .register_command(command)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn builtin_commands() -> Vec<Arc<dyn SlashCommand>> {
    [
        ("help", "List slash commands", BuiltinCommandKind::Help),
        ("status", "Show model, route, trust, session, and learning status", BuiltinCommandKind::Status),
        ("models", "List or filter configured models", BuiltinCommandKind::Models),
        ("model", "Select a model or return to automatic routing", BuiltinCommandKind::Model),
        ("provider", "Pin a provider or return to automatic routing", BuiltinCommandKind::Provider),
        ("route", "Set balanced, cost, latency, quality, or local-first routing", BuiltinCommandKind::Route),
        ("router", "Explain the last routing decision", BuiltinCommandKind::Router),
        ("feedback", "Record good or bad feedback for the last route", BuiltinCommandKind::Feedback),
        ("learn", "Inspect, enable, disable, or reset aggregate learning", BuiltinCommandKind::Learn),
        ("new", "Start a new session", BuiltinCommandKind::New),
        ("sessions", "List sessions", BuiltinCommandKind::Sessions),
        ("resume", "Resume a session", BuiltinCommandKind::Resume),
        ("name", "Rename the active session", BuiltinCommandKind::Name),
        ("compact", "Compact older conversation context", BuiltinCommandKind::Compact),
        ("plugins", "List mounted plugins and capabilities", BuiltinCommandKind::Plugins),
        ("tools", "List available tools", BuiltinCommandKind::Tools),
        ("trust", "Inspect, grant, or revoke project trust", BuiltinCommandKind::Trust),
        ("settings", "Show resolved settings", BuiltinCommandKind::Settings),
        ("clear", "Clear the visible transcript", BuiltinCommandKind::Clear),
        ("quit", "Exit Pire", BuiltinCommandKind::Quit),
        ("exit", "Exit Pire", BuiltinCommandKind::Quit),
    ]
    .into_iter()
    .map(|(name, description, kind)| {
        Arc::new(BuiltinCommand {
            name,
            description,
            kind,
        }) as Arc<dyn SlashCommand>
    })
    .collect()
}

#[derive(Debug, Clone, Copy)]
enum BuiltinCommandKind {
    Help,
    Status,
    Models,
    Model,
    Provider,
    Route,
    Router,
    Feedback,
    Learn,
    New,
    Sessions,
    Resume,
    Name,
    Compact,
    Plugins,
    Tools,
    Trust,
    Settings,
    Clear,
    Quit,
}

struct BuiltinCommand {
    name: &'static str,
    description: &'static str,
    kind: BuiltinCommandKind,
}

impl SlashCommand for BuiltinCommand {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn execute(&self, arguments: &str) -> Result<CommandAction, String> {
        let arguments = arguments.trim();
        match self.kind {
            BuiltinCommandKind::Help => Ok(CommandAction::Help),
            BuiltinCommandKind::Status => Ok(CommandAction::Status),
            BuiltinCommandKind::Models => Ok(CommandAction::Models(nonempty(arguments))),
            BuiltinCommandKind::Model => Ok(CommandAction::SelectModel(normalize_auto(arguments))),
            BuiltinCommandKind::Provider => {
                Ok(CommandAction::SelectProvider(normalize_auto(arguments)))
            }
            BuiltinCommandKind::Route => Ok(CommandAction::SetRoute(nonempty(arguments))),
            BuiltinCommandKind::Router => Ok(CommandAction::ExplainRoute),
            BuiltinCommandKind::Feedback => match arguments {
                "good" | "+" | "yes" => Ok(CommandAction::Feedback(true)),
                "bad" | "-" | "no" => Ok(CommandAction::Feedback(false)),
                _ => Err("usage: /feedback good|bad".to_owned()),
            },
            BuiltinCommandKind::Learn => match arguments {
                "" | "status" => Ok(CommandAction::LearnStatus),
                "on" | "enable" => Ok(CommandAction::LearnEnable(true)),
                "off" | "disable" => Ok(CommandAction::LearnEnable(false)),
                "reset" => Ok(CommandAction::LearnReset),
                _ => Err("usage: /learn status|on|off|reset".to_owned()),
            },
            BuiltinCommandKind::New => Ok(CommandAction::NewSession(nonempty(arguments))),
            BuiltinCommandKind::Sessions => Ok(CommandAction::Sessions),
            BuiltinCommandKind::Resume => Ok(CommandAction::Resume(nonempty(arguments))),
            BuiltinCommandKind::Name => {
                if arguments.is_empty() {
                    Err("usage: /name NAME".to_owned())
                } else {
                    Ok(CommandAction::RenameSession(arguments.to_owned()))
                }
            }
            BuiltinCommandKind::Compact => Ok(CommandAction::Compact(nonempty(arguments))),
            BuiltinCommandKind::Plugins => Ok(CommandAction::Plugins),
            BuiltinCommandKind::Tools => Ok(CommandAction::Tools),
            BuiltinCommandKind::Trust => Ok(CommandAction::Trust(nonempty(arguments))),
            BuiltinCommandKind::Settings => Ok(CommandAction::Settings),
            BuiltinCommandKind::Clear => Ok(CommandAction::Clear),
            BuiltinCommandKind::Quit => Ok(CommandAction::Quit),
        }
    }
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn normalize_auto(value: &str) -> Option<String> {
    match value {
        "" | "auto" | "none" => None,
        value => Some(value.to_owned()),
    }
}

pub struct PromptCommandPlugin {
    id: String,
    command: String,
    description: String,
    template: String,
}

impl PromptCommandPlugin {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        command: impl Into<String>,
        description: impl Into<String>,
        template: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            command: command.into(),
            description: description.into(),
            template: template.into(),
        }
    }
}

impl Plugin for PromptCommandPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            format!("pire.command.prompt.{}", self.id),
            env!("CARGO_PKG_VERSION"),
            "Markdown prompt slash command",
        )
        .with_dependencies(["pire.commands.builtin"])
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        registry
            .register_command(Arc::new(PromptCommand {
                name: self.command.clone(),
                description: self.description.clone(),
                template: self.template.clone(),
            }))
            .map_err(|error| error.to_string())
    }
}

struct PromptCommand {
    name: String,
    description: String,
    template: String,
}

impl SlashCommand for PromptCommand {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn execute(&self, arguments: &str) -> Result<CommandAction, String> {
        let expanded = if self.template.contains("{{args}}") {
            self.template.replace("{{args}}", arguments.trim())
        } else if arguments.trim().is_empty() {
            self.template.clone()
        } else {
            format!("{}\n\n{}", self.template, arguments.trim())
        };
        Ok(CommandAction::Submit(expanded))
    }
}
