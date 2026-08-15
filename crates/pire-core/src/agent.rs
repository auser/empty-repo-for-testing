use thiserror::Error;

use crate::{
    AgentEvent, AgentObserver, ApprovalPolicy, CompletionRequest, Message, ObserverError, Provider,
    ProviderError, Role, ToolContext, ToolLimits, ToolOutput, ToolRegistry, Workspace,
};

#[derive(Debug, Clone, Copy)]
pub struct AgentConfig {
    pub max_steps: usize,
    pub max_tool_calls: usize,
}

#[derive(Debug, Clone)]
pub struct AgentRunRequest {
    pub model: String,
    pub system_prompt: String,
    pub input: String,
    pub history: Vec<Message>,
}

#[derive(Debug, Clone)]
pub struct AgentRunResult {
    pub final_text: String,
    pub messages: Vec<Message>,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Provider(#[from] ProviderError),

    #[error(transparent)]
    Observer(#[from] ObserverError),

    #[error("provider returned neither text nor tool calls")]
    EmptyResponse,

    #[error("agent exceeded the configured step limit")]
    StepLimit,

    #[error("agent exceeded the configured tool-call limit")]
    ToolCallLimit,
}

pub struct Agent {
    provider: Box<dyn Provider>,
    tools: ToolRegistry,
    config: AgentConfig,
    tool_limits: ToolLimits,
}

impl Agent {
    #[must_use]
    pub fn new(
        provider: Box<dyn Provider>,
        tools: ToolRegistry,
        config: AgentConfig,
        tool_limits: ToolLimits,
    ) -> Self {
        Self {
            provider,
            tools,
            config,
            tool_limits,
        }
    }

    #[must_use]
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    pub fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        self.provider.list_models()
    }

    pub fn run(
        &self,
        request: AgentRunRequest,
        workspace: &Workspace,
        approval: &mut dyn ApprovalPolicy,
        observer: &mut dyn AgentObserver,
    ) -> Result<AgentRunResult, AgentError> {
        let mut messages = request.history;
        if !request.system_prompt.is_empty()
            && !messages.iter().any(|message| message.role == Role::System)
        {
            let message = Message::system(request.system_prompt);
            observer.on_message(&message)?;
            messages.push(message);
        }

        let user_message = Message::user(request.input);
        observer.on_message(&user_message)?;
        messages.push(user_message);

        let mut tool_call_count = 0usize;
        for step in 1..=self.config.max_steps {
            observer.on_event(&AgentEvent::StepStarted { step })?;
            let completion = CompletionRequest {
                model: request.model.clone(),
                messages: messages.clone(),
                tools: self.tools.definitions(),
            };
            let response = self.provider.complete(&completion)?;
            observer.on_event(&AgentEvent::ProviderCompleted {
                provider: self.provider.name().to_owned(),
                text_present: response.text.is_some(),
                tool_call_count: response.tool_calls.len(),
            })?;

            if response.tool_calls.is_empty() {
                let text = response.text.filter(|text| !text.is_empty()).ok_or(AgentError::EmptyResponse)?;
                let message = Message::assistant(text.clone());
                observer.on_message(&message)?;
                messages.push(message);
                observer.on_event(&AgentEvent::Final { text: text.clone() })?;
                return Ok(AgentRunResult {
                    final_text: text,
                    messages,
                });
            }

            let assistant_message = Message::assistant_with_tools(
                response.text.unwrap_or_default(),
                response.tool_calls.clone(),
            );
            observer.on_message(&assistant_message)?;
            messages.push(assistant_message);

            for call in response.tool_calls {
                tool_call_count = tool_call_count.saturating_add(1);
                if tool_call_count > self.config.max_tool_calls {
                    return Err(AgentError::ToolCallLimit);
                }

                observer.on_event(&AgentEvent::ToolStarted {
                    id: call.id.clone(),
                    name: call.name.clone(),
                })?;
                let mut context = ToolContext {
                    workspace,
                    approval,
                    limits: self.tool_limits,
                };
                let output = self
                    .tools
                    .execute(&call.name, &mut context, &call.arguments)
                    .unwrap_or_else(|error| ToolOutput::error(error.to_string()));
                let tool_message = Message::tool(&call, output.content.clone());
                observer.on_message(&tool_message)?;
                messages.push(tool_message);
                observer.on_event(&AgentEvent::ToolFinished {
                    id: call.id,
                    name: call.name,
                    is_error: output.is_error,
                })?;
            }
        }

        Err(AgentError::StepLimit)
    }
}
