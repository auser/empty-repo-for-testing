use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Message, ToolCall, ToolDefinition};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,

    #[serde(default)]
    pub output_tokens: u64,

    #[serde(default)]
    pub cached_input_tokens: u64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

impl Usage {
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteInfo {
    pub candidate: String,
    pub provider: String,
    pub model: String,
    pub task: String,
    pub reason: String,

    #[serde(default)]
    pub local: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<RouteInfo>,
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
