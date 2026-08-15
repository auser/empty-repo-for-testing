# Local models

Pire supports two local integration levels.

## OpenAI-compatible endpoint

Use `provider.kind = "open-ai-compatible"` for llama.cpp server, LM Studio, LocalAI, vLLM, or any compatible `/v1/chat/completions` implementation. API-key configuration is optional.

## Command provider

Use `provider.kind = "command"` when a local runner should be started only for a request. Pire writes a `pire_core::CompletionRequest` JSON object to stdin and reads a `pire_core::ProviderResponse` JSON object from stdout.

The subprocess is bounded by timeout and output-size limits and runs with the workspace as its current directory.

## Future embedded runtimes

A native embedded adapter for Candle, Burn, mistral.rs, or llama.cpp bindings can implement `pire_core::Provider` without changing the agent, tool, session, or CLI architecture. It should remain feature-gated because model runtimes materially affect binary size and platform support.
