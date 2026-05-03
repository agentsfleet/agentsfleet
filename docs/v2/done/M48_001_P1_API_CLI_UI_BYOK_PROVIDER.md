# M48_001: BYOK Provider + Credit-Pool Billing — Tenant-Scoped Provider Configuration, Two-Debit Cost Model, Read-Only Billing UX

**Prototype:** v2.0.0
**Milestone:** M48
**Workstream:** 001
**Date:** May 01, 2026
**Status:** DONE

> **CLI rename note (May 03, 2026):** M49's standardization slice renamed `zombiectl tenant provider get/set/reset` → `show/add/delete`. Historical command examples below preserve the verbs as shipped; current canonical verbs follow the `add / show / list / delete` standard.

**Priority:** P1 — launch-blocking (substrate-tier, Week 2-3). BYOK is the second of three v2 differentiation pillars (OSS + BYOK + markdown-defined; self-host deferred to v3). Without user-controlled LLM provider config and a clean credit-based billing model, the launch tweet's BYOK claim is hollow and the differentiation argument collapses.
**Categories:** API, CLI, UI, SCHEMA, BILLING
**Batch:** B1 — substrate-tier alongside M40-M45.
**Branch:** feat/m48-001-byok-provider-billing
**Depends on:**
- **M11_005** (tenant billing — DONE) — provides `core.tenant_billing` and `balance_cents`. M48 extends with cost functions, two debit points in `processEvent`, and a one-time $10 starter grant insertion at tenant creation.
- **M44_001** (install contract + doctor) — owns the `zombiectl doctor --json` surface that this spec extends with a `tenant_provider` block.
- **M45_001** (vault structured credentials) — opaque JSON-object credentials keyed by name; this spec stores BYOK records there.

**Canonical architecture:**
- [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) — credit-pool billing model, two debit points, `compute_receive_charge` + `compute_stage_charge`, posture transitions, model-caps endpoint with token rates, api_key visibility boundary, read-only dashboard layout.
- [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.7 — three-rail diagram (platform vs BYOK origin), worker sentinel overlay.
- [`docs/architecture/scenarios/01_default_install.md`](../../architecture/scenarios/01_default_install.md), [`02_byok.md`](../../architecture/scenarios/02_byok.md), [`03_balance_gate.md`](../../architecture/scenarios/03_balance_gate.md) — John Doe's end-to-end journey across both postures.

---

## Tier promotion provenance (Apr 25, 2026 /plan-ceo-review)

CEO scope review on Apr 25, 2026 promoted M48 from "soft-blocks BYOK launch claim, post-launch" to "Launch-blocking substrate, Week 2-3." Self-host was deferred to v3 in the same session, compressing the v2 differentiation pillars from four to three (**OSS + BYOK + markdown-defined**). Dropping BYOK leaves only two pillars, both matchable by competitors (AWS DevOps Agent / Sonarly / incident.io) within a week.

Subsequent review (May 01, 2026) folded the credit-pool billing model rewrite into M48's scope. M48 now ships (a) the provider state machine, (b) the credit-based cost functions and two debit points, and (c) a read-only Amp-style billing dashboard + `zombiectl billing show`. Stripe Purchase Credits is deferred to v2.1.

| Tier | Specs |
|---|---|
| **Launch-blocking substrate (Week 1-3)** | M40 Worker, M41 Context Layering, M42 Streaming, M43 Webhook Ingest, M44 Install Contract + Doctor, M45 Vault Structured, **M48 BYOK Provider + Billing** |
| **Launch-shipping packaging (Week 4-7)** | M46 Frontmatter Schema, M49 Install-Skill, M51 Docs + Install-Pingback |
| **Parallel from Week 1, validation-blocking** | M50 Customer-Development Parallel Workstream |
| **Post-launch** | M47 Approval Inbox, Stripe Purchase Credits (v2.1) |

---

## Overview

**Problem.** Two intertwined problems landed on M48 at the same time:

1. **No user-controlled LLM provider.** A user who already has an Anthropic / Fireworks / OpenAI / OpenRouter account cannot route inference through their own quota; UseZombie is the LLM bill payer for everyone. That kills the differentiation against AWS DevOps Agent (Bedrock-locked) and Sonarly (Claude-Code-managed).
2. **No credit-based billing model.** Today's billing is plan-tier-shaped (Free included events, Team overage, etc.) which doesn't match how users actually want to think about cost — they want a credit pool that drains, not a tier ladder. Amp's model (one balance, deduct per event) is the simpler ergonomic story we're aligning with.

M48 ships both, integrated. The billing model and the provider posture share the same `processEvent` code path; posture changes the per-event drain rate, not the structure of the gate.

**Goal (testable).** A new tenant (John Doe — the persona running through every scenario in this spec) experiences the following coherent journey:

1. **Sign-up.** Tenant created; `core.tenant_billing.balance_cents = 1000` (one-time $10 starter grant inserted at tenant create). `core.tenant_providers` has no row. `zombiectl doctor --json`'s `tenant_provider` block reports the synthesised platform default.
2. **Cold install on default platform-managed posture.** John runs `/usezombie-install-platform-ops` (M49); the install-skill reads doctor's block and writes resolved frontmatter values. First webhook fires, gate passes, two telemetry rows written (`charge_type='receive'` + `charge_type='stage'`), balance drops by ~3¢ (1¢ receive + ~2¢ stage = receive-overhead + token-based stage cost on `accounts/fireworks/models/kimi-k2.6`).
3. **Brings own key.** A couple of weeks in, John runs:
   ```bash
   op read 'op://<vault>/fireworks/api_key' |
     jq -Rn '{provider:"fireworks", api_key: input, model:"accounts/fireworks/models/kimi-k2.6"}' |
     zombiectl credential set account-fireworks-byok --data @-
   zombiectl tenant provider set --credential account-fireworks-byok
   ```
   `core.tenant_providers` now has a row with `mode=byok`, `credential_ref="account-fireworks-byok"`, model + cap resolved from the model-caps endpoint.
4. **Drain rate drops.** Next event resolves `mode=byok`. Receive deduct = 0¢ (BYOK receive is free); stage deduct = 1¢ flat (orchestration only — Fireworks bills John for the LLM tokens directly). John's UseZombie balance now drains ~3× slower for the same workload.
5. **Eventually exhausts.** Some weeks later balance hits 0¢. Gate trips on the next event with `failure_label='balance_exhausted'`. CLI prints `"See https://app.usezombie.com/settings/billing"`. Dashboard shows the empty-balance state with a disabled "Purchase Credits" button (Stripe ships in v2.1; v2.0 users contact support for a manual top-up).

The api_key never leaves the resolver-to-executor path: it's not in the doctor block, not in HTTP responses, not in the agent tool context, not in event logs.

**Solution shape.** Three concerns, one milestone:

1. **Provider state machine.** Tenant-scoped `core.tenant_providers` row with `mode/provider/model/context_cap_tokens/credential_ref`. User-named credentials in the M45 vault. One active provider per tenant. Doctor JSON extension. CLI `tenant provider {get|set|reset}`. Settings → Provider page.
2. **Credit-pool billing extension.** No new schema (`core.tenant_billing.balance_cents` already exists from M11_005). One-time $10 starter grant on tenant create. `compute_receive_charge(posture)` + `compute_stage_charge(posture, model, in_tok, out_tok)`. Two debit points wired into `processEvent`. Per-model token rates resolved at API server boot from the extended model-caps endpoint.
3. **Read-only billing UX.** Settings → Billing dashboard (Amp-style layout; Balance card with disabled Purchase Credits button; Usage tab showing the two-row-per-event telemetry; empty-state Invoices and Payment Method tabs). `zombiectl billing show` for the CLI surface.

---

## End-to-end user scenarios

Single persona: **John Doe**. Every test in the Test Specification maps back to one of these scenarios.

### Scenario A — John starts on default platform-managed

1. John signs up. Tenant created. `core.tenant_billing.balance_cents = 1000` (starter grant inserted synchronously at tenant create). `core.tenant_providers` has **no row**.
2. He runs `/usezombie-install-platform-ops` (M49). The skill calls `zombiectl doctor --json`.
3. Doctor's `tenant_provider` block reports the synthesised platform default:
   ```json
   { "mode": "platform",
     "provider": "fireworks",
     "model": "accounts/fireworks/models/kimi-k2.6",
     "context_cap_tokens": 256000 }
   ```
4. Skill writes resolved values into frontmatter (`model: accounts/fireworks/models/kimi-k2.6`, `context_cap_tokens: 256000`).
5. First event:
   - `processEvent` resolves posture → synthesised platform default.
   - Estimate cost = `compute_receive_charge(.platform)` + worst-case `compute_stage_charge(.platform, accounts/fireworks/models/kimi-k2.6, est_in, est_out)` = 1¢ + ~2¢ = 3¢.
   - Balance gate: 1000¢ ≥ 3¢ → pass.
   - Receive deduct: 1¢ → 999¢. INSERT `zombie_execution_telemetry (charge_type='receive', credit_deducted_cents=1)`.
   - Stage deduct (conservative estimate): ~2¢ → ~997¢. INSERT `zombie_execution_telemetry (charge_type='stage', credit_deducted_cents≈2)`.
   - Worker calls `executor.startStage` with the Fireworks api_key returned by `resolveActiveProvider` (the resolver looked it up via `core.platform_llm_keys` → admin workspace's `vault.secrets`). Outbound call hits `api.fireworks.ai/inference/v1`.
   - StageResult returns. UPDATE the stage telemetry row with actual `token_count_input/output, wall_ms`.
6. Drains over time at ~3¢/event (token-based). Two telemetry rows per event (`charge_type='receive'`, `charge_type='stage'`).

### Scenario B — John brings his own key (BYOK setup)

1. After a couple of weeks at platform rates John gets a Fireworks AI account.
2. He runs:
   ```bash
   op read 'op://<vault>/fireworks/api_key' |
     jq -Rn '{provider:"fireworks", api_key: input, model:"accounts/fireworks/models/kimi-k2.6"}' |
     zombiectl credential set account-fireworks-byok --data @-
   ```
   This writes a row to `core.vault` at `(tenant_id, "account-fireworks-byok")` with the JSON as encrypted opaque data (M45 contract). The API key does not appear in shell history or process argv because it flows through stdin.
3. He runs `zombiectl tenant provider set --credential account-fireworks-byok`. The CLI's API call:
   - Loads the vault row at `(tenant_id, "account-fireworks-byok")`.
   - Validates `provider`/`api_key`/`model` are present (eager structural validation — 400 `credential_data_malformed` otherwise).
   - GETs `https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json?model=accounts%2Ffireworks%2Fmodels%2Fkimi-k2.6` → returns `context_cap_tokens: 256000` (and the per-model token rates already cached at API boot).
   - If model isn't in the catalogue: 400 `model_not_in_caps_catalogue`; row not written.
   - UPSERTs `core.tenant_providers`:
     ```
     tenant_id          = <john>
     mode               = byok
     provider           = fireworks
     model              = accounts/fireworks/models/kimi-k2.6
     context_cap_tokens = 256000
     credential_ref     = account-fireworks-byok
     ```
   - CLI prints "Tip: run a test event to verify the key works against fireworks."
4. The next event:
   - Resolver returns `{mode: byok, provider: fireworks, api_key: fw_LIVE_…, model: kimi-k2.6, context_cap_tokens: 256000}`.
   - Estimate cost = `compute_receive_charge(.byok)` + `compute_stage_charge(.byok, …)` = 0¢ + 1¢ = 1¢.
   - Receive deduct: 0¢ (BYOK receive is free in v2.0). INSERT receive telemetry row with `credit_deducted_cents=0`.
   - Stage deduct: 1¢ flat. INSERT stage telemetry row with `credit_deducted_cents=1`.
   - Outbound call hits `api.fireworks.ai/inference/v1/chat/completions`. Fireworks bills John's Fireworks account directly for the tokens.
   - StageResult returns. UPDATE stage telemetry row with actual `token_count_input/output, wall_ms`. The cents column does **not** change (BYOK stage cost is flat).
5. UseZombie's drain on `balance_cents` drops from ~3¢ to 1¢ per event. John's $10 starter grant lasts ~3× longer.

### Scenario C — John switches BYOK model

1. A month later John wants to try DeepSeek V4 Pro on the same Fireworks account.
2. Updates the credential body and re-runs `tenant provider set`:
   ```bash
   op read 'op://<vault>/fireworks/api_key' |
     jq -Rn '{provider:"fireworks", api_key: input, model:"accounts/fireworks/models/deepseek-v4-pro"}' |
     zombiectl credential set account-fireworks-byok --data @-
   zombiectl tenant provider set --credential account-fireworks-byok
   ```
3. CLI re-resolves the cap (e.g. `131072`), rewrites the `tenant_providers` row's `model` and `context_cap_tokens` columns. `credential_ref` stays the same.
4. In-flight events that were already claimed under Kimi K2 finish under Kimi K2 (Invariant 4). Next event uses DeepSeek V4 Pro.
5. Existing `.usezombie/platform-ops/SKILL.md` does not need regeneration — its `model: ""` and `context_cap_tokens: 0` sentinels keep working; the worker overlays the new values.

### Scenario D — John reverts to platform

1. John's Fireworks account hits a billing issue. He runs `zombiectl tenant provider reset`.
2. CLI updates the `tenant_providers` row to `mode = platform`, `credential_ref = NULL`, `provider = fireworks`, `model = accounts/fireworks/models/kimi-k2.6`, re-resolves cap from caps endpoint → `context_cap_tokens = 256000`.
3. In-flight events finish under BYOK. Next event runs against platform-managed Fireworks at the platform-rate (~3¢/event).
4. If `tenant_billing.balance_cents` is too low for the platform-rate worst-case, the next event blocks at the balance gate with `failure_label='balance_exhausted'`. The flip itself succeeds regardless of balance — John chose this posture explicitly.

### Scenario E — John deletes BYOK credential while still in BYOK mode

1. John runs `zombiectl credential delete account-fireworks-byok` without first running `tenant provider reset`. Vault row is gone; `tenant_providers` still says `mode=byok`, `credential_ref=account-fireworks-byok`.
2. Next event: `resolveActiveProvider` finds the row but the vault load returns NULL. Resolver returns `error.CredentialMissing`.
3. Event is dead-lettered with `failure_label='provider_credential_missing'`. **No deduction is taken** (we couldn't even resolve posture). The system does NOT auto-revert to platform — that would silently re-enable platform billing without consent.
4. John sees the error in his event log and via `tenant provider get` (which surfaces a `⚠ Credential X is missing from vault` warning). He must either re-add the credential under the same name OR run `tenant provider reset` to opt back into platform billing explicitly.

### Scenario F — John exhausts his credits

1. After ~5 weeks of mixed-posture use (Scenarios A–C), `balance_cents` reaches 0¢.
2. Next event: gate trips. UPDATE `zombie_events` SET `status='gate_blocked'`, `failure_label='balance_exhausted'`. XACK terminal.
3. CLI surfaces (in `zombiectl events <id>` and as a one-line hint after the next `zombiectl steer`):
   `ⓘ Credits exhausted. See https://app.usezombie.com/settings/billing`
4. `zombiectl billing show` prints balance = $0.00 + the last 10 drains.
5. Dashboard `/settings/billing` shows the empty-balance hero. **Purchase Credits** button is visible but disabled with a tooltip *"Coming in v2.1 — contact support for a top-up."* Auto Top Up card hidden in v2.0.
6. John emails support. Manual top-up brings balance back. Events resume on the next trigger; previously gate-blocked events are NOT auto-replayed (he re-triggers from source if he wants them processed).

---

## Architecture

### Tenant-provider state — `core.tenant_providers`

One row per tenant who has explicitly configured a provider. **Absence of row = synthesised platform default.** The resolver treats "no row" and "row with `mode=platform`" as semantically identical for runtime behaviour; the row exists only when a user has explicitly touched provider config (an explicit `tenant provider reset` writes the explicit `mode=platform` row so the dashboard can distinguish "never configured" from "explicitly reset").

```sql
CREATE TABLE core.tenant_providers (
    tenant_id          UUID         PRIMARY KEY
                                    REFERENCES core.tenants(id)
                                    ON DELETE CASCADE,

    mode               TEXT         NOT NULL,
    -- Application-enforced domain: { "platform", "byok" }.
    -- Per RULE STS, no SQL CHECK constraint with hardcoded strings;
    -- enforced via constants in src/state/tenant_provider.zig.

    provider           TEXT         NOT NULL,
    -- Resolved at write time from the referenced credential's `provider`
    -- field when mode=byok; "anthropic" when mode=platform. Persisted so
    -- the doctor block reads this without re-parsing vault JSON.

    model              TEXT         NOT NULL,
    -- Authoritative model name. Under mode=platform: the platform default
    -- ("accounts/fireworks/models/kimi-k2.6" at v2.0). Under mode=byok: the model from the
    -- referenced credential (CLI --model flag overrides at set time).

    context_cap_tokens INTEGER      NOT NULL,
    -- Resolved at write time by GETting the model-caps endpoint with `model`.
    -- Re-resolved on every `tenant provider set` so cap stays in sync with
    -- the model. Worker reads this when frontmatter carries a sentinel.

    credential_ref     TEXT,
    -- NULL when mode=platform.
    -- User-chosen credential name when mode=byok (e.g.
    -- "account-fireworks-byok", "anthropic-prod"). The vault row at
    -- (tenant_id, credential_ref) holds the JSON {provider, api_key, model}.
    -- v1 invariants (application-checked):
    --   mode=platform  ⇒ credential_ref IS NULL
    --   mode=byok      ⇒ credential_ref is non-empty + vault row exists

    created_at         TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX core_tenant_providers_mode_idx ON core.tenant_providers(mode);
```

**Why user-named `credential_ref`** (not hardcoded `"llm"`). The Apr 30 review of M43 had pinned a "convention name = `llm` for BYOK" rule by analogy to webhook credentials (`name = trigger.source`). For BYOK the analogy doesn't hold — there is no trigger.source. User-naming gives the schema honest semantics, unlocks multi-credential tenants from day 1 (a user can store `anthropic-prod` AND `fireworks-staging` in vault, flip between them with `tenant provider set --credential <name>`), and avoids namespace collision with M43's `github`-style workspace-scoped convention.

### Tenant-billing extension — starter grant + cost functions, no new schema

`core.tenant_billing.balance_cents` already exists from M11_005 — **no schema change to that table**. The schema work in M48 lives elsewhere: §1 below adds `core.tenant_providers`, and `core.zombie_execution_telemetry` is rewritten in place to add `charge_type` + `posture` columns and replace its UNIQUE on `event_id` with UNIQUE on `(event_id, charge_type)` (per the two-row-per-event contract). Both schema changes are in the Files Changed table.

M48 adds the runtime billing surface on top of those:

- **One-time $10 starter grant** at tenant creation. Insertion of `STARTER_GRANT_CENTS = 1000` into `tenant_billing.balance_cents` synchronously when the tenant row is created. Implementation: extend the tenant-creation handler (M11_005-owned code) to write the grant in the same transaction as the tenant row.
- **`compute_receive_charge(posture)`** and **`compute_stage_charge(posture, model, in_tok, out_tok)`** in `src/state/tenant_billing.zig`. No `plan` parameter — the credit-pool model has no plan-tier branching in the cost function.
- **Two debit points wired into `processEvent`** (`src/zombie/event_loop_helpers.zig`): a receive deduct after the gate passes, a stage deduct before `executor.startStage`. Each debit + telemetry insert is one transaction.
- **Per-model token rate cache** in `src/state/model_rate_cache.zig` (NEW), populated at API server boot from the extended model-caps endpoint. `lookup_model_rate(model) → {input_cents_per_mtok, output_cents_per_mtok}`.

### Resolver — `tenant_provider.resolveActiveProvider(tenant_id)`

```
resolveActiveProvider(tenant_id) → ResolvedProvider {
  mode:               "platform" | "byok",
  provider:           "anthropic" | "fireworks" | "openai" | ...,
  api_key:            string,    // process-internal only — see api_key boundary
  model:              string,
  context_cap_tokens: u32,
}

Algorithm:
  row = SELECT * FROM core.tenant_providers WHERE tenant_id = $1
  if row IS NULL OR row.mode == "platform":
    return resolvePlatformDefault(row)
      // 1. plk = SELECT provider, source_workspace_id FROM core.platform_llm_keys
      //          WHERE active = true LIMIT 1
      //    (Admin-managed via PUT /v1/admin/platform-keys; one active row at v2.0,
      //     pointing at the usezombie-admin user's workspace. See M11_006 spec
      //     and playbooks/012_usezombie_admin_bootstrap/.)
      //
      // 2. cred = vault.loadJson(plk.source_workspace_id, plk.provider)
      //    (Same vault.secrets path as any user's BYOK — admin's workspace
      //     just happens to be the source for platform-managed events.)
      //
      // 3. Return { mode: "platform",
      //             provider: plk.provider,                        // e.g. "fireworks"
      //             api_key: cred.api_key,                          // process-internal only
      //             model: row?.model    ?? PLATFORM_DEFAULT_MODEL, // synth-default if no row
      //             context_cap_tokens: row?.context_cap_tokens
      //                              ?? PLATFORM_DEFAULT_CAP }
      //
      // PLATFORM_DEFAULT_MODEL / PLATFORM_DEFAULT_CAP are RULE-UFS constants
      // declared once in src/state/tenant_provider.zig — at v2.0:
      // "accounts/fireworks/models/kimi-k2.6" + 256000.
      //
      // No api_key constant lives in code; the api_key always comes through
      // vault.loadJson on the admin workspace. If platform_llm_keys has no
      // active row OR the admin's vault row is missing, return
      // error.PlatformKeyMissing (operator-side incident, not a user error).

  if row.mode == "byok":
    cred = vault.loadJson(tenant_id, row.credential_ref)
    if cred IS NULL:
      return error.CredentialMissing
    if cred.provider IS NULL or cred.api_key IS NULL:
      return error.CredentialDataMalformed
    return { mode: "byok",
             provider: cred.provider,
             api_key: cred.api_key,
             model: row.model,                  // authoritative copy on row
             context_cap_tokens: row.context_cap_tokens }
```

The resolver is the only code path that handles the api_key. Every consumer (executor, doctor, HTTP handlers) calls the resolver and then either uses the api_key for an outbound call or strips it out before any user-facing surface.

### Doctor surface

`zombiectl doctor --json` (M44) is extended with a `tenant_provider` block. M48 owns the field; M44 owns the surface. M48's PR adds the doctor handler integration.

```json
{
  ...,
  "tenant_provider": {
    "mode":               "platform" | "byok",
    "provider":           "anthropic" | "fireworks" | "openai" | ...,
    "model":              "accounts/fireworks/models/kimi-k2.6",
    "context_cap_tokens": 256000,
    "credential_ref":     "account-fireworks-byok" | null,
    "error":              "credential_missing" | "credential_data_malformed"   // optional — present only when resolver failed
  }
}
```

The block is **always present** — for tenants with no row, it carries the synthesised platform default. The api_key is **never** in this block. Doctor is a readiness surface, not a secret surface.

### Two debit points in `processEvent`

The full sequence at `src/zombie/event_loop_helpers.zig`:

```
processEvent(envelope):
  1. INSERT zombie_events (status='received', actor, request_json)

  2. resolved = tenant_provider.resolveActiveProvider(tenant_id)
     if resolved is .CredentialMissing OR .CredentialDataMalformed:
       UPDATE zombie_events SET status='dead_lettered',
                                failure_label='provider_credential_missing'
       PUBLISH event_complete; XACK; return.
     // Note: no deduction taken; we couldn't determine posture.

  3. Estimate cost (conservative — uses worst-case stage tokens):
       est_receive = compute_receive_charge(resolved.posture)
       est_stage   = compute_stage_charge(resolved.posture, resolved.model,
                                          ESTIMATE_FLOOR_INPUT,
                                          ESTIMATE_FLOOR_OUTPUT)
       est_total = est_receive + est_stage

  4. Balance gate:
       if balance_cents < est_total:
         UPDATE zombie_events SET status='gate_blocked',
                                  failure_label='balance_exhausted'
         PUBLISH event_complete; XACK; return.

  5. RECEIVE DEBIT (transactional):
       cents = compute_receive_charge(resolved.posture)
       UPDATE tenant_billing SET balance_cents = balance_cents - cents
       INSERT zombie_execution_telemetry (
         event_id, posture, model,
         charge_type='receive', credit_deducted_cents=cents
       )
       (single transaction; either both writes succeed or neither)

  6. Approval gate (orthogonal to billing — same gate_blocked pattern if blocked).

  7. resolveSecretsMap → tool credentials (NOT the api_key — different path).

  8. STAGE DEBIT (transactional, conservative estimate):
       cents = compute_stage_charge(resolved.posture, resolved.model,
                                    ESTIMATE_FLOOR_INPUT,
                                    ESTIMATE_FLOOR_OUTPUT)
       UPDATE tenant_billing SET balance_cents = balance_cents - cents
       INSERT zombie_execution_telemetry (
         event_id, posture, model,
         charge_type='stage', credit_deducted_cents=cents,
         token_count_input=NULL, token_count_output=NULL, wall_ms=NULL
       )

  9. executor.createExecution(workspace, {network_policy, tools, secrets_map,
                                          context, model: resolved.model,
                                          provider_api_key: resolved.api_key,
                                          provider_endpoint: <from catalogue>})
     executor.startStage(execution_id, message)
     // outbound LLM call hits resolved.provider's endpoint with resolved.api_key

  10. StageResult returns: { tokens_input, tokens_output, wall_ms, … }

  11. UPDATE zombie_execution_telemetry stage row
       SET token_count_input = StageResult.tokens_input,
           token_count_output = StageResult.tokens_output,
           wall_ms = StageResult.wall_ms
      // The credit_deducted_cents column is NOT updated; the conservative
      // estimate at step 8 is the charge. v3 may add refund-on-actual.

  12. UPDATE zombie_events SET status='processed', response_text, completed_at
      UPSERT zombie_sessions
      PUBLISH event_complete
      XACK
```

Properties:

- **Two debits, two telemetry rows per event.** UNIQUE on `(event_id, charge_type)`. Auditable: a query like *"what fraction of revenue came from receive overhead vs stage cost last month"* is a one-line SQL.
- **Conservative estimate at step 8.** We don't know real tokens until step 10. v2.0 charges the estimate at debit time and does not refund the difference. v3 may reconcile.
- **Each debit + telemetry insert is one transaction.** If the worker crashes between steps 5 and 8, the receive row is durable; on retry the gate re-runs and either passes (balance left) or blocks.
- **Step 2 dead-letter does NOT deduct.** Provider-credential-missing fails before posture is known; charging without knowing posture is incoherent.

### `compute_receive_charge` and `compute_stage_charge`

```zig
// src/state/tenant_billing.zig — extended by M48

pub const Posture = enum { platform, byok };

const STARTER_GRANT_CENTS:           u32 = 1000;  // $10 one-time on tenant create
const RECEIVE_PLATFORM_CENTS:        u32 = 1;
const RECEIVE_BYOK_CENTS:            u32 = 0;
const STAGE_OVERHEAD_PLATFORM_CENTS: u32 = 1;
const STAGE_OVERHEAD_BYOK_CENTS:     u32 = 1;

const ESTIMATE_FLOOR_INPUT:  u32 = 100;  // conservative floor for gate estimate
const ESTIMATE_FLOOR_OUTPUT: u32 = 100;

pub fn compute_receive_charge(posture: Posture) u32 {
    return switch (posture) {
        .platform => RECEIVE_PLATFORM_CENTS,
        .byok     => RECEIVE_BYOK_CENTS,
    };
}

pub fn compute_stage_charge(
    posture:       Posture,
    model:         []const u8,
    input_tokens:  u32,
    output_tokens: u32,
) u32 {
    return switch (posture) {
        .platform => blk: {
            const rate = lookup_model_rate(model)
                orelse @panic("model not in cached caps catalogue");
            const in_cents  = (rate.input_cents_per_mtok  * input_tokens)  / 1_000_000;
            const out_cents = (rate.output_cents_per_mtok * output_tokens) / 1_000_000;
            break :blk STAGE_OVERHEAD_PLATFORM_CENTS + in_cents + out_cents;
        },
        .byok => STAGE_OVERHEAD_BYOK_CENTS,
    };
}

// lookup_model_rate reads from a process-local cache populated at API
// server boot from /_um/<key>/model-caps.json. Refreshed on a slow timer
// (e.g. every hour). The hot path (this function) never makes network calls.
```

`@panic` under platform with an unknown model is correct: such a model would have been rejected at `tenant provider set` time (`400 model_not_in_caps_catalogue`) or at install-skill frontmatter generation. Reaching this function with an unknown model is an internal inconsistency.

### Worker overlay (sentinels)

A zombie's frontmatter (`x-usezombie.model` and `x-usezombie.context.context_cap_tokens`) can carry resolved values, sentinel values, or omit the keys entirely. The worker overlay rule (per [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.7):

| Frontmatter `model` | Frontmatter `context_cap_tokens` | Worker behavior |
|---|---|---|
| Non-empty string (e.g. `accounts/fireworks/models/kimi-k2.6`) | — | Use frontmatter value |
| Empty string (`""`) | — | Overlay from `tenant_providers.model` (or synth-default) |
| Key absent entirely | — | Overlay from `tenant_providers.model` (or synth-default) |
| — | Non-zero int (e.g. `200000`) | Use frontmatter value |
| — | Zero (`0`) | Overlay from `tenant_providers.context_cap_tokens` (or synth-default) |
| — | Key absent entirely | Overlay from `tenant_providers.context_cap_tokens` (or synth-default) |

Per-field, independent. Visible sentinels (`""`/`0`) for human readability; absent-key as the safety net.

### api_key visibility boundary

The api_key (platform OR BYOK) **may exist in:**
- `core.vault` rows (encrypted at rest via M45).
- Server-side process memory — return value of `resolveActiveProvider`, executor session.
- Outbound HTTPS request headers to the LLM provider.

The api_key **must never appear in:**
- HTTP response bodies (doctor, `GET /v1/tenants/me/provider`, billing API, etc.).
- Logs (worker, executor, structured, request).
- Agent tool context or tool-call records.
- Persisted event rows (`core.zombie_events`, `core.zombie_execution_telemetry`).
- User-facing artefacts (frontmatter, dashboard, CLI output).

Audit grep across event log, worker logs, executor logs, telemetry rows, and HTTP responses for the api_key bytes is a CI-level invariant.

---

## Implementation slices

### §1 — Schema migration (`schema/020_tenant_providers.sql`) — DONE

Full `core.tenant_providers` SQL above. Register in `schema/embed.zig` and `src/cmd/common.zig` migration array. Pre-v2.0.0 teardown-rebuild semantics per Schema Table Removal Guard — no `ALTER`/`DROP` migrations, no slot-marker files.

### §2 — Resolver (`src/state/tenant_provider.zig` — NEW) — DONE

**Vault scope (workspace-keyed; pre-v2.0 bridge).** `vault.secrets` is keyed by `(workspace_id, key_name)` — that's what M45 shipped, and reworking it is out of M48 scope. Wherever this spec writes `vault.loadJson(tenant_id, name)` for narrative clarity (Scenario B, the resolver pseudocode below, etc.), the actual call signature is `vault.loadJson(alloc, conn, ws_id, name)` where `ws_id` comes from `tenant_provider_resolver.resolvePrimaryWorkspace(tenant_id)`. That helper picks the earliest-named workspace owned by the tenant. Single-workspace tenants (the common v2.0 case) work transparently. Multi-workspace tenants implicitly pin **all** BYOK credentials to the earliest-named workspace; if a tenant ever needs per-workspace credential isolation, fully tenant-keyed vault is the post-v2.0 path. Until then, the bridge is the contract — documented here so future readers don't trip on the discrepancy. (Implementation landed in commit `78b6d3d4`.)

Exports:
- `pub const Mode = enum { platform, byok }`.
- `pub const ResolvedProvider = struct { mode, provider, api_key, model, context_cap_tokens }`.
- `pub fn resolveActiveProvider(allocator, conn, tenant_id) !ResolvedProvider`.
- `pub fn upsertByok(allocator, conn, tenant_id, credential_ref, model_override, cap) !void`.
- `pub fn upsertPlatform(allocator, conn, tenant_id) !void` (for `tenant provider reset`).
- `pub fn deleteRow(allocator, conn, tenant_id) !void` (test-only).

Resolves platform default by joining `core.platform_llm_keys` → admin workspace `vault.secrets` (per the M11_006 design and the playbook in `playbooks/012_usezombie_admin_bootstrap/`). The platform default **model + cap** are hardcoded constants in this module (per RULE UFS, declared once — at v2.0: `accounts/fireworks/models/kimi-k2.6` + `256000`). The platform default **api_key** is fetched on-demand from the admin tenant's vault, the same M45 path used for any user's BYOK; no api_key constant exists in code. If `platform_llm_keys` has no active row OR the admin's vault row is missing, the resolver returns `error.PlatformKeyMissing` (an operator-side incident, surfaced via dead-letter on the next event — not a user-recoverable error).

### §3 — Cost functions and debit wiring — DONE

**`src/state/tenant_billing.zig` (EDIT):**
- Add `Posture` enum + the constants above.
- Add `compute_receive_charge(posture)` and `compute_stage_charge(posture, model, in_tok, out_tok)`.
- `lookup_model_rate(model)` is imported from `src/state/model_rate_cache.zig` (the new module dedicated to the cache; see §11). `tenant_billing.zig` does not own the cache — it just reads from it.
- Add `insert_starter_grant(conn, tenant_id)` — writes `STARTER_GRANT_CENTS` into `balance_cents`. Called from the tenant-creation transaction.
- Add `deduct(conn, tenant_id, cents)` — `UPDATE tenant_billing SET balance_cents = balance_cents - $cents`.
- Add `insert_telemetry_row(conn, event_id, posture, model, charge_type, cents)` — INSERT into `zombie_execution_telemetry`.
- Add `update_telemetry_stage_row(conn, event_id, in_tok, out_tok, wall_ms)` — UPDATE the stage row post-execution.

**Tenant creation hook:** the existing M11_005-owned tenant creation handler (in `src/http/handlers/tenants/create.zig` or wherever it landed) gains one line: `try tenant_billing.insert_starter_grant(conn, tenant_id);` inside the same transaction as the tenant row INSERT. M48's PR touches this file but the change is one line.

**`src/zombie/event_loop_helpers.zig` (EDIT):** wire the two debit points into `processEvent` per the algorithm above. Each debit + telemetry insert runs in a transaction.

**`schema/zombie_execution_telemetry.sql`:** schema change — drop the existing UNIQUE on `event_id` (if present) and replace with UNIQUE on `(event_id, charge_type)`. Add `charge_type TEXT NOT NULL`. Pre-v2.0.0 teardown-rebuild: the existing telemetry table file is rewritten in place; old data is wiped on rebuild.

### §4 — Doctor extension (`src/http/handlers/doctor.zig` — EDIT) — DONE

Add `tenant_provider` block to the JSON response. Calls `resolveActiveProvider`, strips `api_key`, returns the rest. On resolver failure (`CredentialMissing`, `CredentialDataMalformed`), surfaces `tenant_provider: { mode: "byok", error: "credential_missing", credential_ref: "<name>", … }` so the install-skill can detect the broken state.

### §5 — HTTP API (`src/http/handlers/tenant_provider.zig` — NEW) — DONE

```
GET    /v1/tenants/me/provider
PUT    /v1/tenants/me/provider
DELETE /v1/tenants/me/provider          # equivalent to PUT mode=platform
```

5-place route registration per REST guide §7.

Tenant-admin guard: `bearer + tenant_admin_required` middleware. Non-admin → 403.

PUT validation order (eager structural, lazy auth):
1. Body shape — `{mode, credential_ref?, model?}`. Malformed → 400.
2. `mode=byok` + missing `credential_ref` → 400 `credential_ref_required`.
3. `mode=byok` + vault row at `(tenant_id, credential_ref)` not found → 400 `credential_not_found`.
4. `mode=byok` + vault JSON lacks `provider`, `api_key`, or `model` → 400 `credential_data_malformed`.
5. Resolve effective model (CLI `--model` override → vault `model`). Check `lookup_model_rate(model)` returns non-null AND model is in the cached caps catalogue. Not in catalogue → 400 `model_not_in_caps_catalogue`.
6. UPSERT row. Return 200 with the new resolved config (no api_key).

The handler does NOT make a synthetic call to the LLM provider. Auth-validity surfaces at first event as `provider_auth_failed` (lazy auth validation).

**Note on plan-tier rejection.** The credit-pool model has no plan-tier code path, so there is no `byok_requires_paid_plan` 403 (which earlier drafts proposed). Any tenant with credits can flip to BYOK; default-platform-managed and BYOK both run through the same `processEvent` and the same `compute_*_charge` functions. They differ in drain rate, not in eligibility. "Free" is not a tier — it's just "the user hasn't exhausted the $10 starter grant yet."

### §6 — CLI: `tenant provider {get|set|reset}` (`zombiectl/src/commands/tenant.js` + `tenant_provider.js` — NEW) — DONE

```bash
zombiectl tenant provider get
zombiectl tenant provider set --credential <name> [--model <override>]
zombiectl tenant provider reset
```

Subcommand structure: `tenant` is a parent group, `provider` is a subgroup. Wired via `zombiectl/src/program/routes.js`.

`set` requires `--credential <name>`. No default name — forces the user to see the link between "the credential I just stored" and "the credential my tenant uses."

Output:
- `set` prints `Tip: run a test event to verify the key works against <provider>.` after success, plus the resolved row contents (no api_key).
- `get` prints a table (mode, provider, model, context_cap_tokens, credential_ref or "—") + footer noting "this is the platform default" when the row is absent. Surfaces `⚠ Credential <name> is missing from vault` when resolver returned `CredentialMissing` (Scenario E).
- `reset` prints the new platform-default config and warns if `tenant_billing.balance_cents` is below a threshold.

### §7 — CLI: `zombiectl billing show` (`zombiectl/src/commands/billing.js` — NEW) — DONE

Read-only.

```bash
zombiectl billing show [--limit N] [--json]
```

Output (text mode):

```
Tenant balance:    $4.71 (471¢)

Last 10 events drained credits:
  EVENT_ID         POSTURE   MODEL                IN_TOK  OUT_TOK  RECEIVE  STAGE  TOTAL
  evt_01HXG2K4…    platform  accounts/fireworks/models/kimi-k2.6     820     1040       1¢     2¢     3¢
  evt_01HXG3M2…    byok      accounts/.../kimi-k2.6 800   1320       0¢     1¢     1¢
  …

ⓘ Out of credits? See https://app.usezombie.com/settings/billing
   Stripe purchase ships in v2.1; for now contact support for a top-up.
```

`--json` emits machine-readable output for scripting.

No `purchase` / `topup` / `configure` subcommands in v2.0. The CLI's job is to surface state, not to drive Stripe.

When the gate trips, every event-emitting CLI command (e.g. `zombiectl steer`) prints a one-line pointer at the dashboard billing page. The gate is server-side; the CLI surfaces the eventual rejection via `zombiectl events`.

### §8 — UI Settings → Provider (`ui/packages/app/app/(dashboard)/settings/provider/page.tsx` — NEW) — DONE

Page at `/settings/provider`:

- Current provider summary card: mode badge ("Platform" or "BYOK"), provider name, model, context cap, credential reference (if BYOK).
- Mode toggle: Platform | BYOK.
- BYOK form:
  - **Credential dropdown** — populated from the tenant's vault credentials list (M45 tenant-scoped credentials API). User picks the named credential. If the list is empty, render "Add a credential first" CTA linking to the credentials page.
  - **Model override field** — auto-filled from the picked credential's `model`. Editable.
  - On Save → PUT /v1/tenants/me/provider → success toast → revalidate.

New components: `ui/packages/app/src/routes/settings/provider.tsx`, `ui/packages/app/src/components/ProviderSelector.tsx`. Use design-system primitives per the UI Component Substitution Gate.

### §9 — UI Settings → Billing (`ui/packages/app/app/(dashboard)/settings/billing/page.tsx` — NEW) — DONE

Read-only Amp-style billing dashboard. Layout mirrors Amp Code's `/settings` Billing card.

**Balance card (top):**
- Headline: `$X.XX USD` (formatted from `tenant_billing.balance_cents`).
- Subtitle: *"Covers all your zombie events."*
- **Purchase Credits** button — **disabled in v2.0** with tooltip *"Coming in v2.1 — contact support for a top-up."* When disabled, click is a no-op; the button is rendered grey.

**Tabs (under the balance card):**

- **Usage** (default tab, shipped in v2.0). Per-event credit-drain history filterable by zombie / time range. Each row shows `event_id`, zombie name, timestamp, posture, model, in/out tokens (under platform; BYOK shows tokens for transparency), receive cents, stage cents, total cents. Sortable; CSV export.
- **Invoices** (shipped as empty state in v2.0). Renders *"No invoices yet — invoicing arrives with Purchase Credits in v2.1."*
- **Payment Method** (shipped as empty state in v2.0). Renders *"No payment method on file — coming in v2.1."*

**Auto Top Up card** — hidden in v2.0. Rendered in v2.1 alongside Stripe.

**Data sources (all already populated by the runtime):**
- `core.tenant_billing.balance_cents` for the balance headline.
- `core.zombie_execution_telemetry` (filtered by tenant_id, with `charge_type` discriminator and `posture` / `model` columns) for the Usage tab.

New components:
- `ui/packages/app/src/routes/settings/billing.tsx` — the page.
- `ui/packages/app/src/components/BillingBalanceCard.tsx` — balance display + disabled Purchase button.
- `ui/packages/app/src/components/BillingUsageTab.tsx` — drain history table with filters + CSV export.

Use design-system primitives per the UI Component Substitution Gate. No design-system primitive exists for the disabled-with-tooltip Purchase button pattern → compose from `Button` + `Tooltip` (no new primitive).

### §10 — Worker overlay integration (two-debit metering in `src/zombie/metering.zig`) — DONE

Per the worker overlay table above. The same edit that wires the two debit points (§3) also wires:
1. Read frontmatter `model` and `context_cap_tokens`.
2. Call `resolveActiveProvider(tenant_id)` (also needed for the cost-estimate step).
3. Apply per-field overlay rule.
4. Pass `{api_key, model, context_cap_tokens}` to `executor.startStage`.
5. Executor → NullClaw provider client uses `api_key` for the outbound request and never logs / echoes it.

The legacy `resolveFirstCredential` (deprecated for tool-level secrets in M45) is unused for provider resolution — `resolveActiveProvider` is the only path.

### §11 — Model-caps endpoint extension (token-rate columns) — DONE

The endpoint at `https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json` is extended to carry per-model token rates alongside the existing `context_cap_tokens` column.

```
200 {
  "version": "2026-05-01",
  "models": [
    { "id": "claude-opus-4-7",
      "context_cap_tokens": 1000000,
      "input_cents_per_mtok":  1500,
      "output_cents_per_mtok": 7500 },
    { "id": "claude-sonnet-4-6",
      "context_cap_tokens": 256000,
      "input_cents_per_mtok":  300,
      "output_cents_per_mtok": 1500 },
    { "id": "claude-haiku-4-5-20251001",
      "context_cap_tokens": 256000,
      "input_cents_per_mtok":  100,
      "output_cents_per_mtok":  500 },
    { "id": "accounts/fireworks/models/kimi-k2.6",
      "context_cap_tokens": 256000,
      "input_cents_per_mtok":  150,
      "output_cents_per_mtok":  600 },
    ...
  ]
}
```

Two changes in this repo:
1. **The static JSON file** served by the `_um/<key>/model-caps.json` route handler gets two new columns per row.
2. **The API server** reads this file (or the cached fetched response) at boot to populate the `lookup_model_rate` cache used by `compute_stage_charge`. Refresh timer: every hour.

The endpoint stays public-but-unguessable. **Pricing visibility caveat:** the per-model rates are now in the public-but-unguessable response. Anyone who finds the URL can read platform margins. Acknowledged-controversial trade-off: the alternative (auth-required pricing endpoint) breaks the "hot, unauthenticated, cacheable" property that lets `tenant provider set` resolve at low latency without a tenant token. We accept the trade-off and revisit if a competitor uses the data strategically.

### §12 — Workspace `/credentials/llm` route removal — DONE

Pre-v2.0.0 cleanup. The `PUT|GET|DELETE /v1/workspaces/{ws}/credentials/llm` route exists in `main` but has zero runtime consumers (verified by grep across `src/zombie/`, `src/executor/`, `src/state/` — only the vault module references it, in comments). It was a write surface that was never wired to a resolver. RULE NLG forbids leaving it in place pre-v2.0.0.

Removed in this PR:
- `src/http/route_matchers.zig` — `matchWorkspaceLlmCredential` and the `workspaces/credentials/llm` matcher branch.
- `src/http/router.zig` — `workspace_llm_credential` route variant.
- `src/http/route_table.zig` — corresponding spec entry.
- `src/http/route_manifest.zig` — three manifest entries (GET/PUT/DELETE).
- `src/http/route_table_invoke.zig` — `invokeWorkspaceLlmCredential` import + dispatch.
- `src/http/handlers/workspaces/credentials.zig` — BYOK-specific handler functions (generic credential CRUD stays).
- `src/http/byok_http_integration_test.zig` — wholesale delete (tests an obsolete route).
- `src/http/credentials_json_integration_test.zig` — drop the two routing-distinction tests.
- `src/http/route_matchers_test.zig`, `src/http/router_test.zig` — drop the `M24_001:` test cases naming the workspace LLM route.

Migration cleanup: a one-line `DELETE FROM core.vault WHERE name='llm' AND tenant_id IS NULL` in M48's migration to clean up any orphaned workspace-scoped rows. Pre-v2.0.0 the DB is wiped on rebuild anyway; the DELETE is belt-and-braces.

Comment scrubs: `src/state/vault.zig:10` and `src/zombie/credential_key.zig:9` reference "BYOK provider record (`llm`)" — update to "BYOK provider records (user-named)".

### §13 — Provider catalog (`samples/fixtures/m48-provider-fixtures.json` — NEW) — DONE

```json
{
  "anthropic":  {"default_model": "claude-sonnet-4-6",                          "api_base": "https://api.anthropic.com"},
  "openai":     {"default_model": "gpt-5",                                      "api_base": "https://api.openai.com"},
  "fireworks":  {"default_model": "accounts/fireworks/models/kimi-k2.6",        "api_base": "https://api.fireworks.ai/inference/v1"},
  "moonshot":   {"default_model": "kimi-k2.6",                                  "api_base": "https://api.moonshot.cn/v1"},
  "zhipu":      {"default_model": "glm-5.1",                                    "api_base": "https://open.bigmodel.cn/api/paas/v4"},
  "together":   {"default_model": "deepseek-coder",                             "api_base": "https://api.together.xyz"},
  "openrouter": {"default_model": "anthropic/claude-sonnet-4-6",          "api_base": "https://openrouter.ai/api/v1"}
}
```

UI uses this for the provider-name display in BYOK mode (the user's `provider` field is matched against this catalogue for a friendly name; unknown providers display the raw string). Adding a new provider = appending here + ensuring NullClaw routes to its endpoint.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/0NN_tenant_providers.sql` | NEW | Schema migration (provider state) |
| `schema/zombie_execution_telemetry.sql` | EDIT | UNIQUE on `(event_id, charge_type)`; add `charge_type` and `posture` columns |
| `schema/embed.zig` | EXTEND | Register schema |
| `src/cmd/common.zig` | EXTEND | Migration array |
| `src/state/tenant_provider.zig` | NEW | Resolver + CRUD |
| `src/state/tenant_billing.zig` | EDIT | `compute_*_charge` + `insert_starter_grant` + helper deduct/insert/update. Imports `lookup_model_rate` from the new `model_rate_cache.zig` |
| `src/http/handlers/tenants/provider.zig` | NEW | GET/PUT/DELETE `/v1/tenants/me/provider` |
| `src/http/handlers/tenants/create.zig` (or wherever M11_005 landed) | EDIT | One line: call `tenant_billing.insert_starter_grant` in the tenant-create transaction |
| `src/http/handlers/doctor.zig` | EDIT | Add `tenant_provider` block |
| `src/http/handlers/tenants/billing.zig` (likely existing from M11_005) | EDIT | Add usage-history endpoint reading `zombie_execution_telemetry` for the dashboard Usage tab |
| `src/http/route_matchers.zig` | EDIT | Add tenant-provider matcher; **remove** `matchWorkspaceLlmCredential` |
| `src/http/router.zig` | EDIT | Add `tenant_provider` variant; **remove** `workspace_llm_credential` |
| `src/http/route_table.zig` | EDIT | Add tenant-provider spec; **remove** workspace LLM spec |
| `src/http/route_manifest.zig` | EDIT | Add 3 tenant routes; **remove** 3 workspace LLM routes |
| `src/http/route_table_invoke.zig` | EDIT | Add `invokeTenantProvider`; **remove** `invokeWorkspaceLlmCredential` |
| `src/http/handlers/workspaces/credentials.zig` | EDIT | **Remove** BYOK-specific functions |
| `src/zombie/event_loop_helpers.zig` | EDIT | `resolveActiveProvider` + sentinel overlay + two debit points |
| `src/state/vault.zig`, `src/zombie/credential_key.zig` | EDIT | Comment scrub (user-named, not "llm") |
| `src/http/handlers/_um/model_caps.zig` | EDIT | Extend response with `input_cents_per_mtok` / `output_cents_per_mtok` |
| `src/state/model_rate_cache.zig` | NEW | Process-local cache populated at API boot from the model-caps endpoint |
| `samples/fixtures/m48-model-caps.json` | NEW (or EDIT existing) | Static JSON backing the endpoint, with token-rate columns |
| `zombiectl/src/commands/tenant.js` | NEW | `tenant` parent group |
| `zombiectl/src/commands/provider.js` | NEW | `tenant provider {get,set,reset}` |
| `zombiectl/src/commands/billing.js` | NEW | `zombiectl billing show` |
| `zombiectl/src/program/routes.js` | EDIT | Wire `tenant provider` and `billing` routes |
| `ui/packages/app/src/routes/settings/provider.tsx` | NEW | Settings → Provider page |
| `ui/packages/app/src/routes/settings/billing.tsx` | NEW | Settings → Billing page (Amp-style read-only) |
| `ui/packages/app/src/components/ProviderSelector.tsx` | NEW | Mode + credential + model selector |
| `ui/packages/app/src/components/BillingBalanceCard.tsx` | NEW | Balance display + disabled Purchase button |
| `ui/packages/app/src/components/BillingUsageTab.tsx` | NEW | Drain history table with filters + CSV export |
| `tests/integration/byok_provider_test.zig` | NEW | E2E provider state machine |
| `tests/integration/billing_test.zig` | NEW | E2E credit-pool billing + two debits |
| `samples/fixtures/m48-provider-fixtures.json` | NEW | Provider catalog |
| `src/http/byok_http_integration_test.zig` | DELETE | Tests obsolete workspace route |
| `src/http/credentials_json_integration_test.zig` | EDIT | Drop routing-distinction tests for `/credentials/llm` |
| `src/http/route_matchers_test.zig`, `src/http/router_test.zig` | EDIT | Drop workspace LLM route test cases |

---

## Interfaces

```
HTTP — provider:
  GET /v1/tenants/me/provider
    → 200 {
        mode: "platform" | "byok",
        provider: string,
        model: string,
        context_cap_tokens: u32,
        credential_ref: string | null
      }

  PUT /v1/tenants/me/provider
    body: { mode: "platform" | "byok", credential_ref?: string, model?: string }
    → 200 (echoes resolved config; no api_key)
    → 400 missing_or_malformed_body
    → 400 credential_ref_required          (mode=byok without credential_ref)
    → 400 credential_not_found             (vault row missing)
    → 400 credential_data_malformed        (vault JSON lacks provider/api_key/model)
    → 400 model_not_in_caps_catalogue      (model not in cached catalogue)
    → 403 (caller is not a tenant admin)

  DELETE /v1/tenants/me/provider
    → 200 (equivalent to PUT mode=platform; UPSERTs explicit platform row)

HTTP — billing (read-only):
  GET /v1/tenants/me/billing/balance
    → 200 { balance_cents: u32 }

  GET /v1/tenants/me/billing/charges?limit=
    → 200 { items: [{ id, tenant_id, workspace_id, zombie_id, event_id,
                      charge_type, posture, model,
                      credit_deducted_cents, token_count_input,
                      token_count_output, wall_ms, recorded_at }, ...] }
    Note: REST §1 forbids `/usage` as a final segment (not a plural noun);
    the resource is `/charges`. Each event yields up to two rows
    (`charge_type=receive` then `charge_type=stage`); UI groups by event_id.

Doctor extension (surface owned by M44, field owned by M48):
  zombiectl doctor --json
    → { ..., tenant_provider: {
            mode, provider, model, context_cap_tokens,
            credential_ref?: string | null,
            error?: "credential_missing" | "credential_data_malformed"
          } }
    The api_key is NEVER in this block.

CLI:
  zombiectl tenant provider get
  zombiectl tenant provider set --credential <name> [--model <override>]
  zombiectl tenant provider reset
  zombiectl billing show [--limit N] [--json]

  zombiectl credential set <name> --data @-         # M45 surface; JSON on stdin
  zombiectl credential delete <name>                # M45 surface

Internal:
  tenant_provider.resolveActiveProvider(allocator, conn, tenant_id)
    → !ResolvedProvider {
        mode:               .platform | .byok,
        provider:           []const u8,
        api_key:            []const u8,   // process-internal only
        model:              []const u8,
        context_cap_tokens: u32,
      }
    Errors: error.CredentialMissing, error.CredentialDataMalformed.

  tenant_billing.compute_receive_charge(posture: Posture) → u32
  tenant_billing.compute_stage_charge(posture: Posture, model, in_tok, out_tok) → u32
  tenant_billing.insert_starter_grant(conn, tenant_id) → !void
  tenant_billing.deduct(conn, tenant_id, cents) → !void
  tenant_billing.insert_telemetry_row(conn, event_id, posture, model,
                                       charge_type, cents) → !void
  tenant_billing.update_telemetry_stage_row(conn, event_id, in_tok, out_tok,
                                             wall_ms) → !void
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| BYOK set but credential row missing at PUT time | User forgot `credential set` first | 400 `credential_not_found`; row not written |
| BYOK credential JSON lacks `provider`/`api_key`/`model` at PUT time | User built malformed JSON | 400 `credential_data_malformed`; row not written |
| BYOK `model` not in caps catalogue at PUT time | Typo, or model not yet in catalogue | 400 `model_not_in_caps_catalogue`; row not written |
| BYOK API key invalid (rejected by provider) | User pasted wrong key | First event fails with `provider_auth_failed` in event log; provider config remains set so user can fix the credential |
| Balance below estimate | User out of credits | Next event blocks at gate with `failure_label='balance_exhausted'`; CLI / dashboard surface the empty-balance UX |
| BYOK → platform with low balance | User flips back to platform; platform-rate exceeds remaining balance | PUT succeeds; next event blocks at balance gate |
| BYOK credential deleted while `mode=byok` | User removed credential without flipping mode | Next event's `resolveActiveProvider` returns `CredentialMissing`; event dead-lettered with `provider_credential_missing`; **no deduction taken**. Mode does NOT auto-revert to platform (would silently re-enable platform billing without consent) |
| Concurrent PUTs from two admin sessions | Race | Last write wins; both return 200; revalidate shows the winner |
| `tenant provider set --credential` referencing a vault row that was just deleted | Race between credential delete and provider set | 400 `credential_not_found` |
| In-flight event when posture flips | Event was claimed under old posture | Event finishes under the snapshot taken at gate time (Invariant 4) |
| Concurrent events on near-zero balance | Race | Both events pass the gate; both deduct; balance briefly negative; next event sees negative balance and gate trips |

---

## Invariants

1. **api_key visibility boundary.** The api_key (platform OR BYOK) exists only in: vault rows, server-side process memory (resolver / executor session), and outbound HTTPS request headers. It MUST NOT appear in HTTP responses, logs, agent tool context, persisted event/telemetry rows, or any user-facing artefact.
2. **One active provider per tenant.** `core.tenant_providers.tenant_id` is the primary key. UPSERT semantics; no concurrent rows.
3. **Absence of row = synthesised platform default.** No eager row insertion at tenant creation. The resolver treats "no row" and `mode=platform` row identically for runtime behaviour, but `tenant provider reset` writes an explicit `mode=platform` row so the dashboard can distinguish "never configured" from "explicitly reset."
4. **Posture snapshot at gate time.** `resolveActiveProvider` runs once per event, before the receive deduct. Both deductions and the outbound LLM call use that snapshot. Posture flips during the event have no effect on the in-flight event.
5. **Two telemetry rows per event.** `UNIQUE (event_id, charge_type)` with `charge_type IN ('receive', 'stage')`. The receive row is INSERTed at gate-pass; the stage row is INSERTed before `startStage` and UPDATEd post-execution with token counts.
6. **Worker overlay is sentinel-or-absent.** Frontmatter `model: ""` OR `model:` absent ⇒ overlay from `tenant_providers.model`. Same rule for `context_cap_tokens: 0` OR absent. Per-field, not all-or-nothing.
7. **`credential_ref` is user-named.** No hardcoded name. v1 invariant: `mode=platform` ⇒ `credential_ref IS NULL`; `mode=byok` ⇒ `credential_ref` non-empty AND vault row exists.
8. **Eager structural validation, lazy auth validation.** `tenant provider set` PUT validates body shape, credential presence, JSON shape, and model-caps catalogue membership synchronously. PUT does NOT make a synthetic call to the LLM provider; key validity surfaces at first event.
9. **Workspace-scoped `/credentials/llm` is removed.** Pre-v2.0.0 cleanup; no compat shim.
10. **Starter grant is one-time.** Inserted once at tenant create as $10 (1000¢). No replenishing. Top-ups are manual (v2.0) or via Stripe (v2.1+).
11. **No plan-tier branching in `compute_*_charge`.** The functions take `(posture, model, tokens)`. Plans (Free / Team / Scale) only show up at credit-grant time as different starting numbers; never inside the cost function.
12. **Receive deduct precedes stage deduct.** If receive succeeds and the worker crashes before stage, the receive row is durable. On retry the gate re-runs against the now-lower balance.
13. **Step-2 dead-letter does NOT deduct.** Provider-credential-missing is detected before posture is known; charging without knowing posture is incoherent.

---

## Test Specification

Every test maps back to a scenario or invariant.

| Test | Scenario / Invariant | Asserts |
|------|----------------------|---------|
| `test_starter_grant_inserted_at_tenant_create` | A, Inv 10 | New tenant row → `tenant_billing.balance_cents = 1000` synchronously in the same transaction |
| `test_default_provider_synthesised_when_no_row` | A | New tenant with no `tenant_providers` row → resolver returns synthesised platform default; doctor block reports it |
| `test_explicit_platform_row_matches_synth_default` | Inv 3 | Tenant after `tenant provider reset` has explicit `mode=platform` row; resolver output is byte-identical to synthesised default |
| `test_byok_set_writes_row_and_resolves_cap` | B | PUT mode=byok with valid credential → row written with mode/provider/model/credential_ref/context_cap_tokens from caps endpoint |
| `test_byok_set_routes_to_user_provider` | B | Set BYOK fireworks → run zombie → outbound LLM call hits `api.fireworks.ai/inference/v1` with user's api_key |
| `test_byok_model_switch_rewrites_row` | C | Re-run `tenant provider set` with new model → row's `model` and `context_cap_tokens` updated; `credential_ref` unchanged |
| `test_byok_reset_to_platform` | D | `tenant provider reset` → `mode=platform`, `credential_ref=NULL`; next event uses platform-managed Fireworks |
| `test_credential_delete_in_byok_dead_letters_event` | E, Inv 13 | Delete vault row while `mode=byok` → next event dead-lettered with `provider_credential_missing`; **no deduction taken**; `tenant_providers` row unchanged |
| `test_byok_with_missing_credential_ref` | Validation | PUT mode=byok without credential_ref → 400 `credential_ref_required` |
| `test_byok_with_unknown_credential_name` | Validation | PUT mode=byok referencing a non-existent vault row → 400 `credential_not_found` |
| `test_byok_with_malformed_credential_data` | Validation | Vault JSON missing provider/api_key/model → 400 `credential_data_malformed` |
| `test_byok_with_unknown_model_400s` | Validation, Inv 8 | Model not in caps catalogue → 400 `model_not_in_caps_catalogue`; row not written |
| `test_byok_invalid_key_surfaces_at_first_event` | Inv 8 | `tenant provider set` with bad key succeeds (lazy auth); first event fails with `provider_auth_failed` |
| `test_compute_receive_charge_platform` | Inv 11 | `compute_receive_charge(.platform)` returns `RECEIVE_PLATFORM_CENTS` |
| `test_compute_receive_charge_byok` | Inv 11 | `compute_receive_charge(.byok)` returns `RECEIVE_BYOK_CENTS` (0¢ in v2.0) |
| `test_compute_stage_charge_platform_token_based` | Inv 11 | `compute_stage_charge(.platform, sonnet, 800, 1000)` returns overhead + token math |
| `test_compute_stage_charge_byok_flat` | Inv 11 | `compute_stage_charge(.byok, anything, anything, anything)` returns `STAGE_OVERHEAD_BYOK_CENTS` regardless of token count |
| `test_compute_stage_charge_unknown_model_panics` | Inv 11 | `compute_stage_charge(.platform, "unknown-model", …)` panics in dev / errors in prod |
| `test_processEvent_two_telemetry_rows_per_event` | Inv 5 | One event → two `zombie_execution_telemetry` rows: charge_type='receive' and charge_type='stage' |
| `test_processEvent_receive_row_durable_on_worker_crash` | Inv 12 | Crash worker between receive deduct and stage deduct → receive row exists, stage row absent; on retry gate runs against post-receive balance |
| `test_processEvent_dead_letter_no_deduction` | Inv 13 | Event dead-lettered at step 2 (CredentialMissing) → no telemetry rows, no balance change |
| `test_balance_gate_blocks_at_zero` | F | Tenant with balance=0 → next event blocks at gate; status='gate_blocked', failure_label='balance_exhausted' |
| `test_balance_gate_runs_under_platform_mode` | F | Platform tenant with balance below platform-rate estimate → blocked |
| `test_balance_gate_runs_under_byok_mode` | F | BYOK tenant with balance below BYOK-rate estimate (1¢) → blocked |
| `test_concurrent_events_overshoot` | Inv 12 | Two concurrent events claim with balance=2¢; both pass gate; both deduct; balance briefly negative; next event blocks |
| `test_in_flight_event_keeps_gate_time_posture` | Inv 4 | Claim event under BYOK; flip to platform mid-event; event finishes under BYOK |
| `test_doctor_block_under_platform_mode` | A | Doctor reports mode=platform, platform default model+cap, no api_key field |
| `test_doctor_block_under_byok_mode` | B | Doctor reports mode=byok, user's provider/model/cap, no api_key field |
| `test_doctor_block_under_byok_with_missing_credential` | E | Doctor reports mode=byok + error: credential_missing |
| `test_api_key_no_leak_into_http_responses` | Inv 1 | Audit grep on every PUT/GET/doctor/billing response for the api_key bytes → 0 matches |
| `test_api_key_no_leak_into_event_log` | Inv 1 | Run zombie under BYOK → grep `core.zombie_events` for api_key bytes → 0 matches |
| `test_api_key_no_leak_into_telemetry` | Inv 1 | Grep `core.zombie_execution_telemetry` for api_key bytes → 0 matches |
| `test_api_key_no_leak_into_tool_context` | Inv 1 | Run zombie under BYOK → capture agent's tool-call records → 0 matches |
| `test_api_key_no_leak_into_logs` | Inv 1 | Grep worker + executor logs → 0 matches |
| `test_concurrent_put_last_write_wins` | Inv 2 | Two PUTs racing → final config = whichever committed last; both return 200 |
| `test_tenant_admin_required` | API auth | Non-admin PUT → 403 |
| `test_workspace_credentials_llm_route_404s` | Inv 9 | `PUT /v1/workspaces/{ws}/credentials/llm` → 404 |
| `test_billing_show_cli_renders_balance_and_history` | UI | `zombiectl billing show` text output matches the documented format |
| `test_billing_dashboard_balance_card_renders` | UI | `/settings/billing` renders Balance card with disabled Purchase button + tooltip |
| `test_billing_dashboard_usage_tab_two_rows_per_event` | UI, Inv 5 | Usage tab fetches `/v1/tenants/me/billing/charges` and renders one row per (event_id, charge_type) |
| `test_model_caps_endpoint_includes_token_rates` | §11 | `GET /_um/<key>/model-caps.json` response includes `input_cents_per_mtok` and `output_cents_per_mtok` per model |
| `test_model_rate_cache_populated_at_boot` | §11 | API server boot calls the caps endpoint and populates `lookup_model_rate` |

---

## Acceptance Criteria

- [ ] `make test-integration` passes the suite above.
- [ ] `make test` passes (unit-level).
- [ ] `make lint` clean.
- [ ] `make memleak` clean for any handler / executor changes.
- [ ] `make check-pg-drain` clean.
- [ ] Cross-compile clean: `x86_64-linux` + `aarch64-linux`.
- [ ] **Manual A:** new tenant on default platform → doctor reports synthesised default → first event drains 3¢ (1¢ receive + 2¢ stage); two telemetry rows visible in Usage tab.
- [ ] **Manual B:** John runs `credential set` + `tenant provider set --credential …` → outbound LLM call observed (NullClaw debug logging) hitting Fireworks; balance drains at 1¢/event (0¢ receive + 1¢ stage).
- [ ] **Manual C:** Switch from Kimi K2 to DeepSeek V4 Pro → `tenant_providers.context_cap_tokens` updated; in-flight event finishes on K2; next event runs on V4.
- [ ] **Manual D:** `tenant provider reset` → next event uses platform-managed Fireworks; if balance too low, gate trips with credit-exhausted UX.
- [ ] **Manual E:** Delete BYOK credential while in BYOK mode → next event dead-lettered; no deduction taken; system does NOT auto-revert.
- [ ] **Manual F:** Drain balance to 0¢ → next event blocks; CLI prints `See https://app.usezombie.com/settings/billing`; dashboard shows empty-balance state with disabled Purchase button.
- [ ] **Audit grep:** BYOK api_key bytes never appear in `core.zombie_events`, `core.zombie_execution_telemetry`, `core.tenant_providers`, worker logs, executor logs, doctor JSON, or any HTTP response body across the full test run.
- [ ] **Workspace route removal:** `PUT /v1/workspaces/{ws}/credentials/llm` returns 404; `git grep "workspace_llm_credential"` returns 0 hits in non-historical files.
- [ ] **Architecture cross-reference:** [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) and [`docs/architecture/scenarios/03_balance_gate.md`](../../architecture/scenarios/03_balance_gate.md) reflect the credit-pool model and the M48 contract (already landed in PR #TBD).
- [ ] **Model-caps endpoint:** the public response carries `input_cents_per_mtok` / `output_cents_per_mtok` per model. The API server populates `lookup_model_rate` cache at boot.

---

## Out of Scope

- **Stripe Purchase Credits.** v2.1. The dashboard renders the disabled button + tooltip; the CLI has no purchase subcommand.
- **Auto Top Up.** v2.1, alongside Stripe.
- **Plan tiers as recurring credit grants.** v2.1+ if onboarding metrics suggest the $10 starter is the wrong knob. Plan tiers ship as Stripe charges that top up `balance_cents` — never as branches in `compute_charge`.
- **Refund-on-actual-tokens.** v3. Today the conservative estimate at stage-debit time is the charge.
- **Per-workspace provider override.** Tenant-scoped only in v2.0.
- **Per-zombie provider override.** Frontmatter cannot pin an api_key.
- **Auto-fallback BYOK → platform on provider error.** Errors surface to the user; no silent fallback.
- **BYOK metering for user-side cost reporting.** User checks their provider's dashboard.
- **Custom OIDC / external vault-backend integrations.** Treat M45's vault as the source of truth.
- **Synthetic test call to LLM provider on `tenant provider set`.** Lazy auth validation.
- **Multi-active providers per tenant.** Multi-credential vault storage is supported; only one is active at a time per tenant.
- **The model-caps endpoint admin tooling and the admin-zombie that maintains it.** Owned by [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) §10 and platform infra; this spec consumes the endpoint and adds the token-rate columns to its schema.
- **Per-workspace soft caps inside a tenant.** v3 — needs a new gate at the workspace level.
