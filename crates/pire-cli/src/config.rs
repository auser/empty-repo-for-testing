use std::{fmt, path::{Path, PathBuf}};

use clap::{Args, ValueEnum};
use config::{Config as LayeredConfig, ConfigError, Environment, File};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub provider: ProviderConfig,
    pub agent: AgentConfig,
    pub tools: ToolsConfig,
    pub sessions: SessionsConfig,
    pub security: SecurityConfig,
    pub resources: ResourcesConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResourcesConfig {
    pub max_total_bytes: usize,
    pub include_user_instructions: bool,
}

impl Default for ResourcesConfig {
    fn default() -> Self {
        Self {
            max_total_bytes: 1024 * 1024,
            include_user_instructions: true,
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
        formatter.write_str(match self {
            Self::Offline => "offline",
            Self::OpenAi => "open-ai",
            Self::OpenAiCompatible => "open-ai-compatible",
            Self::Command => "command",
        })
    }
}

#[derive(Debug, Default, Args, Serialize)]
pub struct CliOverrides {
    #[command(flatten)]
    pub provider: ProviderCliOverrides,
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
    #[arg(long, value_enum)]
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
    #[arg(long = "json-logs", num_args = 0..=1, require_equals = true, default_missing_value = "true")]
    pub json: Option<bool>,
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
    #[error("agent limits must be greater than zero")]
    ZeroAgentLimit,
    #[error("tool size, timeout, and entry limits must be greater than zero")]
    ZeroToolLimit,
    #[error("resources.max_total_bytes must be greater than zero")]
    ZeroResourceLimit,
}

impl AppConfig {
    pub fn load(
        workspace: &Path,
        explicit_file: Option<&Path>,
        overrides: &CliOverrides,
    ) -> Result<Self, ConfigError> {
        let mut builder = LayeredConfig::builder();
        if let Some(path) = user_config_path() {
            builder = builder.add_source(File::from(path).required(false));
        }
        builder = builder.add_source(File::from(workspace.join(".pire/config.toml")).required(false));
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
        if self.provider.kind != ProviderKind::Offline && self.provider.model.trim().is_empty() {
            return Err(ConfigValidationError::EmptyModel);
        }
        if matches!(self.provider.kind, ProviderKind::OpenAi | ProviderKind::OpenAiCompatible)
            && !(self.provider.base_url.starts_with("http://")
                || self.provider.base_url.starts_with("https://"))
        {
            return Err(ConfigValidationError::InvalidBaseUrl);
        }
        if self.provider.kind == ProviderKind::Command
            && self.provider.command.as_deref().is_none_or(str::is_empty)
        {
            return Err(ConfigValidationError::MissingCommand);
        }
        if self.provider.request_timeout_secs == 0 {
            return Err(ConfigValidationError::ZeroProviderTimeout);
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
        if self.resources.max_total_bytes == 0 {
            return Err(ConfigValidationError::ZeroResourceLimit);
        }
        Ok(())
    }
}

pub fn user_config_path() -> Option<PathBuf> {
    ProjectDirs::from("dev", "pire", "pire").map(|dirs| dirs.config_dir().join("config.toml"))
}

pub fn user_data_directory() -> Option<PathBuf> {
    ProjectDirs::from("dev", "pire", "pire").map(|dirs| dirs.data_local_dir().to_path_buf())
}
