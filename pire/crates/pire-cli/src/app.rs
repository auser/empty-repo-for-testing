use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use pire_core::{
    Agent, AgentLimits, AgentOptions, AgentTurn, CancellationToken, CommandAction, Conversation,
    EventSink, ExecutionRequest, FanoutEventSink, Message, Operation, Registry, RouteStrategy,
    SessionRecord, SessionStore, SessionSummary, ToolContext, ToolLimits,
};
use serde_json::json;

use crate::{
    approval::CliApprovalPolicy,
    config::{AppConfig, RouteStrategyConfig},
    error::{PireError, Result},
    output::{JsonEventSink, SessionEventSink, TerminalEventSink},
    resources,
    trust::TrustStore,
};

pub struct App {
    config: AppConfig,
    workspace: PathBuf,
    trusted: bool,
    assume_yes: bool,
    json_events: bool,
    registry: Arc<Registry>,
    agent: Agent,
    conversation: Conversation,
    system_prompt: String,
    options: AgentOptions,
    session_store: Option<Arc<dyn SessionStore>>,
    session: Option<SessionSummary>,
    last_turn: Option<AgentTurn>,
    trust_store: TrustStore,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: AppConfig,
        workspace: PathBuf,
        trusted: bool,
        assume_yes: bool,
        json_events: bool,
        registry: Arc<Registry>,
        system_prompt: String,
        session_store: Option<Arc<dyn SessionStore>>,
        session: Option<SessionSummary>,
        trust_store: TrustStore,
    ) -> Self {
        let options = AgentOptions {
            strategy: RouteStrategy::from(config.router.strategy),
            pinned_model: config.router.pinned_model.clone(),
            pinned_provider: config.router.pinned_provider.clone(),
            prefer_local: config.router.prefer_local,
            max_estimated_cost_usd: config.router.max_estimated_cost_usd,
            assumed_output_tokens: config.router.assumed_output_tokens,
            max_fallbacks: config.router.max_fallbacks,
            ..AgentOptions::default()
        };
        let agent = Agent::new(
            Arc::clone(&registry),
            AgentLimits {
                max_steps: config.agent.max_steps,
                max_tool_calls: config.agent.max_tool_calls,
            },
        );
        Self {
            config,
            workspace,
            trusted,
            assume_yes,
            json_events,
            registry,
            agent,
            conversation: Conversation::new(system_prompt.clone()),
            system_prompt,
            options,
            session_store,
            session,
            last_turn: None,
            trust_store,
        }
    }

    #[must_use]
    pub fn prompt(&self) -> String {
        let model = self.options.pinned_model.as_deref().unwrap_or("auto");
        format!("pire[{model}|{:?}]> ", self.options.strategy)
    }

    #[must_use]
    pub fn commands(&self) -> Vec<(String, String)> {
        let mut commands = self
            .registry
            .commands()
            .into_iter()
            .map(|command| (command.name().to_owned(), command.description().to_owned()))
            .collect::<Vec<_>>();
        commands.sort_by(|left, right| left.0.cmp(&right.0));
        commands
    }

    pub fn handle_line(&mut self, line: &str) -> Result<bool> {
        let line = line.trim_end();
        if line.is_empty() {
            return Ok(true);
        }
        if let Some(command) = line.strip_prefix('/') {
            let (name, arguments) = command.split_once(' ').unwrap_or((command, ""));
            let command = self
                .registry
                .command(name)
                .ok_or_else(|| PireError::Message(format!("unknown command `/{name}`")))?;
            let action = command.execute(arguments).map_err(PireError::Message)?;
            return self.handle_action(action);
        }
        if let Some(command) = line.strip_prefix("!!") {
            let output = self.run_shell(command.trim())?;
            let input = format!(
                "Command `{}` completed. Analyze this output:\n\n{}",
                command.trim(), output
            );
            self.submit(input)?;
            return Ok(true);
        }
        if let Some(command) = line.strip_prefix('!') {
            let output = self.run_shell(command.trim())?;
            println!("{output}");
            return Ok(true);
        }
        self.submit(line.to_owned())?;
        Ok(true)
    }

    pub fn submit(&mut self, input: String) -> Result<AgentTurn> {
        let input = resources::expand_input(
            self.registry.as_ref(),
            &[input],
            None,
            self.config.resources.max_total_bytes,
        )?;
        self.append_session_message("user", &input);
        let cancellation = CancellationToken::default();
        let execution = self
            .registry
            .execution("host")
            .ok_or_else(|| PireError::Message("host execution backend is unavailable".to_owned()))?;
        let approval = Arc::new(CliApprovalPolicy::new(
            self.config.security.approval_mode,
            self.trusted,
            self.assume_yes,
        ));
        let context = ToolContext::new(
            self.workspace.clone(),
            approval,
            execution,
            ToolLimits {
                max_read_bytes: self.config.tools.max_read_bytes,
                max_write_bytes: self.config.tools.max_write_bytes,
                max_process_output_bytes: self.config.tools.max_process_output_bytes,
                process_timeout: Duration::from_secs(self.config.tools.process_timeout_secs),
                max_list_entries: self.config.tools.max_list_entries,
            },
            cancellation.clone(),
        );
        let terminal = Arc::new(TerminalEventSink::new(!self.json_events));
        let mut sinks: Vec<Arc<dyn EventSink>> = Vec::new();
        if self.json_events {
            sinks.push(Arc::new(JsonEventSink));
        } else {
            sinks.push(terminal.clone());
        }
        if let (Some(store), Some(session)) = (&self.session_store, &self.session) {
            sinks.push(Arc::new(SessionEventSink::new(
                Arc::clone(store),
                session.id.clone(),
            )));
        }
        let events = FanoutEventSink::new(sinks);
        let turn = self.agent.run_turn(
            &mut self.conversation,
            input,
            &self.options,
            &events,
            &cancellation,
            &context,
        )?;
        if !self.json_events {
            if terminal.wrote_text() {
                println!();
            } else {
                println!("{}", turn.text);
            }
        }
        self.append_session_message("assistant", &turn.text);
        self.last_turn = Some(turn.clone());
        Ok(turn)
    }

    fn handle_action(&mut self, action: CommandAction) -> Result<bool> {
        match action {
            CommandAction::Help => {
                for (name, description) in self.commands() {
                    println!("/{name:<14} {description}");
                }
            }
            CommandAction::Status => self.print_status(),
            CommandAction::Models(filter) => self.print_models(filter.as_deref()),
            CommandAction::SelectModel(model) => {
                self.options.pinned_model = model;
                println!(
                    "model: {}",
                    self.options.pinned_model.as_deref().unwrap_or("auto")
                );
            }
            CommandAction::SelectProvider(provider) => {
                self.options.pinned_provider = provider;
                println!(
                    "provider: {}",
                    self.options.pinned_provider.as_deref().unwrap_or("auto")
                );
            }
            CommandAction::SetRoute(strategy) => {
                if let Some(strategy) = strategy {
                    self.options.strategy = parse_strategy(&strategy)?;
                }
                println!("route: {:?}", self.options.strategy);
            }
            CommandAction::ExplainRoute => {
                if let Some(turn) = &self.last_turn {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&turn.route)
                            .map_err(|error| PireError::Message(error.to_string()))?
                    );
                } else {
                    println!("no routing decision has been made yet");
                }
            }
            CommandAction::Feedback(positive) => {
                let turn = self
                    .last_turn
                    .as_ref()
                    .ok_or_else(|| PireError::Message("no completed turn to rate".to_owned()))?;
                let learning = self
                    .registry
                    .learning("default")
                    .ok_or_else(|| PireError::Message("learning store is unavailable".to_owned()))?;
                learning
                    .feedback(&turn.model_id, turn.task, positive)
                    .map_err(PireError::Message)?;
                println!("feedback recorded");
            }
            CommandAction::LearnStatus => {
                let learning = self
                    .registry
                    .learning("default")
                    .ok_or_else(|| PireError::Message("learning store is unavailable".to_owned()))?;
                println!("{:?}", learning.summary());
            }
            CommandAction::LearnEnable(enabled) => {
                let learning = self
                    .registry
                    .learning("default")
                    .ok_or_else(|| PireError::Message("learning store is unavailable".to_owned()))?;
                learning.set_enabled(enabled).map_err(PireError::Message)?;
                println!("learning: {}", if enabled { "on" } else { "off" });
            }
            CommandAction::LearnReset => {
                let learning = self
                    .registry
                    .learning("default")
                    .ok_or_else(|| PireError::Message("learning store is unavailable".to_owned()))?;
                learning.reset().map_err(PireError::Message)?;
                println!("learning statistics reset");
            }
            CommandAction::NewSession(name) => self.new_session(name.as_deref())?,
            CommandAction::Sessions => self.print_sessions()?,
            CommandAction::Resume(id) => self.resume_session(id.as_deref())?,
            CommandAction::RenameSession(name) => {
                let session = self
                    .session
                    .as_mut()
                    .ok_or_else(|| PireError::Message("sessions are disabled".to_owned()))?;
                let store = self
                    .session_store
                    .as_ref()
                    .ok_or_else(|| PireError::Message("sessions are disabled".to_owned()))?;
                store.rename(&session.id, &name).map_err(PireError::Message)?;
                session.name = Some(name);
            }
            CommandAction::Compact(instructions) => self.compact(instructions.as_deref()),
            CommandAction::Plugins => {
                for plugin in self.registry.plugins() {
                    println!("{} {} — {}", plugin.id, plugin.version, plugin.description);
                }
            }
            CommandAction::Tools => {
                for tool in self.registry.tools() {
                    let definition = tool.definition();
                    println!("{} — {}", definition.name, definition.description);
                }
            }
            CommandAction::Trust(action) => self.handle_trust(action.as_deref())?,
            CommandAction::Settings => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&self.config)
                        .map_err(|error| PireError::Message(error.to_string()))?
                );
            }
            CommandAction::Clear => print!("\x1b[2J\x1b[H"),
            CommandAction::Submit(prompt) => {
                self.submit(prompt)?;
            }
            CommandAction::Message(message) => println!("{message}"),
            CommandAction::Quit => return Ok(false),
        }
        Ok(true)
    }

    fn run_shell(&self, command: &str) -> Result<String> {
        let parts = shlex::split(command)
            .ok_or_else(|| PireError::Message("unable to parse shell shortcut".to_owned()))?;
        let (program, args) = parts
            .split_first()
            .ok_or_else(|| PireError::Message("shell shortcut is empty".to_owned()))?;
        let execution = self
            .registry
            .execution("host")
            .ok_or_else(|| PireError::Message("host execution backend is unavailable".to_owned()))?;
        let result = execution
            .execute(
                &ExecutionRequest {
                    program: program.clone(),
                    args: args.to_vec(),
                    cwd: self.workspace.clone(),
                    env: Default::default(),
                    clear_env: false,
                    timeout: Duration::from_secs(self.config.tools.process_timeout_secs),
                    max_output_bytes: self.config.tools.max_process_output_bytes,
                },
                &CancellationToken::default(),
            )
            .map_err(|error| PireError::Message(error.to_string()))?;
        Ok(format!(
            "exit={:?}\n{}{}",
            result.exit_code, result.stdout, result.stderr
        ))
    }

    fn print_status(&self) {
        println!("workspace: {}", self.workspace.display());
        println!("trusted: {}", self.trusted);
        println!("model: {}", self.options.pinned_model.as_deref().unwrap_or("auto"));
        println!("provider: {}", self.options.pinned_provider.as_deref().unwrap_or("auto"));
        println!("route: {:?}", self.options.strategy);
        println!(
            "session: {}",
            self.session.as_ref().map_or("disabled", |session| session.id.as_str())
        );
        if let Some(turn) = &self.last_turn {
            println!("last model: {}", turn.model_id);
            println!("last estimated cost: ${:.6}", turn.estimated_cost_usd);
            println!("last tokens: {}", turn.usage.total_tokens());
        }
    }

    fn print_models(&self, filter: Option<&str>) {
        let filter = filter.unwrap_or("").to_ascii_lowercase();
        let mut models = self
            .registry
            .providers()
            .into_iter()
            .map(|provider| provider.descriptor().clone())
            .filter(|model| {
                filter.is_empty()
                    || model.id.to_ascii_lowercase().contains(&filter)
                    || model.provider_name.to_ascii_lowercase().contains(&filter)
            })
            .collect::<Vec<_>>();
        models.sort_by(|left, right| left.id.cmp(&right.id));
        for model in models {
            println!(
                "{:<24} provider={:<18} local={} tools={} input=${:.4}/M output=${:.4}/M",
                model.id,
                model.provider_name,
                model.local,
                model.capabilities.tools,
                model.input_cost_per_million,
                model.output_cost_per_million,
            );
        }
    }

    fn new_session(&mut self, name: Option<&str>) -> Result<()> {
        let store = self
            .session_store
            .as_ref()
            .ok_or_else(|| PireError::Message("sessions are disabled".to_owned()))?;
        let session = store.create(name, None).map_err(PireError::Message)?;
        println!("session: {}", session.id);
        self.session = Some(session);
        self.conversation = Conversation::new(self.system_prompt.clone());
        Ok(())
    }

    fn print_sessions(&self) -> Result<()> {
        let store = self
            .session_store
            .as_ref()
            .ok_or_else(|| PireError::Message("sessions are disabled".to_owned()))?;
        for session in store.list().map_err(PireError::Message)? {
            println!(
                "{}  {}",
                session.id,
                session.name.as_deref().unwrap_or("(unnamed)")
            );
        }
        Ok(())
    }

    fn resume_session(&mut self, id: Option<&str>) -> Result<()> {
        let store = self
            .session_store
            .as_ref()
            .ok_or_else(|| PireError::Message("sessions are disabled".to_owned()))?;
        let sessions = store.list().map_err(PireError::Message)?;
        let selected = if let Some(id) = id {
            sessions
                .into_iter()
                .find(|session| session.id == id || session.id.starts_with(id))
                .ok_or_else(|| PireError::Message(format!("session `{id}` was not found")))?
        } else {
            sessions
                .into_iter()
                .next()
                .ok_or_else(|| PireError::Message("no sessions are available".to_owned()))?
        };
        let records = store.load(&selected.id).map_err(PireError::Message)?;
        let mut conversation = Conversation::new(self.system_prompt.clone());
        for record in records {
            if record.kind != "message" {
                continue;
            }
            let role = record.payload.get("role").and_then(serde_json::Value::as_str);
            let content = record
                .payload
                .get("content")
                .and_then(serde_json::Value::as_str);
            match (role, content) {
                (Some("user"), Some(content)) => conversation.push(Message::user(content)),
                (Some("assistant"), Some(content)) => {
                    conversation.push(Message::assistant(content, Vec::new()));
                }
                _ => {}
            }
        }
        println!("resumed {}", selected.id);
        self.session = Some(selected);
        self.conversation = conversation;
        Ok(())
    }

    fn compact(&mut self, instructions: Option<&str>) {
        let messages = self.conversation.messages();
        let keep_from = messages.len().saturating_sub(8).max(1);
        let retained = messages[keep_from..].to_vec();
        let summary = format!(
            "Earlier context was compacted deterministically. Preserve the current objective, unresolved failures, files changed, and verification state. {}",
            instructions.unwrap_or("")
        );
        let mut compacted = Conversation::new(self.system_prompt.clone());
        compacted.push(Message::system(summary));
        for message in retained {
            compacted.push(message);
        }
        self.conversation = compacted;
        println!("context compacted; retained {} recent message(s)", messages.len().saturating_sub(keep_from));
    }

    fn handle_trust(&mut self, action: Option<&str>) -> Result<()> {
        match action.unwrap_or("status") {
            "status" => println!("trusted: {}", self.trusted),
            "grant" => {
                self.trust_store
                    .grant(&self.workspace)
                    .map_err(PireError::Message)?;
                self.trusted = true;
                println!("project trusted; restart or /new to reload project plugins and prompts");
            }
            "revoke" => {
                self.trust_store
                    .revoke(&self.workspace)
                    .map_err(PireError::Message)?;
                self.trusted = false;
                println!("project trust revoked");
            }
            other => {
                return Err(PireError::Message(format!(
                    "unknown trust action `{other}`; use status, grant, or revoke"
                )));
            }
        }
        Ok(())
    }

    fn append_session_message(&self, role: &str, content: &str) {
        let (Some(store), Some(session)) = (&self.session_store, &self.session) else {
            return;
        };
        let now = now_millis();
        let record = SessionRecord {
            id: format!("message-{}-{role}", now),
            parent_id: None,
            timestamp_ms: now,
            kind: "message".to_owned(),
            payload: json!({ "role": role, "content": content }),
        };
        let _ = store.append(&session.id, &record);
    }
}

fn parse_strategy(value: &str) -> Result<RouteStrategy> {
    let parsed = match value {
        "balanced" => RouteStrategyConfig::Balanced,
        "cost" => RouteStrategyConfig::Cost,
        "latency" => RouteStrategyConfig::Latency,
        "quality" => RouteStrategyConfig::Quality,
        "local-first" | "local" => RouteStrategyConfig::LocalFirst,
        _ => {
            return Err(PireError::Message(
                "route must be balanced, cost, latency, quality, or local-first".to_owned(),
            ));
        }
    };
    Ok(parsed.into())
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

#[allow(dead_code)]
fn _operation_marker(_operation: Operation) {}
