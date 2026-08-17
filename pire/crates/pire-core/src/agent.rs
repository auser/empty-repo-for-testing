use std::{sync::Arc, time::Instant};

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::{
    CancellationToken, CompletionRequest, Event, EventSink, LearningObservation, Message,
    ModelDescriptor, ProviderError, Registry, RouteDecision, RouteRequest, RouteStrategy,
    TaskKind, ToolContext, ToolDefinition, Usage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLimits {
    pub max_steps: usize,
    pub max_tool_calls: usize,
}

impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            max_steps: 24,
            max_tool_calls: 96,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentOptions {
    pub router_id: String,
    pub learning_id: Option<String>,
    pub strategy: RouteStrategy,
    pub pinned_model: Option<String>,
    pub pinned_provider: Option<String>,
    pub prefer_local: bool,
    pub max_estimated_cost_usd: Option<f64>,
    pub assumed_output_tokens: u64,
    pub max_fallbacks: usize,
}

impl Default for AgentOptions {
    fn default() -> Self {
        Self {
            router_id: "default".to_owned(),
            learning_id: Some("default".to_owned()),
            strategy: RouteStrategy::Balanced,
            pinned_model: None,
            pinned_provider: None,
            prefer_local: true,
            max_estimated_cost_usd: None,
            assumed_output_tokens: 2_048,
            max_fallbacks: 2,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversation {
    messages: Vec<Message>,
}

impl Conversation {
    #[must_use]
    pub fn new(system_prompt: impl Into<String>) -> Self {
        Self {
            messages: vec![Message::system(system_prompt)],
        }
    }

    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn clear_after_system(&mut self) {
        self.messages.truncate(1);
    }

    #[must_use]
    pub fn estimated_tokens(&self) -> u64 {
        let bytes = self
            .messages
            .iter()
            .map(|message| message.content.len())
            .sum::<usize>();
        u64::try_from(bytes.div_ceil(4)).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone)]
pub struct AgentTurn {
    pub text: String,
    pub route: RouteDecision,
    pub usage: Usage,
    pub estimated_cost_usd: f64,
    pub task: TaskKind,
    pub model_id: String,
    pub provider_id: String,
    pub tool_calls: usize,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent run was cancelled")]
    Cancelled,
    #[error("router `{0}` is not registered")]
    MissingRouter(String),
    #[error("provider `{0}` is not registered")]
    MissingProvider(String),
    #[error("tool `{0}` is not registered")]
    MissingTool(String),
    #[error("routing failed: {0}")]
    Route(String),
    #[error("all routed providers failed: {0}")]
    Providers(String),
    #[error("provider response did not contain text or tool calls")]
    EmptyResponse,
    #[error("agent exceeded its maximum step count")]
    StepLimit,
    #[error("agent exceeded its maximum tool-call count")]
    ToolLimit,
    #[error("tool `{tool}` failed: {message}")]
    Tool { tool: String, message: String },
}

pub struct Agent {
    registry: Arc<Registry>,
    limits: AgentLimits,
}

impl Agent {
    #[must_use]
    pub fn new(registry: Arc<Registry>, limits: AgentLimits) -> Self {
        Self { registry, limits }
    }

    pub fn run_turn(
        &self,
        conversation: &mut Conversation,
        input: impl Into<String>,
        options: &AgentOptions,
        events: &dyn EventSink,
        cancellation: &CancellationToken,
        tool_context: &ToolContext,
    ) -> Result<AgentTurn, AgentError> {
        if cancellation.is_cancelled() {
            return Err(AgentError::Cancelled);
        }

        let input = input.into();
        let task = TaskKind::classify(&input);
        conversation.push(Message::user(input));
        events.emit(&Event::TurnStarted {
            task: task.tag().to_owned(),
        });

        let tools = self
            .registry
            .tools()
            .into_iter()
            .map(|tool| tool.definition())
            .collect::<Vec<_>>();
        let models = self
            .registry
            .providers()
            .into_iter()
            .map(|provider| provider.descriptor().clone())
            .collect::<Vec<_>>();
        let router = self
            .registry
            .router(&options.router_id)
            .ok_or_else(|| AgentError::MissingRouter(options.router_id.clone()))?;
        let learning = options
            .learning_id
            .as_deref()
            .and_then(|id| self.registry.learning(id));
        let route_request = RouteRequest {
            task,
            strategy: options.strategy,
            estimated_input_tokens: conversation.estimated_tokens(),
            assumed_output_tokens: options.assumed_output_tokens,
            requires_tools: !tools.is_empty(),
            pinned_model: options.pinned_model.clone(),
            pinned_provider: options.pinned_provider.clone(),
            prefer_local: options.prefer_local,
            max_estimated_cost_usd: options.max_estimated_cost_usd,
            max_fallbacks: options.max_fallbacks,
        };
        let route = router
            .route(
                &route_request,
                &models,
                learning.as_deref().map(|store| store as &dyn crate::LearningStore),
            )
            .map_err(|error| AgentError::Route(error.to_string()))?;
        events.emit(&Event::RouteSelected {
            decision: route.clone(),
        });

        let started = Instant::now();
        let mut current_candidate = 0usize;
        let mut tool_calls = 0usize;
        let mut had_side_effect = false;
        let mut aggregate_usage = Usage::default();
        let mut last_provider_error: Option<String> = None;

        for _step in 0..self.limits.max_steps {
            if cancellation.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            let candidate = route
                .candidates
                .get(current_candidate)
                .ok_or_else(|| {
                    AgentError::Providers(
                        last_provider_error
                            .clone()
                            .unwrap_or_else(|| "no routed provider was available".to_owned()),
                    )
                })?;
            let provider = self
                .registry
                .provider(&candidate.model.provider_id)
                .ok_or_else(|| AgentError::MissingProvider(candidate.model.provider_id.clone()))?;
            events.emit(&Event::ProviderStarted {
                provider_id: candidate.model.provider_id.clone(),
                model_id: candidate.model.id.clone(),
            });

            let request = CompletionRequest {
                model: candidate.model.model.clone(),
                messages: conversation.messages.clone(),
                tools: tools.clone(),
            };

            let response = match provider.complete(&request, events, cancellation) {
                Ok(response) => response,
                Err(error) => {
                    last_provider_error = Some(format_provider_error(&error));
                    let can_fallback = error.retryable()
                        && !had_side_effect
                        && current_candidate.saturating_add(1) < route.candidates.len()
                        && current_candidate < options.max_fallbacks;
                    if can_fallback {
                        events.emit(&Event::Warning {
                            message: format!(
                                "provider {} failed; trying the next routed candidate: {error}",
                                candidate.model.id
                            ),
                        });
                        current_candidate = current_candidate.saturating_add(1);
                        continue;
                    }
                    record_learning(
                        learning.as_deref(),
                        &candidate.model,
                        task,
                        false,
                        started.elapsed().as_millis(),
                        candidate.estimated_cost_usd,
                    );
                    events.emit(&Event::TurnFinished { success: false });
                    return Err(AgentError::Providers(error.to_string()));
                }
            };

            aggregate_usage.input_tokens = aggregate_usage
                .input_tokens
                .saturating_add(response.usage.input_tokens);
            aggregate_usage.output_tokens = aggregate_usage
                .output_tokens
                .saturating_add(response.usage.output_tokens);

            let assistant_text = response.text.clone().unwrap_or_default();
            conversation.push(Message::assistant(
                assistant_text.clone(),
                response.tool_calls.clone(),
            ));

            if response.tool_calls.is_empty() {
                if response.text.is_none() {
                    return Err(AgentError::EmptyResponse);
                }
                let model = &candidate.model;
                let estimated_cost = estimate_cost(model, &aggregate_usage, &route_request);
                record_learning(
                    learning.as_deref(),
                    model,
                    task,
                    true,
                    started.elapsed().as_millis(),
                    estimated_cost,
                );
                events.emit(&Event::UsageUpdated {
                    usage: aggregate_usage.clone(),
                    estimated_cost_usd: estimated_cost,
                });
                events.emit(&Event::TurnFinished { success: true });
                return Ok(AgentTurn {
                    text: assistant_text,
                    route,
                    usage: aggregate_usage,
                    estimated_cost_usd: estimated_cost,
                    task,
                    model_id: model.id.clone(),
                    provider_id: model.provider_id.clone(),
                    tool_calls,
                });
            }

            for call in response.tool_calls {
                tool_calls = tool_calls.saturating_add(1);
                if tool_calls > self.limits.max_tool_calls {
                    return Err(AgentError::ToolLimit);
                }
                events.emit(&Event::ToolCallStarted { call: call.clone() });
                let tool = self
                    .registry
                    .tool(&call.name)
                    .ok_or_else(|| AgentError::MissingTool(call.name.clone()))?;
                let result = tool.execute(call.arguments.clone(), tool_context);
                match result {
                    Ok(output) => {
                        had_side_effect |= output.side_effect;
                        events.emit(&Event::ToolCallFinished {
                            call_id: call.id.clone(),
                            tool: call.name.clone(),
                            success: true,
                            output: json!({
                                "content": output.content,
                                "data": output.data,
                            }),
                        });
                        conversation.push(Message::tool(
                            call.id,
                            call.name,
                            serde_json::to_string(&json!({
                                "ok": true,
                                "content": output.content,
                                "data": output.data,
                            }))
                            .map_err(|error| AgentError::Tool {
                                tool: "serialization".to_owned(),
                                message: error.to_string(),
                            })?,
                        ));
                    }
                    Err(error) => {
                        events.emit(&Event::ToolCallFinished {
                            call_id: call.id.clone(),
                            tool: call.name.clone(),
                            success: false,
                            output: json!({ "error": error.to_string() }),
                        });
                        conversation.push(Message::tool(
                            call.id,
                            call.name.clone(),
                            serde_json::to_string(&json!({
                                "ok": false,
                                "error": error.to_string(),
                            }))
                            .map_err(|serialization| AgentError::Tool {
                                tool: call.name.clone(),
                                message: serialization.to_string(),
                            })?,
                        ));
                    }
                }
            }
        }

        events.emit(&Event::TurnFinished { success: false });
        Err(AgentError::StepLimit)
    }
}

fn format_provider_error(error: &ProviderError) -> String {
    error.to_string()
}

fn record_learning(
    learning: Option<&dyn crate::LearningStore>,
    model: &ModelDescriptor,
    task: TaskKind,
    success: bool,
    latency_ms: u128,
    estimated_cost_usd: f64,
) {
    let Some(learning) = learning else {
        return;
    };
    let latency_ms = u64::try_from(latency_ms).unwrap_or(u64::MAX);
    let _ = learning.record(LearningObservation {
        model_id: model.id.clone(),
        task,
        success,
        latency_ms,
        estimated_cost_usd,
    });
}

fn estimate_cost(model: &ModelDescriptor, usage: &Usage, route: &RouteRequest) -> f64 {
    let input_tokens = if usage.input_tokens == 0 {
        route.estimated_input_tokens
    } else {
        usage.input_tokens
    };
    let output_tokens = if usage.output_tokens == 0 {
        route.assumed_output_tokens
    } else {
        usage.output_tokens
    };
    (input_tokens as f64 * model.input_cost_per_million
        + output_tokens as f64 * model.output_cost_per_million)
        / 1_000_000.0
}

#[allow(dead_code)]
fn _tool_definitions(tools: &[ToolDefinition]) -> usize {
    tools.len()
}
