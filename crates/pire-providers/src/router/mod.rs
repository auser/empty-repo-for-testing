use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard,
    },
    time::Instant,
};

use pire_core::{CompletionRequest, Provider, ProviderError, ProviderResponse, RouteInfo};
use serde::{Deserialize, Serialize};

use crate::{build_provider, ProviderBuildError, ProviderSpec};

mod learning;
mod scoring;

pub use learning::{ModelLearningStats, TaskLearningStats};
use learning::LearningStore;
use scoring::{
    apply_usage_cost, classify_task, estimate_input_tokens, estimated_cost, score_candidate,
    should_explore, ScoredCandidate,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingStrategy {
    Cost,
    Latency,
    Quality,
    LocalFirst,
    #[default]
    Balanced,
}

impl RoutingStrategy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cost => "cost",
            Self::Latency => "latency",
            Self::Quality => "quality",
            Self::LocalFirst => "local-first",
            Self::Balanced => "balanced",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoutedModelSpec {
    pub id: String,
    pub provider_name: String,
    pub model: String,
    pub provider: ProviderSpec,
    pub enabled: bool,
    pub local: bool,
    pub priority: i32,
    pub tags: Vec<String>,
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub context_window: usize,
}

#[derive(Debug, Clone)]
pub struct RouterSpec {
    pub models: Vec<RoutedModelSpec>,
    pub enabled: bool,
    pub strategy: RoutingStrategy,
    pub default_model: Option<String>,
    pub prefer_local: bool,
    pub allow_fallback: bool,
    pub max_fallbacks: usize,
    pub max_estimated_cost_usd: Option<f64>,
    pub assumed_output_tokens: u64,
    pub exploration_percent: u8,
    pub learning_enabled: bool,
    pub learning_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelSummary {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub local: bool,
    pub priority: i32,
    pub tags: Vec<String>,
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub context_window: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouterStatus {
    pub automatic: bool,
    pub strategy: RoutingStrategy,
    pub manual_model: Option<String>,
    pub manual_provider: Option<String>,
    pub prefer_local: bool,
    pub learning_enabled: bool,
    pub last_route: Option<RouteInfo>,
    pub total_models: usize,
    pub last_learning_error: Option<String>,
}

struct Candidate {
    summary: ModelSummary,
    tags: Vec<String>,
    provider: Box<dyn Provider>,
}

struct RouterState {
    automatic: AtomicBool,
    strategy: RwLock<RoutingStrategy>,
    manual_model: RwLock<Option<String>>,
    manual_provider: RwLock<Option<String>>,
    last_route: Mutex<Option<RouteInfo>>,
}

pub struct RouterProvider {
    candidates: Vec<Candidate>,
    state: Arc<RouterState>,
    learning: Arc<LearningStore>,
    prefer_local: bool,
    allow_fallback: bool,
    max_fallbacks: usize,
    max_estimated_cost_usd: Option<f64>,
    assumed_output_tokens: u64,
    exploration_percent: u8,
}

#[derive(Clone)]
pub struct RouterControl {
    models: Arc<Vec<ModelSummary>>,
    state: Arc<RouterState>,
    learning: Arc<LearningStore>,
    prefer_local: bool,
}

impl RouterControl {
    #[must_use]
    pub fn models(&self) -> &[ModelSummary] {
        &self.models
    }

    pub fn select_model(&self, selector: Option<&str>) -> Result<(), ProviderError> {
        match selector.map(str::trim) {
            None | Some("") | Some("auto") => {
                *write(&self.state.manual_model) = None;
                *write(&self.state.manual_provider) = None;
                self.state.automatic.store(true, Ordering::Relaxed);
                Ok(())
            }
            Some(selector) => {
                let matches = self
                    .models
                    .iter()
                    .filter(|model| {
                        model.id == selector
                            || model.model == selector
                            || format!("{}/{}", model.provider, model.model) == selector
                    })
                    .collect::<Vec<_>>();
                match matches.as_slice() {
                    [model] => {
                        *write(&self.state.manual_model) = Some(model.id.clone());
                        *write(&self.state.manual_provider) = None;
                        self.state.automatic.store(false, Ordering::Relaxed);
                        Ok(())
                    }
                    [] => Err(ProviderError::new(format!(
                        "no configured model matches {selector}"
                    ))),
                    _ => Err(ProviderError::new(format!(
                        "model selector is ambiguous: {selector}"
                    ))),
                }
            }
        }
    }

    pub fn select_provider(&self, selector: Option<&str>) -> Result<(), ProviderError> {
        match selector.map(str::trim) {
            None | Some("") | Some("auto") => {
                *write(&self.state.manual_provider) = None;
                *write(&self.state.manual_model) = None;
                self.state.automatic.store(true, Ordering::Relaxed);
                Ok(())
            }
            Some(selector) => {
                if !self.models.iter().any(|model| model.provider == selector) {
                    return Err(ProviderError::new(format!(
                        "no configured provider matches {selector}"
                    )));
                }
                *write(&self.state.manual_provider) = Some(selector.to_owned());
                *write(&self.state.manual_model) = None;
                self.state.automatic.store(false, Ordering::Relaxed);
                Ok(())
            }
        }
    }

    pub fn set_strategy(&self, strategy: RoutingStrategy) {
        *write(&self.state.strategy) = strategy;
        self.state.automatic.store(true, Ordering::Relaxed);
        *write(&self.state.manual_model) = None;
        *write(&self.state.manual_provider) = None;
    }

    pub fn set_learning_enabled(&self, enabled: bool) {
        self.learning.set_enabled(enabled);
    }

    pub fn feedback(&self, positive: bool) -> Result<String, ProviderError> {
        let route = lock(&self.state.last_route)
            .clone()
            .ok_or_else(|| ProviderError::new("no routed model has completed yet"))?;
        self.learning.feedback(&route.candidate, positive);
        Ok(route.candidate)
    }

    pub fn reset_learning(&self) -> Result<(), ProviderError> {
        self.learning.reset()
    }

    #[must_use]
    pub fn learning_stats(&self) -> BTreeMap<String, ModelLearningStats> {
        self.learning.all_stats()
    }

    #[must_use]
    pub fn status(&self) -> RouterStatus {
        RouterStatus {
            automatic: self.state.automatic.load(Ordering::Relaxed),
            strategy: *read(&self.state.strategy),
            manual_model: read(&self.state.manual_model).clone(),
            manual_provider: read(&self.state.manual_provider).clone(),
            prefer_local: self.prefer_local,
            learning_enabled: self.learning.enabled(),
            last_route: lock(&self.state.last_route).clone(),
            total_models: self.models.len(),
            last_learning_error: self.learning.last_error(),
        }
    }
}

impl RouterProvider {
    pub fn new(spec: RouterSpec) -> Result<(Self, RouterControl), ProviderBuildError> {
        if spec.models.is_empty() {
            return Err(ProviderBuildError::Invalid(
                "model router requires at least one model".to_owned(),
            ));
        }
        let mut candidates = Vec::new();
        let mut summaries = Vec::new();
        for model in spec.models.into_iter().filter(|model| model.enabled) {
            if model.id.trim().is_empty() || model.model.trim().is_empty() {
                return Err(ProviderBuildError::Invalid(
                    "routed model IDs and model names cannot be empty".to_owned(),
                ));
            }
            if summaries
                .iter()
                .any(|summary: &ModelSummary| summary.id == model.id)
            {
                return Err(ProviderBuildError::Invalid(format!(
                    "duplicate routed model ID: {}",
                    model.id
                )));
            }
            let summary = ModelSummary {
                id: model.id,
                provider: model.provider_name,
                model: model.model,
                local: model.local,
                priority: model.priority,
                tags: model.tags.clone(),
                input_cost_per_million: model.input_cost_per_million,
                output_cost_per_million: model.output_cost_per_million,
                context_window: model.context_window,
            };
            let provider = build_provider(model.provider)?;
            candidates.push(Candidate {
                summary: summary.clone(),
                tags: model.tags,
                provider,
            });
            summaries.push(summary);
        }
        if candidates.is_empty() {
            return Err(ProviderBuildError::Invalid(
                "all routed models are disabled".to_owned(),
            ));
        }

        let state = Arc::new(RouterState {
            automatic: AtomicBool::new(spec.enabled),
            strategy: RwLock::new(spec.strategy),
            manual_model: RwLock::new(if spec.enabled {
                None
            } else {
                spec.default_model.clone()
            }),
            manual_provider: RwLock::new(None),
            last_route: Mutex::new(None),
        });
        if spec.default_model.is_some() && !spec.enabled {
            state.automatic.store(false, Ordering::Relaxed);
        }
        let learning = Arc::new(LearningStore::open(
            spec.learning_path,
            spec.learning_enabled,
        ));
        let models = Arc::new(summaries);
        let control = RouterControl {
            models,
            state: Arc::clone(&state),
            learning: Arc::clone(&learning),
            prefer_local: spec.prefer_local,
        };
        Ok((
            Self {
                candidates,
                state,
                learning,
                prefer_local: spec.prefer_local,
                allow_fallback: spec.allow_fallback,
                max_fallbacks: spec.max_fallbacks,
                max_estimated_cost_usd: spec.max_estimated_cost_usd,
                assumed_output_tokens: spec.assumed_output_tokens,
                exploration_percent: spec.exploration_percent.min(100),
            },
            control,
        ))
    }

    fn ranked(&self, request: &CompletionRequest) -> Result<Vec<ScoredCandidate>, ProviderError> {
        let task = classify_task(request);
        let manual_model = read(&self.state.manual_model).clone();
        let manual_provider = read(&self.state.manual_provider).clone();
        let automatic = self.state.automatic.load(Ordering::Relaxed);
        let strategy = *read(&self.state.strategy);
        let input_tokens = estimate_input_tokens(request);
        let exploration = should_explore(request, self.exploration_percent);
        let mut ranked = Vec::new();

        for (index, candidate) in self.candidates.iter().enumerate() {
            if let Some(manual) = &manual_model
                && candidate.summary.id != *manual
            {
                continue;
            }
            if let Some(provider) = &manual_provider
                && candidate.summary.provider != *provider
            {
                continue;
            }
            if !automatic && manual_model.is_none() && manual_provider.is_none() && index > 0 {
                continue;
            }
            let estimated_cost = estimated_cost(
                &candidate.summary,
                input_tokens,
                self.assumed_output_tokens,
            );
            if self
                .max_estimated_cost_usd
                .is_some_and(|limit| estimated_cost > limit)
            {
                continue;
            }
            let stats = self.learning.stats(&candidate.summary.id);
            let (score, reason) = score_candidate(
                candidate,
                &task,
                strategy,
                self.prefer_local,
                &stats,
                exploration,
            );
            ranked.push(ScoredCandidate {
                index,
                score,
                task: task.clone(),
                reason,
                estimated_cost,
            });
        }

        if ranked.is_empty() {
            return Err(ProviderError::new(
                "no configured model satisfies the active routing constraints",
            ));
        }
        ranked.sort_by(|left, right| {
            right.score.total_cmp(&left.score).then_with(|| {
                self.candidates[left.index]
                    .summary
                    .id
                    .cmp(&self.candidates[right.index].summary.id)
            })
        });
        Ok(ranked)
    }
}

impl Provider for RouterProvider {
    fn name(&self) -> &str {
        "router"
    }

    fn complete(&self, request: &CompletionRequest) -> Result<ProviderResponse, ProviderError> {
        let ranked = self.ranked(request)?;
        let attempts = if self.allow_fallback {
            self.max_fallbacks.saturating_add(1).min(ranked.len())
        } else {
            1
        };
        let mut errors = Vec::new();

        for candidate_score in ranked.into_iter().take(attempts) {
            let candidate = &self.candidates[candidate_score.index];
            let mut routed_request = request.clone();
            routed_request.model.clone_from(&candidate.summary.model);
            let started = Instant::now();
            match candidate.provider.complete(&routed_request) {
                Ok(mut response) => {
                    let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    let cost =
                        apply_usage_cost(&candidate.summary, response.usage.as_mut(), latency_ms);
                    let route = RouteInfo {
                        candidate: candidate.summary.id.clone(),
                        provider: candidate.summary.provider.clone(),
                        model: candidate.summary.model.clone(),
                        task: candidate_score.task.clone(),
                        reason: candidate_score.reason,
                        local: candidate.summary.local,
                        estimated_cost_usd: Some(candidate_score.estimated_cost),
                    };
                    response.route = Some(route.clone());
                    self.learning.record(
                        &candidate.summary.id,
                        &candidate_score.task,
                        true,
                        latency_ms,
                        cost,
                    );
                    *lock(&self.state.last_route) = Some(route);
                    return Ok(response);
                }
                Err(error) => {
                    let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                    self.learning.record(
                        &candidate.summary.id,
                        &candidate_score.task,
                        false,
                        latency_ms,
                        None,
                    );
                    errors.push(format!("{}: {error}", candidate.summary.id));
                }
            }
        }

        Err(ProviderError::new(format!(
            "all routed models failed: {}",
            errors.join("; ")
        )))
    }

    fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(self
            .candidates
            .iter()
            .map(|candidate| candidate.summary.id.clone())
            .collect())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
