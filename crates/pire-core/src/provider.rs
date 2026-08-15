use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Message, ToolCall, ToolDefinition};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ProviderError {
    message: String,
}

impl ProviderError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;

    fn complete(&self, request: &CompletionRequest) -> Result<ProviderResponse, ProviderError>;

    fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(Vec::new())
    }
}
