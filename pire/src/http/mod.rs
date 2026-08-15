use std::{sync::OnceLock, time::Duration};

use asupersync::tls::{TlsConnector, TlsConnectorBuilder};
use thiserror::Error;

use crate::config::{ConfigValidationError, HttpClientConfig, validate_http_client};

static TLS_CONNECTOR: OnceLock<Result<TlsConnector, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RequestTimeout {
    Default,
    Explicit(Duration),
    Disabled,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ResponseLimits {
    pub max_header_bytes: usize,
    pub read_chunk_bytes: usize,
    pub max_buffered_bytes: usize,
    pub max_text_body_bytes: usize,
    pub max_request_headers: usize,
}

impl From<&HttpClientConfig> for ResponseLimits {
    fn from(config: &HttpClientConfig) -> Self {
        Self {
            max_header_bytes: config.max_header_bytes,
            read_chunk_bytes: config.read_chunk_bytes,
            max_buffered_bytes: config.max_buffered_bytes,
            max_text_body_bytes: config.max_text_body_bytes,
            max_request_headers: config.max_request_headers,
        }
    }
}

#[derive(Debug, Error)]
pub enum HttpClientError {
    #[error("unable to initialize the TLS connector: {0}")]
    TlsInitialization(String),

    #[error(transparent)]
    InvalidConfiguration(#[from] ConfigValidationError),
}

#[derive(Debug)]
pub struct Client {
    tls: TlsConnector,
    user_agent: String,
    request_timeout: RequestTimeout,
    response_limits: ResponseLimits,
}

impl Client {
    pub fn from_config(config: &HttpClientConfig) -> Result<Self, HttpClientError> {
        validate_http_client(config)?;

        let request_timeout = match config.request_timeout_secs {
            None => RequestTimeout::Default,
            Some(0) => RequestTimeout::Disabled,
            Some(seconds) => RequestTimeout::Explicit(Duration::from_secs(seconds)),
        };

        Ok(Self {
            tls: shared_tls_connector()?,
            user_agent: config.user_agent.clone(),
            request_timeout,
            response_limits: ResponseLimits::from(config),
        })
    }

    #[must_use]
    pub fn tls_connector(&self) -> &TlsConnector {
        &self.tls
    }

    #[must_use]
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    #[must_use]
    pub const fn request_timeout(&self) -> RequestTimeout {
        self.request_timeout
    }

    #[must_use]
    pub const fn response_limits(&self) -> ResponseLimits {
        self.response_limits
    }
}

fn shared_tls_connector() -> Result<TlsConnector, HttpClientError> {
    TLS_CONNECTOR
        .get_or_init(|| {
            TlsConnectorBuilder::new()
                .with_webpki_roots()
                .alpn_protocols(vec![b"http/1.1".to_vec()])
                .build()
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(HttpClientError::TlsInitialization)
}
