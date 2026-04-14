# M21_001: BYOK Provider Configuration — CLI / UI / API for LLM Provider and Credits

**Prototype:** v2
**Milestone:** M21
**Workstream:** 001
**Date:** Apr 13, 2026
**Status:** PENDING
**Priority:** P1 — Blocks operators who exhaust free credits; enables enterprise self-serve
**Batch:** B6 — after M12 (settings page shell), M13 (credential vault), M15_001 (credit metering, done)
**Branch:** feat/m21-byok-provider
**Depends on:** M12_001 (settings page), M13_001 (credential vault), M15_001 (credit metering, done)

> **v2 note — identifier reuse:** `M21_001` is also used in v1 for "Agent Interrupt and Steer" (`docs/v1/done/M21_001_AGENT_INTERRUPT_AND_STEER.md`, done). That v1 primitive is what M23_001 (v2) builds on. When resolving `M21_001` in a cross-reference, check the version prefix (`v1/done/` vs `v2/pending/`) — they are unrelated features sharing the same milestone slot across schema generations.

---

## Overview

**Goal (testable):** An operator who has exhausted their free credits can: (a) open Settings > Provider in the dashboard, switch to "Bring your own key", select their Anthropic credential from the vault, choose a model, and click Save — after which all zombie runs bill against their own API quota; OR (b) run `zombiectl provider set --workspace ws --provider anthropic --credential-ref my_anthropic_key --model claude-sonnet-4-6` with identical effect; OR (c) call `PUT /v1/workspaces/{ws}/provider` via the API. A per-zombie override lets individual zombies use different models (e.g., Haiku for cheap routing tasks, Sonnet for complex reasoning).

**Problem:** Credits run out. Today, when an operator's 20 free credits are exhausted, their only paths are: upgrade the UseZombie plan (pay UseZombie to keep running hosted Claude), or stop using the product. BYOK (bring your own key) is the enterprise self-serve unlock: the operator adds their own Anthropic or OpenAI API key, and UseZombie becomes the orchestration layer without being the LLM cost center. Without this, operators who want to control their LLM spend, choose specific models, or route to an on-prem/private model cannot do so.

**Conceptual distinction:** Provider credentials are different from tool credentials. Tool credentials (M13) are injected by the firewall into outbound HTTP calls that the zombie makes to external services. Provider credentials are used by the UseZombie runtime itself to make LLM API calls on behalf of the zombie — they are never visible to the zombie's tool calls, never logged in the activity stream, never accessible via zombie code.

**Solution summary:** (1) New `credential_type = llm_provider` in the credential vault — stored with the same encryption as tool credentials but tagged differently and excluded from the firewall injection path. (2) Provider config API: `GET/PUT /v1/workspaces/{ws}/provider` for workspace default; `GET/PUT /v1/workspaces/{ws}/zombies/{id}/provider` for per-zombie override. (3) Dashboard: Settings > Provider tab. (4) CLI: `zombiectl provider set/get`. (5) Usage panel on Settings showing per-zombie token usage and estimated cost from M15_001's metering data.

**DX paths:**

| Action | CLI | UI | API |
|---|---|---|---|
| View current provider | `zombiectl provider get` | Settings > Provider | `GET /v1/workspaces/{ws}/provider` |
| Set BYOK (workspace) | `zombiectl provider set --provider anthropic ...` | Settings > Provider > BYOK | `PUT /v1/workspaces/{ws}/provider` |
| Per-zombie override | `zombiectl zombie provider set --zombie {id} ...` | Zombie detail > Config > Provider | `PUT /v1/workspaces/{ws}/zombies/{id}/provider` |
| View credit usage | `zombiectl credits status` | Settings > Usage panel | `GET /v1/workspaces/{ws}/credits` |

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
  - expected: `PUT /v1/workspaces/{ws}/provider` with `{ type: "anthropic", credential_ref: "my_anthropic_key", model: "claude-sonnet-4-6" }` — success toast
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

## 2.0 Per-Zombie Provider Override

**Status:** PENDING

On the zombie detail page > Config tab, an operator can override the workspace provider for that specific zombie. Useful for: cheap routing zombies (Haiku), high-stakes reasoning zombies (Opus), or zombies that must use a different company API key.

**Layout:**

```
Zombie detail: blog-writer
[Activity] [Pending] [Integrations] [Memory] [Config]

Provider
● Inherit workspace default (Anthropic / claude-sonnet-4-6)
○ Override for this zombie
  Provider    [OpenAI ▾]
  Credential  [openai_key ▾]
  Model       [gpt-4o ▾]

[Save]
```

**Dimensions:**
- 2.1 PENDING
  - target: `app/zombies/[id]/components/ZombieConfig.tsx` (provider section)
  - input: zombie inheriting workspace default
  - expected: "Inherit workspace default" selected, current workspace provider shown
  - test_type: unit (component test)
- 2.2 PENDING
  - target: `app/zombies/[id]/components/ZombieConfig.tsx`
  - input: user selects override, picks OpenAI, selects model gpt-4o, saves
  - expected: `PUT /v1/workspaces/{ws}/zombies/{id}/provider` with override config — success toast
  - test_type: integration (API mock)

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

# Per-zombie override
$ zombiectl zombie provider set \
    --zombie zom_01xyz \
    --provider openai \
    --credential-ref openai_key \
    --model gpt-4o
✓ blog-writer will use OpenAI gpt-4o

# Clear override (back to workspace default)
$ zombiectl zombie provider set --zombie zom_01xyz --inherit
✓ blog-writer inheriting workspace provider

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
  - expected: `PUT /v1/workspaces/{ws}/provider` called; success message printed
  - test_type: CLI integration
- 3.2 PENDING
  - target: `zombiectl provider set --hosted`
  - input: workspace currently on BYOK
  - expected: provider switched to hosted; remaining credits shown
  - test_type: CLI integration
- 3.3 PENDING
  - target: `zombiectl zombie provider set --inherit`
  - input: zombie with existing override
  - expected: override cleared; zombie inherits workspace provider
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
GET  /v1/workspaces/{ws}/provider                   — get workspace LLM provider config
PUT  /v1/workspaces/{ws}/provider                   — set workspace LLM provider config
GET  /v1/workspaces/{ws}/zombies/{id}/provider      — get per-zombie override
PUT  /v1/workspaces/{ws}/zombies/{id}/provider      — set per-zombie override
```

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
| 3 | `GET/PUT /v1/workspaces/{ws}/zombies/{id}/provider` handlers | dim 2.2 |
| 4 | M13 credential modal: add Type field | dim 4.1 |
| 5 | Settings > Provider tab (dashboard UI) | dims 1.1–1.5 |
| 6 | Per-zombie provider section in Config tab | dims 2.1–2.2 |
| 7 | CLI `zombiectl provider set/get` + `zombiectl zombie provider set` | dims 3.1–3.3 |
| 8 | Usage panel with M15_001 metering data | dim 1.4 |
| 9 | Cross-compile + full test gate | all dims |

---

## 8.0 Acceptance Criteria

- [ ] BYOK set from dashboard — verify: dim 1.2
- [ ] Provider credentials excluded from firewall injection — verify: dim 4.2
- [ ] Per-zombie override works — verify: dim 2.2
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

- Per-zombie model fine-tuning or custom system prompts from UI (skill template is the right surface)
- On-prem / private LLM endpoints (noted as `base_url` extension, not V1)
- Provider-level rate limiting / cost caps per zombie (future)
- Credit purchase flow from dashboard (links to billing/Stripe page for V1)
- Multi-model routing within a single run (route cheap tasks to Haiku, complex to Opus) — V3
