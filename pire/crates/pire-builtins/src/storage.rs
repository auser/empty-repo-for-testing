use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}},
    time::{SystemTime, UNIX_EPOCH},
};

use pire_core::{
    LearningObservation, LearningStore, LearningSummary, Plugin, PluginMetadata, Registry,
    SessionRecord, SessionStore, SessionSummary, TaskKind,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct JsonLearningPlugin {
    path: PathBuf,
    enabled: bool,
}

impl JsonLearningPlugin {
    #[must_use]
    pub fn new(path: PathBuf, enabled: bool) -> Self {
        Self { path, enabled }
    }
}

impl Plugin for JsonLearningPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "pire.learning.aggregate-json",
            env!("CARGO_PKG_VERSION"),
            "privacy-preserving aggregate routing learning",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        let store = JsonLearningStore::open(self.path.clone(), self.enabled)?;
        registry
            .register_learning(Arc::new(store))
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ModelStats {
    attempts: u64,
    successes: u64,
    failures: u64,
    positive_feedback: u64,
    negative_feedback: u64,
    latency_total_ms: u128,
    estimated_cost_total_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LearningState {
    enabled: bool,
    observations: u64,
    positive_feedback: u64,
    negative_feedback: u64,
    #[serde(default)]
    models: BTreeMap<String, ModelStats>,
}

impl Default for LearningState {
    fn default() -> Self {
        Self {
            enabled: true,
            observations: 0,
            positive_feedback: 0,
            negative_feedback: 0,
            models: BTreeMap::new(),
        }
    }
}

struct JsonLearningStore {
    path: PathBuf,
    state: Mutex<LearningState>,
}

impl JsonLearningStore {
    fn open(path: PathBuf, enabled: bool) -> Result<Self, String> {
        let mut state = if path.is_file() {
            let bytes = fs::read(&path).map_err(|error| error.to_string())?;
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?
        } else {
            LearningState::default()
        };
        state.enabled = enabled;
        let store = Self {
            path,
            state: Mutex::new(state),
        };
        store.persist()?;
        Ok(store)
    }

    fn with_state<T>(&self, function: impl FnOnce(&mut LearningState) -> T) -> T {
        match self.state.lock() {
            Ok(mut state) => function(&mut state),
            Err(poisoned) => function(&mut poisoned.into_inner()),
        }
    }

    fn snapshot(&self) -> LearningState {
        match self.state.lock() {
            Ok(state) => state.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let bytes = serde_json::to_vec_pretty(&self.snapshot()).map_err(|error| error.to_string())?;
        atomic_write(&self.path, &bytes)
    }
}

impl LearningStore for JsonLearningStore {
    fn id(&self) -> &str {
        "default"
    }

    fn enabled(&self) -> bool {
        self.snapshot().enabled
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.with_state(|state| state.enabled = enabled);
        self.persist()
    }

    fn score(&self, model_id: &str, task: TaskKind) -> f64 {
        let state = self.snapshot();
        if !state.enabled {
            return 0.0;
        }
        let key = stat_key(model_id, task);
        let Some(stats) = state.models.get(&key) else {
            return 0.0;
        };
        if stats.attempts < 3 {
            return 0.0;
        }
        let success_rate = stats.successes as f64 / stats.attempts as f64;
        let feedback_total = stats
            .positive_feedback
            .saturating_add(stats.negative_feedback);
        let feedback = if feedback_total == 0 {
            0.0
        } else {
            (stats.positive_feedback as f64 - stats.negative_feedback as f64)
                / feedback_total as f64
        };
        let average_latency = stats.latency_total_ms as f64 / stats.attempts as f64;
        success_rate * 20.0 + feedback * 10.0 - (average_latency / 10_000.0).min(5.0)
    }

    fn record(&self, observation: LearningObservation) -> Result<(), String> {
        self.with_state(|state| {
            if !state.enabled {
                return;
            }
            state.observations = state.observations.saturating_add(1);
            let stats = state
                .models
                .entry(stat_key(&observation.model_id, observation.task))
                .or_default();
            stats.attempts = stats.attempts.saturating_add(1);
            if observation.success {
                stats.successes = stats.successes.saturating_add(1);
            } else {
                stats.failures = stats.failures.saturating_add(1);
            }
            stats.latency_total_ms = stats
                .latency_total_ms
                .saturating_add(u128::from(observation.latency_ms));
            stats.estimated_cost_total_usd += observation.estimated_cost_usd;
        });
        self.persist()
    }

    fn feedback(&self, model_id: &str, task: TaskKind, positive: bool) -> Result<(), String> {
        self.with_state(|state| {
            if !state.enabled {
                return;
            }
            let stats = state.models.entry(stat_key(model_id, task)).or_default();
            if positive {
                state.positive_feedback = state.positive_feedback.saturating_add(1);
                stats.positive_feedback = stats.positive_feedback.saturating_add(1);
            } else {
                state.negative_feedback = state.negative_feedback.saturating_add(1);
                stats.negative_feedback = stats.negative_feedback.saturating_add(1);
            }
        });
        self.persist()
    }

    fn summary(&self) -> LearningSummary {
        let state = self.snapshot();
        LearningSummary {
            enabled: state.enabled,
            observations: state.observations,
            positive_feedback: state.positive_feedback,
            negative_feedback: state.negative_feedback,
        }
    }

    fn reset(&self) -> Result<(), String> {
        let enabled = self.enabled();
        self.with_state(|state| {
            *state = LearningState {
                enabled,
                ..LearningState::default()
            };
        });
        self.persist()
    }
}

fn stat_key(model_id: &str, task: TaskKind) -> String {
    format!("{model_id}|{}", task.tag())
}

pub struct JsonlSessionPlugin {
    directory: PathBuf,
}

impl JsonlSessionPlugin {
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

impl Plugin for JsonlSessionPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "pire.sessions.jsonl",
            env!("CARGO_PKG_VERSION"),
            "append-only, forkable JSONL sessions",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        registry
            .register_session(Arc::new(JsonlSessionStore {
                directory: self.directory.clone(),
            }))
            .map_err(|error| error.to_string())
    }
}

struct JsonlSessionStore {
    directory: PathBuf,
}

impl JsonlSessionStore {
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err("session id contains invalid characters".to_owned());
        }
        Ok(self.directory.join(format!("{id}.jsonl")))
    }

    fn append_value(&self, session_id: &str, value: &serde_json::Value) -> Result<(), String> {
        let path = self.path(session_id)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut file, value).map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
        file.sync_data().map_err(|error| error.to_string())
    }

    fn summary_from_records(
        &self,
        id: String,
        records: &[serde_json::Value],
    ) -> SessionSummary {
        let first = records.first();
        let created = first
            .and_then(|value| value.get("created_at_ms"))
            .and_then(serde_json::Value::as_u64)
            .map_or(0, u128::from);
        let parent = first
            .and_then(|value| value.get("parent_session_id"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        let name = records
            .iter()
            .rev()
            .find_map(|value| value.get("name").and_then(serde_json::Value::as_str))
            .map(ToOwned::to_owned);
        let updated = records
            .iter()
            .rev()
            .find_map(|value| {
                value
                    .get("timestamp_ms")
                    .and_then(serde_json::Value::as_u64)
            })
            .map_or(created, u128::from);
        SessionSummary {
            id,
            name,
            parent_session_id: parent,
            created_at_ms: created,
            updated_at_ms: updated,
        }
    }

    fn read_values(&self, id: &str) -> Result<Vec<serde_json::Value>, String> {
        let path = self.path(id)?;
        let file = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        let mut values = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|error| error.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            values.push(serde_json::from_str(&line).map_err(|error| error.to_string())?);
        }
        Ok(values)
    }
}

impl SessionStore for JsonlSessionStore {
    fn id(&self) -> &str {
        "default"
    }

    fn create(&self, name: Option<&str>, parent: Option<&str>) -> Result<SessionSummary, String> {
        let id = new_id("session");
        let now = now_millis();
        self.append_value(
            &id,
            &json!({
                "schema": 1,
                "kind": "session",
                "id": id,
                "name": name,
                "parent_session_id": parent,
                "created_at_ms": now,
                "timestamp_ms": now,
            }),
        )?;
        Ok(SessionSummary {
            id,
            name: name.map(ToOwned::to_owned),
            parent_session_id: parent.map(ToOwned::to_owned),
            created_at_ms: now,
            updated_at_ms: now,
        })
    }

    fn append(&self, session_id: &str, record: &SessionRecord) -> Result<(), String> {
        self.append_value(
            session_id,
            &serde_json::to_value(record).map_err(|error| error.to_string())?,
        )
    }

    fn load(&self, session_id: &str) -> Result<Vec<SessionRecord>, String> {
        self.read_values(session_id)?
            .into_iter()
            .filter(|value| value.get("kind").and_then(serde_json::Value::as_str) != Some("session"))
            .map(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
            .collect()
    }

    fn list(&self) -> Result<Vec<SessionSummary>, String> {
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let records = self.read_values(id)?;
            sessions.push(self.summary_from_records(id.to_owned(), &records));
        }
        sessions.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
        Ok(sessions)
    }

    fn rename(&self, session_id: &str, name: &str) -> Result<(), String> {
        self.append_value(
            session_id,
            &json!({
                "kind": "session_name",
                "name": name,
                "timestamp_ms": now_millis(),
            }),
        )
    }

    fn fork(&self, session_id: &str, name: Option<&str>) -> Result<SessionSummary, String> {
        let _ = self.path(session_id)?;
        let fork = self.create(name, Some(session_id))?;
        self.append_value(
            &fork.id,
            &json!({
                "kind": "fork",
                "source_session_id": session_id,
                "timestamp_ms": now_millis(),
            }),
        )?;
        Ok(fork)
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("tmp-{}", new_id("write")));
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error.to_string()
    })
}

fn new_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}-{}-{}",
        std::process::id(),
        now_millis(),
        ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
