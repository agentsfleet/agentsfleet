# M48_001: BYOK Provider — Tenant-Scoped LLM Provider Configuration

**Prototype:** v2.0.0
**Milestone:** M48
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — launch-blocking (substrate-tier, Week 2-3). BYOK is the second of three v2 differentiation pillars (OSS + BYOK + markdown-defined; self-host deferred to v3). Without operator-controlled LLM provider config, the launch tweet's BYOK claim is hollow and the differentiation argument collapses to two pillars, both matchable by competitors within a week. Promoted from "soft-blocks launch claim" to substrate-tier on Apr 25, 2026 — see "Tier promotion provenance" below. Adds ~1 week to the milestone (~6-7 weeks total).
**Categories:** API, CLI, UI
**Batch:** B1 — substrate-tier alongside M40-M45. Depends on M45 (vault stores opaque JSON-object plaintext per credential).
**Branch:** feat/m48-byok-provider (to be created)
**Depends on:** M45_001 (vault structured creds — JSON-object plaintext lands there; BYOK uses a credential named `llm` carrying `{provider, api_key, model}`). M11_005 (tenant billing — DONE; provides the `tenant_billing.balance_cents` gate that triggers the credit-exhausted UX).

**Canonical architecture:** `docs/ARCHITECHTURE.md` §0 ("not a coding-agent product" — BYOK matters when the provider key drives all LLM cost), §10 (capabilities — implicit in "secrets never in agent context").

---

## Tier promotion provenance (Apr 25, 2026 /plan-ceo-review decision)

**Tier breakdown after the CEO scope review:**

| Tier | Specs |
|---|---|
| **Launch-blocking substrate (Week 1-3)** | M40 Worker, M41 Context Layering, M42 Streaming, M43 Webhook Ingest, M44 Install Contract + Doctor, M45 Vault Structured, **M48 BYOK Provider** (this spec — promoted from "soft-blocks") |
| **Launch-shipping packaging (Week 4-7)** | M46 Frontmatter Schema, M49 Install-Skill, M51 Docs + Install-Pingback (also owns architecture cross-reference + ship reflection) |
| **Parallel from Week 1, validation-blocking** | M50 Customer-Development Parallel Workstream (~2-3 hours/week founder time alongside substrate) |
| **Post-launch** | M47 Approval Inbox |

**Why M48 was promoted (CEO review reasoning):** with self-host deferred to v3 (decided earlier in the same /plan-ceo-review session), the v2 differentiation pillars compressed from four to three — **OSS + BYOK + markdown-defined**. Dropping BYOK leaves only two pillars, both matchable by competitors (AWS DevOps Agent / Sonarly / incident.io) within a week. M48 moved from "Soft-blocks BYOK launch claim, post-launch" to "Launch-blocking substrate, Week 2-3." Adds ~1 week to the milestone (5-6 → 6-7 weeks). The differentiation argument holds at three pillars; collapses at two.

This provenance was folded inline here so the M48 spec is self-contained — previously it lived in a separate handoff file (now retired) which was the only durable reference to the §A decision and the tier table.

---

## Cross-spec amendment (Apr 26, 2026)

M45 dropped the typed credential registry. There is no `type=llm_provider` discriminator and no `samples/fixtures/m45-credential-fixtures.json` — credentials are opaque JSON objects keyed by name. M48 retains every BYOK capability described below by storing the provider config as a vault credential whose `data` is `{provider, api_key, model}`. The CLI / UI side filters by **credential name convention** (BYOK reads the vault credential named `llm` per tenant) instead of by type. Wherever this spec says "credential of type `llm_provider`", read it as "the tenant's `llm` credential". Wherever it references the M45 fixtures file, ignore — that file does not exist.

---

## Cross-spec amendment (Apr 29, 2026 — billing model + cap origin)

Two corrections to the design below, both arising from the M41-spillover Discovery work in M49 and the architecture refactor of `docs/ARCHITECHTURE.md` (now split into per-topic files under `docs/architecture/`).

**Correction 1 — balance gate stays on for both postures.** The original §3 said "BYOK skips the balance gate." That is wrong. The gate runs for both postures; only the per-event cost function differs:

- **Platform-managed.** Per-event credit deducted bundles language-model tokens, orchestration, and egress / storage. Margin over our wholesale language-model cost.
- **Bring Your Own Key.** Per-event credit deducted is orchestration only (the operator pays the provider for tokens directly). Smaller, but non-zero.

The Free plan does not allow Bring Your Own Key — Free is the evaluation tier; giving free orchestration to operators with their own language-model key would be a vector for abuse. BYOK requires Team or Scale.

Code shape: `processEvent` runs one balance gate at step 3 (resolve plan + posture, estimate cost, compare against `tenant_billing.balance_cents`). Step 9's telemetry insert calls `compute_charge(plan, posture, tokens)` which returns the right cents for the posture.

Full billing reasoning: [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md).

**Correction 2 — `tenant_providers` carries `context_cap_tokens`.** The credential body stays `{provider, api_key, model}` only. The model's context cap is resolved separately, at `provider set` time, by GETting the model-caps endpoint (`https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json?model=<urlencoded>`) and persisted as `core.tenant_providers.context_cap_tokens`. Splitting cap from credential lets the cap be re-resolved when the model changes without touching the vault.

Resolver shape additions:

```
resolveActiveProvider(tenant_id) → {
  provider:           "platform" | "anthropic" | "fireworks" | "openai" | ...,
  api_key:            string,
  model:              string,
  context_cap_tokens: u32,           ← NEW; resolved at provider set time, persisted on tenant_providers
  mode:               "platform" | "byok",
}
```

Worker overlay at `processEvent`: if the zombie's frontmatter carries `model: ""` or `context_cap_tokens: 0` (the install-skill writes these sentinels under BYOK posture), the worker overlays from `tenant_providers`. Both fields are independently overlayable. Full diagram: [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.7.

Schema change relative to original §1 below: `core.tenant_providers` gains `context_cap_tokens INTEGER`. Also `model TEXT` (originally optional override; now persisted authoritatively under BYOK).

Test additions on top of original §10:
- `test_provider_set_resolves_cap_from_caps_endpoint` — `provider set` writes `tenant_providers.context_cap_tokens` from the endpoint response.
- `test_byok_overlays_cap_at_trigger_time` — frontmatter `context_cap_tokens: 0` + `mode=byok` → worker uses `tenant_providers.context_cap_tokens`.
- `test_provider_set_with_unknown_model_400s` — model not in caps endpoint → `400 model_not_in_caps_catalogue`.
- `test_byok_blocked_on_free_plan` — Free-plan tenant trying to set BYOK → 403 with `byok_requires_paid_plan`.

---

## Implementing agent — read these first

1. M45's spec (sibling) — credentials are opaque JSON objects keyed by name. The BYOK record is the credential named `llm` with `data = {provider, api_key, model}`. No type discriminator.
2. `src/zombie/event_loop_helpers.zig` — `resolveSecretsMap` (M45) returns parsed JSON objects per name; the BYOK resolver reads the `llm` credential through this path. The legacy `resolveFirstCredential` is being phased out.
3. M11_005 done spec — tenant billing balance check; BYOK bypasses platform metering for LLM calls.
4. NullClaw provider routing — read NullClaw's provider abstraction layer to know which provider keys map to which API endpoints.

---

## Overview

**Goal (testable):** An operator at a tenant who has exhausted their free credits can: (a) open Settings → Provider in the dashboard, switch to "Bring your own key", point at the vault credential named `llm` (whose `data` is `{provider, api_key, model}`), optionally override the model, click Save → after which **all zombie runs in every workspace under their tenant** bill against their own API quota; OR (b) run `zombiectl provider set --model claude-sonnet-4-6` after `zombiectl credential add llm --data '{"provider":"anthropic","api_key":"sk-ant-...","model":"claude-sonnet-4-6"}'`, with identical effect (no `--workspace` flag — the setting is tenant-scoped); OR (c) call `PUT /v1/tenants/me/provider`. After BYOK is set, `tenant_billing.balance_cents` no longer gates new zombie runs (operator pays Anthropic, not us). Reverting to platform provider re-enables the credit gate.

**Problem:** Today, when a tenant exhausts their 20 free credits, their only paths are: upgrade the platform plan (pay UseZombie to keep running platform-hosted Claude), or stop. BYOK is the enterprise self-serve unlock — operators with their own Anthropic / OpenAI / Cerebras / Together accounts route LLM calls through their own quota and let UseZombie be the orchestration layer. Without this, operators wanting cost control, model choice, or routing to on-prem inference cannot do so. The differentiation against AWS DevOps Agent (Bedrock-locked) and Sonarly (Claude-Code-managed) is hollow.

**Solution summary:** Provider config lives at the tenant scope, not workspace. One provider per tenant; switching providers changes which API the LLM calls hit. Stored as a vault credential named `llm` carrying `data = {provider, api_key, model}` — opaque JSON object per M45. Settings → Provider page lets operator switch between "platform-hosted" (default, gated by `tenant_billing.balance_cents`) and "BYOK" (uses the `llm` credential, no balance gate). Provider keys flow into `executor.startStage` separately from tool-level secrets — they never enter the agent's tool context, never log, never echo.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/0NN_tenant_provider.sql` | NEW | `core.tenant_providers (tenant_id, mode, credential_ref, model)` |
| `schema/embed.zig` | EXTEND | Register schema |
| `src/cmd/common.zig` | EXTEND | Migration array |
| `src/state/tenant_provider.zig` | NEW | CRUD + resolver: `resolveActiveProvider(tenant_id) → {provider, api_key, model, mode}` |
| `src/zombie/event_loop_helpers.zig` | EDIT | Replace platform-key path with `resolveActiveProvider`; if `mode=byok`, skip balance gate |
| `src/http/handlers/tenants/provider.zig` | NEW | `GET /v1/tenants/me/provider`, `PUT /v1/tenants/me/provider` |
| `zombiectl/src/commands/provider.js` | NEW | `zombiectl provider {get|set|reset}` |
| `zombiectl/src/program/routes.js` | EXTEND | Wire `provider` route |
| `ui/packages/app/src/routes/settings/provider.tsx` | NEW | Settings page |
| `ui/packages/app/src/components/ProviderSelector.tsx` | NEW | Mode + credential + model selector |
| `tests/integration/byok_provider_test.zig` | NEW | E2E: set BYOK → run zombie → assert it called the operator's Anthropic key, not platform |
| `samples/fixtures/m48-provider-fixtures.json` | NEW | Provider catalog: anthropic, openai, cerebras, together, etc., with default models |

---

## Sections (implementation slices)

### §1 — Schema: tenant_providers

The mode discriminator is `platform` (default) or `byok`. When `byok`, the resolver reads the vault credential named `llm` (parsed JSON object) for the tenant. Model is optional override on the `tenant_providers` row; absent → use the credential's `model` field. `credential_ref` is fixed to `"llm"` in v1 (no per-tenant rename); the column exists for forward-compat if multi-key BYOK ships later.

> **Implementation default:** unique constraint on `tenant_id` (one provider config per tenant). Soft-delete semantics if needed for audit (out of scope here).

### §2 — Resolver

`resolveActiveProvider(tenant_id)`:

1. Read `core.tenant_providers` for tenant_id (default if missing: `mode=platform`).
2. If `mode=platform`: return `{ provider: 'platform', api_key: <platform_key>, model: <default> }`.
3. If `mode=byok`: read vault credential named `llm` (the tenant-scoped BYOK record), parse `{provider, api_key, model}`, return with `mode=byok`. Surface `credential_missing` if the row is absent.
4. Return shape consumed by `executor.startStage` to choose provider routing.

### §3 — Balance gate cost function (corrected — see Apr 29 amendment above)

In `processEvent`, before invoking the executor, the balance gate checks `tenant_billing.balance_cents` against the estimated event cost. The estimate uses the per-event cost function for the active posture:

- `mode=platform` → bundled rate (covers language-model tokens + orchestration + egress).
- `mode=byok` → orchestration-only rate (operator pays their language-model provider directly).

The gate runs for both postures. The Free plan does not allow BYOK (paid plans only). Step 9's telemetry insert calls `compute_charge(plan, posture, tokens)` and updates `tenant_billing.balance_cents`. Full reasoning in [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md).

### §4 — Provider catalog

`samples/fixtures/m48-provider-fixtures.json`:

```json
{
  "anthropic": {"default_model": "claude-sonnet-4-6", "api_base": "https://api.anthropic.com"},
  "openai": {"default_model": "gpt-5", "api_base": "https://api.openai.com"},
  "fireworks": {"default_model": "accounts/fireworks/models/kimi-k2.6", "api_base": "https://api.fireworks.ai/inference/v1"},
  "moonshot": {"default_model": "kimi-k2.6", "api_base": "https://api.moonshot.cn/v1"},
  "zhipu": {"default_model": "glm-5.1", "api_base": "https://open.bigmodel.cn/api/paas/v4"},
  "together": {"default_model": "deepseek-coder", "api_base": "https://api.together.xyz"}
}
```

UI uses this to populate the provider dropdown. Adding a new provider = appending here + ensuring NullClaw routes to its endpoint.

### §5 — UI Settings → Provider

Page at `/settings/provider`:

- Current provider summary (mode, credential name if BYOK, model)
- Mode toggle: Platform | BYOK
- If BYOK: read the vault credential named `llm` (parsed JSON object) — show its `provider` field as the active provider; model dropdown (auto-populated from provider catalog with override field). If the `llm` credential is absent, render an inline "Add `llm` credential" call-to-action linking to the credentials page.
- Save button → PUT request → success toast → revalidate

> **Implementation default:** show a "current spend" widget pulling `tenant_billing.balance_cents` only when in platform mode. In BYOK mode, link out to "Manage spend at <provider> directly."

### §6 — CLI

```
zombiectl provider get                     # current config
zombiectl provider set [--model X]                                  # switch to BYOK (reads `llm` credential)
zombiectl provider reset                   # back to platform
```

### §7 — Tenant-scope enforcement

PUT requires the caller to be a tenant admin (or the tenant's only user, common for solo founders). Per-workspace provider config is explicitly NOT supported in v1. Out-of-scope rationale: billing is tenant-scoped (M11_005), so the provider key paying the LLM bill must be tenant-scoped too. Rejecting `--workspace` flag at CLI level + at API level.

---

## Interfaces

```
HTTP:
  GET /v1/tenants/me/provider
    → 200 { mode: "platform"|"byok", credential_ref?: string, model?: string, provider?: string }

  PUT /v1/tenants/me/provider
    body: { mode: "platform"|"byok", credential_ref?: string, model?: string }
    → 200 (echoes new config)
    → 403 if caller is not a tenant admin
    → 400 if mode=byok and the `llm` credential is missing OR its `data` lacks `provider`/`api_key`

CLI:
  zombiectl provider get
  zombiectl provider set [--model <model>]   # BYOK reads the `llm` credential
  zombiectl provider reset

Internal:
  tenant_provider.resolveActiveProvider(tenant_id) → ResolvedProvider {
    provider: "platform"|"anthropic"|"openai"|...,
    api_key: string,
    model: string,
    mode: "platform"|"byok",
  }
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| BYOK set with non-existent credential_ref | Operator typo | 400 with `credential_not_found` |
| BYOK credential is wrong type | Selected a `fly` credential | 400 with `wrong_credential_type` |
| BYOK API key invalid (rejected by provider) | Operator pasted wrong key | First zombie run fails with `provider_auth_failed`; operator sees error in event log; provider config remains set (operator can fix it) |
| Switch from BYOK back to platform when balance=0 | Operator forgot they had run dry | PUT succeeds; next event blocks at balance gate; operator sees clear "balance exhausted" error |
| Concurrent PUTs from two admin sessions | Race | Last write wins; both succeed; revalidate shows the winner |

---

## Invariants

1. **Provider key never in agent context.** Same as tool-level secrets — passed through executor session, substituted at the API client level (not the agent's tool bridge — different layer; see M41).
2. **One provider per tenant.** Schema unique constraint.
3. **BYOK skips balance gate.** Tested explicitly.
4. **Mode change applies on NEXT event.** In-flight events use the provider that was active when claimed.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_default_provider_is_platform` | New tenant → GET returns mode=platform |
| `test_set_byok_routes_to_operator_key` | Set BYOK → run zombie → outbound LLM call hits operator's API base, uses operator's API key |
| `test_byok_skips_balance_gate` | Tenant with balance=0 + BYOK active → zombie runs (no gate block) |
| `test_platform_mode_enforces_balance_gate` | Tenant with balance=0 + mode=platform → zombie blocks at gate |
| `test_byok_with_invalid_credential_ref` | PUT with non-existent ref → 400 |
| `test_byok_with_wrong_credential_type` | PUT pointing at fly credential → 400 |
| `test_provider_key_no_leak_into_event_log` | Run zombie under BYOK → grep `core.zombie_events` for the API key bytes → 0 matches |
| `test_provider_key_no_leak_into_tool_context` | Run zombie under BYOK → capture agent's tool-call records → 0 matches for API key bytes |
| `test_concurrent_put_last_write_wins` | Two PUTs racing → final config = whichever committed last |
| `test_tenant_admin_required` | Non-admin PUT → 403 |

---

## Acceptance Criteria

- [ ] `make test-integration` passes the 10 tests above
- [ ] Manual: tenant with 0 platform credits switches to BYOK with own Anthropic key, runs platform-ops zombie, no errors
- [ ] Manual: switching back to platform with 0 credits surfaces the credit-exhausted UX
- [ ] Audit grep: BYOK API key bytes never appear in `core.zombie_events`, worker logs, or executor logs
- [ ] `make check-pg-drain` clean
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux

---

## Out of Scope

- Per-workspace provider override (tenant-scoped only in v1)
- Per-zombie provider override (rare; defer)
- BYOK metering for cost reporting (operator manages cost at provider directly)
- Provider failover (if BYOK key fails, error to operator — no auto-fallback to platform)
- Custom OIDC / vault-backend integrations (treat the vault credential as the source of truth)
