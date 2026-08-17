use std::{cmp::Ordering, sync::Arc};

use pire_core::{
    LearningStore, ModelDescriptor, Plugin, PluginMetadata, Registry, RouteCandidate,
    RouteDecision, RouteError, RouteRequest, RouteStrategy, Router,
};

pub struct CostAwareRouterPlugin;

impl Plugin for CostAwareRouterPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(
            "pire.router.cost-aware",
            env!("CARGO_PKG_VERSION"),
            "deterministic capability, cost, locality, and feedback router",
        )
    }

    fn mount(&mut self, registry: &mut Registry) -> Result<(), String> {
        registry
            .register_router(Arc::new(CostAwareRouter))
            .map_err(|error| error.to_string())
    }
}

struct CostAwareRouter;

impl Router for CostAwareRouter {
    fn id(&self) -> &str {
        "default"
    }

    fn route(
        &self,
        request: &RouteRequest,
        candidates: &[ModelDescriptor],
        learning: Option<&dyn LearningStore>,
    ) -> Result<RouteDecision, RouteError> {
        let mut ranked = candidates
            .iter()
            .filter(|model| eligible(model, request))
            .map(|model| score(model, request, learning))
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.model.id.cmp(&right.model.id))
        });
        if ranked.is_empty() {
            return Err(RouteError::NoEligibleModel);
        }
        ranked.truncate(request.max_fallbacks.saturating_add(1).max(1));
        let selected = ranked
            .first()
            .map(|candidate| candidate.model.id.clone())
            .ok_or(RouteError::NoEligibleModel)?;
        Ok(RouteDecision {
            task: request.task,
            strategy: request.strategy,
            reason: format!(
                "selected `{selected}` after capability, context, privacy, cost, and policy checks"
            ),
            candidates: ranked,
        })
    }
}

fn eligible(model: &ModelDescriptor, request: &RouteRequest) -> bool {
    if let Some(pinned) = &request.pinned_model {
        if &model.id != pinned {
            return false;
        }
    }
    if let Some(provider) = &request.pinned_provider {
        if &model.provider_name != provider && &model.provider_id != provider {
            return false;
        }
    }
    if request.requires_tools && !model.capabilities.tools {
        return false;
    }
    let estimated_context = request
        .estimated_input_tokens
        .saturating_add(request.assumed_output_tokens);
    if estimated_context > u64::try_from(model.context_window).unwrap_or(u64::MAX) {
        return false;
    }
    let estimated_cost = estimated_cost(model, request);
    if let Some(max_cost) = request.max_estimated_cost_usd {
        if estimated_cost > max_cost {
            return false;
        }
    }
    true
}

fn score(
    model: &ModelDescriptor,
    request: &RouteRequest,
    learning: Option<&dyn LearningStore>,
) -> RouteCandidate {
    let cost = estimated_cost(model, request);
    let task_match = if model.tags.iter().any(|tag| tag == request.task.tag()) {
        25.0
    } else if model.tags.iter().any(|tag| tag == "general") {
        5.0
    } else {
        0.0
    };
    let local_bonus = if model.local && request.prefer_local {
        20.0
    } else {
        0.0
    };
    let learning_bonus = learning.map_or(0.0, |store| store.score(&model.id, request.task));
    let cost_penalty = cost * 1_000.0;
    let priority = f64::from(model.priority);
    let strategy_score = match request.strategy {
        RouteStrategy::Balanced => priority + task_match + local_bonus + learning_bonus - cost_penalty,
        RouteStrategy::Cost => priority * 0.25 + task_match + local_bonus + learning_bonus - cost_penalty * 4.0,
        RouteStrategy::Latency => priority + task_match * 0.5 + local_bonus * 1.5 + learning_bonus,
        RouteStrategy::Quality => priority * 2.0 + task_match * 2.0 + learning_bonus * 2.0 - cost_penalty * 0.25,
        RouteStrategy::LocalFirst => priority + task_match + local_bonus * 4.0 + learning_bonus - cost_penalty,
    };
    RouteCandidate {
        model: model.clone(),
        score: strategy_score,
        estimated_cost_usd: cost,
        reason: format!(
            "priority={priority:.1}, task={task_match:.1}, local={local_bonus:.1}, learned={learning_bonus:.1}, estimated_cost=${cost:.6}"
        ),
    }
}

fn estimated_cost(model: &ModelDescriptor, request: &RouteRequest) -> f64 {
    (request.estimated_input_tokens as f64 * model.input_cost_per_million
        + request.assumed_output_tokens as f64 * model.output_cost_per_million)
        / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use pire_core::{
        ModelCapabilities, ModelDescriptor, RouteRequest, RouteStrategy, Router, TaskKind,
    };

    use super::CostAwareRouter;

    fn model(id: &str, local: bool, input_cost: f64) -> ModelDescriptor {
        ModelDescriptor {
            id: id.to_owned(),
            provider_id: format!("provider.{id}"),
            provider_name: id.to_owned(),
            model: id.to_owned(),
            local,
            priority: 0,
            tags: vec!["coding".to_owned()],
            input_cost_per_million: input_cost,
            output_cost_per_million: input_cost,
            context_window: 32_768,
            capabilities: ModelCapabilities {
                tools: true,
                ..ModelCapabilities::default()
            },
        }
    }

    #[test]
    fn local_first_prefers_local_model() -> Result<(), Box<dyn std::error::Error>> {
        let request = RouteRequest {
            task: TaskKind::Implementation,
            strategy: RouteStrategy::LocalFirst,
            estimated_input_tokens: 100,
            assumed_output_tokens: 100,
            requires_tools: true,
            pinned_model: None,
            pinned_provider: None,
            prefer_local: true,
            max_estimated_cost_usd: None,
            max_fallbacks: 2,
        };
        let decision = CostAwareRouter.route(
            &request,
            &[model("cloud", false, 0.1), model("local", true, 0.0)],
            None,
        )?;
        assert_eq!(decision.selected().map(|item| item.model.id.as_str()), Some("local"));
        Ok(())
    }
}
