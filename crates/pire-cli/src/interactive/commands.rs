#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: String,
    pub usage: String,
    pub description: String,
}

impl SlashCommand {
    #[must_use]
    pub fn builtin(name: &str, usage: &str, description: &str) -> Self {
        Self {
            name: name.to_owned(),
            usage: usage.to_owned(),
            description: description.to_owned(),
        }
    }
}

#[must_use]
pub fn builtins() -> Vec<SlashCommand> {
    vec![
        SlashCommand::builtin("/help", "/help", "Show slash commands and keyboard shortcuts"),
        SlashCommand::builtin("/model", "/model [auto|MODEL]", "Select a model or restore automatic routing"),
        SlashCommand::builtin("/models", "/models [FILTER]", "List configured models, prices, and capabilities"),
        SlashCommand::builtin("/provider", "/provider [auto|PROVIDER]", "Select a provider or restore automatic routing"),
        SlashCommand::builtin("/route", "/route [balanced|cost|latency|quality|local-first]", "Inspect or change routing strategy"),
        SlashCommand::builtin("/router", "/router", "Show model-router decisions and learning statistics"),
        SlashCommand::builtin("/feedback", "/feedback good|bad", "Rate the most recent routed response"),
        SlashCommand::builtin("/learn", "/learn status|on|off|reset", "Control bounded routing self-improvement"),
        SlashCommand::builtin("/status", "/status", "Show workspace, session, model, cost, and context status"),
        SlashCommand::builtin("/session", "/session", "Show current session information"),
        SlashCommand::builtin("/new", "/new [NAME]", "Start a fresh session"),
        SlashCommand::builtin("/resume", "/resume [SESSION]", "Resume a previous session"),
        SlashCommand::builtin("/name", "/name NAME", "Name the current session"),
        SlashCommand::builtin("/tree", "/tree [MESSAGE_INDEX]", "Show or rewind the current conversation"),
        SlashCommand::builtin("/fork", "/fork [NAME]", "Fork the current conversation into a new session"),
        SlashCommand::builtin("/clone", "/clone [NAME]", "Clone the current active conversation"),
        SlashCommand::builtin("/compact", "/compact [INSTRUCTIONS]", "Summarize older context"),
        SlashCommand::builtin("/copy", "/copy", "Copy the last assistant response using OSC 52"),
        SlashCommand::builtin("/reload", "/reload", "Reload context files, prompts, and skills"),
        SlashCommand::builtin("/prompts", "/prompts", "List prompt-template slash commands"),
        SlashCommand::builtin("/skills", "/skills", "List on-demand skills"),
        SlashCommand::builtin("/tools", "/tools", "List enabled tools"),
        SlashCommand::builtin("/trust", "/trust [status|grant|revoke]", "Inspect or change project trust"),
        SlashCommand::builtin("/settings", "/settings", "Show the resolved configuration"),
        SlashCommand::builtin("/hotkeys", "/hotkeys", "Show interactive keyboard shortcuts"),
        SlashCommand::builtin("/clear", "/clear", "Clear the visible terminal transcript"),
        SlashCommand::builtin("/login", "/login", "Show provider credential configuration"),
        SlashCommand::builtin("/llama", "/llama", "Show configured local llama.cpp-compatible models"),
        SlashCommand::builtin("/quit", "/quit", "Exit Pire"),
    ]
}

#[must_use]
pub fn parse(input: &str) -> Option<(&str, &str)> {
    let input = input.trim();
    if !input.starts_with('/') {
        return None;
    }
    Some(
        input
            .split_once(char::is_whitespace)
            .map_or((input, ""), |(name, arguments)| (name, arguments.trim())),
    )
}
