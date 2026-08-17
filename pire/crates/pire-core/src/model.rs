use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl Message {
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::plain(Role::System, content)
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::plain(Role::User, content)
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls,
        }
    }

    #[must_use]
    pub fn tool(call_id: impl Into<String>, name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            name: Some(name.into()),
            tool_call_id: Some(call_id.into()),
            tool_calls: Vec::new(),
        }
    }

    fn plain(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub parallel_tools: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub structured_output: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub streaming: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    /// Stable Pire profile identifier, for example `local-coder`.
    pub id: String,
    /// Registry identifier of the provider instance serving this model.
    pub provider_id: String,
    pub provider_name: String,
    /// Model identifier understood by the provider endpoint.
    pub model: String,
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub input_cost_per_million: f64,
    #[serde(default)]
    pub output_cost_per_million: f64,
    #[serde(default = "default_context_window")]
    pub context_window: usize,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
}

const fn default_context_window() -> usize {
    32_768
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

impl Usage {
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub usage: Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteStrategy {
    Balanced,
    Cost,
    Latency,
    Quality,
    LocalFirst,
}

impl Default for RouteStrategy {
    fn default() -> Self {
        Self::Balanced
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskKind {
    General,
    Implementation,
    Review,
    Debugging,
    Planning,
    Summarization,
    LongContext,
}

impl TaskKind {
    #[must_use]
    pub fn classify(input: &str) -> Self {
        let lower = input.to_ascii_lowercase();
        if input.len() > 48_000 {
            return Self::LongContext;
        }
        if contains_any(&lower, &["review", "audit", "security", "vulnerability"]) {
            return Self::Review;
        }
        if contains_any(&lower, &["debug", "fix", "error", "failure", "panic", "bug"]) {
            return Self::Debugging;
        }
        if contains_any(&lower, &["plan", "design", "architecture", "proposal", "adr"]) {
            return Self::Planning;
        }
        if contains_any(&lower, &["summarize", "summary", "condense", "compact"]) {
            return Self::Summarization;
        }
        if contains_any(
            &lower,
            &["implement", "build", "create", "edit", "refactor", "write code"],
        ) {
            return Self::Implementation;
        }
        Self::General
    }

    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Implementation => "coding",
            Self::Review => "review",
            Self::Debugging => "debugging",
            Self::Planning => "planning",
            Self::Summarization => "summarization",
            Self::LongContext => "long-context",
        }
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteRequest {
    pub task: TaskKind,
    pub strategy: RouteStrategy,
    pub estimated_input_tokens: u64,
    pub assumed_output_tokens: u64,
    pub requires_tools: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_provider: Option<String>,
    #[serde(default)]
    pub prefer_local: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_estimated_cost_usd: Option<f64>,
    #[serde(default)]
    pub max_fallbacks: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteCandidate {
    pub model: ModelDescriptor,
    pub score: f64,
    pub estimated_cost_usd: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteDecision {
    pub task: TaskKind,
    pub strategy: RouteStrategy,
    pub candidates: Vec<RouteCandidate>,
    pub reason: String,
}

impl RouteDecision {
    #[must_use]
    pub fn selected(&self) -> Option<&RouteCandidate> {
        self.candidates.first()
    }
}
