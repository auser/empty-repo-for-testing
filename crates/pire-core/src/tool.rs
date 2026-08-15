use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{ApprovalPolicy, Workspace};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    #[must_use]
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    #[must_use]
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ToolLimits {
    pub max_read_bytes: usize,
    pub max_write_bytes: usize,
    pub max_process_output_bytes: usize,
    pub process_timeout_seconds: u64,
    pub max_list_entries: usize,
}

pub struct ToolContext<'a> {
    pub workspace: &'a Workspace,
    pub approval: &'a mut dyn ApprovalPolicy,
    pub limits: ToolLimits,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    Unknown(String),

    #[error("duplicate tool registration: {0}")]
    Duplicate(String),

    #[error("invalid tool arguments: {0}")]
    InvalidArguments(String),

    #[error("tool operation was denied: {0}")]
    Denied(String),

    #[error("tool execution failed: {0}")]
    Execution(String),
}

pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    fn execute(
        &self,
        context: &mut ToolContext<'_>,
        arguments: &Value,
    ) -> Result<ToolOutput, ToolError>;
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(&mut self, tool: T) -> Result<(), ToolError>
    where
        T: Tool + 'static,
    {
        let definition = tool.definition();
        if self.tools.contains_key(&definition.name) {
            return Err(ToolError::Duplicate(definition.name));
        }
        self.tools.insert(definition.name, Arc::new(tool));
        Ok(())
    }

    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }

    pub fn execute(
        &self,
        name: &str,
        context: &mut ToolContext<'_>,
        arguments: &Value,
    ) -> Result<ToolOutput, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::Unknown(name.to_owned()))?;
        tool.execute(context, arguments)
    }
}
