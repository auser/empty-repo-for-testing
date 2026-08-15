use std::{error::Error as StdError, io};

use thiserror::Error;

use crate::{config::ConfigValidationError, http::HttpClientError};

pub type PireResult<T = ()> = Result<T, PireError>;

#[derive(Debug, Error)]
pub enum PireError {
    #[error("configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("invalid configuration: {0}")]
    InvalidConfig(#[from] ConfigValidationError),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("runtime initialization failed: {0}")]
    Runtime(#[source] Box<asupersync::Error>),

    #[error(transparent)]
    Http(#[from] HttpClientError),

    #[error("tracing initialization failed: {0}")]
    Tracing(String),
}

impl From<asupersync::Error> for PireError {
    fn from(error: asupersync::Error) -> Self {
        Self::Runtime(Box::new(error))
    }
}

impl PireError {
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Config(_) | Self::InvalidConfig(_) => 2,
            Self::Io(_) | Self::Runtime(_) | Self::Http(_) | Self::Tracing(_) => 1,
        }
    }
}

pub fn print_error(error: &PireError, verbose: bool) {
    eprintln!("error: {error}");

    if !verbose {
        return;
    }

    let mut source = error.source();
    while let Some(cause) = source {
        eprintln!("  caused by: {cause}");
        source = cause.source();
    }
}
