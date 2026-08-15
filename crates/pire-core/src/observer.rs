use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    StepStarted { step: usize },
    ProviderCompleted {
        provider: String,
        text_present: bool,
        tool_call_count: usize,
    },
    ToolStarted {
        id: String,
        name: String,
    },
    ToolFinished {
        id: String,
        name: String,
        is_error: bool,
    },
    Final { text: String },
}

#[derive(Debug, Error)]
#[error("observer failed: {message}")]
pub struct ObserverError {
    message: String,
}

impl ObserverError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait AgentObserver {
    fn on_message(&mut self, message: &Message) -> Result<(), ObserverError>;
    fn on_event(&mut self, event: &AgentEvent) -> Result<(), ObserverError>;
}

pub struct NullObserver;

impl AgentObserver for NullObserver {
    fn on_message(&mut self, _message: &Message) -> Result<(), ObserverError> {
        Ok(())
    }

    fn on_event(&mut self, _event: &AgentEvent) -> Result<(), ObserverError> {
        Ok(())
    }
}
