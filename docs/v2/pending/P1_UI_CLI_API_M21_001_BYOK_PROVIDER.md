# M21_001: BYOK Provider Configuration — CLI / UI / API for LLM Provider and Credits

**Prototype:** v2
**Milestone:** M21
**Workstream:** 001
**Date:** Apr 13, 2026
**Status:** PENDING
**Priority:** P1 — Blocks operators who exhaust free credits; enables enterprise self-serve
**Batch:** B2 — parallel with M19_001, M13_001, M11_005
**Branch:** feat/m21-byok-provider
**Depends on:** M12_001 (settings page), **M13_001 (credential vault — provides Add Credential modal and Type selector that M21 extends with `llm_provider`)**, M15_001 (credit metering, done), M11_005 (tenant billing — provides the `tenant_billing.balance_cents` gate that triggers the credit-exhausted UX).

> **v2 note — identifier reuse:** `M21_001` is also used in v1 for "Agent Interrupt and Steer" (`docs/v1/done/M21_001_AGENT_INTERRUPT_AND_STEER.md`, done). That v1 primitive is what M23_001 (v2) builds on. When resolving `M21_001` in a cross-reference, check the version prefix (`v1/done/` vs `v2/pending/`) — they are unrelated features sharing the same milestone slot across schema generations.

---

## Overview

**Goal (testable):** An operator who has exhausted their free tenant credits can: (a) open Settings > Provider in the dashboard, switch to "Bring your own key", select their Anthropic credential from the vault, choose a model, and click Save — after which **all zombie runs in every workspace under their tenant** bill against their own API quota; OR (b) run `zombiectl provider set --provider anthropic --credential-ref my_anthropic_key --model claude-sonnet-4-6` with identical effect (no `--workspace` flag — the setting is tenant-scoped); OR (c) call `PUT /v1/tenants/me/provider` via the API.

**Scope note — tenant, not workspace.** BYOK configuration is tenant-scoped. Per-workspace overrides and per-zombie overrides are explicitly out of scope for the MVP: billing is tenant-scoped (M11_005), so the provider key that pays the LLM bill must be tenant-scoped too. Settings > Provider lives in the tenant-settings tab, not any workspace-settings surface.

**Problem:** Credits run out. Today, when an operator's 20 free credits are exhausted, their only paths are: upgrade the UseZombie plan (pay UseZombie to keep running hosted Claude), or stop using the product. BYOK (bring your own key) is the enterprise self-serve unlock: the operator adds their own Anthropic or OpenAI API key, and UseZombie becomes the orchestration layer without being the LLM cost center. Without this, operators who want to control their LLM spend, choose specific models, or route to an on-prem/private model cannot do so.

**Conceptual distinction:** Provider credentials are different from tool credentials. Tool credentials (M13) are injected by the firewall into outbound HTTP calls that the zombie makes to external services. Provider credentials are used by the UseZombie runtime itself to make LLM API calls on behalf of the zombie — they are never visible to the zombie's tool calls, never logged in the activity stream, never accessible via zombie code.

**Solution summary:** (1) Extend the M13_001 credential-type enum to include `llm_provider` — stored with the same encryption as tool credentials but tagged differently and excluded from the firewall injection path. (2) Tenant-scoped provider config API: `GET/PUT /v1/tenants/me/provider`. No per-workspace, no per-zombie override endpoints. (3) Dashboard: Settings > Provider tab (under tenant settings). (4) CLI: `zombiectl provider set/get` (no `--workspace` flag). (5) Usage panel on Settings showing per-zombie token usage and estimated cost from M15_001's metering data.

**DX paths:**

| Action | CLI | UI | API |
|---|---|---|---|
| View current provider | `zombiectl provider get` | Settings > Provider | `GET /v1/tenants/me/provider` |
| Set BYOK (tenant) | `zombiectl provider set --provider anthropic ...` | Settings > Provider > BYOK | `PUT /v1/tenants/me/provider` |
| View credit usage | `zombiectl credits status` | Settings > Usage panel | `GET /v1/tenants/me/billing` |

---

## 1.0 Settings > Provider Tab (Dashboard)

**Status:** PENDING

A new "Provider" tab on the Settings page (M12 §5.0). Shows current provider and credit status. Switching to BYOK: select provider type, bind a credential from the vault (only `llm_provider` type credentials shown), select model.

**Layout:**

```
Settings

[Billing] [API Keys] [Provider] [Team]

LLM Provider

● UseZombie Hosted
  12 of 20 free runs used  ████████░░  60%
  [Upgrade plan →]

○ Bring your own key (BYOK)
  Provider    [Anthropic ▾]
  Credential  [Select from vault ▾]
              + Add new credential
  Model       [claude-sonnet-4-6 ▾]
              claude-haiku-4-5 · claude-opus-4-6

              [Save provider settings]

──────────────────────────────────────
Usage (last 30 days)

ZOMBIE          RUNS   TOKENS      EST. COST
lead-collector    8    1.2M        $0.84
blog-writer       4     890K       $0.62
ops-zombie       12     410K       $0.29
─────────────────────────────────────
TOTAL            24    2.5M        $1.75
```

**Dimensions:**
- 1.1 PENDING
  - target: `app/settings/components/ProviderSettings.tsx`
  - input: workspace on free plan, 12/20 runs used
  - expected: hosted option selected, credit bar at 60%, upgrade CTA visible
  - test_type: unit (component test)
- 1.2 PENDING
  - target: `app/settings/components/ProviderSettings.tsx`
  - input: user selects BYOK, selects Anthropic, selects credential "my_anthropic_key" from vault, clicks Save
  - expected: `PUT /v1/tenants/me/provider` with `{ type: "anthropic", credential_ref: "my_anthropic_key", model: "claude-sonnet-4-6" }` — success toast
  - test_type: integration (API mock)
- 1.3 PENDING
  - target: `app/settings/components/ProviderSettings.tsx`
  - input: credential dropdown with a mix of tool credentials and provider credentials
  - expected: only `credential_type = llm_provider` credentials shown in the provider dropdown
  - test_type: unit (component test)
- 1.4 PENDING
  - target: `app/settings/components/UsagePanel.tsx`
  - input: workspace with 3 zombies, 30 days of M15_001 metering data
  - expected: table shows per-zombie runs, tokens, estimated cost; total row
  - test_type: unit (component test)
- 1.5 PENDING
  - target: `app/settings/components/ProviderSettings.tsx`
  - input: user saves BYOK with no credential selected
  - expected: validation error "Select a credential to use as your LLM provider key"
  - test_type: unit (component test)

---

## 3.0 CLI Surface

**Status:** PENDING

```bash
# View workspace provider
$ zombiectl provider get
Provider:    anthropic (BYOK)
Credential:  my_anthropic_key
Model:       claude-sonnet-4-6

# Set workspace to BYOK
$ zombiectl provider set \
    --provider anthropic \
    --credential-ref my_anthropic_key \
    --model claude-sonnet-4-6
✓ Workspace provider updated

# Set back to hosted
$ zombiectl provider set --hosted
✓ Workspace using UseZombie hosted credits (8 remaining)

# Credit status
$ zombiectl credits status
Workspace:    acme-prod
Plan:         Free (hosted)
Used:         12 / 20 runs
Remaining:    8 runs
Reset:        Never (one-time credits)
```

**Dimensions:**
- 3.1 PENDING
  - target: `zombiectl provider set`
  - input: `--provider anthropic --credential-ref my_anthropic_key --model claude-sonnet-4-6`
  - expected: `PUT /v1/tenants/me/provider` called; success message printed
  - test_type: CLI integration
- 3.2 PENDING
  - target: `zombiectl provider set --hosted`
  - input: workspace currently on BYOK
  - expected: provider switched to hosted; remaining credits shown
  - test_type: CLI integration

---

## 4.0 Credential Vault Extension (llm_provider type)

**Status:** PENDING

M13 manages tool credentials. Provider credentials are stored in the same vault but with `credential_type = llm_provider`. The distinction matters at the API layer:
- Tool credentials: returned by `GET /v1/workspaces/{ws}/credentials` (with name, scope, last_used — no value)
- Provider credentials: returned by the same endpoint but flagged as `type: llm_provider`; excluded from the firewall injection path; only shown in provider-specific dropdowns

An operator adds a provider credential the same way as any credential: Add Credential modal in M13 with a new "Type" field defaulting to "Tool" with an "LLM Provider" option.

**Dimensions:**
- 4.1 PENDING
  - target: `app/credentials/components/AddCredentialModal.tsx`
  - input: user adds credential "my_anthropic_key" with type "LLM Provider"
  - expected: credential created with `credential_type = llm_provider`; visible in provider selector; NOT shown in firewall injection paths
  - test_type: unit (component test)
- 4.2 PENDING
  - target: firewall injection exclusion
  - input: zombie run with `credential_type = llm_provider` credential in vault
  - expected: provider credential never appears in `credential_ref` dropdown for zombie tool calls; never injected into outbound HTTP
  - test_type: integration (verify via firewall logs — no injection events for provider creds)

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 New API Endpoints

```
GET  /v1/tenants/me/provider                   — get tenant LLM provider config
PUT  /v1/tenants/me/provider                   — set tenant LLM provider config
```

No per-workspace and no per-zombie provider endpoints. Those are explicitly out of scope (see §9).

### 5.2 Provider Config Schema

```json
{
  "type": "hosted" | "anthropic" | "openai" | "custom",
  "credential_ref": "my_anthropic_key",   // null if type = hosted
  "model": "claude-sonnet-4-6",           // null defaults to provider default
  "base_url": null                        // custom endpoint for on-prem (future)
}
```

### 5.3 Error Contracts

| Error condition | Code | HTTP |
|---|---|---|
| Credential not found or wrong type | `UZ-PROV-001` | 404 |
| Model not supported by provider | `UZ-PROV-002` | 422 |
| Hosted credits exhausted | `UZ-PROV-003` | 402 — prompts upgrade or BYOK |
| BYOK credential invalid (LLM API rejected it) | `UZ-PROV-004` | 422 |

---

## 6.0 Implementation Constraints

| Constraint | How to verify |
|---|---|
| Provider credentials excluded from firewall injection | Dim 4.2 |
| Provider credentials stored encrypted (same vault) | Code review |
| `credential_type` column added to credentials table | Schema + migration |
| No BYOK credential value ever returned by API | grep handlers for credential value in response |
| Hosted credit deduction atomic with run creation | Integration test: kill after run create before debit → rollback |

---

## 7.0 Execution Plan

| Step | Action | Verify |
|---|---|---|
| 1 | Schema: add `credential_type` column to credentials table | zig build |
| 2 | `GET/PUT /v1/workspaces/{ws}/provider` handlers | dim 3.1 |
| 3 | M13 credential modal: add Type field | dim 4.1 |
| 4 | Settings > Provider tab (dashboard UI) | dims 1.1–1.5 |
| 5 | CLI `zombiectl provider set/get` | dims 3.1–3.2 |
| 6 | Usage panel with M15_001 metering data | dim 1.4 |
| 7 | Cross-compile + full test gate | all dims |

---

## 7.5 Credit-Exhausted User Journey

**Status:** PENDING

The tenant balance gate is the bridge between M11_005 (credit enforcement) and M21_001 (BYOK unlock). This section specifies exactly what an operator sees when `billing.tenant_billing.balance_cents <= 0` and they have not yet configured BYOK.

### 7.5.1 Worker-side gate (NEW zombie invocations)

When a worker picks up a new zombie invocation for tenant T:

1. Worker looks up `billing.tenant_billing.balance_cents` for T (single read, cached for the run's lifetime).
2. Worker looks up `tenant.provider.type` — `hosted` vs BYOK.
3. **If `provider.type == 'hosted'` AND `balance_cents <= 0`:** worker rejects with HTTP `402 UZ-CREDIT-EXHAUSTED`. The activity stream records a `credit_exhausted_at_dispatch` event with the tenant_id and zombie_id.
4. **If `provider.type != 'hosted'` (BYOK set):** the `balance_cents` check is SKIPPED. LLM calls bill against the operator's provider quota directly. `tenant_billing.balance_cents` is not mutated on BYOK runs.

### 7.5.2 Dashboard banner

When the active tenant has `balance_cents <= 0` AND `provider.type == 'hosted'`:

- Global top-of-page banner (all dashboard routes): "You've used all your free credits. Add your own LLM key to keep running." with a `[Configure BYOK →]` button linking to Settings > Provider.
- Banner is dismissible per session but re-appears next session until BYOK is configured or credits are topped up.
- If `provider.type != 'hosted'`, banner is NEVER shown (even with balance_cents <= 0), because BYOK runs don't consume the credit pool.

### 7.5.3 CLI credit-exhausted behavior

Both `zombiectl zombie install` and `zombiectl zombie trigger` (and any future invocation verb) return exit code 2 when the API returns `402 UZ-CREDIT-EXHAUSTED`, with this stderr message:

```
Error: Credits exhausted.

Your tenant has used all 1000 free credits. To keep running, bring your own LLM key:

  zombiectl provider set --provider anthropic --credential-ref <vault-name>

Or add credits at https://app.usezombie.com/settings/billing.
```

### 7.5.4 Existing in-flight zombies (CRITICAL)

Runs already executing when the tenant balance crosses zero must not be killed mid-step — that would leave partial tool calls and corrupt state. Instead:

- The in-flight run **finishes the current step** (completes the current LLM call + any pending tool call chain for that step).
- At the next **step boundary**, the worker re-reads `balance_cents`. If still `<= 0` and provider is still hosted, the run halts with an `credit_exhausted_at_step` event in the activity stream, including the step_id it stopped at.
- The run does NOT auto-resume. It stays in `status = credit_exhausted` until either:
  - BYOK is configured for the tenant (worker resumes from the halted step), OR
  - Tenant balance is topped up (post-MVP billing flow — for now, halted runs remain halted).
- **State integrity:** all tool outputs, grants, activity events produced up to the halt step are preserved. Resuming is deterministic from the halted step boundary.

### 7.5.5 Once BYOK is set

After a successful `PUT /v1/tenants/me/provider` with `type != 'hosted'`:

- Worker stops reading `tenant_billing.balance_cents` on future invocations.
- Any `credit_exhausted` zombies can be resumed (manual trigger for MVP; automatic resume deferred).
- LLM calls bill against the operator's provider directly. The UseZombie hosted LLM quota is not touched.

---

## 8.0 Acceptance Criteria

- [ ] BYOK set from dashboard — verify: dim 1.2
- [ ] Provider credentials excluded from firewall injection — verify: dim 4.2
- [ ] CLI `provider set` works — verify: dim 3.1
- [ ] Hosted credits exhausted → UZ-PROV-003 — verify: dim 5.3 integration test
- [ ] Usage panel shows per-zombie cost — verify: dim 1.4

---

## Applicable Rules

RULE FLL, RULE FLS (drain — Zig handlers), RULE XCC (cross-compile), RULE TXN (atomic credit deduction).
Schema change triggers Schema Table Removal Guard (adding `credential_type` column — pre-v2.0 teardown era: verify full teardown cycle).

---

## Eval Commands

```bash
zig build 2>&1 | head -5; echo "zig_build=$?"
make test 2>&1 | tail -5
make test-integration 2>&1 | grep -i "provider\|byok\|credit" | tail -10
npm run build 2>&1 | head -5
zig build -Dtarget=x86_64-linux 2>&1 | tail -3
zig build -Dtarget=aarch64-linux 2>&1 | tail -3
make check-pg-drain 2>&1 | tail -3
gitleaks detect 2>&1 | tail -3
```

---

## Out of Scope

- Per-zombie provider override — deferred to post-MVP; workspace-level is sufficient for operator "my credits ran out, use my Anthropic key" motion.
- Per-zombie model fine-tuning or custom system prompts from UI (skill template is the right surface)
- On-prem / private LLM endpoints (noted as `base_url` extension, not V1)
- Provider-level rate limiting / cost caps per zombie (future)
- Credit purchase flow from dashboard (links to billing/Stripe page for V1)
- Multi-model routing within a single run (route cheap tasks to Haiku, complex to Opus) — V3
