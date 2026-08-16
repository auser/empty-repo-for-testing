use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

use clap::{Args, ValueEnum};
use config::{Config as LayeredConfig, ConfigError, Environment, File};
use directories::ProjectDirs;
use pire_providers::RoutingStrategy;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    /// Backward-compatible single-provider configuration. It is also used as
    /// the inheritance source for fields omitted from `[[models]]` entries.
    pub provider: ProviderConfig,
    pub router: RouterConfig,
    pub learning: LearningConfig,
    pub models: Vec<ModelConfig>,
    pub agent: AgentConfig,
    pub tools: ToolsConfig,
    pub sessions: SessionsConfig,
    pub security: SecurityConfig,
    pub resources: ResourcesConfig,
    pub ui: UiConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub model: String,
    pub base_url: String,
    pub api_key_env: Option<String>,
    pub command: Option<String>,
    pub request_timeout_secs: u64,
    pub max_response_bytes: usize,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            kind: ProviderKind::Offline,
            model: "offline".to_owned(),
            base_url: "http://127.0.0.1:8080/v1".to_owned(),
            api_key_env: None,
            command: None,
            request_timeout_secs: 120,
            max_response_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    /// Stable selector used by `/model`, routing feedback, and session events.
    pub id: String,
    pub provider: ProviderKind,
    pub provider_name: Option<String>,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub command: Option<String>,
    pub enabled: bool,
    pub local: bool,
    pub priority: i32,
    pub tags: Vec<String>,
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub context_window: usize,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            provider: ProviderKind::Offline,
            provider_name: None,
            model: String::new(),
            base_url: None,
            api_key_env: None,
            command: None,
            enabled: true,
            local: false,
            priority: 0,
            tags: vec!["general".to_owned()],
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            context_window: 128_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RouterConfig {
    pub enabled: bool,
    pub strategy: RoutingStrategy,
    pub default_model: Option<String>,
    pub prefer_local: bool,
    pub allow_fallback: bool,
    pub max_fallbacks: usize,
    pub max_estimated_cost_usd: Option<f64>,
    pub assumed_output_tokens: u64,
    pub exploration_percent: u8,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strategy: RoutingStrategy::Balanced,
            default_model: None,
            prefer_local: true,
            allow_fallback: true,
            max_fallbacks: 2,
            max_estimated_cost_usd: None,
            assumed_output_tokens: 2_048,
            exploration_percent: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LearningConfig {
    /// Aggregate routing outcomes only. Prompt and response bodies are never
    /// stored in the learning file.
    pub enabled: bool,
    pub path: Option<PathBuf>,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfig {
    pub max_steps: usize,
    pub max_tool_calls: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 16,
            max_tool_calls: 64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolsConfig {
    pub allow_read: bool,
    pub allow_write: bool,
    pub allow_process: bool,
    pub max_read_bytes: usize,
    pub max_write_bytes: usize,
    pub max_process_output_bytes: usize,
    pub process_timeout_secs: u64,
    pub max_list_entries: usize,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            allow_read: true,
            allow_write: true,
            allow_process: true,
            max_read_bytes: 4 * 1024 * 1024,
            max_write_bytes: 4 * 1024 * 1024,
            max_process_output_bytes: 2 * 1024 * 1024,
            process_timeout_secs: 120,
            max_list_entries: 10_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionsConfig {
    pub enabled: bool,
    pub directory: Option<PathBuf>,
    pub auto_name: bool,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            directory: None,
            auto_name: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecurityConfig {
    pub require_project_trust: bool,
    pub approval_mode: ApprovalMode,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            require_project_trust: true,
            approval_mode: ApprovalMode::Prompt,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResourcesConfig {
    pub max_total_bytes: usize,
    pub max_compaction_bytes: usize,
    pub include_user_instructions: bool,
    pub load_skills_on_demand: bool,
}

impl Default for ResourcesConfig {
    fn default() -> Self {
        Self {
            max_total_bytes: 1024 * 1024,
            max_compaction_bytes: 256 * 1024,
            include_user_instructions: true,
            load_skills_on_demand: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub color: bool,
    pub show_cost: bool,
    pub history_limit: usize,
    pub completion_limit: usize,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            color: true,
            show_cost: true,
            history_limit: 500,
            completion_limit: 8,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub json: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    #[default]
    Offline,
    OpenAi,
    OpenAiCompatible,
    Command,
}

impl ProviderKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::OpenAi => "open-ai",
            Self::OpenAiCompatible => "open-ai-compatible",
            Self::Command => "command",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    #[default]
    Prompt,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Default, Args, Serialize)]
pub struct CliOverrides {
    #[command(flatten)]
    pub provider: ProviderCliOverrides,
    #[command(flatten)]
    pub router: RouterCliOverrides,
    #[command(flatten)]
    pub agent: AgentCliOverrides,
    #[command(flatten)]
    pub security: SecurityCliOverrides,
    #[command(flatten)]
    pub logging: LoggingCliOverrides,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Args, Serialize)]
pub struct ProviderCliOverrides {
    #[arg(long = "provider", value_enum)]
    pub kind: Option<ProviderKind>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub base_url: Option<String>,
    #[arg(long)]
    pub api_key_env: Option<String>,
    #[arg(long)]
    pub command: Option<String>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Args, Serialize)]
pub struct RouterCliOverrides {
    #[arg(long = "route", value_parser = parse_routing_strategy)]
    pub strategy: Option<RoutingStrategy>,

    #[arg(
        long = "router",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub enabled: Option<bool>,

    #[arg(
        long = "prefer-local",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub prefer_local: Option<bool>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Args, Serialize)]
pub struct AgentCliOverrides {
    #[arg(long)]
    pub max_steps: Option<usize>,
    #[arg(long)]
    pub max_tool_calls: Option<usize>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Args, Serialize)]
pub struct SecurityCliOverrides {
    #[arg(long, value_enum)]
    pub approval_mode: Option<ApprovalMode>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Args, Serialize)]
pub struct LoggingCliOverrides {
    #[arg(long, value_enum)]
    pub level: Option<LogLevel>,
    #[arg(
        long = "json-logs",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub json: Option<bool>,
}

fn parse_routing_strategy(value: &str) -> Result<RoutingStrategy, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "balanced" | "auto" => Ok(RoutingStrategy::Balanced),
        "cost" => Ok(RoutingStrategy::Cost),
        "latency" | "fast" => Ok(RoutingStrategy::Latency),
        "quality" | "best" => Ok(RoutingStrategy::Quality),
        "local" | "local-first" => Ok(RoutingStrategy::LocalFirst),
        _ => Err("expected balanced, cost, latency, quality, or local-first".to_owned()),
    }
}

#[derive(Debug, Error)]
pub enum ConfigValidationError {
    #[error("provider.model cannot be empty")]
    EmptyModel,
    #[error("provider.base_url must start with http:// or https://")]
    InvalidBaseUrl,
    #[error("provider.command is required for the command provider")]
    MissingCommand,
    #[error("provider.request_timeout_secs must be greater than zero")]
    ZeroProviderTimeout,
    #[error("model IDs must be non-empty and unique")]
    InvalidModelId,
    #[error("model {0} has an empty provider model name")]
    EmptyCatalogModel(String),
    #[error("model {0} has invalid cost metadata")]
    InvalidModelCost(String),
    #[error("model {0} has a zero context window")]
    ZeroContextWindow(String),
    #[error("router.assumed_output_tokens must be greater than zero")]
    ZeroAssumedOutput,
    #[error("router.exploration_percent cannot exceed 100")]
    InvalidExplorationPercent,
    #[error("router.max_estimated_cost_usd cannot be negative")]
    InvalidCostLimit,
    #[error("agent limits must be greater than zero")]
    ZeroAgentLimit,
    #[error("tool size, timeout, and entry limits must be greater than zero")]
    ZeroToolLimit,
    #[error("resource size limits must be greater than zero")]
    ZeroResourceLimit,
    #[error("UI history and completion limits must be greater than zero")]
    ZeroUiLimit,
}

impl AppConfig {
    pub fn load(
        workspace: &Path,
        explicit_file: Option<&Path>,
        overrides: &CliOverrides,
        include_workspace: bool,
    ) -> Result<Self, ConfigError> {
        let mut builder = LayeredConfig::builder();
        if let Some(path) = user_config_path() {
            builder = builder.add_source(File::from(path).required(false));
        }
        if include_workspace {
            builder = builder
                .add_source(File::from(workspace.join(".pire/config.toml")).required(false));
        }
        if let Some(path) = explicit_file {
            builder = builder.add_source(File::from(path.to_path_buf()).required(true));
        }
        let cli_source = LayeredConfig::try_from(overrides)?;
        builder
            .add_source(
                Environment::with_prefix("PIRE")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true)
                    .ignore_empty(true),
            )
            .add_source(cli_source)
            .build()?
            .try_deserialize()
    }

    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        validate_provider(&self.provider)?;
        let mut ids = BTreeSet::new();
        for model in &self.models {
            if model.id.trim().is_empty() || !ids.insert(model.id.clone()) {
                return Err(ConfigValidationError::InvalidModelId);
            }
            if model.model.trim().is_empty() {
                return Err(ConfigValidationError::EmptyCatalogModel(model.id.clone()));
            }
            if !model.input_cost_per_million.is_finite()
                || model.input_cost_per_million < 0.0
                || !model.output_cost_per_million.is_finite()
                || model.output_cost_per_million < 0.0
            {
                return Err(ConfigValidationError::InvalidModelCost(model.id.clone()));
            }
            if model.context_window == 0 {
                return Err(ConfigValidationError::ZeroContextWindow(model.id.clone()));
            }
            validate_model_endpoint(model, &self.provider)?;
        }
        if self.router.assumed_output_tokens == 0 {
            return Err(ConfigValidationError::ZeroAssumedOutput);
        }
        if self.router.exploration_percent > 100 {
            return Err(ConfigValidationError::InvalidExplorationPercent);
        }
        if self
            .router
            .max_estimated_cost_usd
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(ConfigValidationError::InvalidCostLimit);
        }
        if self.agent.max_steps == 0 || self.agent.max_tool_calls == 0 {
            return Err(ConfigValidationError::ZeroAgentLimit);
        }
        if self.tools.max_read_bytes == 0
            || self.tools.max_write_bytes == 0
            || self.tools.max_process_output_bytes == 0
            || self.tools.process_timeout_secs == 0
            || self.tools.max_list_entries == 0
        {
            return Err(ConfigValidationError::ZeroToolLimit);
        }
        if self.resources.max_total_bytes == 0 || self.resources.max_compaction_bytes == 0 {
            return Err(ConfigValidationError::ZeroResourceLimit);
        }
        if self.ui.history_limit == 0 || self.ui.completion_limit == 0 {
            return Err(ConfigValidationError::ZeroUiLimit);
        }
        Ok(())
    }

    #[must_use]
    pub fn effective_models(&self) -> Vec<ModelConfig> {
        if !self.models.is_empty() {
            return self.models.clone();
        }
        vec![ModelConfig {
            id: self.provider.model.clone(),
            provider: self.provider.kind,
            provider_name: Some(self.provider.kind.as_str().to_owned()),
            model: self.provider.model.clone(),
            base_url: Some(self.provider.base_url.clone()),
            api_key_env: self.provider.api_key_env.clone(),
            command: self.provider.command.clone(),
            enabled: true,
            local: matches!(
                self.provider.kind,
                ProviderKind::Offline | ProviderKind::Command | ProviderKind::OpenAiCompatible
            ) && self.provider.base_url.contains("127.0.0.1"),
            priority: 0,
            tags: vec!["general".to_owned(), "tools".to_owned()],
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            context_window: 128_000,
        }]
    }
}

fn validate_provider(provider: &ProviderConfig) -> Result<(), ConfigValidationError> {
    if provider.model.trim().is_empty() {
        return Err(ConfigValidationError::EmptyModel);
    }
    if matches!(
        provider.kind,
        ProviderKind::OpenAi | ProviderKind::OpenAiCompatible
    ) && !(provider.base_url.starts_with("http://") || provider.base_url.starts_with("https://"))
    {
        return Err(ConfigValidationError::InvalidBaseUrl);
    }
    if provider.kind == ProviderKind::Command
        && provider.command.as_deref().is_none_or(str::is_empty)
    {
        return Err(ConfigValidationError::MissingCommand);
    }
    if provider.request_timeout_secs == 0 {
        return Err(ConfigValidationError::ZeroProviderTimeout);
    }
    Ok(())
}

fn validate_model_endpoint(
    model: &ModelConfig,
    fallback: &ProviderConfig,
) -> Result<(), ConfigValidationError> {
    let base_url = model.base_url.as_deref().unwrap_or(&fallback.base_url);
    if matches!(
        model.provider,
        ProviderKind::OpenAi | ProviderKind::OpenAiCompatible
    ) && !(base_url.starts_with("http://") || base_url.starts_with("https://"))
    {
        return Err(ConfigValidationError::InvalidBaseUrl);
    }
    if model.provider == ProviderKind::Command
        && model
            .command
            .as_deref()
            .or(fallback.command.as_deref())
            .is_none_or(str::is_empty)
    {
        return Err(ConfigValidationError::MissingCommand);
    }
    Ok(())
}

pub fn user_config_path() -> Option<PathBuf> {
    ProjectDirs::from("dev", "pire", "pire").map(|dirs| dirs.config_dir().join("config.toml"))
}

pub fn user_data_directory() -> Option<PathBuf> {
    ProjectDirs::from("dev", "pire", "pire").map(|dirs| dirs.data_local_dir().to_path_buf())
}
