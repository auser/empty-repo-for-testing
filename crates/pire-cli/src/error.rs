use std::io;

use thiserror::Error;

use crate::{config::ConfigValidationError, trust::TrustError};

#[derive(Debug, Error)]
pub enum PireError {
    #[error("configuration error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("invalid configuration: {0}")]
    InvalidConfig(#[from] ConfigValidationError),
    #[error(transparent)]
    Workspace(#[from] pire_core::WorkspaceError),
    #[error(transparent)]
    ProviderBuild(#[from] pire_providers::ProviderBuildError),
    #[error(transparent)]
    Provider(#[from] pire_core::ProviderError),
    #[error(transparent)]
    Agent(#[from] pire_core::AgentError),
    #[error(transparent)]
    Session(#[from] pire_core::SessionError),
    #[error(transparent)]
    Trust(#[from] TrustError),
    #[error(transparent)]
    Tool(#[from] pire_core::ToolError),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Message(String),
}

impl PireError {
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Config(_) | Self::InvalidConfig(_) => 2,
            Self::Workspace(_)
            | Self::ProviderBuild(_)
            | Self::Provider(_)
            | Self::Agent(_)
            | Self::Session(_)
            | Self::Trust(_)
            | Self::Tool(_)
            | Self::Io(_)
            | Self::Json(_)
            | Self::Message(_) => 1,
        }
    }
}
