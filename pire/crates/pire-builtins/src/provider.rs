use std::{
    collections::BTreeMap,
    io::Read,
    sync::Arc,
    time::Duration,
};

use pire_core::{
    CancellationToken, CompletionRequest, Event, EventSink, Message, ModelCapabilities,
    ModelDescriptor, Plugin, PluginMetadata, Provider, ProviderError, ProviderResponse, Registry,
    Role, ToolCall, Usage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::execution::run_bounded_command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: String,
    pub provider_name: String,
    pub model: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub input_cost_per_million: f64,
    #[serde(default)]
    pub output_cost_per_million: f64,
    #[serde(default = "default_context_window")]
    pub context_window: usize,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_response_bytes")]
    pub max_response_bytes: usize,
}

impl ProviderProfile {
    #[must_use]
    pub fn descriptor(&self, provider_id: impl Into<String>) -> ModelDescriptor {
        ModelDescriptor {
            id: self.id.clone(),
            provider_id: provider_id.into(),
            provider_name: self.provider_name.clone(),
            model: self.model.clone(),
            local: self.local,
            priority: self.priority,
            tags: self.tags.clone(),
            input_cost_per_million: self.input_cost_per_million,
            output_cost_per_million: self.output_cost_per_million,
            context_window: self.context_window,
            capabilities: self.capabilities.clone(),
        }
    }
}

const fn default_context_window() -> usize {
    32_768
}

const fn default_timeout_secs() -> u64 {
    180
}

const fn default_response_bytes() -> usize {
    8 * 1024 * 1024
}

pub struct OfflineProviderPlugin {
    profile: ProviderProfile,
}

impl OfflineProviderPlugin {
    #[must_use]
    pub fn new(profile: ProviderProfile) -> Self {
        Self { profile }
    }
}

impl Plugin for OfflineProviderPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            format!("pire.provider.{}", self.profile.id),
            env!("CARGO_PKG_VERSION"),
            "deterministic offline provider",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        let provider_id = format!("provider.{}", self.profile.id);
        registry
            .register_provider(Arc::new(OfflineProvider {
                descriptor: self.profile.descriptor(provider_id),
            }))
            .map_err(|error| error.to_string())
    }
}

struct OfflineProvider {
    descriptor: ModelDescriptor,
}

impl Provider for OfflineProvider {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        events: &dyn EventSink,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        if cancellation.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let last = request
            .messages
            .last()
            .ok_or_else(|| ProviderError::InvalidResponse("request has no messages".to_owned()))?;
        if last.role == Role::Tool {
            let text = format!("Offline tool result:\n{}", last.content);
            events.emit(&Event::TextDelta { text: text.clone() });
            return Ok(ProviderResponse {
                text: Some(text),
                tool_calls: Vec::new(),
                usage: estimate_usage(request, 8),
            });
        }
        if last.role == Role::User {
            if let Some(call) = parse_tool_directive(&last.content)? {
                return Ok(ProviderResponse {
                    text: None,
                    tool_calls: vec![call],
                    usage: estimate_usage(request, 4),
                });
            }
        }
        let text = format!(
            "Offline provider received {} message(s). Configure a local or cloud model for generated answers.",
            request.messages.len()
        );
        events.emit(&Event::TextDelta { text: text.clone() });
        Ok(ProviderResponse {
            text: Some(text),
            tool_calls: Vec::new(),
            usage: estimate_usage(request, 16),
        })
    }
}

fn parse_tool_directive(input: &str) -> Result<Option<ToolCall>, ProviderError> {
    let Some(rest) = input.trim().strip_prefix("tool:") else {
        return Ok(None);
    };
    let Some((name, arguments)) = rest.trim().split_once(' ') else {
        return Err(ProviderError::InvalidResponse(
            "offline tool directive uses: tool:<name> <json>".to_owned(),
        ));
    };
    let arguments = serde_json::from_str(arguments)
        .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
    Ok(Some(ToolCall {
        id: "offline-tool-1".to_owned(),
        name: name.to_owned(),
        arguments,
    }))
}

pub struct CommandProviderPlugin {
    profile: ProviderProfile,
}

impl CommandProviderPlugin {
    #[must_use]
    pub fn new(profile: ProviderProfile) -> Self {
        Self { profile }
    }
}

impl Plugin for CommandProviderPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            format!("pire.provider.{}", self.profile.id),
            env!("CARGO_PKG_VERSION"),
            "bounded JSON command provider",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        let command = self.profile.command.clone();
        if command.is_empty() {
            return Err("command provider requires a non-empty command array".to_owned());
        }
        let provider_id = format!("provider.{}", self.profile.id);
        registry
            .register_provider(Arc::new(CommandProvider {
                descriptor: self.profile.descriptor(provider_id),
                command,
                timeout: Duration::from_secs(self.profile.timeout_secs),
                max_response_bytes: self.profile.max_response_bytes,
            }))
            .map_err(|error| error.to_string())
    }
}

struct CommandProvider {
    descriptor: ModelDescriptor,
    command: Vec<String>,
    timeout: Duration,
    max_response_bytes: usize,
}

impl Provider for CommandProvider {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        events: &dyn EventSink,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        let (program, args) = self
            .command
            .split_first()
            .ok_or_else(|| ProviderError::Unavailable("empty command".to_owned()))?;
        let payload = serde_json::to_vec(&json!({
            "protocol": "pire-provider-v1",
            "request": request,
        }))
        .map_err(|error| ProviderError::Request(error.to_string()))?;
        let result = run_bounded_command(
            program,
            args,
            &std::env::current_dir().map_err(|error| ProviderError::Request(error.to_string()))?,
            &BTreeMap::new(),
            false,
            Some(&payload),
            self.timeout,
            self.max_response_bytes,
            cancellation,
        )
        .map_err(|error| ProviderError::Request(error.to_string()))?;
        if result.timed_out {
            return Err(ProviderError::Unavailable(
                "command provider timed out".to_owned(),
            ));
        }
        if result.exit_code != Some(0) {
            return Err(ProviderError::Request(format!(
                "command provider exited {:?}: {}",
                result.exit_code, result.stderr
            )));
        }
        let response: ProviderResponse = serde_json::from_str(&result.stdout)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        if let Some(text) = &response.text {
            events.emit(&Event::TextDelta { text: text.clone() });
        }
        Ok(response)
    }
}

#[cfg(feature = "http")]
pub struct OpenAiCompatibleProviderPlugin {
    profile: ProviderProfile,
}

#[cfg(feature = "http")]
impl OpenAiCompatibleProviderPlugin {
    #[must_use]
    pub fn new(profile: ProviderProfile) -> Self {
        Self { profile }
    }
}

#[cfg(feature = "http")]
impl Plugin for OpenAiCompatibleProviderPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            format!("pire.provider.{}", self.profile.id),
            env!("CARGO_PKG_VERSION"),
            "OpenAI-compatible HTTP provider",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        let base_url = self
            .profile
            .base_url
            .clone()
            .ok_or_else(|| "HTTP provider requires base_url".to_owned())?;
        if !(base_url.starts_with("https://") || is_loopback_http(&base_url)) {
            return Err("remote provider base_url must use HTTPS".to_owned());
        }
        let provider_id = format!("provider.{}", self.profile.id);
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(self.profile.timeout_secs))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        registry
            .register_provider(Arc::new(OpenAiCompatibleProvider {
                descriptor: self.profile.descriptor(provider_id),
                base_url: base_url.trim_end_matches('/').to_owned(),
                api_key_env: self.profile.api_key_env.clone(),
                client,
                max_response_bytes: self.profile.max_response_bytes,
            }))
            .map_err(|error| error.to_string())
    }
}

#[cfg(feature = "http")]
struct OpenAiCompatibleProvider {
    descriptor: ModelDescriptor,
    base_url: String,
    api_key_env: Option<String>,
    client: reqwest::blocking::Client,
    max_response_bytes: usize,
}

#[cfg(feature = "http")]
impl Provider for OpenAiCompatibleProvider {
    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    fn complete(
        &self,
        request: &CompletionRequest,
        events: &dyn EventSink,
        cancellation: &CancellationToken,
    ) -> Result<ProviderResponse, ProviderError> {
        if cancellation.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let body = openai_request(request);
        let mut builder = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&body);
        if let Some(environment) = &self.api_key_env {
            let key = std::env::var(environment).map_err(|_| {
                ProviderError::Authentication(format!(
                    "environment variable `{environment}` is not set"
                ))
            })?;
            builder = builder.bearer_auth(key);
        }
        let mut response = builder
            .send()
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(u64::try_from(self.max_response_bytes).unwrap_or(u64::MAX).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| ProviderError::Request(error.to_string()))?;
        if bytes.len() > self.max_response_bytes {
            return Err(ProviderError::InvalidResponse(
                "response exceeded configured byte limit".to_owned(),
            ));
        }
        if !status.is_success() {
            let message = String::from_utf8_lossy(&bytes).into_owned();
            return match status.as_u16() {
                401 | 403 => Err(ProviderError::Authentication(message)),
                429 => Err(ProviderError::RateLimited(message)),
                500..=599 => Err(ProviderError::Unavailable(message)),
                _ => Err(ProviderError::Request(message)),
            };
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let parsed = parse_openai_response(&value)?;
        if let Some(text) = &parsed.text {
            events.emit(&Event::TextDelta { text: text.clone() });
        }
        Ok(parsed)
    }
}

#[cfg(feature = "http")]
fn openai_request(request: &CompletionRequest) -> Value {
    let messages = request.messages.iter().map(openai_message).collect::<Vec<_>>();
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": false,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = Value::String("auto".to_owned());
    }
    body
}

#[cfg(feature = "http")]
fn openai_message(message: &Message) -> Value {
    let role = match message.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    };
    let mut value = json!({ "role": role, "content": message.content });
    if let Some(name) = &message.name {
        value["name"] = Value::String(name.clone());
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        value["tool_call_id"] = Value::String(tool_call_id.clone());
    }
    if !message.tool_calls.is_empty() {
        value["tool_calls"] = Value::Array(
            message
                .tool_calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": call.arguments.to_string(),
                        }
                    })
                })
                .collect(),
        );
    }
    value
}

#[cfg(feature = "http")]
fn parse_openai_response(value: &Value) -> Result<ProviderResponse, ProviderError> {
    let message = value
        .pointer("/choices/0/message")
        .ok_or_else(|| ProviderError::InvalidResponse("missing choices[0].message".to_owned()))?;
    let text = message
        .get("content")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let mut tool_calls = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| ProviderError::InvalidResponse("tool call is missing id".to_owned()))?;
            let function = call.get("function").ok_or_else(|| {
                ProviderError::InvalidResponse("tool call is missing function".to_owned())
            })?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ProviderError::InvalidResponse("tool call is missing name".to_owned()))?;
            let arguments = function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let arguments = serde_json::from_str(arguments)
                .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
            tool_calls.push(ToolCall {
                id: id.to_owned(),
                name: name.to_owned(),
                arguments,
            });
        }
    }
    let usage = Usage {
        input_tokens: value
            .pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: value
            .pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    };
    Ok(ProviderResponse {
        text,
        tool_calls,
        usage,
    })
}

#[cfg(feature = "http")]
fn is_loopback_http(url: &str) -> bool {
    url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost")
        || url.starts_with("http://[::1]")
}

fn estimate_usage(request: &CompletionRequest, output_tokens: u64) -> Usage {
    let bytes = request
        .messages
        .iter()
        .map(|message| message.content.len())
        .sum::<usize>();
    Usage {
        input_tokens: u64::try_from(bytes.div_ceil(4)).unwrap_or(u64::MAX),
        output_tokens,
    }
}
