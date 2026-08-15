use std::{io::Read, time::Duration};

use pire_core::{
    CompletionRequest, Message, Provider, ProviderError, ProviderResponse, Role, ToolCall,
    ToolDefinition,
};
use reqwest::blocking::{Client, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ProviderBuildError;

pub struct OpenAiCompatibleProvider {
    name: String,
    chat_url: String,
    models_url: String,
    api_key: Option<String>,
    client: Client,
    max_response_bytes: usize,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        name: impl Into<String>,
        base_url: impl AsRef<str>,
        api_key: Option<String>,
        timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self, ProviderBuildError> {
        if max_response_bytes == 0 {
            return Err(ProviderBuildError::Invalid(
                "max_response_bytes must be greater than zero".to_owned(),
            ));
        }
        let base_url = base_url.as_ref().trim().trim_end_matches('/');
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(ProviderBuildError::Invalid(
                "base_url must start with http:// or https://".to_owned(),
            ));
        }
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| ProviderBuildError::Initialization(error.to_string()))?;
        Ok(Self {
            name: name.into(),
            chat_url: format!("{base_url}/chat/completions"),
            models_url: format!("{base_url}/models"),
            api_key,
            client,
            max_response_bytes,
        })
    }

    fn request(&self, request: &CompletionRequest) -> Result<ProviderResponse, ProviderError> {
        let body = json!({
            "model": request.model,
            "messages": request.messages.iter().map(message_json).collect::<Vec<_>>(),
            "tools": request.tools.iter().map(tool_json).collect::<Vec<_>>(),
        });
        let mut builder = self.client.post(&self.chat_url).json(&body);
        if let Some(api_key) = self.api_key.as_deref().filter(|value| !value.is_empty()) {
            builder = builder.bearer_auth(api_key);
        }
        let response = builder
            .send()
            .map_err(|error| ProviderError::new(format!("provider request failed: {error}")))?;
        let status = response.status();
        let bytes = read_bounded(response, self.max_response_bytes)?;
        if !status.is_success() {
            return Err(ProviderError::new(format!(
                "provider returned HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        let response: ChatResponse = serde_json::from_slice(&bytes)
            .map_err(|error| ProviderError::new(format!("invalid provider response: {error}")))?;
        let message = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::new("provider returned no choices"))?
            .message;
        let tool_calls = message
            .tool_calls
            .into_iter()
            .map(|call| {
                let arguments = serde_json::from_str(&call.function.arguments).map_err(|error| {
                    ProviderError::new(format!("invalid tool-call arguments: {error}"))
                })?;
                Ok(ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments,
                })
            })
            .collect::<Result<Vec<_>, ProviderError>>()?;
        Ok(ProviderResponse {
            text: message.content,
            tool_calls,
        })
    }
}

impl Provider for OpenAiCompatibleProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn complete(&self, request: &CompletionRequest) -> Result<ProviderResponse, ProviderError> {
        self.request(request)
    }

    fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let mut builder = self.client.get(&self.models_url);
        if let Some(api_key) = self.api_key.as_deref().filter(|value| !value.is_empty()) {
            builder = builder.bearer_auth(api_key);
        }
        let response = builder
            .send()
            .map_err(|error| ProviderError::new(format!("model discovery failed: {error}")))?;
        let status = response.status();
        let bytes = read_bounded(response, self.max_response_bytes)?;
        if !status.is_success() {
            return Err(ProviderError::new(format!(
                "model discovery returned HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        let response: ModelsResponse = serde_json::from_slice(&bytes)
            .map_err(|error| ProviderError::new(format!("invalid models response: {error}")))?;
        Ok(response.data.into_iter().map(|model| model.id).collect())
    }
}

fn read_bounded(mut response: Response, max_bytes: usize) -> Result<Vec<u8>, ProviderError> {
    let mut bytes = Vec::new();
    response
        .by_ref()
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| ProviderError::new(format!("unable to read provider response: {error}")))?;
    if bytes.len() > max_bytes {
        return Err(ProviderError::new(format!(
            "provider response exceeded {max_bytes} bytes"
        )));
    }
    Ok(bytes)
}

fn message_json(message: &Message) -> Value {
    match message.role {
        Role::System => json!({ "role": "system", "content": message.content }),
        Role::User => json!({ "role": "user", "content": message.content }),
        Role::Assistant => {
            let calls = message
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
                .collect::<Vec<_>>();
            json!({
                "role": "assistant",
                "content": message.content,
                "tool_calls": calls,
            })
        }
        Role::Tool => json!({
            "role": "tool",
            "content": message.content,
            "tool_call_id": message.tool_call_id,
        }),
    }
}

fn tool_json(tool: &ToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCall>,
}

#[derive(Debug, Deserialize)]
struct ChatToolCall {
    id: String,
    function: ChatFunction,
}

#[derive(Debug, Deserialize)]
struct ChatFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelRecord>,
}

#[derive(Debug, Deserialize)]
struct ModelRecord {
    id: String,
}

#[cfg(test)]
mod tests {
    use super::OpenAiCompatibleProvider;

    #[test]
    fn rejects_non_http_urls() {
        let result = OpenAiCompatibleProvider::new(
            "local",
            "file:///tmp/model",
            None,
            std::time::Duration::from_secs(1),
            1024,
        );
        assert!(result.is_err());
    }
}
