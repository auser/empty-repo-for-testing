use std::{
    fmt,
    path::{Path, PathBuf},
};

use clap::{Args, ValueEnum};
use config::{Config as LayeredConfig, ConfigError, Environment, File};
use directories::ProjectDirs;
use pire_core::{ModelCapabilities, RouteStrategy};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub models: Vec<ModelConfig>,
    pub router: RouterConfig,
    pub agent: AgentConfig,
    pub tools: ToolsConfig,
    pub sessions: SessionsConfig,
    pub security: SecurityConfig,
    pub resources: ResourcesConfig,
    pub learning: LearningConfig,
    pub plugins: PluginsConfig,
    pub ui: UiConfig,
    pub logging: LoggingConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            models: vec![ModelConfig::offline()],
            router: RouterConfig::default(),
            agent: AgentConfig::default(),
            tools: ToolsConfig::default(),
            sessions: SessionsConfig::default(),
            security: SecurityConfig::default(),
            resources: ResourcesConfig::default(),
            learning: LearningConfig::default(),
            plugins: PluginsConfig::default(),
            ui: UiConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub provider_name: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub command: Vec<String>,
    pub enabled: bool,
    pub local: bool,
    pub priority: i32,
    pub tags: Vec<String>,
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub context_window: usize,
    pub capabilities: ModelCapabilities,
    pub timeout_secs: u64,
    pub max_response_bytes: usize,
}

impl ModelConfig {
    #[must_use]
    pub fn offline() -> Self {
        Self {
            id: "offline".to_owned(),
            kind: ProviderKind::Offline,
            provider_name: "offline".to_owned(),
            model: "offline".to_owned(),
            base_url: None,
            api_key_env: None,
            command: Vec::new(),
            enabled: true,
            local: true,
            priority: -100,
            tags: vec!["general".to_owned(), "testing".to_owned()],
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            context_window: 128_000,
            capabilities: ModelCapabilities {
                tools: true,
                ..ModelCapabilities::default()
            },
            timeout_secs: 120,
            max_response_bytes: 8 * 1024 * 1024,
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self::offline()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    #[default]
    Offline,
    OpenAiCompatible,
    Command,
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Offline => "offline",
            Self::OpenAiCompatible => "open-ai-compatible",
            Self::Command => "command",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RouterConfig {
    pub enabled: bool,
    pub strategy: RouteStrategyConfig,
    pub pinned_model: Option<String>,
    pub pinned_provider: Option<String>,
    pub prefer_local: bool,
    pub max_fallbacks: usize,
    pub assumed_output_tokens: u64,
    pub max_estimated_cost_usd: Option<f64>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strategy: RouteStrategyConfig::Balanced,
            pinned_model: None,
            pinned_provider: None,
            prefer_local: true,
            max_fallbacks: 2,
            assumed_output_tokens: 2_048,
            max_estimated_cost_usd: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum RouteStrategyConfig {
    #[default]
    Balanced,
    Cost,
    Latency,
    Quality,
    LocalFirst,
}

impl From<RouteStrategyConfig> for RouteStrategy {
    fn from(value: RouteStrategyConfig) -> Self {
        match value {
            RouteStrategyConfig::Balanced => Self::Balanced,
            RouteStrategyConfig::Cost => Self::Cost,
            RouteStrategyConfig::Latency => Self::Latency,
            RouteStrategyConfig::Quality => Self::Quality,
            RouteStrategyConfig::LocalFirst => Self::LocalFirst,
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
            max_steps: 24,
            max_tool_calls: 96,
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
            allow_process: false,
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
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            directory: None,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalMode {
    #[default]
    Prompt,
    Always,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResourcesConfig {
    pub max_total_bytes: usize,
    pub include_user_instructions: bool,
    pub load_skills_on_demand: bool,
}

impl Default for ResourcesConfig {
    fn default() -> Self {
        Self {
            max_total_bytes: 1024 * 1024,
            include_user_instructions: true,
            load_skills_on_demand: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LearningConfig {
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
pub struct PluginsConfig {
    pub enabled: bool,
    pub directories: Vec<PathBuf>,
    pub timeout_secs: u64,
    pub max_message_bytes: usize,
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            directories: Vec::new(),
            timeout_secs: 120,
            max_message_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    pub color: bool,
    pub history_limit: usize,
    pub show_cost: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            color: true,
            history_limit: 500,
            show_cost: true,
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

#[derive(Debug, Default, Args, Serialize)]
pub struct CliOverrides {
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
pub struct RouterCliOverrides {
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long, value_enum)]
    pub route: Option<RouteStrategyConfig>,
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
    #[arg(long = "log-level", value_enum)]
    pub level: Option<LogLevel>,
    #[arg(
        long = "json-logs",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub json: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectConfig {
    pub router: Option<ProjectRouterConfig>,
    pub agent: Option<AgentConfig>,
    pub tools: Option<ProjectToolsConfig>,
    pub resources: Option<ResourcesConfig>,
    pub ui: Option<UiConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectRouterConfig {
    pub strategy: Option<RouteStrategyConfig>,
    pub prefer_local: Option<bool>,
    pub max_fallbacks: Option<usize>,
    pub assumed_output_tokens: Option<u64>,
    pub max_estimated_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectToolsConfig {
    pub allow_read: Option<bool>,
    pub allow_write: Option<bool>,
    pub allow_process: Option<bool>,
    pub max_read_bytes: Option<usize>,
    pub max_write_bytes: Option<usize>,
    pub max_process_output_bytes: Option<usize>,
    pub process_timeout_secs: Option<u64>,
    pub max_list_entries: Option<usize>,
}

#[derive(Debug, Error)]
pub enum ConfigValidationError {
    #[error("at least one enabled model is required")]
    NoModel,
    #[error("model IDs must be non-empty and unique")]
    InvalidModelId,
    #[error("model `{0}` requires a base_url")]
    MissingBaseUrl(String),
    #[error("model `{0}` requires a command array")]
    MissingCommand(String),
    #[error("model `{0}` has invalid limits")]
    InvalidModelLimits(String),
    #[error("agent, tool, resource, UI, and plugin limits must be greater than zero")]
    ZeroLimit,
}

impl AppConfig {
    pub fn load(
        workspace: &Path,
        trusted: bool,
        explicit_file: Option<&Path>,
        overrides: &CliOverrides,
    ) -> Result<Self, ConfigError> {
        let user = user_config_path();
        let mut global_builder = LayeredConfig::builder();
        if let Some(path) = &user {
            global_builder = global_builder.add_source(File::from(path.clone()).required(false));
        }
        let global: Self = global_builder.build()?.try_deserialize()?;

        let project_source = if trusted {
            load_project_source(&workspace.join(".pire/config.toml"), &global)?
        } else {
            None
        };

        let mut builder = LayeredConfig::builder();
        if let Some(path) = user {
            builder = builder.add_source(File::from(path).required(false));
        }
        if let Some(source) = project_source {
            builder = builder.add_source(source);
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
        let enabled = self.models.iter().filter(|model| model.enabled).collect::<Vec<_>>();
        if enabled.is_empty() {
            return Err(ConfigValidationError::NoModel);
        }
        let mut ids = std::collections::BTreeSet::new();
        for model in enabled {
            if model.id.trim().is_empty() || !ids.insert(model.id.clone()) {
                return Err(ConfigValidationError::InvalidModelId);
            }
            if model.kind == ProviderKind::OpenAiCompatible && model.base_url.is_none() {
                return Err(ConfigValidationError::MissingBaseUrl(model.id.clone()));
            }
            if model.kind == ProviderKind::Command && model.command.is_empty() {
                return Err(ConfigValidationError::MissingCommand(model.id.clone()));
            }
            if model.timeout_secs == 0
                || model.max_response_bytes == 0
                || model.context_window == 0
            {
                return Err(ConfigValidationError::InvalidModelLimits(model.id.clone()));
            }
        }
        if self.agent.max_steps == 0
            || self.agent.max_tool_calls == 0
            || self.router.assumed_output_tokens == 0
            || self.tools.max_read_bytes == 0
            || self.tools.max_write_bytes == 0
            || self.tools.max_process_output_bytes == 0
            || self.tools.process_timeout_secs == 0
            || self.tools.max_list_entries == 0
            || self.resources.max_total_bytes == 0
            || self.plugins.timeout_secs == 0
            || self.plugins.max_message_bytes == 0
            || self.ui.history_limit == 0
        {
            return Err(ConfigValidationError::ZeroLimit);
        }
        Ok(())
    }
}

fn load_project_source(
    path: &Path,
    global: &AppConfig,
) -> Result<Option<LayeredConfig>, ConfigError> {
    if !path.is_file() {
        return Ok(None);
    }
    let mut project: ProjectConfig = LayeredConfig::builder()
        .add_source(File::from(path.to_path_buf()).required(true))
        .build()?
        .try_deserialize()?;
    if let Some(tools) = &mut project.tools {
        tools.allow_read = tools.allow_read.map(|value| value && global.tools.allow_read);
        tools.allow_write = tools.allow_write.map(|value| value && global.tools.allow_write);
        tools.allow_process = tools
            .allow_process
            .map(|value| value && global.tools.allow_process);
    }
    LayeredConfig::try_from(&project).map(Some)
}

#[must_use]
pub fn user_config_path() -> Option<PathBuf> {
    ProjectDirs::from("dev", "pire", "pire").map(|dirs| dirs.config_dir().join("config.toml"))
}

#[must_use]
pub fn user_data_directory() -> Option<PathBuf> {
    ProjectDirs::from("dev", "pire", "pire").map(|dirs| dirs.data_local_dir().to_path_buf())
}
