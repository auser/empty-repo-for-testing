use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, Ordering}, Mutex, MutexGuard},
};

use pire_core::ProviderError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelLearningStats {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub positive_feedback: u64,
    pub negative_feedback: u64,
    pub total_latency_ms: u128,
    pub total_cost_usd: f64,
    #[serde(default)]
    pub tasks: BTreeMap<String, TaskLearningStats>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskLearningStats {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LearningData {
    version: u32,
    models: BTreeMap<String, ModelLearningStats>,
}

pub(super) struct LearningStore {
    path: Option<PathBuf>,
    enabled: AtomicBool,
    data: Mutex<LearningData>,
    last_error: Mutex<Option<String>>,
}

impl LearningStore {
    pub(super) fn open(path: Option<PathBuf>, enabled: bool) -> Self {
        let data = path
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| LearningData {
                version: 1,
                models: BTreeMap::new(),
            });
        Self {
            path,
            enabled: AtomicBool::new(enabled),
            data: Mutex::new(data),
            last_error: Mutex::new(None),
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub(super) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub(super) fn stats(&self, model: &str) -> ModelLearningStats {
        lock(&self.data)
            .models
            .get(model)
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn all_stats(&self) -> BTreeMap<String, ModelLearningStats> {
        lock(&self.data).models.clone()
    }

    pub(super) fn record(
        &self,
        model: &str,
        task: &str,
        success: bool,
        latency_ms: u64,
        cost_usd: Option<f64>,
    ) {
        if !self.enabled() {
            return;
        }
        {
            let mut data = lock(&self.data);
            let stats = data.models.entry(model.to_owned()).or_default();
            stats.attempts = stats.attempts.saturating_add(1);
            stats.total_latency_ms = stats
                .total_latency_ms
                .saturating_add(u128::from(latency_ms));
            stats.total_cost_usd += cost_usd.unwrap_or_default();
            let task_stats = stats.tasks.entry(task.to_owned()).or_default();
            task_stats.attempts = task_stats.attempts.saturating_add(1);
            if success {
                stats.successes = stats.successes.saturating_add(1);
                task_stats.successes = task_stats.successes.saturating_add(1);
            } else {
                stats.failures = stats.failures.saturating_add(1);
                task_stats.failures = task_stats.failures.saturating_add(1);
            }
        }
        self.save_best_effort();
    }

    pub(super) fn feedback(&self, model: &str, positive: bool) {
        if !self.enabled() {
            return;
        }
        {
            let mut data = lock(&self.data);
            let stats = data.models.entry(model.to_owned()).or_default();
            if positive {
                stats.positive_feedback = stats.positive_feedback.saturating_add(1);
            } else {
                stats.negative_feedback = stats.negative_feedback.saturating_add(1);
            }
        }
        self.save_best_effort();
    }

    pub(super) fn reset(&self) -> Result<(), ProviderError> {
        *lock(&self.data) = LearningData {
            version: 1,
            models: BTreeMap::new(),
        };
        if let Some(path) = &self.path {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(ProviderError::new(format!(
                        "unable to reset routing learning: {error}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub(super) fn last_error(&self) -> Option<String> {
        lock(&self.last_error).clone()
    }

    fn save_best_effort(&self) {
        if let Err(error) = self.save() {
            *lock(&self.last_error) = Some(error.to_string());
        } else {
            *lock(&self.last_error) = None;
        }
    }

    fn save(&self) -> Result<(), ProviderError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                ProviderError::new(format!("unable to create learning directory: {error}"))
            })?;
        }
        let bytes = serde_json::to_vec_pretty(&*lock(&self.data)).map_err(|error| {
            ProviderError::new(format!("unable to encode routing learning: {error}"))
        })?;
        let temporary = temporary_path(path);
        fs::write(&temporary, bytes).map_err(|error| {
            ProviderError::new(format!("unable to write routing learning: {error}"))
        })?;
        if path.exists() {
            fs::remove_file(path).map_err(|error| {
                ProviderError::new(format!("unable to replace routing learning: {error}"))
            })?;
        }
        fs::rename(&temporary, path).map_err(|error| {
            ProviderError::new(format!("unable to finalize routing learning: {error}"))
        })?;
        Ok(())
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map_or_else(|| "tmp".to_owned(), |value| format!("{value}.tmp"));
    path.with_extension(extension)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
