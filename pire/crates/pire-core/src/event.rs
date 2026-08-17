use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{RouteDecision, ToolCall, Usage};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    TurnStarted {
        task: String,
    },
    RouteSelected {
        decision: RouteDecision,
    },
    ProviderStarted {
        provider_id: String,
        model_id: String,
    },
    TextDelta {
        text: String,
    },
    ToolCallStarted {
        call: ToolCall,
    },
    ToolCallFinished {
        call_id: String,
        tool: String,
        success: bool,
        output: Value,
    },
    UsageUpdated {
        usage: Usage,
        estimated_cost_usd: f64,
    },
    Warning {
        message: String,
    },
    TurnFinished {
        success: bool,
    },
}

pub trait EventSink: Send + Sync {
    fn emit(&self, event: &Event);
}

#[derive(Debug, Default)]
pub struct NullEventSink;

impl EventSink for NullEventSink {
    fn emit(&self, _event: &Event) {}
}

#[derive(Debug, Default)]
pub struct VecEventSink {
    events: Mutex<Vec<Event>>,
}

impl VecEventSink {
    #[must_use]
    pub fn events(&self) -> Vec<Event> {
        match self.events.lock() {
            Ok(events) => events.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

impl EventSink for VecEventSink {
    fn emit(&self, event: &Event) {
        match self.events.lock() {
            Ok(mut events) => events.push(event.clone()),
            Err(poisoned) => poisoned.into_inner().push(event.clone()),
        }
    }
}

pub struct FanoutEventSink {
    sinks: Vec<Arc<dyn EventSink>>,
}

impl FanoutEventSink {
    #[must_use]
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { sinks }
    }
}

impl EventSink for FanoutEventSink {
    fn emit(&self, event: &Event) {
        for sink in &self.sinks {
            sink.emit(event);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
