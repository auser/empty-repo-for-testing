mod command;
mod offline;
mod openai_compatible;

use std::{path::PathBuf, time::Duration};

use pire_core::Provider;
use thiserror::Error;

pub use command::CommandProvider;
pub use offline::OfflineProvider;
pub use openai_compatible::OpenAiCompatibleProvider;

#[derive(Debug, Clone)]
pub enum ProviderSpec {
    Offline,
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
