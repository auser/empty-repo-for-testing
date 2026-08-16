use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use pire_core::{CompletionRequest, Usage};

use super::{Candidate, ModelLearningStats, ModelSummary, RoutingStrategy};

pub(super) struct ScoredCandidate {
    pub(super) index: usize,
    pub(super) score: f64,
    pub(super) task: String,
    pub(super) reason: String,
    pub(super) estimated_cost: f64,
}

pub(super) fn score_candidate(
    candidate: &Candidate,
    task: &str,
    strategy: RoutingStrategy,
    prefer_local: bool,
    stats: &ModelLearningStats,
    exploration: bool,
) -> (f64, String) {
    let mut score = f64::from(candidate.summary.priority);
    let mut reasons = vec![format!("priority {}", candidate.summary.priority)];
    let task_bonus = task_bonus(&candidate.tags, task);
    score += task_bonus;
    if task_bonus > 0.0 {
        reasons.push(format!("{task} capability +{task_bonus:.0}"));
    }
    if prefer_local && candidate.summary.local {
        score += 18.0;
        reasons.push("local preference +18".to_owned());
    }

    let combined_cost = candidate.summary.input_cost_per_million
        + candidate.summary.output_cost_per_million;
    match strategy {
        RoutingStrategy::Cost => {
            let bonus = 35.0 / (1.0 + combined_cost.max(0.0));
            score += bonus;
            reasons.push(format!("cost +{bonus:.1}"));
        }
        RoutingStrategy::Latency => {
            let average = average_latency(stats);
            let bonus = average.map_or(10.0, |value| 25.0 / (1.0 + value / 1_000.0));
            score += bonus;
            reasons.push(format!("latency +{bonus:.1}"));
        }
        RoutingStrategy::Quality => {
            let reliability = reliability(stats);
            let bonus = reliability * 35.0;
            score += bonus;
            reasons.push(format!("quality +{bonus:.1}"));
        }
        RoutingStrategy::LocalFirst => {
            if candidate.summary.local {
                score += 40.0;
                reasons.push("local-first +40".to_owned());
            }
        }
        RoutingStrategy::Balanced => {
            let reliability_bonus = reliability(stats) * 16.0;
            let cost_bonus = 12.0 / (1.0 + combined_cost.max(0.0));
            score += reliability_bonus + cost_bonus;
            reasons.push(format!(
                "balanced reliability +{reliability_bonus:.1}, cost +{cost_bonus:.1}"
            ));
        }
    }

    let feedback_total = stats
        .positive_feedback
        .saturating_add(stats.negative_feedback);
    if feedback_total > 0 {
        let feedback = (stats.positive_feedback as f64 - stats.negative_feedback as f64)
            / feedback_total as f64;
        let bonus = feedback * 12.0;
        score += bonus;
        reasons.push(format!("feedback {bonus:+.1}"));
    }
    if exploration && stats.attempts < 3 {
        score += 8.0;
        reasons.push("bounded exploration +8".to_owned());
    }
    (score, reasons.join(", "))
}

fn task_bonus(tags: &[String], task: &str) -> f64 {
    let has = |tag: &str| tags.iter().any(|value| value.eq_ignore_ascii_case(tag));
    match task {
        "code-review" => {
            if has("review") || has("coding") || has("reasoning") {
                28.0
            } else {
                0.0
            }
        }
        "debugging" => {
            if has("debugging") || has("coding") || has("reasoning") {
                30.0
            } else {
                0.0
            }
        }
        "implementation" => {
            if has("coding") || has("tools") {
                28.0
            } else {
                0.0
            }
        }
        "planning" => {
            if has("planning") || has("reasoning") {
                26.0
            } else {
                0.0
            }
        }
        "summarization" => {
            if has("summarization") || has("fast") || has("cheap") {
                22.0
            } else {
                0.0
            }
        }
        "long-context" => {
            if has("long-context") {
                32.0
            } else {
                0.0
            }
        }
        _ => {
            if has("general") {
                10.0
            } else {
                0.0
            }
        }
    }
}

pub(super) fn classify_task(request: &CompletionRequest) -> String {
    let latest = request
        .messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, pire_core::Role::User))
        .map(|message| message.content.to_ascii_lowercase())
        .unwrap_or_default();
    let total_bytes = request
        .messages
        .iter()
        .map(|message| message.content.len())
        .sum::<usize>();
    if total_bytes > 80_000 {
        return "long-context".to_owned();
    }
    if contains_any(&latest, &["review", "audit", "security", "vulnerability"]) {
        "code-review"
    } else if contains_any(
        &latest,
        &["debug", "failing", "failure", "error", "panic", "broken"],
    ) {
        "debugging"
    } else if contains_any(
        &latest,
        &["implement", "build", "write", "edit", "refactor", "fix"],
    ) {
        "implementation"
    } else if contains_any(
        &latest,
        &["plan", "design", "architecture", "strategy", "compare"],
    ) {
        "planning"
    } else if contains_any(&latest, &["summarize", "summary", "explain", "overview"]) {
        "summarization"
    } else {
        "general"
    }
    .to_owned()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

pub(super) fn estimate_input_tokens(request: &CompletionRequest) -> u64 {
    let bytes = request
        .messages
        .iter()
        .map(|message| message.content.len())
        .sum::<usize>()
        .saturating_add(
            request
                .tools
                .iter()
                .map(|tool| tool.description.len() + tool.input_schema.to_string().len())
                .sum::<usize>(),
        );
    u64::try_from(bytes / 4).unwrap_or(u64::MAX).max(1)
}

pub(super) fn estimated_cost(
    model: &ModelSummary,
    input_tokens: u64,
    output_tokens: u64,
) -> f64 {
    input_tokens as f64 * model.input_cost_per_million / 1_000_000.0
        + output_tokens as f64 * model.output_cost_per_million / 1_000_000.0
}

pub(super) fn apply_usage_cost(
    model: &ModelSummary,
    usage: Option<&mut Usage>,
    latency_ms: u64,
) -> Option<f64> {
    let usage = usage?;
    usage.latency_ms = Some(latency_ms);
    let cost = estimated_cost(model, usage.input_tokens, usage.output_tokens);
    usage.cost_usd = Some(cost);
    Some(cost)
}

fn reliability(stats: &ModelLearningStats) -> f64 {
    (stats.successes as f64 + 1.0) / (stats.attempts as f64 + 2.0)
}

fn average_latency(stats: &ModelLearningStats) -> Option<f64> {
    (stats.attempts > 0).then(|| stats.total_latency_ms as f64 / stats.attempts as f64)
}

pub(super) fn should_explore(request: &CompletionRequest, percent: u8) -> bool {
    if percent == 0 {
        return false;
    }
    let mut hasher = DefaultHasher::new();
    for message in &request.messages {
        message.content.hash(&mut hasher);
    }
    hasher.finish() % 100 < u64::from(percent)
}
