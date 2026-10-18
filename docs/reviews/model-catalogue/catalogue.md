---
type: explanation
audience: contributor
verified: 2026-10-18
product_version: 0.61.0
executable: false
---

# Curated catalogue verification

## What it is

Issue [#771](https://github.com/agentsfleet/agentsfleet/issues/771) declares 17 default entries and one advanced entry for `agentsfleet`.
The [catalogue data](../../../scripts/model-library-allowlist.json) contains those exact 18 identities and 24 dated price rows.
The section remains in progress; this document records verified inputs, not a completed deployed picker.

## Why it exists

Provider identifiers route requests. Display names label choices. A native Google route and an OpenRouter Gemini route use separate identities.
A provider's own models API supplies each context integer. Marketing abbreviations cannot supply an integer.

## How it behaves

| Provider | Exact API identifier | Display name | Picker group | Context tokens, source and verification time |
|---|---|---|---|---|
| anthropic | `claude-fable-5-1` | Fable 5.1 | Default | NOT SEEDED: exact provider API limit unreadable |
| anthropic | `claude-opus-5-5` | Opus 5.5 | Default | NOT SEEDED: exact provider API limit unreadable |
| anthropic | `claude-sonnet-5-5` | Sonnet 5.5 | Default | NOT SEEDED: exact provider API limit unreadable |
| anthropic | `claude-haiku-5-5` | Haiku 5.5 | Default | NOT SEEDED: exact provider API limit unreadable |
| openai | `gpt-6.1-sol` | GPT-6.1 Sol | Default | NOT SEEDED: exact provider API limit unreadable |
| openai | `gpt-6-astra` | GPT-6 Astra | Default | NOT SEEDED: exact provider API limit unreadable |
| openai | `gpt-6-luna` | GPT-6 Luna | Default | NOT SEEDED: exact provider API limit unreadable |
| openai | `gpt-5.6-terra` | GPT-5.6 Terra | Advanced | NOT SEEDED: exact provider API limit unreadable |
| fireworks | `accounts/fireworks/models/glm-5p3-flash` | GLM 5.3 Flash | Default | 1,048,576 · [models API](https://api.fireworks.ai/v1/accounts/fireworks/models/glm-5p3-flash#contextLength) · 2026-10-18T13:32:07Z |
| fireworks | `accounts/fireworks/models/kimi-k3` | Kimi K3 | Default | 1,048,576 · [models API](https://api.fireworks.ai/v1/accounts/fireworks/models/kimi-k3#contextLength) · 2026-10-18T13:32:07Z |
| fireworks | `accounts/fireworks/models/deepseek-v4p1-flash` | DeepSeek V4.1 Flash | Default | 1,048,576 · [models API](https://api.fireworks.ai/v1/accounts/fireworks/models/deepseek-v4p1-flash#contextLength) · 2026-10-18T13:32:07Z |
| openrouter | `meta/muse-wonder` | Muse Wonder | Default | 1,048,576 · [models API](https://openrouter.ai/api/v1/models#meta/muse-wonder) · 2026-10-18T13:32:07Z |
| openrouter | `google/gemini-4-argon` | Gemini 4 Argon | Default | 1,048,576 · [models API](https://openrouter.ai/api/v1/models#google/gemini-4-argon) · 2026-10-18T13:32:07Z |
| openrouter | `qwen/qwen3.8-max-0902` | Qwen3.8 Max 0902 | Default | 1,000,000 · [models API](https://openrouter.ai/api/v1/models#qwen/qwen3.8-max-0902) · 2026-10-18T13:32:07Z |
| groq | `openai/gpt-oss-120b` | GPT OSS 120B | Default | NOT SEEDED: exact provider API limit unreadable |
| mistral | `mistral-large-4` | Mistral Large 4 | Default | NOT SEEDED: exact provider API limit unreadable |
| mistral | `mistral-small-2603` | Mistral Small 4 | Default | NOT SEEDED: exact provider API limit unreadable |
| gemini | `gemini-4-argon` | Gemini 4 Argon | Default | NOT SEEDED: exact provider API limit unreadable |

All prices below use United States dollars (USD) per 1,000,000 tokens.
The input threshold is exclusive: the higher tier applies above that integer and prices the whole request.
Dates use Coordinated Universal Time (UTC), with an inclusive start and exclusive end.
An unknown write rate is absent evidence, never a free rate.

| Provider and model | Prompt tokens above | Effective interval | Input | Cached read | Write, lifetime unspecified | Write, 5 minutes | Write, 1 hour | Output | Source |
|---|---|---|---|---|---|---|---|---|---|
| anthropic / `claude-fable-5-1` | 0 | 2026-10-18T00:00:00Z → open | 10 | 0.25 | unknown | 12.5 | 20 | 50 | [price source](https://platform.claude.com/docs/en/about-claude/pricing) |
| anthropic / `claude-opus-5-5` | 0 | 2026-10-18T00:00:00Z → open | 4 | 0.20 | unknown | 5 | 8 | 20 | [price source](https://platform.claude.com/docs/en/about-claude/pricing) |
| anthropic / `claude-sonnet-5-5` | 0 | 2026-10-18T00:00:00Z → open | 2 | 0.10 | unknown | 2.5 | 4 | 10 | [price source](https://platform.claude.com/docs/en/about-claude/pricing) |
| anthropic / `claude-haiku-5-5` | 0 | 2026-10-18T00:00:00Z → open | 0.10 | 0.01 | unknown | 0.125 | 0.20 | 0.50 | [price source](https://platform.claude.com/docs/en/about-claude/pricing) |
| anthropic / `claude-haiku-5-5` | 100000 | 2026-10-18T00:00:00Z → open | 0.50 | 0.05 | unknown | 0.625 | 1 | 2.50 | [price source](https://platform.claude.com/docs/en/about-claude/pricing) |
| openai / `gpt-6.1-sol` | 0 | 2026-10-18T00:00:00Z → open | 2 | 0.10 | 2.5 | unknown | unknown | 10 | [price source](https://developers.openai.com/api/docs/pricing) |
| openai / `gpt-6.1-sol` | 272000 | 2026-10-18T00:00:00Z → open | 4 | 0.20 | 5 | unknown | unknown | 15 | [price source](https://developers.openai.com/api/docs/pricing) |
| openai / `gpt-6-astra` | 0 | 2026-10-18T00:00:00Z → open | 10 | 1 | 12.5 | unknown | unknown | 50 | [price source](https://developers.openai.com/api/docs/pricing) |
| openai / `gpt-6-astra` | 272000 | 2026-10-18T00:00:00Z → open | 20 | 2 | 25 | unknown | unknown | 75 | [price source](https://developers.openai.com/api/docs/pricing) |
| openai / `gpt-6-luna` | 0 | 2026-10-18T00:00:00Z → open | 0.10 | 0.01 | 0.125 | unknown | unknown | 0.50 | [price source](https://developers.openai.com/api/docs/pricing) |
| openai / `gpt-6-luna` | 272000 | 2026-10-18T00:00:00Z → open | 0.20 | 0.02 | 0.25 | unknown | unknown | 0.75 | [price source](https://developers.openai.com/api/docs/pricing) |
| openai / `gpt-5.6-terra` | 0 | 2026-10-18T00:00:00Z → open | 2 | 0.20 | 2.5 | unknown | unknown | 12 | [price source](https://developers.openai.com/api/docs/pricing) |
| openai / `gpt-5.6-terra` | 272000 | 2026-10-18T00:00:00Z → open | 4 | 0.40 | 5 | unknown | unknown | 18 | [price source](https://developers.openai.com/api/docs/pricing) |
| fireworks / `accounts/fireworks/models/glm-5p3-flash` | 0 | 2026-10-18T00:00:00Z → open | 0.15 | 0.03 | unknown | unknown | unknown | 0.50 | [price source](https://fireworks.ai/models/fireworks/glm-5p3-flash) |
| fireworks / `accounts/fireworks/models/kimi-k3` | 0 | 2026-10-18T00:00:00Z → open | 3 | 0.30 | unknown | unknown | unknown | 15 | [price source](https://fireworks.ai/models/fireworks/kimi-k3) |
| fireworks / `accounts/fireworks/models/deepseek-v4p1-flash` | 0 | 2026-10-18T00:00:00Z → open | 0.30 | 0.006 | unknown | unknown | unknown | 1.20 | [price source](https://fireworks.ai/models/deepseek-ai/deepseek-v4p1-flash) |
| openrouter / `meta/muse-wonder` | 0 | 2026-10-18T00:00:00Z → open | 4 | 0.20 | unknown | unknown | unknown | 20 | [price source](https://openrouter.ai/api/v1/models) |
| openrouter / `google/gemini-4-argon` | 0 | 2026-10-18T00:00:00Z → open | 2 | 0.10 | 0.1875 | unknown | unknown | 10 | [price source](https://openrouter.ai/api/v1/models) |
| openrouter / `qwen/qwen3.8-max-0902` | 0 | 2026-10-18T00:00:00Z → open | 2 | 0.25 | unknown | unknown | unknown | 6 | [price source](https://openrouter.ai/api/v1/models) |
| groq / `openai/gpt-oss-120b` | 0 | 2026-10-18T00:00:00Z → open | 0.15 | 0.075 | unknown | unknown | unknown | 0.60 | [price source](https://console.groq.com/docs/prompt-caching) |
| mistral / `mistral-large-4` | 0 | 2026-10-18T00:00:00Z → open | 0.68 | 0.07 | unknown | unknown | unknown | 2.09 | [price source](https://docs.mistral.ai/inference/pricing) |
| mistral / `mistral-small-2603` | 0 | 2026-10-18T00:00:00Z → open | 0.15 | 0.015 | unknown | unknown | unknown | 0.60 | [price source](https://docs.mistral.ai/inference/pricing) |
| gemini / `gemini-4-argon` | 0 | 2026-10-18T00:00:00Z → 2026-12-01T00:00:00Z | 2 | 0.10 | unknown | unknown | unknown | 10 | [price source](https://ai.google.dev/gemini-api/docs/pricing) |
| gemini / `gemini-4-argon` | 0 | 2026-12-01T00:00:00Z → open | 4 | 0.20 | unknown | unknown | unknown | 20 | [price source](https://ai.google.dev/gemini-api/docs/pricing) |

The JSON file records each row's `verified_at` and conditions. Haiku's prompt includes cache reads and writes.
Mistral Large 4 keeps its promotional rate without an invented end date.
Gemini 4 Argon uses implicit caching; neither Gemini route receives cache-control instructions from the runner adapter.

Amounts remain decimal strings through validation. `decimal.js` multiplies each by exactly 1,000,000,000; a decimal integer string binds to Postgres `BIGINT`.
No JavaScript floating-point amount enters storage. Allowed amounts have at most nine fractional digits and fit signed 64-bit integer nanos.
For example, 0.006 becomes 6,000,000 nanos per million tokens; 0.1875 becomes 187,500,000.

The provider models responses recorded at 2026-10-18T13:32:07Z supply six context integers.
OpenRouter's public endpoint was read again on Oct 18 and returned the same three limits and rates.
Unauthenticated Fireworks repeats returned HTTP 401; they supplied no new limit and did not replace the recorded response evidence.

## Limits

OpenRouter serves Muse Wonder at **1,048,576**, while #771 states 1,000,000. The provider API integer wins.
Fireworks' model pages say “1040k”; the stored API responses supply **1,048,576** for all three exact identifiers.
Twelve entries lack exact readable provider API limits and remain unseeded. An authenticated models API check must settle those limits.
DeepSeek V4 Pro 0813 remains absent because Fireworks deprecated its serverless route on Sep 25, 2026.

OpenAI Batch, Flex, Fast, Ultrafast and regional modifiers are excluded. Anthropic's US-only 1.1× modifier is excluded.
The listed prices apply to standard synchronous calls. No provider model inference request was made for this verification.

Indy approved a separate unspecified-lifetime cache-write category. Its rate never becomes a 5-minute or 1-hour rate by assumption.
The date columns keep the brief's exact `effective_from` and `effective_until` names under Indy's narrow naming exception.
Ollama tags must come from the configured server and have unknown token prices. Indy left local ownership blocked for a later decision.
The native Gemini and Ollama transport checks use mocks; they do not prove live provider access or local endpoint authorization.

## Related pages

- [Verification and remaining work](../../../HANDOFF_M229_CATALOGUE.md)
- [Exact handover](handover.md)
- [Required test ledger](test-ledger.md)
- [Operator playbook](../../../playbooks/operations/model_catalogue/001_playbook.md)
