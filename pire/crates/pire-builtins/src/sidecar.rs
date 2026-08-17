use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use pire_core::{
    CancellationToken, CommandAction, CompletionRequest, Event, EventSink, ModelDescriptor,
    Operation, Plugin, PluginMetadata, Provider, ProviderError, ProviderResponse, Registry,
    SlashCommand, Tool, ToolContext, ToolDefinition, ToolError, ToolOutput,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::execution::run_bounded_command;

const PROTOCOL: &str = "pire-plugin-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarManifest {
    pub protocol: String,
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub command: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_message_bytes")]
    pub max_message_bytes: usize,
    #[serde(default)]
    pub providers: Vec<SidecarProviderManifest>,
    #[serde(default)]
    pub tools: Vec<SidecarToolManifest>,
    #[serde(default)]
    pub commands: Vec<SidecarCommandManifest>,
}

impl SidecarManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.protocol != PROTOCOL {
            return Err(format!(
                "sidecar `{}` uses unsupported protocol `{}`",
                self.id, self.protocol
            ));
        }
        if self.id.trim().is_empty() || self.version.trim().is_empty() {
            return Err("sidecar id and version cannot be empty".to_owned());
        }
        if self.command.is_empty() {
            return Err(format!("sidecar `{}` has no command", self.id));
        }
        if self.max_message_bytes == 0 || self.timeout_secs == 0 {
            return Err("sidecar timeout and message limit must be greater than zero".to_owned());
        }
        Ok(())
    }
}

const fn default_timeout_secs() -> u64 {
    120
}

const fn default_message_bytes() -> usize {
    8 * 1024 * 1024
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarProviderManifest {
    pub descriptor: ModelDescriptor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarToolManifest {
    pub definition: ToolDefinition,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SidecarCommandManifest {
    pub name: String,
    pub description: String,
}

pub struct SidecarPlugin {
    manifest: SidecarManifest,
}

impl SidecarPlugin {
    pub fn new(manifest: SidecarManifest) -> Result<Self, String> {
        manifest.validate()?;
        Ok(Self { manifest })
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let manifest = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        Self::new(manifest)
    }
}

impl Plugin for SidecarPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            self.manifest.id.clone(),
            self.manifest.version.clone(),
            self.manifest.description.clone(),
        )
        .with_dependencies(self.manifest.dependencies.clone())
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        let client = Arc::new(SidecarClient {
            plugin_id: self.manifest.id.clone(),
            command: self.manifest.command.clone(),
            timeout: Duration::from_secs(self.manifest.timeout_secs),
            max_message_bytes: self.manifest.max_message_bytes,
        });
        for provider in &self.manifest.providers {
            registry
                .register_provider(Arc::new(SidecarProvider {
                    descriptor: provider.descriptor.clone(),
                    client: Arc::clone(&client),
                }))
                .map_err(|error| error.to_string())?;
        }
        for tool in &self.manifest.tools {
            registry
                .register_tool(Arc::new(SidecarTool {
                    definition: tool.definition.clone(),
                    operation: tool.operation,
                    client: Arc::clone(&client),
                }))
                .map_err(|error| error.to_string())?;
        }
        for command in &self.manifest.commands {
            registry
                .register_command(Arc::new(SidecarCommand {
                    name: command.name.clone(),
                    description: command.description.clone(),
                    client: Arc::clone(&client),
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

struct SidecarClient {
    plugin_id: String,
    command: Vec<String>,
    timeout: Duration,
    max_message_bytes: usize,
}

impl SidecarClient {
    fn call(
        &self,
        capability_kind: &str,
        capability_id: &str,
        request: Value,
        cancellation: &CancellationToken,
    ) -> Result<Value, String> {
        let (program, args) = self
            .command
            .split_first()
            .ok_or_else(|| "sidecar command is empty".to_owned())?;
        let payload = serde_json::to_vec(&json!({
            "protocol": PROTOCOL,
            "plugin_id": self.plugin_id,
            "capability": {
                "kind": capability_kind,
                "id": capability_id,
            },
            "request": request,
        }))
        .map_err(|error| error.to_string())?;
        if payload.len() > self.max_message_bytes {
            return Err("sidecar request exceeds the configured limit".to_owned());
        }
        let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
        let result = run_bounded_command(
            program,
            args,
            &cwd,
            &BTreeMap::new(),
            true,
            Some(&payload),
            self.timeout,
            self.max_message_bytes,
            cancellation,
        )
        .map_err(|error| error.to_string())?;
        if result.timed_out {
            return Err("sidecar timed out".to_owned());
        }
        if result.exit_code != Some(0) {
            return Err(format!(
                "sidecar exited {:?}: {}",
                result.exit_code, result.stderr
            ));
        }
        let response: Value =
            serde_json::from_str(&result.stdout).map_err(|error| error.to_string())?;
        if response.get("protocol").and_then(Value::as_str) != Some(PROTOCOL) {
            return Err("sidecar response has an invalid protocol version".to_owned());
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| "sidecar response is missing result".to_owned())
    }
}

struct SidecarProvider {
    descriptor: ModelDescriptor,
    client: Arc<SidecarClient>,
}

impl Provider for SidecarProvider {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        events: &dyn EventSink,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        let result = self
            .client
            .call(
                "provider",
                &self.descriptor.provider_id,
                serde_json::to_value(request)
                    .map_err(|error| ProviderError::Request(error.to_string()))?,
                cancellation,
            )
            .map_err(ProviderError::Request)?;
        let response: ProviderResponse = serde_json::from_value(result)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        if let Some(text) = &response.text {
            events.emit(&Event::TextDelta { text: text.clone() });
        }
        Ok(response)
    }
}

struct SidecarTool {
    definition: ToolDefinition,
    operation: Operation,
    client: Arc<SidecarClient>,
}

impl Tool for SidecarTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    fn operation(&self) -> Operation {
        self.operation
    }

    fn execute(&self, arguments: Value, context: &ToolContext) -> Result<ToolOutput, ToolError> {
        if !context.approval().approve(
            self.operation,
            &self.definition.name,
            "invoke external plugin tool",
        ) {
            return Err(ToolError::Denied(
                "external plugin tool was not approved".to_owned(),
            ));
        }
        let result = self
            .client
            .call(
                "tool",
                &self.definition.name,
                json!({
                    "arguments": arguments,
                    "workspace": context.workspace(),
                }),
                context.cancellation(),
            )
            .map_err(ToolError::Execution)?;
        serde_json::from_value(result).map_err(|error| ToolError::Execution(error.to_string()))
    }
}

struct SidecarCommand {
    name: String,
    description: String,
    client: Arc<SidecarClient>,
}

impl SlashCommand for SidecarCommand {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn execute(&self, arguments: &str) -> Result<CommandAction, String> {
        let result = self.client.call(
            "slash_command",
            &self.name,
            json!({ "arguments": arguments }),
            &CancellationToken::default(),
        )?;
        if let Some(prompt) = result.get("submit").and_then(Value::as_str) {
            return Ok(CommandAction::Submit(prompt.to_owned()));
        }
        if let Some(message) = result.get("message").and_then(Value::as_str) {
            return Ok(CommandAction::Message(message.to_owned()));
        }
        Err("sidecar command result requires `submit` or `message`".to_owned())
    }
}

#[allow(dead_code)]
fn _path_marker(_path: &PathBuf) {}
