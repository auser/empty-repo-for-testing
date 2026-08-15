use pire_core::{AgentEvent, AgentObserver, Message, ObserverError, SessionWriter};

pub struct CliObserver {
    json: bool,
    session: Option<SessionWriter>,
}

impl CliObserver {
    #[must_use]
    pub fn new(json: bool, session: Option<SessionWriter>) -> Self {
        Self { json, session }
    }
}

impl AgentObserver for CliObserver {
    fn on_message(&mut self, message: &Message) -> Result<(), ObserverError> {
        if let Some(session) = self.session.as_mut() {
            session
                .append_message(message)
                .map_err(|error| ObserverError::new(error.to_string()))?;
        }
        Ok(())
    }

    fn on_event(&mut self, event: &AgentEvent) -> Result<(), ObserverError> {
        if let Some(session) = self.session.as_mut() {
            session
                .append_event(event)
                .map_err(|error| ObserverError::new(error.to_string()))?;
        }

        if self.json {
            let line = serde_json::to_string(event)
                .map_err(|error| ObserverError::new(error.to_string()))?;
            println!("{line}");
            return Ok(());
        }

        match event {
            AgentEvent::ToolStarted { name, .. } => eprintln!("→ {name}"),
            AgentEvent::ToolFinished { name, is_error, .. } => {
                if *is_error {
                    eprintln!("✗ {name}");
                } else {
                    eprintln!("✓ {name}");
                }
            }
            AgentEvent::Final { text } => println!("{text}"),
            AgentEvent::StepStarted { step } => tracing::debug!(step, "agent step started"),
            AgentEvent::ProviderCompleted {
                provider,
                text_present,
                tool_call_count,
            } => tracing::debug!(
                provider,
                text_present,
                tool_call_count,
                "provider completion received"
            ),
        }
        Ok(())
    }
}
