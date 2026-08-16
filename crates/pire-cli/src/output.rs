use std::path::Path;

use pire_core::{
    AgentEvent, AgentObserver, Message, ObserverError, Role, RouteInfo, SessionMetadata,
    SessionWriter,
};

#[derive(Debug, Clone, Default)]
pub struct RuntimeStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cost_usd: f64,
    pub tool_calls: u64,
    pub last_route: Option<RouteInfo>,
    pub last_assistant: Option<String>,
}

pub struct CliObserver {
    json: bool,
    session: Option<SessionWriter>,
    stats: RuntimeStats,
}

impl CliObserver {
    #[must_use]
    pub fn new(json: bool, session: Option<SessionWriter>) -> Self {
        Self {
            json,
            session,
            stats: RuntimeStats::default(),
        }
    }

    #[must_use]
    pub fn stats(&self) -> &RuntimeStats {
        &self.stats
    }

    #[must_use]
    pub fn session_metadata(&self) -> Option<&SessionMetadata> {
        self.session.as_ref().map(SessionWriter::metadata)
    }

    #[must_use]
    pub fn session_path(&self) -> Option<&Path> {
        self.session.as_ref().map(SessionWriter::path)
    }

    pub fn replace_session(&mut self, session: Option<SessionWriter>) {
        self.session = session;
    }

    pub fn rename_session(&mut self, name: Option<String>) -> Result<(), ObserverError> {
        if let Some(session) = self.session.as_mut() {
            session
                .rename(name)
                .map_err(|error| ObserverError::new(error.to_string()))?;
        }
        Ok(())
    }

    pub fn checkpoint(&mut self, reason: &str, messages: &[Message]) -> Result<(), ObserverError> {
        if let Some(session) = self.session.as_mut() {
            session
                .checkpoint(reason, messages)
                .map_err(|error| ObserverError::new(error.to_string()))?;
        }
        Ok(())
    }
}

impl AgentObserver for CliObserver {
    fn on_message(&mut self, message: &Message) -> Result<(), ObserverError> {
        if let Some(session) = self.session.as_mut() {
            session
                .append_message(message)
                .map_err(|error| ObserverError::new(error.to_string()))?;
        }
        if message.role == Role::Assistant && !message.content.is_empty() {
            self.stats.last_assistant = Some(message.content.clone());
        }
        Ok(())
    }

    fn on_event(&mut self, event: &AgentEvent) -> Result<(), ObserverError> {
        if let Some(session) = self.session.as_mut() {
            session
                .append_event(event)
                .map_err(|error| ObserverError::new(error.to_string()))?;
        }

        if let AgentEvent::ProviderCompleted { route, usage, .. } = event {
            if let Some(route) = route {
                self.stats.last_route = Some(route.clone());
            }
            if let Some(usage) = usage {
                self.stats.input_tokens = self.stats.input_tokens.saturating_add(usage.input_tokens);
                self.stats.output_tokens = self
                    .stats
                    .output_tokens
                    .saturating_add(usage.output_tokens);
                self.stats.cached_input_tokens = self
                    .stats
                    .cached_input_tokens
                    .saturating_add(usage.cached_input_tokens);
                self.stats.cost_usd += usage.cost_usd.unwrap_or_default();
            }
        }
        if matches!(event, AgentEvent::ToolStarted { .. }) {
            self.stats.tool_calls = self.stats.tool_calls.saturating_add(1);
        }

        if self.json {
            let line = serde_json::to_string(event)
                .map_err(|error| ObserverError::new(error.to_string()))?;
            println!("{line}");
            return Ok(());
        }

        match event {
            AgentEvent::ToolStarted { name, .. } => eprintln!("  → {name}"),
            AgentEvent::ToolFinished { name, is_error, .. } => {
                if *is_error {
                    eprintln!("  ✗ {name}");
                } else {
                    eprintln!("  ✓ {name}");
                }
            }
            AgentEvent::Compacted {
                before_messages,
                after_messages,
            } => eprintln!("  compacted {before_messages} → {after_messages} messages"),
            AgentEvent::FeedbackRecorded {
                candidate,
                positive,
            } => eprintln!(
                "  feedback recorded for {candidate}: {}",
                if *positive { "good" } else { "bad" }
            ),
            AgentEvent::Final { text } => println!("{text}"),
            AgentEvent::StepStarted { step } => tracing::debug!(step, "agent step started"),
            AgentEvent::ProviderCompleted {
                provider,
                model,
                route,
                usage,
                text_present,
                tool_call_count,
            } => tracing::debug!(
                provider,
                model,
                route = ?route,
                usage = ?usage,
                text_present,
                tool_call_count,
                "provider completion received"
            ),
        }
        Ok(())
    }
}
