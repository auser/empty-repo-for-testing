# Routing and learning

Every enabled model is mounted as a provider capability with a
`ModelDescriptor` containing:

- stable Pire model ID;
- provider instance ID and display name;
- provider model identifier;
- local/remote status;
- priority;
- task tags;
- input/output prices per million tokens;
- context window;
- capability flags.

## Hard eligibility

The router first excludes models that cannot satisfy the request:

```text
manual model/provider pin does not match
required tool support is absent
estimated context exceeds the model window
estimated request cost exceeds the configured ceiling
provider is not mounted or available
```

Hard constraints are never converted into soft score penalties.

## Task classification

The deterministic classifier recognizes:

```text
general
implementation
review
debugging
planning
summarization
long-context
```

It uses input size and simple task signals. The default path does not spend an
extra model call merely to select a model.

## Strategies

```text
balanced     task fit, priority, locality, learned evidence, and cost
cost         stronger configured-cost penalty
latency      stronger local/observed-latency preference
quality      stronger task, priority, and learned-success preference
local-first  strong preference for local endpoints
```

Use interactively:

```text
/model auto
/model local-coder
/provider openai
/route cost
/route local-first
/router
```

Manual model/provider pins remain active until returned to `auto`.

## Cost estimates

Pire calculates request estimates from configured price metadata and provider
usage when available. Prices are user-owned configuration because vendor
prices change and may differ by account or deployment.

A request can be rejected before execution with:

```toml
[router]
max_estimated_cost_usd = 0.25
```

## Fallback

Candidates are ranked once per turn. Retryable provider failures may advance
to the next candidate up to `max_fallbacks`.

Fallback becomes sticky after a mutating tool result. This prevents a second
model from repeating an edit or process action that already succeeded.
Authentication failures and malformed responses are reported rather than
blindly retried as if they were transient quality failures.

## Aggregate self-improvement

The default learning plugin records only:

```text
model ID
task category
success/failure
latency
estimated cost
explicit positive/negative feedback
```

It never stores prompts, responses, file contents, tool arguments, tool
output, credentials, or hidden reasoning.

A minimum of three observations is required before learned evidence affects
routing. The effect is bounded and remains subordinate to hard eligibility,
trust, tool policy, and cost ceilings.

Commands:

```text
/learn status
/learn on
/learn off
/learn reset
/feedback good
/feedback bad
```

Pire does not autonomously rewrite its binary, source tree, configuration,
trust records, approval policy, or system instructions. Code-level
improvement remains an ordinary proposed patch subject to review and the
normal compiler gate.
