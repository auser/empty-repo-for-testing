use pire_core::{CompletionRequest, Provider, ProviderError, ProviderResponse, Role, ToolCall};

pub struct OfflineProvider;

impl Provider for OfflineProvider {
    fn name(&self) -> &str {
        "offline"
    }

    fn complete(&self, request: &CompletionRequest) -> Result<ProviderResponse, ProviderError> {
        let Some(last) = request.messages.last() else {
            return Err(ProviderError::new("offline provider received no messages"));
        };

        if last.role == Role::Tool {
            return Ok(ProviderResponse {
                text: Some(format!("Offline tool result:\n{}", last.content)),
                tool_calls: Vec::new(),
                usage: None,
                route: None,
            });
        }

        if last.role == Role::User
            && let Some(tool_call) = parse_tool_directive(&last.content)?
        {
            return Ok(ProviderResponse {
                text: None,
                tool_calls: vec![tool_call],
                usage: None,
                route: None,
            });
        }

        Ok(ProviderResponse {
            text: Some(format!(
                "Offline provider received {} message(s). Configure a local or remote model to generate an AI response.",
                request.messages.len()
            )),
            tool_calls: Vec::new(),
            usage: None,
            route: None,
        })
    }

    fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec!["offline".to_owned()])
    }
}

fn parse_tool_directive(input: &str) -> Result<Option<ToolCall>, ProviderError> {
    let Some(rest) = input.strip_prefix("tool:") else {
        return Ok(None);
    };
    let Some((name, arguments)) = rest.trim().split_once(' ') else {
        return Err(ProviderError::new(
            "offline tool directives use: tool:<name> <json-arguments>",
        ));
    };
    let arguments = serde_json::from_str(arguments)
        .map_err(|error| ProviderError::new(format!("invalid tool JSON: {error}")))?;
    Ok(Some(ToolCall {
        id: "offline-tool-1".to_owned(),
        name: name.to_owned(),
        arguments,
    }))
}
