use std::io;

use thiserror::Error;

use crate::config::ConfigValidationError;

pub type Result<T = ()> = std::result::Result<T, PireError>;

#[derive(Debug, Error)]
pub enum PireError {
    #[error("configuration error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("invalid configuration: {0}")]
    InvalidConfig(#[from] ConfigValidationError),
    #[error("plugin error: {0}")]
    Plugin(#[from] pire_core::PluginError),
    #[error("agent error: {0}")]
    Agent(#[from] pire_core::AgentError),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Message(String),
}

impl PireError {
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Config(_) | Self::InvalidConfig(_) => 2,
            Self::Plugin(_) | Self::Agent(_) | Self::Io(_) | Self::Message(_) => 1,
        }
    }
}
