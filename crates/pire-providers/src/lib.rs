mod command;
mod offline;
mod openai_compatible;
#[path = "router/mod.rs"]
mod router;

use std::{path::PathBuf, time::Duration};

use pire_core::{CompletionRequest, Provider, ProviderError, ProviderResponse};
use thiserror::Error;

pub use command::CommandProvider;
pub use offline::OfflineProvider;
pub use openai_compatible::OpenAiCompatibleProvider;
pub use router::{
    ModelLearningStats, ModelSummary, RoutedModelSpec, RouterControl, RouterProvider, RouterSpec,
    RouterStatus, RoutingStrategy,
};

#[derive(Debug, Clone)]
pub enum ProviderSpec {
    Offline,
    Unavailable {
        name: String,
        reason: String,
    },
    OpenAi {
        model: String,
        api_key: String,
        timeout: Duration,
        max_response_bytes: usize,
    },
    OpenAiCompatible {
        name: String,
        base_url: String,
        api_key: Option<String>,
        timeout: Duration,
        max_response_bytes: usize,
    },
    Command {
        command: String,
        timeout: Duration,
        max_output_bytes: usize,
        current_dir: PathBuf,
    },
}

#[derive(Debug, Error)]
pub enum ProviderBuildError {
    #[error("provider configuration is invalid: {0}")]
    Invalid(String),

    #[error("provider initialization failed: {0}")]
    Initialization(String),
}

pub fn build_provider(spec: ProviderSpec) -> Result<Box<dyn Provider>, ProviderBuildError> {
    match spec {
        ProviderSpec::Offline => Ok(Box::new(OfflineProvider)),
        ProviderSpec::Unavailable { name, reason } => {
            Ok(Box::new(UnavailableProvider { name, reason }))
        }
        ProviderSpec::OpenAi {
            model: _,
            api_key,
            timeout,
            max_response_bytes,
        } => Ok(Box::new(OpenAiCompatibleProvider::new(
            "openai",
            "https://api.openai.com/v1",
            Some(api_key),
            timeout,
            max_response_bytes,
        )?)),
        ProviderSpec::OpenAiCompatible {
            name,
            base_url,
            api_key,
            timeout,
            max_response_bytes,
        } => Ok(Box::new(OpenAiCompatibleProvider::new(
            name,
            base_url,
            api_key,
            timeout,
            max_response_bytes,
        )?)),
        ProviderSpec::Command {
            command,
            timeout,
            max_output_bytes,
            current_dir,
        } => Ok(Box::new(CommandProvider::new(
            command,
            timeout,
            max_output_bytes,
            current_dir,
        )?)),
    }
}

pub fn build_router(
    spec: RouterSpec,
) -> Result<(Box<dyn Provider>, RouterControl), ProviderBuildError> {
    let (router, control) = RouterProvider::new(spec)?;
    Ok((Box::new(router), control))
}

struct UnavailableProvider {
    name: String,
    reason: String,
}

impl Provider for UnavailableProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn complete(
        &self,
        _request: &CompletionRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        Err(ProviderError::new(format!(
            "provider {} is unavailable: {}",
            self.name, self.reason
        )))
    }
}
