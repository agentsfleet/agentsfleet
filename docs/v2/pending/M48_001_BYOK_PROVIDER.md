# M48_001: BYOK Provider — Tenant-Scoped LLM Provider Configuration

**Prototype:** v2.0.0
**Milestone:** M48
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — soft-blocks the launch tweet's BYOK claim. The OSS/self-hostable positioning is materially weaker without operator-controlled LLM provider config. Not strictly substrate-blocking but launch-shipping for the wedge messaging.
**Categories:** API, CLI, UI
**Batch:** B2 — depends on M45 (vault structured creds with `type=llm_provider`).
**Branch:** feat/m48-byok-provider (to be created)
**Depends on:** M45_001 (vault structured creds — `llm_provider` type defined there). M11_005 (tenant billing — DONE; provides the `tenant_billing.balance_cents` gate that triggers the credit-exhausted UX).

**Canonical architecture:** `docs/ARCHITECHTURE.md` §0 ("not a coding-agent product" — BYOK matters when the provider key drives all LLM cost), §10 (capabilities — implicit in "secrets never in agent context").

---

## Implementing agent — read these first

1. M45's spec (sibling) — `llm_provider` is one of the canonical credential types. The vault stores `{provider, api_key, model}`.
2. `src/zombie/event_loop_helpers.zig` — current LLM provider key resolution path (the legacy `resolveFirstCredential` returning the platform key).
3. `samples/fixtures/m45-credential-fixtures.json` — `llm_provider` field shape.
4. M11_005 done spec — tenant billing balance check; BYOK bypasses platform metering for LLM calls.
5. NullClaw provider routing — read NullClaw's provider abstraction layer to know which provider keys map to which API endpoints.

---

## Overview

**Goal (testable):** An operator at a tenant who has exhausted their free credits can: (a) open Settings → Provider in the dashboard, switch to "Bring your own key", select an `llm_provider`-typed credential from the vault, choose a model, click Save → after which **all zombie runs in every workspace under their tenant** bill against their own API quota; OR (b) run `zombiectl provider set --credential-ref my_anthropic_key --model claude-sonnet-4-6` with identical effect (no `--workspace` flag — the setting is tenant-scoped); OR (c) call `PUT /v1/tenants/me/provider`. After BYOK is set, `tenant_billing.balance_cents` no longer gates new zombie runs (operator pays Anthropic, not us). Reverting to platform provider re-enables the credit gate.

**Problem:** Today, when a tenant exhausts their 20 free credits, their only paths are: upgrade the platform plan (pay UseZombie to keep running platform-hosted Claude), or stop. BYOK is the enterprise self-serve unlock — operators with their own Anthropic / OpenAI / Cerebras / Together accounts route LLM calls through their own quota and let UseZombie be the orchestration layer. Without this, operators wanting cost control, model choice, or routing to on-prem inference cannot do so. The differentiation against AWS DevOps Agent (Bedrock-locked) and Sonarly (Claude-Code-managed) is hollow.

**Solution summary:** Provider config lives at the tenant scope, not workspace. One provider per tenant; switching providers changes which API the LLM calls hit. Stored as a separate vault credential (type `llm_provider` with fields `provider`, `api_key`, `model`). Settings → Provider page lets operator switch between "platform-hosted" (default, gated by `tenant_billing.balance_cents`) and "BYOK" (uses the selected `llm_provider` credential, no balance gate). Provider keys flow into `executor.startStage` separately from tool-level secrets — they never enter the agent's tool context, never log, never echo.

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

The mode discriminator is `platform` (default) or `byok`. When `byok`, `credential_ref` points at a vault credential of type `llm_provider`. Model is optional override; absent → use the credential's `model` field.

> **Implementation default:** unique constraint on `tenant_id` (one provider config per tenant). Soft-delete semantics if needed for audit (out of scope here).

### §2 — Resolver

`resolveActiveProvider(tenant_id)`:

1. Read `core.tenant_providers` for tenant_id (default if missing: `mode=platform`).
2. If `mode=platform`: return `{ provider: 'platform', api_key: <platform_key>, model: <default> }`.
3. If `mode=byok`: read vault credential `credential_ref` of type `llm_provider`, return its fields with `mode=byok`.
4. Return shape consumed by `executor.startStage` to choose provider routing.

### §3 — Balance gate skip on BYOK

In `processEvent`, before invoking the executor, the existing balance gate checks `tenant_billing.balance_cents > 0`. With BYOK active, the gate is short-circuited (operator pays their LLM provider directly; we don't meter LLM tokens). Tool-call metering for non-LLM costs (network egress, storage) stays in place if the project has it; otherwise no metering.

### §4 — Provider catalog

`samples/fixtures/m48-provider-fixtures.json`:

```json
{
  "anthropic": {"default_model": "claude-sonnet-4-6", "api_base": "https://api.anthropic.com"},
  "openai": {"default_model": "gpt-5", "api_base": "https://api.openai.com"},
  "cerebras": {"default_model": "qwen3-coder", "api_base": "https://api.cerebras.ai"},
  "together": {"default_model": "deepseek-coder", "api_base": "https://api.together.xyz"}
}
```

UI uses this to populate the provider dropdown. Adding a new provider = appending here + ensuring NullClaw routes to its endpoint.

### §5 — UI Settings → Provider

Page at `/settings/provider`:

- Current provider summary (mode, credential name if BYOK, model)
- Mode toggle: Platform | BYOK
- If BYOK: credential dropdown (filter vault credentials by `type=llm_provider`); model dropdown (auto-populated from provider catalog with override field)
- Save button → PUT request → success toast → revalidate

> **Implementation default:** show a "current spend" widget pulling `tenant_billing.balance_cents` only when in platform mode. In BYOK mode, link out to "Manage spend at <provider> directly."

### §6 — CLI

```
zombiectl provider get                     # current config
zombiectl provider set --credential-ref my_anthropic_key [--model X]  # switch to BYOK
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
    → 400 if mode=byok and credential_ref missing OR credential is not type=llm_provider

CLI:
  zombiectl provider get
  zombiectl provider set --credential-ref <name> [--model <model>]
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
