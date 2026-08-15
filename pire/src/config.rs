use std::{
    fmt,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};

use clap::{Args, ValueEnum};
use config::{Config as LayeredConfig, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_USER_AGENT: &str = concat!("pire_agent/", env!("CARGO_PKG_VERSION"));

const DEFAULT_CONFIG_PATH: &str = "config/app.toml";
const DEFAULT_SERVER_PORT: u16 = 8080;
const DEFAULT_SERVER_REQUEST_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_BLOCKING_MIN_THREADS: usize = 1;
const DEFAULT_BLOCKING_MAX_THREADS: usize = 2;
const DEFAULT_MAX_STDIN_BYTES: usize = 50 * 1024 * 1024;
const DEFAULT_MAX_HEADER_BYTES: usize = 64 * 1024;
const DEFAULT_READ_CHUNK_BYTES: usize = 16 * 1024;
const DEFAULT_MAX_BUFFERED_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_TEXT_BODY_BYTES: usize = 50 * 1024 * 1024;
const DEFAULT_MAX_REQUEST_HEADERS: usize = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub runtime: RuntimeConfig,
    pub input: InputConfig,
    pub logging: LoggingConfig,
    pub http_client_config: HttpClientConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
    pub request_timeout_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: DEFAULT_SERVER_PORT,
            request_timeout_ms: DEFAULT_SERVER_REQUEST_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfig {
    pub blocking_min_threads: usize,
    pub blocking_max_threads: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            blocking_min_threads: DEFAULT_BLOCKING_MIN_THREADS,
            blocking_max_threads: DEFAULT_BLOCKING_MAX_THREADS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InputConfig {
    pub max_stdin_bytes: usize,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            max_stdin_bytes: DEFAULT_MAX_STDIN_BYTES,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpClientConfig {
    pub request_timeout_secs: Option<u64>,
    pub user_agent: String,
    pub max_header_bytes: usize,
    pub read_chunk_bytes: usize,
    pub max_buffered_bytes: usize,
    pub max_text_body_bytes: usize,
    pub max_request_headers: usize,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: None,
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            max_header_bytes: DEFAULT_MAX_HEADER_BYTES,
            read_chunk_bytes: DEFAULT_READ_CHUNK_BYTES,
            max_buffered_bytes: DEFAULT_MAX_BUFFERED_BYTES,
            max_text_body_bytes: DEFAULT_MAX_TEXT_BODY_BYTES,
            max_request_headers: DEFAULT_MAX_REQUEST_HEADERS,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
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

impl fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Sparse values supplied explicitly on the command line.
///
/// Most configuration fields are intentionally file/environment-only, so a
/// normal new setting is added only to its resolved section above. Add a field
/// here only when that setting genuinely needs a per-invocation CLI flag.
#[derive(Debug, Default, Args, Serialize)]
pub struct CliOverrides {
    #[command(flatten)]
    pub logging: LoggingCliOverrides,

    #[command(flatten)]
    pub http_client_config: HttpClientCliOverrides,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Args, Serialize)]
pub struct LoggingCliOverrides {
    /// Override `logging.level`.
    #[arg(long = "log-level", value_enum, value_name = "LEVEL")]
    pub level: Option<LogLevel>,

    /// Enable or disable JSON logging.
    #[arg(
        long = "json-logs",
        value_name = "BOOL",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true"
    )]
    pub json: Option<bool>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Default, Args, Serialize)]
pub struct HttpClientCliOverrides {
    /// Override `http_client_config.request_timeout_secs`.
    #[arg(
        long = "request-timeout",
        visible_alias = "request-timeout-secs",
        value_name = "SECONDS"
    )]
    pub request_timeout_secs: Option<u64>,

    /// Override `http_client_config.user_agent`.
    #[arg(long, value_name = "USER_AGENT")]
    pub user_agent: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigValidationError {
    #[error("runtime.blocking_min_threads cannot exceed runtime.blocking_max_threads")]
    InvalidBlockingThreadRange,

    #[error("input.max_stdin_bytes must be greater than zero")]
    ZeroStdinLimit,

    #[error("http_client_config.user_agent must be non-empty ASCII without control characters")]
    InvalidUserAgent,

    #[error("http_client_config.read_chunk_bytes must be greater than zero")]
    ZeroReadChunk,

    #[error("http_client_config.max_buffered_bytes must be greater than zero")]
    ZeroBufferedLimit,

    #[error("http_client_config.read_chunk_bytes cannot exceed max_buffered_bytes")]
    ReadChunkExceedsBuffer,

    #[error("http_client_config.max_header_bytes cannot exceed max_buffered_bytes")]
    HeadersExceedBuffer,

    #[error("http_client_config.max_buffered_bytes cannot exceed max_text_body_bytes")]
    BufferExceedsBodyLimit,

    #[error("http_client_config.max_request_headers must be greater than zero")]
    ZeroHeaderCount,
}

impl AppConfig {
    pub fn load(config_path: Option<&Path>, overrides: &CliOverrides) -> Result<Self, ConfigError> {
        let file = match config_path {
            Some(path) => File::from(path.to_path_buf()).required(true),
            None => File::from(PathBuf::from(DEFAULT_CONFIG_PATH)).required(false),
        };

        let cli_source = LayeredConfig::try_from(overrides)?;

        LayeredConfig::builder()
            .add_source(file)
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
        if self.runtime.blocking_min_threads > self.runtime.blocking_max_threads {
            return Err(ConfigValidationError::InvalidBlockingThreadRange);
        }

        if self.input.max_stdin_bytes == 0 {
            return Err(ConfigValidationError::ZeroStdinLimit);
        }

        validate_http_client(&self.http_client_config)
    }
}

pub fn validate_http_client(config: &HttpClientConfig) -> Result<(), ConfigValidationError> {
    let user_agent = config.user_agent.as_bytes();
    if user_agent.is_empty()
        || !user_agent.is_ascii()
        || user_agent.iter().any(u8::is_ascii_control)
    {
        return Err(ConfigValidationError::InvalidUserAgent);
    }

    if config.read_chunk_bytes == 0 {
        return Err(ConfigValidationError::ZeroReadChunk);
    }

    if config.max_buffered_bytes == 0 {
        return Err(ConfigValidationError::ZeroBufferedLimit);
    }

    if config.read_chunk_bytes > config.max_buffered_bytes {
        return Err(ConfigValidationError::ReadChunkExceedsBuffer);
    }

    if config.max_header_bytes > config.max_buffered_bytes {
        return Err(ConfigValidationError::HeadersExceedBuffer);
    }

    if config.max_buffered_bytes > config.max_text_body_bytes {
        return Err(ConfigValidationError::BufferExceedsBodyLimit);
    }

    if config.max_request_headers == 0 {
        return Err(ConfigValidationError::ZeroHeaderCount);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, ConfigValidationError};

    #[test]
    fn defaults_are_valid() {
        assert_eq!(AppConfig::default().validate(), Ok(()));
    }

    #[test]
    fn invalid_buffer_relationship_is_rejected() {
        let mut config = AppConfig::default();
        config.http_client_config.read_chunk_bytes = 4096;
        config.http_client_config.max_buffered_bytes = 1024;

        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::ReadChunkExceedsBuffer)
        );
    }
}
