from __future__ import annotations

import pathlib
import sys


def replace_once(path: pathlib.Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"expected exactly one match in {path} but found {count}: {old!r}"
        )
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: apply-overrides.py PROJECT_ROOT")

    root = pathlib.Path(sys.argv[1])
    mutation = root / "crates/pire-core/src/mutation.rs"
    workspace = root / "crates/pire-core/src/workspace.rs"
    command = root / "crates/pire-providers/src/command.rs"
    offline = root / "crates/pire-providers/src/offline.rs"
    openai = root / "crates/pire-providers/src/openai_compatible.rs"
    router = root / "crates/pire-providers/src/router.rs"
    interactive = root / "crates/pire-cli/src/interactive.rs"
    app = root / "crates/pire-cli/src/app.rs"
    resources = root / "crates/pire-cli/src/resources.rs"
    main_rs = root / "crates/pire-cli/src/main.rs"
    output = root / "crates/pire-cli/src/output.rs"
    config = root / "crates/pire-cli/src/config.rs"

    replace_once(
        mutation,
        "use std::{collections::BTreeMap, path::PathBuf, sync::Mutex};",
        "use std::{\n"
        "    collections::BTreeMap,\n"
        "    path::{Path, PathBuf},\n"
        "    sync::Mutex,\n"
        "};",
    )
    replace_once(
        mutation,
        "fn read_optional(&self, path: &PathBuf)",
        "fn read_optional(&self, path: &Path)",
    )
    replace_once(
        mutation,
        "    path: &PathBuf,\n    old: Option<&[u8]>,",
        "    path: &Path,\n    old: Option<&[u8]>,",
    )
    replace_once(
        mutation,
        "        _ => Err(MutationError::Stale(path.clone())),",
        "        _ => Err(MutationError::Stale(path.to_path_buf())),",
    )
    replace_once(
        workspace,
        "fs::rename(&temporary, &destination).or_else(|error| {\n"
        "            let _ = fs::remove_file(&temporary);\n"
        "            Err(error)\n"
        "        })?;",
        "fs::rename(&temporary, &destination).inspect_err(|_| {\n"
        "            let _ = fs::remove_file(&temporary);\n"
        "        })?;",
    )

    replace_once(
        command,
        '''                    if let Some(protocol) = envelope.protocol.as_deref() {
                        if protocol != "pire-provider" || envelope.version != Some(1) {
                            return Err(ProviderError::new(
                                ProviderErrorKind::Protocol,
                                "unsupported command-provider protocol",
                            ));
                        }
                    }''',
        '''                    if let Some(protocol) = envelope.protocol.as_deref()
                        && (protocol != "pire-provider" || envelope.version != Some(1))
                    {
                        return Err(ProviderError::new(
                            ProviderErrorKind::Protocol,
                            "unsupported command-provider protocol",
                        ));
                    }''',
    )
    replace_once(
        offline,
        '''        if last.role == Role::User {
            if let Some(call) = parse_tool_directive(&last.content)? {
                sink.event(ProviderEvent::ToolCall(call.clone()))?;
                return Ok(ProviderResponse {
                    tool_calls: vec![call],
                    ..ProviderResponse::default()
                });
            }
        }''',
        '''        if last.role == Role::User
            && let Some(call) = parse_tool_directive(&last.content)?
        {
            sink.event(ProviderEvent::ToolCall(call.clone()))?;
            return Ok(ProviderResponse {
                tool_calls: vec![call],
                ..ProviderResponse::default()
            });
        }''',
    )
    replace_once(
        openai,
        '''        if let Some(length) = response.content_length() {
            if length > self.max_response_bytes as u64 {
                return Err(ProviderError::new(
                    ProviderErrorKind::Protocol,
                    format!("provider response exceeds {} bytes", self.max_response_bytes),
                ));
            }
        }''',
        '''        if let Some(length) = response.content_length()
            && length > self.max_response_bytes as u64
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Protocol,
                format!("provider response exceeds {} bytes", self.max_response_bytes),
            ));
        }''',
    )
    replace_once(
        router,
        '''        if state.active_turn.as_deref() == Some(&request.turn_id) {
            if let Some(index) = state.sticky_index {
                return Ok(vec![(index, f64::MAX, self.estimate(index, request))]);
            }
        }''',
        '''        if state.active_turn.as_deref() == Some(&request.turn_id)
            && let Some(index) = state.sticky_index
        {
            return Ok(vec![(index, f64::MAX, self.estimate(index, request))]);
        }''',
    )
    replace_once(
        router,
        '''            if let Some(model) = pinned_model.as_deref() {
                if descriptor.id != model && descriptor.model != model {
                    continue;
                }
            }''',
        '''            if let Some(model) = pinned_model.as_deref()
                && descriptor.id != model
                && descriptor.model != model
            {
                continue;
            }''',
    )
    replace_once(
        router,
        '''            if let Some(provider_name) = pinned_provider.as_deref() {
                if descriptor.provider != provider_name {
                    continue;
                }
            }''',
        '''            if let Some(provider_name) = pinned_provider.as_deref()
                && descriptor.provider != provider_name
            {
                continue;
            }''',
    )

    replace_once(
        interactive,
        '''        if let Some(name) = model.api_key_env {
            if seen.insert(name.clone()) {
                println!(
                    "{}: {}",
                    name,
                    if std::env::var_os(&name).is_some() {
                        "configured"
                    } else {
                        "missing"
                    }
                );
            }
        }''',
        '''        if let Some(name) = model.api_key_env
            && seen.insert(name.clone())
        {
            println!(
                "{}: {}",
                name,
                if std::env::var_os(&name).is_some() {
                    "configured"
                } else {
                    "missing"
                }
            );
        }''',
    )
    replace_once(
        app,
        '''        if let Some(model) = options.overrides.provider.model.clone() {
            if config.models.iter().any(|candidate| candidate.id == model || candidate.model == model)
            {
                router.pin_model(Some(model))?;
            }
        }''',
        '''        if let Some(model) = options.overrides.provider.model.clone()
            && config
                .models
                .iter()
                .any(|candidate| candidate.id == model || candidate.model == model)
        {
            router.pin_model(Some(model))?;
        }''',
    )
    replace_once(
        resources,
        '''        if config.include_user_instructions {
            if let Some(directory) = user_config_path().and_then(|path| path.parent().map(Path::to_path_buf)) {
                load_instruction(
                    &directory.join("PIRE.md"),
                    ContextKind::UserInstruction,
                    PrivacyLevel::Sensitive,
                    &mut set.system_segments,
                    &mut used,
                    config.max_total_bytes,
                )?;
                load_prompts(&directory.join("prompts"), &mut set.prompts, config.max_total_bytes)?;
                load_skills(&directory.join("skills"), &mut set.skills, config.max_total_bytes)?;
            }
        }''',
        '''        if config.include_user_instructions
            && let Some(directory) =
                user_config_path().and_then(|path| path.parent().map(Path::to_path_buf))
        {
            load_instruction(
                &directory.join("PIRE.md"),
                ContextKind::UserInstruction,
                PrivacyLevel::Sensitive,
                &mut set.system_segments,
                &mut used,
                config.max_total_bytes,
            )?;
            load_prompts(
                &directory.join("prompts"),
                &mut set.prompts,
                config.max_total_bytes,
            )?;
            load_skills(
                &directory.join("skills"),
                &mut set.skills,
                config.max_total_bytes,
            )?;
        }''',
    )
    replace_once(
        main_rs,
        '''    if input.trim().is_empty() {
        if cli.print {
            return Err(PireError::Message(
                "print mode requires a message, @file reference, or piped input".to_owned(),
            ));
        }
        return interactive::run(&mut app, cli.json_events, cli.yes);
    }''',
        '''    if input.trim().is_empty() && cli.print {
        return Err(PireError::Message(
            "print mode requires a message, @file reference, or piped input".to_owned(),
        ));
    }
    if input.trim().is_empty() {
        return interactive::run(&mut app, cli.json_events, cli.yes);
    }''',
    )
    replace_once(
        output,
        '''        if self.json {
            if let Ok(line) = serde_json::to_string(event) {
                println!("{line}");
            }
            return;
        }''',
        '''        if self.json
            && let Ok(line) = serde_json::to_string(event)
        {
            println!("{line}");
        }
        if self.json {
            return;
        }''',
    )
    replace_once(
        config,
        '''        if let Some(router) = patch.router {
            if let Some(strategy) = router.strategy {''',
        '''        if let Some(router) = patch.router {
            let ProjectRouterConfig {
                strategy,
                prefer_local,
                max_estimated_cost_usd,
            } = router;
            if let Some(strategy) = strategy {''',
    )
    replace_once(
        config,
        '''            if let Some(prefer_local) = router.prefer_local {
                self.router.prefer_local = self.router.prefer_local || prefer_local;
            }
            if let Some(cost) = router.max_estimated_cost_usd {''',
        '''            if let Some(prefer_local) = prefer_local {
                self.router.prefer_local = self.router.prefer_local || prefer_local;
            }
            if let Some(cost) = max_estimated_cost_usd {''',
    )
    replace_once(
        config,
        '''        if let Some(agent) = patch.agent {
            if let Some(value) = agent.max_steps {''',
        '''        if let Some(agent) = patch.agent {
            let ProjectAgentConfig {
                max_steps,
                max_tool_calls,
                default_profile,
                privacy,
            } = agent;
            if let Some(value) = max_steps {''',
    )
    replace_once(
        config,
        '''            if let Some(value) = agent.max_tool_calls {
                self.agent.max_tool_calls = self.agent.max_tool_calls.min(value);
            }
            if let Some(profile) = agent.default_profile {
                self.agent.default_profile = profile;
            }
            if let Some(privacy) = agent.privacy {''',
        '''            if let Some(value) = max_tool_calls {
                self.agent.max_tool_calls = self.agent.max_tool_calls.min(value);
            }
            if let Some(profile) = default_profile {
                self.agent.default_profile = profile;
            }
            if let Some(privacy) = privacy {''',
    )


if __name__ == "__main__":
    main()
