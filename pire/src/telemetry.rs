use tracing_subscriber::EnvFilter;

use crate::{config::LoggingConfig, error::PireError};

pub fn init(config: &LoggingConfig, verbose: bool) -> Result<(), PireError> {
    let default_level = if verbose {
        "debug"
    } else {
        config.level.as_str()
    };

    if config.json {
        tracing_subscriber::fmt()
            .with_env_filter(filter(default_level))
            .with_target(false)
            .with_writer(std::io::stderr)
            .json()
            .try_init()
            .map_err(|error| PireError::Tracing(error.to_string()))
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter(default_level))
            .with_target(false)
            .with_writer(std::io::stderr)
            .try_init()
            .map_err(|error| PireError::Tracing(error.to_string()))
    }
}

fn filter(default_level: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level))
}
