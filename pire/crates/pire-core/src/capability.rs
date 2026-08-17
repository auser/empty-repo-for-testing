use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
    CancellationToken, CompletionRequest, EventSink, ModelDescriptor, ProviderResponse,
    RouteDecision, RouteRequest, TaskKind, ToolDefinition,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityKind {
    Provider,
    Tool,
    SlashCommand,
    Router,
    Learning,
    Session,
    Resource,
    Execution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub kind: CapabilityKind,
    pub id: String,
    pub plugin_id: String,
    pub description: String,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider request was cancelled")]
    Cancelled,
    #[error("provider is unavailable: {0}")]
    Unavailable(String),
    #[error("provider authentication failed: {0}")]
    Authentication(String),
    #[error("provider rate limited the request: {0}")]
    RateLimited(String),
    #[error("provider request failed: {0}")]
    Request(String),
    #[error("provider returned an invalid response: {0}")]
    InvalidResponse(String),
}

impl ProviderError {
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Unavailable(_) | Self::RateLimited(_) | Self::Request(_)
        )
    }
}

pub trait Provider: Send + Sync {
    fn descriptor(&self) -> &ModelDescriptor;

    fn complete(
        &self,
        request: &CompletionRequest,
        events: &dyn EventSink,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError>;
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool arguments are invalid: {0}")]
    InvalidArguments(String),
    #[error("tool operation was denied: {0}")]
    Denied(String),
    #[error("tool operation was cancelled")]
    Cancelled,
    #[error("tool operation failed: {0}")]
    Execution(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Read,
    Write,
    Process,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub side_effect: bool,
}

impl ToolOutput {
    #[must_use]
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            data: Value::Null,
            side_effect: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ToolLimits {
    pub max_read_bytes: usize,
    pub max_write_bytes: usize,
    pub max_process_output_bytes: usize,
    pub process_timeout: Duration,
    pub max_list_entries: usize,
}

pub struct ToolContext {
    workspace: PathBuf,
    approval: Arc<dyn ApprovalPolicy>,
    execution: Arc<dyn ExecutionBackend>,
    limits: ToolLimits,
    cancellation: CancellationToken,
}

impl ToolContext {
    #[must_use]
    pub fn new(
        workspace: PathBuf,
        approval: Arc<dyn ApprovalPolicy>,
        execution: Arc<dyn ExecutionBackend>,
        limits: ToolLimits,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            workspace,
            approval,
            execution,
            limits,
            cancellation,
        }
    }

    #[must_use]
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    #[must_use]
    pub fn approval(&self) -> &dyn ApprovalPolicy {
        self.approval.as_ref()
    }

    #[must_use]
    pub fn execution(&self) -> &dyn ExecutionBackend {
        self.execution.as_ref()
    }

    #[must_use]
    pub const fn limits(&self) -> ToolLimits {
        self.limits
    }

    #[must_use]
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
}

pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    fn operation(&self) -> Operation;

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError>;
}

pub trait ApprovalPolicy: Send + Sync {
    fn approve(&self, operation: Operation, subject: &str, reason: &str) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: PathBuf,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub clear_env: bool,
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("execution request was cancelled")]
    Cancelled,
    #[error("unable to start process: {0}")]
    Spawn(String),
    #[error("unable to wait for process: {0}")]
    Wait(String),
    #[error("process output exceeded the configured limit")]
    OutputLimit,
    #[error("unable to read process output: {0}")]
    Output(String),
}

pub trait ExecutionBackend: Send + Sync {
    fn id(&self) -> &str;

    fn execute(
        &self,
        request: &ExecutionRequest,
        cancellation: &CancellationToken,
    ) -> Result<ExecutionResult, ExecutionError>;
}

#[derive(Debug, Error)]
pub enum RouteError {
    #[error("no eligible model is available")]
    NoEligibleModel,
    #[error("routing failed: {0}")]
    Failed(String),
}

pub trait Router: Send + Sync {
    fn id(&self) -> &str;

    fn route(
        &self,
        request: &RouteRequest,
        candidates: &[ModelDescriptor],
        learning: Option<&dyn LearningStore>,
    ) -> Result<RouteDecision, RouteError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningObservation {
    pub model_id: String,
    pub task: TaskKind,
    pub success: bool,
    pub latency_ms: u64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LearningSummary {
    pub enabled: bool,
    pub observations: u64,
    pub positive_feedback: u64,
    pub negative_feedback: u64,
}

pub trait LearningStore: Send + Sync {
    fn id(&self) -> &str;
    fn enabled(&self) -> bool;
    fn set_enabled(&self, enabled: bool) -> Result<(), String>;
    fn score(&self, model_id: &str, task: TaskKind) -> f64;
    fn record(&self, observation: LearningObservation) -> Result<(), String>;
    fn feedback(&self, model_id: &str, task: TaskKind, positive: bool) -> Result<(), String>;
    fn summary(&self) -> LearningSummary;
    fn reset(&self) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp_ms: u128,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

pub trait SessionStore: Send + Sync {
    fn id(&self) -> &str;
    fn create(&self, name: Option<&str>, parent: Option<&str>) -> Result<SessionSummary, String>;
    fn append(&self, session_id: &str, record: &SessionRecord) -> Result<(), String>;
    fn load(&self, session_id: &str) -> Result<Vec<SessionRecord>, String>;
    fn list(&self) -> Result<Vec<SessionSummary>, String>;
    fn rename(&self, session_id: &str, name: &str) -> Result<(), String>;
    fn fork(&self, session_id: &str, name: Option<&str>) -> Result<SessionSummary, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceUri(String);

impl ResourceUri {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        let Some((scheme, rest)) = value.split_once("://") else {
            return Err("resource URI must contain ://".to_owned());
        };
        if scheme.is_empty() || rest.is_empty() {
            return Err("resource URI requires a scheme and path".to_owned());
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn scheme(&self) -> &str {
        self.0.split_once("://").map_or("", |(scheme, _)| scheme)
    }

    #[must_use]
    pub fn path(&self) -> &str {
        self.0.split_once("://").map_or("", |(_, path)| path)
    }
}

impl fmt::Display for ResourceUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub uri: ResourceUri,
    pub media_type: String,
    pub content: String,
}

pub trait ResourceResolver: Send + Sync {
    fn id(&self) -> &str;
    fn schemes(&self) -> &[&str];
    fn read(&self, uri: &ResourceUri, max_bytes: usize) -> Result<Resource, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    Help,
    Status,
    Models(Option<String>),
    SelectModel(Option<String>),
    SelectProvider(Option<String>),
    SetRoute(Option<String>),
    ExplainRoute,
    Feedback(bool),
    LearnStatus,
    LearnEnable(bool),
    LearnReset,
    NewSession(Option<String>),
    Sessions,
    Resume(Option<String>),
    RenameSession(String),
    Compact(Option<String>),
    Plugins,
    Tools,
    Trust(Option<String>),
    Settings,
    Clear,
    Submit(String),
    Quit,
    Message(String),
}

pub trait SlashCommand: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, arguments: &str) -> Result<CommandAction, String>;
}
