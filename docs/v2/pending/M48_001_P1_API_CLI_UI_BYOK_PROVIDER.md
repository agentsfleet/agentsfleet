# M48_001: BYOK Provider — Tenant-Scoped LLM Provider Configuration

**Prototype:** v2.0.0
**Milestone:** M48
**Workstream:** 001
**Date:** May 01, 2026
**Status:** PENDING
**Priority:** P1 — launch-blocking (substrate-tier, Week 2-3). BYOK is the second of three v2 differentiation pillars (OSS + BYOK + markdown-defined; self-host deferred to v3). Without operator-controlled LLM provider config, the launch tweet's BYOK claim is hollow and the differentiation argument collapses to two pillars, both matchable by competitors within a week.
**Categories:** API, CLI, UI, SCHEMA
**Batch:** B1 — substrate-tier alongside M40-M45.
**Branch:** feat/m48-byok-provider (to be created)
**Depends on:**
- **M11_005** (tenant billing — DONE) — provides `core.tenant_billing.balance_cents` and the plan gate (Free vs Team/Scale).
- **M44_001** (install contract + doctor) — owns the `zombiectl doctor --json` surface that this spec extends with a `tenant_provider` block.
- **M45_001** (vault structured credentials) — opaque JSON-object credentials keyed by name; this spec stores the BYOK record there.

**Canonical architecture:**
- [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) — billing model, posture transitions, model-caps endpoint, api_key visibility boundary.
- [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.7 — three-rail diagram (platform vs BYOK origin) and worker sentinel overlay.
- [`docs/architecture/scenarios/01_default_free_tier.md`](../../architecture/scenarios/01_default_free_tier.md) and [`02_byok.md`](../../architecture/scenarios/02_byok.md) — the end-to-end scenarios this spec implements.

---

## Tier promotion provenance (Apr 25, 2026 /plan-ceo-review)

CEO scope review on Apr 25, 2026 promoted M48 from "soft-blocks BYOK launch claim, post-launch" to "Launch-blocking substrate, Week 2-3." Self-host was deferred to v3 in the same session, compressing the v2 differentiation pillars from four to three (**OSS + BYOK + markdown-defined**). Dropping BYOK leaves only two pillars, both matchable by competitors (AWS DevOps Agent / Sonarly / incident.io) within a week. Adds ~1 week to the milestone (5-6 → 6-7 weeks).

| Tier | Specs |
|---|---|
| **Launch-blocking substrate (Week 1-3)** | M40 Worker, M41 Context Layering, M42 Streaming, M43 Webhook Ingest, M44 Install Contract + Doctor, M45 Vault Structured, **M48 BYOK Provider** |
| **Launch-shipping packaging (Week 4-7)** | M46 Frontmatter Schema, M49 Install-Skill, M51 Docs + Install-Pingback |
| **Parallel from Week 1, validation-blocking** | M50 Customer-Development Parallel Workstream |
| **Post-launch** | M47 Approval Inbox |

---

## Overview

**Problem.** A tenant on the Free plan gets 20 platform-managed events. When they exhaust their balance, today their only options are "upgrade the platform plan" or "stop." That's not a real product story for an operator who already has an Anthropic / Fireworks / OpenAI / OpenRouter account and wants UseZombie as the orchestration layer, not the LLM bill payer. Without BYOK, the differentiation against AWS DevOps Agent (Bedrock-locked) and Sonarly (Claude-Code-managed) is hollow.

**Goal (testable).** A Team- or Scale-plan tenant admin can:

1. Store a credential carrying their LLM provider config in the vault under a name they choose (e.g. `account-fireworks-byok`):
   ```bash
   zombiectl credential set account-fireworks-byok --data '{
     "provider": "fireworks",
     "api_key": "fw_LIVE_…",
     "model":   "accounts/fireworks/models/kimi-k2.6"
   }'
   ```
2. Activate it as the tenant's provider:
   ```bash
   zombiectl tenant provider set --credential account-fireworks-byok
   ```
3. Or do the same in the dashboard at Settings → Provider, or via `PUT /v1/tenants/me/provider`.

After activation, every zombie under every workspace under that tenant routes its LLM calls to the operator's provider account, billed to the operator. UseZombie still meters orchestration (an `orchestration-only` per-event credit). Reverting via `zombiectl tenant provider reset` flips back to platform-managed Anthropic, billed at the platform-bundled rate.

**Solution shape.** Provider configuration is **tenant-scoped**, not workspace-scoped. One active provider per tenant at a time. The tenant's row in `core.tenant_providers` carries `mode` (`platform` | `byok`), the resolved `provider`/`model`/`context_cap_tokens`, and a pointer (`credential_ref`) to the operator-named vault row that carries the actual `api_key`. The api_key never leaves the resolver-to-executor path: it's not in the doctor block, not in HTTP responses, not in the agent tool context, not in event logs.

---

## End-to-end user scenarios

These are the scenarios the implementation must satisfy. Every test in the Test Specification maps back to one of them.

### Scenario A — Free-plan tenant, default platform-managed

1. Lakshmi signs up. Tenant created. Plan = Free. `core.tenant_providers` has **no row** for her tenant.
2. She runs `/usezombie-install-platform-ops` (M49). The skill calls `zombiectl doctor --json`.
3. Doctor's `tenant_provider` block reports the synthesized platform default:
   ```json
   { "mode": "platform",
     "provider": "anthropic",
     "model": "claude-sonnet-4-6",
     "context_cap_tokens": 200000 }
   ```
4. Skill writes resolved values into frontmatter (`model: claude-sonnet-4-6`, `context_cap_tokens: 200000`).
5. First event: balance gate runs at platform-bundled rate against `tenant_billing.balance_cents`. Passes. Worker calls `resolveActiveProvider(tenant_id)` → synthesizes the platform default again (still no row), substitutes the platform-managed Anthropic api_key (server-side config), makes the outbound call to `api.anthropic.com`.
6. After 20 events her balance hits zero. Next event blocks at the gate with the credit-exhausted UX. She sees "upgrade your plan or bring your own key."

### Scenario B — Paid-plan operator brings their own key

1. Priya is on the Team plan. She has an existing Fireworks account.
2. She runs:
   ```bash
   zombiectl credential set account-fireworks-byok --data '{
     "provider": "fireworks",
     "api_key": "fw_LIVE_…",
     "model":   "accounts/fireworks/models/kimi-k2.6"
   }'
   ```
   This writes a row to `core.vault` at `(tenant_id, "account-fireworks-byok")` with the JSON as opaque plaintext (M45 contract).
3. She runs `zombiectl tenant provider set --credential account-fireworks-byok`. The CLI:
   - Loads the vault row, validates `provider`/`api_key`/`model` are present (eager structural validation).
   - GETs `https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json?model=accounts%2Ffireworks%2Fmodels%2Fkimi-k2.6` → returns `context_cap_tokens: 256000`. (If the model isn't in the catalogue: `400 model_not_in_caps_catalogue` and the row is NOT written.)
   - UPSERTs `core.tenant_providers`:
     ```
     tenant_id          = <priya>
     mode               = byok
     provider           = fireworks
     model              = accounts/fireworks/models/kimi-k2.6
     context_cap_tokens = 256000
     credential_ref     = account-fireworks-byok
     ```
   - Prints "Tip: run a test event to verify the key works against fireworks."
4. The next event: balance gate runs at orchestration-only rate (smaller). Worker's `resolveActiveProvider` returns `{mode: byok, provider: fireworks, api_key: fw_LIVE_…, model: kimi-k2.6, context_cap_tokens: 256000}`. Outbound call hits `api.fireworks.ai/inference/v1`, billed to Priya's Fireworks account.

### Scenario C — Operator switches BYOK model

1. A month later Priya wants to try DeepSeek V4 Pro on the same Fireworks account.
2. She updates the credential:
   ```bash
   zombiectl credential set account-fireworks-byok --data '{
     "provider": "fireworks",
     "api_key": "fw_LIVE_…",
     "model":   "accounts/fireworks/models/deepseek-v4-pro"
   }'
   ```
3. She re-runs `zombiectl tenant provider set --credential account-fireworks-byok`. The CLI re-resolves the cap (e.g. `131072`), rewrites the `tenant_providers` row's `model` and `context_cap_tokens` columns. `credential_ref` stays the same.
4. In-flight events that were already claimed under Kimi K2 finish under Kimi K2 (worker snapshots posture at claim time, Invariant 4). Next event uses DeepSeek V4 Pro.
5. Her existing `.usezombie/platform-ops/SKILL.md` does not need regeneration — its `model: ""` and `context_cap_tokens: 0` sentinels keep working, the worker just overlays the new values.

### Scenario D — Operator reverts to platform

1. Priya's Fireworks account hits a billing issue. She runs `zombiectl tenant provider reset`.
2. CLI updates the `tenant_providers` row to `mode = platform`, sets `credential_ref = NULL`, sets `provider = anthropic`, `model = claude-sonnet-4-6`, re-resolves cap from the caps endpoint for the platform default → `context_cap_tokens = 200000`.
3. In-flight events finish under BYOK. Next event runs against platform-managed Anthropic at the platform-bundled rate.
4. If `tenant_billing.balance_cents` is too low for platform pricing, the next event blocks at the balance gate with a clear "balance exhausted" error. The flip itself succeeds regardless of balance — operator chose this posture explicitly.

### Scenario E — Operator deletes BYOK credential while still in BYOK mode

1. Priya runs `zombiectl credential delete account-fireworks-byok` without first running `tenant provider reset`. Vault row is gone; `tenant_providers` still says `mode=byok`, `credential_ref=account-fireworks-byok`.
2. Next event: worker's `resolveActiveProvider` finds the row but the vault load returns NULL. Resolver returns `error.CredentialMissing`.
3. Event is dead-lettered with `provider_credential_missing`. The system does NOT auto-revert to platform — that would silently re-enable platform billing without consent.
4. Priya sees the error in her event log. She must either re-add the credential under the same name OR run `tenant provider reset` to opt back into platform billing explicitly.

### Scenario F — Free-plan tenant tries BYOK

1. Lakshmi (Free plan) tries `zombiectl tenant provider set --credential my-byok`.
2. PUT /v1/tenants/me/provider with `mode=byok` returns `403 byok_requires_paid_plan`. No row written.
3. UI shows the same message with an "Upgrade" CTA.

---

## Architecture

### Tenant-provider state — `core.tenant_providers`

One row per tenant who has explicitly configured a provider. **Absence of row = synthesized platform default.** The resolver treats "no row" and "row with `mode=platform`" as semantically identical; the row exists only when an operator has explicitly touched provider config. We do NOT eagerly insert default rows at tenant creation.

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
    -- the doctor block and the balance-gate cost function read this without
    -- re-parsing vault JSON on every event.

    model              TEXT         NOT NULL,
    -- Authoritative model name. Under mode=platform: the platform default
    -- ("claude-sonnet-4-6" at v2.0). Under mode=byok: the model from the
    -- referenced credential (CLI --model flag overrides at set time).

    context_cap_tokens INTEGER      NOT NULL,
    -- Resolved at write time by GETting the model-caps endpoint with `model`.
    -- Re-resolved on every `tenant provider set` so cap stays in sync with
    -- the model. Worker reads this when frontmatter carries a sentinel.

    credential_ref     TEXT,
    -- NULL when mode=platform.
    -- Operator-chosen credential name when mode=byok (e.g.
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

`updated_at` is bumped via the standard trigger pattern used elsewhere in `core.*`. There is no soft-delete column; an audit trail (if ever needed) goes in a separate table out of scope here.

**Why operator-named `credential_ref` (not hardcoded `"llm"`).** The Apr 30 review of M43 had pinned a "convention name = `llm` for BYOK" rule by analogy to webhook credentials (`name = trigger.source`). For BYOK the analogy doesn't hold — there is no trigger.source. Operator-naming gives the schema honest semantics, unlocks multi-credential tenants from day 1 (operator can store `anthropic-prod` AND `fireworks-staging` in vault, flip between them with `tenant provider set --credential <name>`), and avoids namespace collision with M43's `github`-style workspace-scoped convention.

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
  if row IS NULL:
    return synthesizePlatformDefault()
      // { mode: "platform", provider: "anthropic",
      //   api_key: <PLATFORM_ANTHROPIC_KEY from server config>,
      //   model: "claude-sonnet-4-6",
      //   context_cap_tokens: 200000 }
      // PLATFORM_ANTHROPIC_KEY is an admin-primed server config value;
      // never returned in any user-facing surface (see api_key boundary).

  if row.mode == "platform":
    return { mode: "platform",
             provider: "anthropic",
             api_key: <PLATFORM_ANTHROPIC_KEY>,
             model: row.model,
             context_cap_tokens: row.context_cap_tokens }

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
    "model":              "claude-sonnet-4-6",
    "context_cap_tokens": 200000
  }
}
```

The block is **always present** — for tenants with no row, it carries the synthesized platform default (Scenario A). The api_key is **never** in this block. Doctor is a readiness surface, not a secret surface.

The install-skill (M49) reads this block and writes either resolved values (platform) or sentinels (BYOK) into the generated `.usezombie/<zombie>/SKILL.md` frontmatter.

### Worker overlay (sentinels)

A zombie's frontmatter (`x-usezombie.model` and `x-usezombie.context.context_cap_tokens`) can carry resolved values, sentinel values, or omit the keys entirely. The worker overlay rule:

| Frontmatter `model` | Frontmatter `context_cap_tokens` | Worker behavior |
|---|---|---|
| Non-empty string (e.g. `claude-sonnet-4-6`) | — | Use frontmatter value |
| Empty string (`""`) | — | Overlay from `tenant_providers.model` |
| Key absent entirely | — | Overlay from `tenant_providers.model` |
| — | Non-zero int (e.g. `200000`) | Use frontmatter value |
| — | Zero (`0`) | Overlay from `tenant_providers.context_cap_tokens` |
| — | Key absent entirely | Overlay from `tenant_providers.context_cap_tokens` |

The two fields overlay independently. An operator could pin a custom model in frontmatter while leaving `context_cap_tokens` to inherit, or vice versa.

The install-skill emits the **visible sentinels** (`""` and `0`) under BYOK posture rather than omitting the keys. Visible sentinels make it obvious to a human reading the file that "this zombie inherits from tenant config." Hand-edits that strip the keys still work — the absent-key overlay is the safety net.

### Billing gate

The balance gate runs for **both postures**; only the per-event cost function differs:

- **`mode=platform`** — bundled rate (language-model tokens + orchestration + egress).
- **`mode=byok`** — orchestration-only rate (smaller, but non-zero; operator pays their LLM provider directly).

The Free plan disallows BYOK entirely (`byok_requires_paid_plan`). Free is the evaluation tier; giving free orchestration to operators with their own LLM key would be a vector for abuse.

`processEvent` runs one balance gate at step 3 (resolve plan + posture, estimate cost, compare against `core.tenant_billing.balance_cents`). Step 9's telemetry insert calls `compute_charge(plan, posture, tokens)` which returns the right cents.

Full reasoning and the cents math live in [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md).

### api_key visibility boundary

The api_key (platform OR BYOK) **may exist in:**
- `core.vault` rows (encrypted at rest via M45).
- Server-side process memory — return value of `resolveActiveProvider`, executor session.
- Outbound HTTPS request headers to the LLM provider (e.g. `Authorization: Bearer …`).

The api_key **must never appear in:**
- Any HTTP response body (doctor, `GET /v1/tenants/me/provider`, etc.).
- Any log line (worker logs, executor logs, structured logs).
- Any agent tool context or tool-call record.
- Any persisted event row (`core.zombie_events`).
- Any user-facing artifact (frontmatter, dashboard, CLI output).

The boundary is "process-internal vs user-facing," not "in memory vs not in memory." Audit grep in Acceptance Criteria covers the key bytes across event log, worker logs, executor logs.

---

## Implementation slices

### §1 — Schema migration

`schema/0NN_tenant_providers.sql` (full SQL above). Register in `schema/embed.zig` and `src/cmd/common.zig` migration array. Pre-v2.0.0 teardown-rebuild semantics per Schema Table Removal Guard — no `ALTER`/`DROP` migrations, no slot-marker files.

### §2 — Resolver (`src/state/tenant_provider.zig`)

New file. Exports:

- `pub const Mode = enum { platform, byok }` (string-typed at the SQL boundary, enum in Zig).
- `pub const ResolvedProvider = struct { mode, provider, api_key, model, context_cap_tokens }`.
- `pub fn resolveActiveProvider(allocator, conn, tenant_id) !ResolvedProvider`.
- `pub fn upsertByok(allocator, conn, tenant_id, credential_ref, model_override, cap) !void`.
- `pub fn upsertPlatform(allocator, conn, tenant_id) !void` (writes explicit `mode=platform` row; used by `tenant provider reset`).
- `pub fn deleteRow(allocator, conn, tenant_id) !void` (used in tests).

Synthesizes platform default from a server-side constant when no row is present. The platform default model + cap is hardcoded as a constant in this module (per RULE UFS, declared once); the platform api_key is read from server config (M11_005-style env or vault).

### §3 — Doctor extension

`src/http/handlers/doctor.zig` (M44) gains a `tenant_provider` block in the JSON response. Calls `resolveActiveProvider`, strips `api_key`, returns the rest. Failure of the resolver (e.g. `CredentialMissing`) surfaces as `tenant_provider: { mode: "byok", error: "credential_missing", ... }` so the install-skill can detect the broken state.

### §4 — HTTP API

New handlers in `src/http/handlers/tenants/provider.zig`:

```
GET /v1/tenants/me/provider
PUT /v1/tenants/me/provider
DELETE /v1/tenants/me/provider          # equivalent to PUT mode=platform
```

5-place route registration per REST guide §7 (`route_matchers.zig`, `router.zig`, `route_table.zig`, `route_manifest.zig`, `route_table_invoke.zig`).

Tenant-admin guard: `bearer + tenant_admin_required` middleware. Non-admin → 403.

PUT validation order (eager structural, lazy auth):
1. Body shape — `{mode, credential_ref?, model?}`. Malformed → 400.
2. `mode=byok` + Free plan → 403 `byok_requires_paid_plan`.
3. `mode=byok` + missing `credential_ref` → 400 `credential_ref_required`.
4. `mode=byok` + vault row at `(tenant_id, credential_ref)` not found → 400 `credential_not_found`.
5. `mode=byok` + vault JSON lacks `provider` or `api_key` or `model` → 400 `credential_data_malformed`.
6. Resolve effective model (CLI `--model` override → vault `model`). GET model-caps endpoint with that model. Not in catalogue → 400 `model_not_in_caps_catalogue`.
7. UPSERT row. Return 200 with the new resolved config (no api_key).

The handler does NOT make a synthetic call to the LLM provider to verify the key works. Auth-validity surfaces at first event as `provider_auth_failed` (lazy auth validation).

### §5 — CLI (`zombiectl/src/commands/tenant.js` + `provider.js`)

```bash
zombiectl tenant provider get
zombiectl tenant provider set --credential <name> [--model <override>]
zombiectl tenant provider reset
```

Subcommand structure: `tenant` is a parent group, `provider` is a subgroup. Wired via `zombiectl/src/program/routes.js`.

`set` requires `--credential <name>`. No default name. Forces the operator to see the link between "the credential I just stored" and "the credential my tenant uses."

Helpful CLI affordances:
- `set` prints `Tip: run a test event to verify the key works against <provider>.` after success.
- `get` prints a friendly table (mode, provider, model, context_cap_tokens, credential_ref or "—") + a one-line "this is the platform default" footer when the row is absent.
- `reset` prints the new platform-default config and warns if `tenant_billing.balance_cents` is below a threshold.

The legacy `zombiectl provider {get|set|reset}` (without `tenant`) is NOT introduced — the explicit scoping was a design decision, not a transition.

### §6 — UI Settings → Provider

Page at `/settings/provider`:

- Current provider summary card: mode badge ("Platform" or "BYOK"), provider name, model, context cap, credential reference (if BYOK).
- Mode toggle: Platform | BYOK.
- BYOK form:
  - **Credential dropdown** — populated from the tenant's vault credentials list (M45 tenant-scoped credentials API). Operator picks the named credential. If the list is empty, render "Add a credential first" CTA linking to the credentials page.
  - **Model override field** — auto-filled from the picked credential's `model`. Editable.
  - On Save → PUT → success toast → revalidate.
- Platform mode: shows "current spend" widget pulling `tenant_billing.balance_cents`. BYOK mode replaces it with "Manage spend at <provider> directly" + an outbound link.

New components: `ui/packages/app/src/routes/settings/provider.tsx`, `ui/packages/app/src/components/ProviderSelector.tsx`. Use design-system primitives per the UI Component Substitution Gate.

### §7 — Workspace `/credentials/llm` route removal

Pre-v2.0.0 cleanup. The `PUT|GET|DELETE /v1/workspaces/{ws}/credentials/llm` route exists in `main` but has zero runtime consumers (verified by grep across `src/zombie/`, `src/executor/`, `src/state/` — only the vault module references it, in comments). It was a write surface that was never wired to a resolver. RULE NLG forbids leaving it in place pre-v2.0.0.

Removed in this PR:
- `src/http/route_matchers.zig` — `matchWorkspaceLlmCredential` and the `workspaces/credentials/llm` matcher branch.
- `src/http/router.zig` — `workspace_llm_credential` route variant.
- `src/http/route_table.zig` — corresponding spec entry.
- `src/http/route_manifest.zig` — three manifest entries (GET/PUT/DELETE).
- `src/http/route_table_invoke.zig` — `invokeWorkspaceLlmCredential` import + dispatch.
- `src/http/handlers/workspaces/credentials.zig` — BYOK-specific handler functions (generic credential CRUD stays).
- `src/http/byok_http_integration_test.zig` — wholesale delete (tests an obsolete route).
- `src/http/credentials_json_integration_test.zig` — drop the two routing-distinction tests (the generic matcher no longer needs to special-case `/llm`).
- `src/http/route_matchers_test.zig`, `src/http/router_test.zig` — drop the `M24_001:` test cases naming the workspace LLM route.

Migration cleanup: a one-line `DELETE FROM core.vault WHERE name='llm' AND tenant_id IS NULL` in M48's migration to clean up any orphaned workspace-scoped rows. Pre-v2.0.0 the DB is wiped on rebuild anyway; the DELETE is belt-and-braces.

Comment scrubs: `src/state/vault.zig:10` and `src/zombie/credential_key.zig:9` reference "BYOK provider record (`llm`)" — update to "BYOK provider records (operator-named)" since the convention is now operator-named.

### §8 — Provider catalog

`samples/fixtures/m48-provider-fixtures.json`:

```json
{
  "anthropic":  {"default_model": "claude-sonnet-4-6",                          "api_base": "https://api.anthropic.com"},
  "openai":     {"default_model": "gpt-5",                                      "api_base": "https://api.openai.com"},
  "fireworks":  {"default_model": "accounts/fireworks/models/kimi-k2.6",        "api_base": "https://api.fireworks.ai/inference/v1"},
  "moonshot":   {"default_model": "kimi-k2.6",                                  "api_base": "https://api.moonshot.cn/v1"},
  "zhipu":      {"default_model": "glm-5.1",                                    "api_base": "https://open.bigmodel.cn/api/paas/v4"},
  "together":   {"default_model": "deepseek-coder",                             "api_base": "https://api.together.xyz"},
  "openrouter": {"default_model": "anthropic/claude-sonnet-4-6",                "api_base": "https://openrouter.ai/api/v1"}
}
```

UI uses this for the provider-name display in BYOK mode (the operator's `provider` field is matched against this catalogue for a friendly name; unknown providers display the raw string). Adding a new provider = appending here + ensuring NullClaw routes to its endpoint.

### §9 — Worker overlay integration

`src/zombie/event_loop_helpers.zig` (existing) gains the overlay logic at `processEvent`:

1. Read frontmatter `model` and `context_cap_tokens`.
2. If either is sentinel-or-absent (per the worker overlay table), call `resolveActiveProvider(tenant_id)`.
3. Substitute the resolved values per-field.
4. Pass `{api_key, model, context_cap_tokens}` to `executor.startStage` along with the rest of the budget config.
5. The executor → NullClaw provider client uses `api_key` for the outbound request and never logs / echoes it.

The legacy `resolveFirstCredential` (deprecated for tool-level secrets in M45) is also unused for provider resolution — `resolveActiveProvider` is the only path.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/0NN_tenant_providers.sql` | NEW | Schema migration |
| `schema/embed.zig` | EXTEND | Register schema |
| `src/cmd/common.zig` | EXTEND | Migration array |
| `src/state/tenant_provider.zig` | NEW | Resolver + CRUD |
| `src/http/handlers/tenants/provider.zig` | NEW | GET/PUT/DELETE `/v1/tenants/me/provider` |
| `src/http/handlers/doctor.zig` | EDIT | Add `tenant_provider` block |
| `src/http/route_matchers.zig` | EDIT | Add tenant provider matcher; **remove** `matchWorkspaceLlmCredential` |
| `src/http/router.zig` | EDIT | Add `tenant_provider` variant; **remove** `workspace_llm_credential` |
| `src/http/route_table.zig` | EDIT | Add tenant provider spec; **remove** workspace LLM spec |
| `src/http/route_manifest.zig` | EDIT | Add 3 tenant routes; **remove** 3 workspace LLM routes |
| `src/http/route_table_invoke.zig` | EDIT | Add `invokeTenantProvider`; **remove** `invokeWorkspaceLlmCredential` |
| `src/http/handlers/workspaces/credentials.zig` | EDIT | **Remove** BYOK-specific functions |
| `src/zombie/event_loop_helpers.zig` | EDIT | Replace platform-key path with `resolveActiveProvider`; sentinel overlay |
| `src/state/vault.zig`, `src/zombie/credential_key.zig` | EDIT | Comment scrub (operator-named, not "llm") |
| `zombiectl/src/commands/tenant.js` | NEW | `tenant` parent group |
| `zombiectl/src/commands/provider.js` | NEW | `tenant provider {get,set,reset}` |
| `zombiectl/src/program/routes.js` | EDIT | Wire `tenant provider` route |
| `ui/packages/app/src/routes/settings/provider.tsx` | NEW | Settings page |
| `ui/packages/app/src/components/ProviderSelector.tsx` | NEW | Mode + credential + model selector |
| `tests/integration/byok_provider_test.zig` | NEW | E2E |
| `samples/fixtures/m48-provider-fixtures.json` | NEW | Provider catalog |
| `src/http/byok_http_integration_test.zig` | DELETE | Tests obsolete workspace route |
| `src/http/credentials_json_integration_test.zig` | EDIT | Drop routing-distinction tests for `/credentials/llm` |
| `src/http/route_matchers_test.zig`, `src/http/router_test.zig` | EDIT | Drop workspace LLM route test cases |

---

## Interfaces

```
HTTP:
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
    → 400 model_not_in_caps_catalogue      (model not in /_um/.../model-caps.json)
    → 403 byok_requires_paid_plan          (Free plan + mode=byok)
    → 403 (caller is not a tenant admin)

  DELETE /v1/tenants/me/provider
    → 200 (equivalent to PUT mode=platform; UPSERTs explicit platform row)

Doctor extension (surface owned by M44, field owned by M48):
  zombiectl doctor --json
    → { ..., tenant_provider: {
            mode, provider, model, context_cap_tokens,
            credential_ref?: string | null,
            error?: "credential_missing" | "credential_data_malformed"
          } }
    The api_key is NEVER in this block — doctor is a readiness surface.

CLI:
  zombiectl tenant provider get
  zombiectl tenant provider set --credential <name> [--model <override>]
  zombiectl tenant provider reset

  zombiectl credential set <name> --data '<json>'   # M45 surface; sets BYOK credentials
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
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| BYOK set but credential row missing at PUT time | Operator forgot to `credential set` first | 400 `credential_not_found`; row not written |
| BYOK credential JSON lacks `provider`/`api_key`/`model` at PUT time | Operator built malformed JSON | 400 `credential_data_malformed`; row not written |
| BYOK `model` not in caps catalogue at PUT time | Typo, or model not yet in the static catalogue | 400 `model_not_in_caps_catalogue`; row not written |
| BYOK API key invalid (rejected by provider) | Operator pasted wrong key | First event fails with `provider_auth_failed` in event log; provider config remains set so operator can fix the credential |
| BYOK on Free plan | Free disallows BYOK | 403 `byok_requires_paid_plan` |
| Platform mode + balance=0 | Operator out of credits | Next event blocks at balance gate; clear "balance exhausted" error |
| BYOK → platform with low balance | Operator forgot they had run dry | PUT succeeds; next event blocks at balance gate |
| BYOK credential deleted while `mode=byok` | Operator removed credential without flipping mode | Next event's `resolveActiveProvider` returns `CredentialMissing`; event dead-lettered with `provider_credential_missing`. Mode does NOT auto-revert to platform (would silently re-enable platform billing without consent) |
| Concurrent PUTs from two admin sessions | Race | Last write wins; both return 200; revalidate shows the winner |
| `tenant provider set --credential` referencing a vault row that was just deleted | Race between credential delete and provider set | 400 `credential_not_found` |
| In-flight event when posture flips | Event was claimed under old posture | Event finishes under the snapshot taken at claim time (Invariant 4) |

---

## Invariants

1. **api_key visibility boundary.** The api_key (platform OR BYOK) exists only in: vault rows, server-side process memory (resolver / executor session), and outbound HTTPS request headers. It MUST NOT appear in HTTP responses, logs, agent tool context, persisted event rows, or any user-facing artifact.
2. **One active provider per tenant.** `core.tenant_providers.tenant_id` is the primary key. UPSERT semantics; no concurrent rows.
3. **Absence of row = synthesized platform default.** No eager row insertion at tenant creation. The resolver treats "no row" and `mode=platform` row identically for runtime behavior, but `tenant provider reset` writes an explicit `mode=platform` row so the dashboard can distinguish "never configured" from "explicitly reset."
4. **Mode change applies on NEXT event.** In-flight events finish under whatever posture they were claimed under.
5. **Balance gate runs for both postures.** Platform-mode cost includes language-model tokens + orchestration; BYOK-mode cost is orchestration only. Free plan disallows BYOK entirely.
6. **Worker overlay is sentinel-or-absent.** Frontmatter `model: ""` OR `model:` absent ⇒ overlay from `tenant_providers.model`. Same rule for `context_cap_tokens: 0` OR absent. Per-field, not all-or-nothing.
7. **`credential_ref` is operator-named.** No hardcoded name. v1 invariant: `mode=platform` ⇒ `credential_ref IS NULL`; `mode=byok` ⇒ `credential_ref` non-empty AND vault row exists.
8. **Eager structural validation, lazy auth validation.** PUT validates body shape, plan eligibility, credential presence, JSON shape, and model-caps catalogue membership synchronously. PUT does NOT make a synthetic call to the LLM provider; key validity surfaces at first event.
9. **Workspace-scoped `/credentials/llm` is removed.** Pre-v2.0.0 cleanup; no compat shim.

---

## Test Specification

Every test maps back to a scenario or invariant.

| Test | Scenario / Invariant | Asserts |
|------|----------------------|---------|
| `test_default_provider_synthesized_when_no_row` | A | New tenant with no `tenant_providers` row → resolver returns synthesized platform default; doctor block reports it |
| `test_explicit_platform_row_matches_synth_default` | Inv 3 | Tenant after `tenant provider reset` has explicit `mode=platform` row; resolver output is byte-identical to synthesized default |
| `test_byok_set_writes_row_and_resolves_cap` | B | `PUT mode=byok` with valid credential → row written with `mode/provider/model/credential_ref/context_cap_tokens` from caps endpoint |
| `test_byok_set_routes_to_operator_provider` | B | Set BYOK fireworks → run zombie → outbound LLM call hits `api.fireworks.ai/inference/v1` with operator's api_key |
| `test_byok_model_switch_rewrites_row` | C | Re-run `tenant provider set` with new model → row's `model` and `context_cap_tokens` updated; `credential_ref` unchanged |
| `test_byok_reset_to_platform` | D | `tenant provider reset` → `mode=platform`, `credential_ref=NULL`; next event uses platform Anthropic |
| `test_credential_delete_in_byok_dead_letters_event` | E | Delete vault row while `mode=byok` → next event dead-lettered with `provider_credential_missing`; `tenant_providers` row unchanged |
| `test_byok_blocked_on_free_plan` | F, Inv 5 | Free-plan tenant `PUT mode=byok` → 403 `byok_requires_paid_plan`; no row written |
| `test_byok_with_missing_credential_ref` | Validation | `PUT mode=byok` without `credential_ref` → 400 `credential_ref_required` |
| `test_byok_with_unknown_credential_name` | Validation | `PUT mode=byok` referencing a non-existent vault row → 400 `credential_not_found` |
| `test_byok_with_malformed_credential_data` | Validation | Vault JSON missing `provider`/`api_key`/`model` → 400 `credential_data_malformed` |
| `test_byok_with_unknown_model_400s` | Validation, Inv 8 | Model not in caps catalogue → 400 `model_not_in_caps_catalogue`; row not written |
| `test_byok_invalid_key_surfaces_at_first_event` | Inv 8 | `tenant provider set` with bad key succeeds (lazy auth); first event fails with `provider_auth_failed` |
| `test_balance_gate_runs_under_platform_mode` | Inv 5 | Platform tenant with balance=0 → event blocked at gate |
| `test_balance_gate_runs_under_byok_mode` | Inv 5 | BYOK tenant with balance=0 → event blocked at orchestration-only gate |
| `test_worker_overlay_empty_string_sentinel` | Inv 6 | Frontmatter `model: ""` → worker overlays from `tenant_providers.model` |
| `test_worker_overlay_zero_int_sentinel` | Inv 6 | Frontmatter `context_cap_tokens: 0` → worker overlays from `tenant_providers.context_cap_tokens` |
| `test_worker_overlay_absent_key` | Inv 6 | Frontmatter omits `model:` and `context_cap_tokens:` keys → worker overlays both |
| `test_worker_respects_non_sentinel_frontmatter` | Inv 6 | Frontmatter `model: claude-haiku-4-5`, `context_cap_tokens: 100000` → worker uses frontmatter values, no overlay |
| `test_worker_overlay_per_field_independent` | Inv 6 | Frontmatter `model: claude-haiku-4-5`, `context_cap_tokens: 0` → worker uses frontmatter model, overlays cap |
| `test_in_flight_event_keeps_claim_time_posture` | Inv 4 | Claim event under BYOK; flip to platform mid-event; event finishes under BYOK |
| `test_doctor_block_under_platform_mode` | A | Doctor reports `mode=platform`, platform default model+cap, no api_key field |
| `test_doctor_block_under_byok_mode` | B | Doctor reports `mode=byok`, operator's provider/model/cap, no api_key field |
| `test_doctor_block_under_byok_with_missing_credential` | E | Doctor reports `mode=byok` + `error: credential_missing` |
| `test_api_key_no_leak_into_http_responses` | Inv 1 | Audit grep on every PUT/GET/doctor response for the api_key bytes → 0 matches |
| `test_api_key_no_leak_into_event_log` | Inv 1 | Run zombie under BYOK → grep `core.zombie_events` for the api_key bytes → 0 matches |
| `test_api_key_no_leak_into_tool_context` | Inv 1 | Run zombie under BYOK → capture agent's tool-call records → 0 matches |
| `test_api_key_no_leak_into_logs` | Inv 1 | Run zombie under BYOK → grep worker + executor logs → 0 matches |
| `test_concurrent_put_last_write_wins` | Inv 2 | Two PUTs racing → final config = whichever committed last; both return 200 |
| `test_tenant_admin_required` | API auth | Non-admin PUT → 403 |
| `test_workspace_credentials_llm_route_404s` | Inv 9 | `PUT /v1/workspaces/{ws}/credentials/llm` → 404 (route removed) |

---

## Acceptance Criteria

- [ ] `make test-integration` passes the test suite above (32 tests).
- [ ] `make test` passes (unit-level).
- [ ] `make lint` clean.
- [ ] `make memleak` clean for any handler / executor changes.
- [ ] `make check-pg-drain` clean.
- [ ] Cross-compile clean: `x86_64-linux` + `aarch64-linux`.
- [ ] **Manual A:** Free-plan tenant runs through Scenario A end-to-end; doctor reports synthesized platform default; first 20 events run; 21st event blocks at gate.
- [ ] **Manual B:** Team-plan tenant runs Scenario B end-to-end; outbound LLM call observed (NullClaw debug logging) hitting Fireworks; UseZombie balance drains at orchestration-only rate.
- [ ] **Manual C:** Operator switches from Kimi K2 to DeepSeek V4 Pro; `tenant_providers.context_cap_tokens` updated; in-flight event finishes on K2; next event runs on V4.
- [ ] **Manual D:** Operator runs `tenant provider reset` mid-month with low balance; next event blocked at gate with credit-exhausted UX; `tenant provider set` flow re-enables BYOK.
- [ ] **Manual E:** Operator deletes BYOK credential while in BYOK mode; next event dead-lettered; system does NOT auto-revert.
- [ ] **Manual F:** Free-plan tenant attempting `tenant provider set` → 403 with `byok_requires_paid_plan`; UI shows upgrade CTA.
- [ ] **Audit grep:** BYOK api_key bytes never appear in `core.zombie_events`, `core.tenant_providers`, worker logs, executor logs, doctor JSON, or any HTTP response body across the test run.
- [ ] **Workspace route removal:** `PUT /v1/workspaces/{ws}/credentials/llm` returns 404; the corresponding handler functions and integration test file are absent from the diff (`git grep` for `workspace_llm_credential` returns 0 hits in non-historical files).
- [ ] **Architecture cross-reference:** [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) and [`docs/architecture/scenarios/02_byok.md`](../../architecture/scenarios/02_byok.md) updated in the same PR or a sibling commit on the same branch (per Architecture Consult & Update Gate).

---

## Out of Scope

- **Per-workspace provider override.** Tenant-scoped only in v1; billing scope is the tenant.
- **Per-zombie provider override.** Frontmatter cannot pin an api_key. Future spec if a "treat one zombie like a different tenant" use case appears.
- **Auto-fallback BYOK → platform on provider error.** Errors surface to the operator; no silent fallback (would charge the platform without consent).
- **BYOK metering for operator-side cost reporting.** Operator manages their LLM cost at their provider's dashboard; we don't proxy that data.
- **Custom OIDC / external vault-backend integrations.** Treat M45's vault as the source of truth.
- **Synthetic test call to the LLM provider on `tenant provider set`.** Lazy auth validation; the `Tip: run a test event` CLI hint is the ergonomic substitute.
- **Multi-active providers per tenant** (e.g. "use Anthropic for zombie A, Fireworks for zombie B"). One active provider per tenant in v1; multi-credential vault storage is supported (operator can flip via `tenant provider set --credential <other>`) but only one is active.
- **The model-caps catalogue itself, the `/_um/.../model-caps.json` endpoint, and the admin-zombie that maintains it.** Owned by `docs/architecture/billing_and_byok.md` §9 and the platform infra; this spec consumes the endpoint as a black box.
