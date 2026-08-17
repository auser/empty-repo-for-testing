//! Provider-neutral, plugin-first kernel for Pire.
//!
//! This crate deliberately knows nothing about Clap, terminal rendering,
//! configuration files, HTTP, or a particular model vendor.

mod agent;
mod capability;
mod event;
mod kernel;
mod model;

pub use agent::{Agent, AgentError, AgentLimits, AgentOptions, AgentTurn, Conversation};
pub use capability::{
    ApprovalPolicy, CapabilityDescriptor, CapabilityKind, CommandAction, ExecutionBackend,
    ExecutionError, ExecutionRequest, ExecutionResult, LearningObservation, LearningStore,
    LearningSummary, Operation, Provider, ProviderError, Resource, ResourceResolver, ResourceUri,
    RouteError, Router, SessionRecord, SessionStore, SessionSummary, SlashCommand, Tool,
    ToolContext, ToolError, ToolLimits, ToolOutput,
};
pub use event::{CancellationToken, Event, EventSink, FanoutEventSink, NullEventSink, VecEventSink};
pub use kernel::{Kernel, Plugin, PluginError, PluginMetadata, Registry, RegistryError};
pub use model::{
    CompletionRequest, Message, ModelCapabilities, ModelDescriptor, ProviderResponse, Role,
    RouteCandidate, RouteDecision, RouteRequest, RouteStrategy, TaskKind, ToolCall,
    ToolDefinition, Usage,
};
