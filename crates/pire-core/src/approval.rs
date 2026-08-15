use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalAction {
    Read,
    Write,
    Process,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub action: ApprovalAction,
    pub tool: String,
    pub summary: String,
}

#[derive(Debug, Error)]
#[error("approval failed: {message}")]
pub struct ApprovalError {
    message: String,
}

impl ApprovalError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub trait ApprovalPolicy {
    fn approve(&mut self, request: &ApprovalRequest) -> Result<bool, ApprovalError>;
}
