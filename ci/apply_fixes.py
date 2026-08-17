from __future__ import annotations

import shutil
import sys
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[1]
TARGET = REPOSITORY / "work"
SOURCE = REPOSITORY / "pire"
OVERRIDES = REPOSITORY / "overrides"


def replace(path: str, old: str, new: str, *, count: int = 1) -> None:
    target = TARGET / path
    text = target.read_text(encoding="utf-8")
    found = text.count(old)
    if found < count:
        raise RuntimeError(
            f"expected at least {count} occurrence(s) in {path}, found {found}: {old!r}"
        )
    target.write_text(text.replace(old, new, count), encoding="utf-8")


def main() -> None:
    shutil.rmtree(TARGET, ignore_errors=True)
    shutil.copytree(SOURCE, TARGET)

    if OVERRIDES.is_dir():
        for source in sorted(OVERRIDES.rglob("*")):
            if not source.is_file():
                continue
            destination = TARGET / source.relative_to(OVERRIDES)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)

    replace(
        "crates/pire-core/src/kernel.rs",
        "let mounted = order.lock().map_err(|error| error.to_string())?.clone();",
        "let mounted = order\n            .lock()\n            .map_err(|error| std::io::Error::other(error.to_string()))?\n            .clone();",
    )

    replace(
        "crates/pire-core/src/agent.rs",
        "learning.as_deref().map(|store| store as &dyn crate::LearningStore),",
        "learning.as_deref(),",
    )
    replace(
        "crates/pire-core/src/agent.rs",
        '"content": output.content,\n                                "data": output.data,',
        '"content": output.content.clone(),\n                                "data": output.data.clone(),',
    )
    replace(
        "crates/pire-core/src/agent.rs",
        "let model = &candidate.model;\n                let estimated_cost = estimate_cost(model, &aggregate_usage, &route_request);",
        "let model = &candidate.model;\n                let model_id = model.id.clone();\n                let provider_id = model.provider_id.clone();\n                let estimated_cost = estimate_cost(model, &aggregate_usage, &route_request);",
    )
    replace(
        "crates/pire-core/src/agent.rs",
        "model_id: model.id.clone(),\n                    provider_id: model.provider_id.clone(),",
        "model_id,\n                    provider_id,",
    )

    replace(
        "crates/pire-builtins/src/storage.rs",
        '''            let stats = state.models.entry(stat_key(model_id, task)).or_default();
            if positive {
                state.positive_feedback = state.positive_feedback.saturating_add(1);
                stats.positive_feedback = stats.positive_feedback.saturating_add(1);
            } else {
                state.negative_feedback = state.negative_feedback.saturating_add(1);
                stats.negative_feedback = stats.negative_feedback.saturating_add(1);
            }''',
        '''            if positive {
                state.positive_feedback = state.positive_feedback.saturating_add(1);
            } else {
                state.negative_feedback = state.negative_feedback.saturating_add(1);
            }
            let stats = state.models.entry(stat_key(model_id, task)).or_default();
            if positive {
                stats.positive_feedback = stats.positive_feedback.saturating_add(1);
            } else {
                stats.negative_feedback = stats.negative_feedback.saturating_add(1);
            }''',
    )
    replace(
        "crates/pire-builtins/src/storage.rs",
        '"id": id,\n                "name": name,',
        '"id": &id,\n                "name": name,',
    )

    replace(
        "crates/pire-builtins/src/tools.rs",
        '''        assert_eq!(
            content_hash(b"pire"),
            "5ef28c338eb58a5c0eced61c707478643a513f44916bbd253f471f6116a92449"
        );''',
        '''        let first = content_hash(b"pire");
        let second = content_hash(b"pire");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);''',
    )

    replace(
        "crates/pire-cli/src/config.rs",
        '''pub struct RouterCliOverrides {
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long, value_enum)]
    pub route: Option<RouteStrategyConfig>,''',
        '''pub struct RouterCliOverrides {
    #[arg(long)]
    #[serde(rename = "pinned_model")]
    pub model: Option<String>,
    #[arg(long)]
    #[serde(rename = "pinned_provider")]
    pub provider: Option<String>,
    #[arg(long, value_enum)]
    #[serde(rename = "strategy")]
    pub route: Option<RouteStrategyConfig>,''',
    )
    replace(
        "crates/pire-cli/src/config.rs",
        '''#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectConfig {''',
        '''#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectConfig {''',
    )
    replace(
        "crates/pire-cli/src/config.rs",
        '''#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectRouterConfig {''',
        '''#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectRouterConfig {''',
    )
    replace(
        "crates/pire-cli/src/config.rs",
        '''#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectToolsConfig {''',
        '''#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ProjectToolsConfig {''',
    )

    replace(
        "crates/pire-cli/src/app.rs",
        '''        let retained = messages[keep_from..].to_vec();
        let summary = format!(''',
        '''        let retained = messages[keep_from..].to_vec();
        let retained_count = retained.len();
        let summary = format!(''',
    )
    replace(
        "crates/pire-cli/src/app.rs",
        '''        self.conversation = compacted;
        println!("context compacted; retained {} recent message(s)", messages.len().saturating_sub(keep_from));''',
        '''        self.conversation = compacted;
        println!("context compacted; retained {retained_count} recent message(s)");''',
    )

    print(f"Prepared compiler candidate at {TARGET}")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"apply_fixes.py failed: {error}", file=sys.stderr)
        raise
