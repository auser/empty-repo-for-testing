use std::{
    io::{self, Write},
    sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}},
    time::{SystemTime, UNIX_EPOCH},
};

use pire_core::{Event, EventSink, SessionRecord, SessionStore};

static RECORD_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct TerminalEventSink {
    streaming: bool,
    wrote_text: Mutex<bool>,
}

impl TerminalEventSink {
    #[must_use]
    pub fn new(streaming: bool) -> Self {
        Self {
            streaming,
            wrote_text: Mutex::new(false),
        }
    }

    #[must_use]
    pub fn wrote_text(&self) -> bool {
        match self.wrote_text.lock() {
            Ok(value) => *value,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }
}

impl EventSink for TerminalEventSink {
    fn emit(&self, event: &Event) {
        match event {
            Event::TextDelta { text } if self.streaming => {
                print!("{text}");
                let _ = io::stdout().flush();
                match self.wrote_text.lock() {
                    Ok(mut value) => *value = true,
                    Err(poisoned) => *poisoned.into_inner() = true,
                }
            }
            Event::Warning { message } => eprintln!("warning: {message}"),
            Event::ToolCallStarted { call } => {
                eprintln!("→ {}", call.name);
            }
            Event::ToolCallFinished { tool, success, .. } => {
                eprintln!("← {tool} {}", if *success { "ok" } else { "failed" });
            }
            _ => {}
        }
    }
}

pub struct JsonEventSink;

impl EventSink for JsonEventSink {
    fn emit(&self, event: &Event) {
        match serde_json::to_string(event) {
            Ok(line) => println!("{line}"),
            Err(error) => eprintln!("unable to serialize event: {error}"),
        }
    }
}

pub struct SessionEventSink {
    store: Arc<dyn SessionStore>,
    session_id: String,
}

impl SessionEventSink {
    #[must_use]
    pub fn new(store: Arc<dyn SessionStore>, session_id: String) -> Self {
        Self { store, session_id }
    }
}

impl EventSink for SessionEventSink {
    fn emit(&self, event: &Event) {
        let payload = match serde_json::to_value(event) {
            Ok(payload) => payload,
            Err(_) => return,
        };
        let record = SessionRecord {
            id: format!(
                "record-{}-{}",
                now_millis(),
                RECORD_COUNTER.fetch_add(1, Ordering::Relaxed)
            ),
            parent_id: None,
            timestamp_ms: now_millis(),
            kind: "agent_event".to_owned(),
            payload,
        };
        let _ = self.store.append(&self.session_id, &record);
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
