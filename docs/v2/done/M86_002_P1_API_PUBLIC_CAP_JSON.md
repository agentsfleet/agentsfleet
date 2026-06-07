# M86_002: Public `cap.json` — model catalogue + global non-auth config in one endpoint

**Prototype:** v2.0.0
**Milestone:** M86
**Workstream:** 002
**Date:** Jun 07, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the Models BYOK wizard (M86_001) server-fetches this for its model picker + default-model prefill; it cannot ship without it.
**Categories:** API
**Batch:** B1 — sibling of M86_001 on the same PR; build this first.
**Branch:** feat/dashboard-launch-polish
**Depends on:** M48b_001 (the public model-caps endpoint this renames + extends), M48_001 (per-model token rates already in the response)
**Provenance:** agent-generated from the M86 `/plan-eng-review` (Jun 07, 2026) — the wizard's catalogue gap (D1) resolved by serving the catalogue from this endpoint (D2). Cross-check every claim against the named files before EXECUTE.

> **Provenance is load-bearing.** LLM-drafted — cross-check against `src/zombied/http/handlers/model_caps.zig` and `src/zombied/state/tenant_billing.zig` before EXECUTE.

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §10 (the endpoint), `docs/architecture/scenarios/02_self_managed.md` §5, `docs/architecture/user_flow.md`.

---

## Implementing agent — read these first

1. `src/zombied/http/handlers/model_caps.zig` — the handler. `MODEL_CAPS_PATH` (line 41) + `ResponseBody` (line 52) + the doc-comment header are what this spec renames + extends. The model rows already carry per-model token rates.
2. `src/zombied/state/tenant_billing.zig` — the source of truth for the global constants: `RUN_NANOS_PER_SEC` (35), `EVENT_NANOS` (28), `STARTER_CREDIT_NANOS` (17) are `pub const`; `FREE_TRIAL_END_MS` (48) + `FREE_TRIAL_STAGE_NANOS` (49) are file-private `const` and must be exposed (a `pub` accessor) for the handler to read them.
3. `src/zombied/http/handlers/model_caps_integration_test.zig` — the wrong-key 404 path asserts the literal `/model-caps.json`; it must follow the rename.
4. `dispatch/write_zig.md` — ZIG / PUB / LIFECYCLE gates; the new pub surface on `tenant_billing.zig` is a PUB-gate decision.
5. `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` — public endpoint shape.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** part of PR #373 — `cap.json`: public model catalogue + global billing config (renames `model-caps.json`)
- **Intent (one sentence):** Rename the public `_um/<key>/model-caps.json` endpoint to `cap.json` and extend its body with a global, non-auth `rates`/`billing` block (run + event rate, starter credit, free-trial window) sourced from the server billing constants, so one public document carries the model catalogue **and** the global client config — killing the need to hardcode those constants in client code.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: …`; mismatch with the Intent above → STOP.

---

## Applicable Rules

- **`dispatch/write_zig.md`** — ZIG (memory safety, errdefer), **PUB** (exposing `FREE_TRIAL_*` via a `pub` accessor on `tenant_billing.zig` is new pub surface — justify or wrap), LIFECYCLE, file ≤350 / fn ≤50 / method ≤70, cross-compile both linux targets.
- **`dispatch/write_http.md`** — public REST endpoint design; reads `docs/REST_API_DESIGN_GUIDELINES.md`.
- **`dispatch/write_any.md`** — File/Function Length, LOGGING, MILESTONE-ID (no `M86`/§ in source/test names), ERROR REGISTRY, UFS named constants, legacy-workaround family, GREPTILE end-of-turn.
- **RULE NLG (pre-2.0)** — rename is a clean break: no compat alias, the old `model-caps.json` path 404s (not 410). No "legacy" framing.
- **`name_architecture`** — the endpoint is documented in `docs/architecture/`; the rename reconciles those docs in the same change.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG | yes | Handler + the `tenant_billing.zig` accessor; memory-safe, errdefer on the new alloc, cross-compile both linux targets. |
| PUB | yes | Exposing the free-trial constants is new pub surface — prefer a single `pub fn publicConfig()` (or a `pub const PublicConfig`) on `tenant_billing.zig` over making each `const` pub; FILE SHAPE DECISION at PLAN. |
| SCHEMA | no | No migration — `core.model_caps` already carries the catalogue + per-model rates; the global block comes from server constants. |
| ERROR REGISTRY | no | No new error codes (empty-catalogue 503 path unchanged). |
| LOGGING / LIFECYCLE | yes | Existing handler logging/lifecycle preserved; the added alloc frees on the same `hx.alloc` arena path. |
| MILESTONE-ID | yes | No `M86_002` / §x.y in any `.zig` body or `test "…"` name. |
| UFS | yes | The JSON field names + the path string are named constants. |

---

## Overview

**Goal (testable):** `GET /_um/<key>/cap.json` returns `200` with `{ version, models: [...], rates: {...}, billing: {...} }` where the global block equals the `tenant_billing.zig` constants; `GET /_um/<key>/model-caps.json` returns `404`; the per-model `models[]` shape is byte-unchanged.

**Problem:** The public endpoint serves only the model catalogue. The same global, non-secret constants the dashboard needs (run/event rates, starter credit, free-trial window) are hardcoded in client code (`ui/packages/app/lib/types.ts`, pinned by `audit-cross-tier-rates`). There is no public, single source the client can read for "the global config that needs no auth." And the wizard (M86_001) needs the model catalogue client-side, which means the Next.js server fetching this endpoint.

**Solution summary:** Rename `model-caps.json` → `cap.json` (the cryptic path-key prefix is unchanged) and add a `rates` + `billing` block to the response, read from `tenant_billing.zig`. `models[]` is untouched. Pre-launch, so the rename is a clean break — no alias, old path 404s. The Next.js app (M86_001) server-fetches `cap.json` for the wizard's model list + default-model prefill; the `rates`/`billing` block is the future source for the dashboard's billing display (which later drops the `types.ts` mirror — **not** in this spec).

---

## Prior-Art / Reference Implementations

- **The endpoint** → `model_caps.zig` (M48b) already does the DB read, `version` (max `updated_at_ms`), wrong-key 404, empty-catalogue 503. This spec extends its `ResponseBody` and renames the path.
- **The constants** → `tenant_billing.zig` is the cross-tier source of truth (`audit-cross-tier-rates` pins `RUN_NANOS_PER_SEC`/`FREE_TRIAL_END_MS`/`FREE_TRIAL_STAGE_NANOS` here). Serializing from these constants keeps the public document and the server enforcer in lockstep by construction.
- **Pub-surface pattern** → `tenant_billing.zig:228` already surfaces `FREE_TRIAL_END_MS` inside a returned struct (the billing-status path), so a `pub` accessor for the public-config trio follows existing shape.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/http/handlers/model_caps.zig` | EDIT | Rename `MODEL_CAPS_PATH` value `model-caps.json` → `cap.json` (+ the `MODEL_CAPS_PATH` constant well-formed test, doc header); add `rates` + `billing` to `ResponseBody`, populated from `tenant_billing` constants. |
| `src/zombied/state/tenant_billing.zig` | EDIT | Expose the global non-auth constants for the handler — a single `pub fn publicConfig()` / `pub const` (PUB gate) covering run/event/starter/free-trial. |
| `src/zombied/http/handlers/model_caps_integration_test.zig` | EDIT | Rename the wrong-key path; add assertions for the `rates`/`billing` block; keep the `models[]` shape assertions. |
| `docs/architecture/billing_and_provider_keys.md`, `scenarios/01_default_install.md`, `scenarios/02_self_managed.md`, `scenarios/README.md`, `user_flow.md`, `capabilities.md` | EDIT | Reconcile the endpoint name `model-caps.json` → `cap.json` + the new global block (name_architecture). |
| `~/Projects/skills/usezombie-install-platform-ops/{SKILL.md,references/self-managed-handoff.md}` | EDIT (cross-repo, own branch) | Prose mentions of the URL → `cap.json`. Runtime-safe (skill uses doctor), but stale prose must be corrected. |
| `~/Projects/skills-agent-term/usezombie-install-platform-ops/{SKILL.md,references/self-managed-handoff.md}` | EDIT (cross-repo) | Agent-term mirror of the same. |
| `~/Projects/docs/cli/zombiectl.mdx` | EDIT (cross-repo, own branch) | "resolves the cap from the model-caps endpoint" → `cap.json`. `changelog.mdx` is append-only history — a new entry, do not rewrite old. |

> No schema migration. `zombiectl` (`tenant.ts`) does **not** call this endpoint (it uses the authed `/v1/tenants/me/provider`; cap resolved server-side), so no CLI code change — verified via grep (the only `cap.json` hits in `zombiectl/` are an unrelated test capture buffer).

---

## Decomposition & alternatives

- **Chosen shape:** rename + extend the existing handler in place; expose the constants via one accessor. ~Small, single-handler.
- **Alternatives considered:** (a) keep `model-caps.json` and add a *second* `config.json` endpoint — rejected: two public endpoints for one catalogue+config document, more surface to rotate. (b) Add a new authed `/v1/tenants/me/config` aggregate — **rejected by Indy** (the cathedral; the config here is global + non-secret, so it belongs on the public unauth endpoint). (c) Make each `FREE_TRIAL_*` `const` → `pub const` — rejected vs a single accessor (smaller pub surface, PUB gate).
- **Verdict:** in-place rename + extend.

---

## Sections (implementation slices)

### §1 — Rename `model-caps.json` → `cap.json`
`MODEL_CAPS_PATH` value, the doc-comment header, and the `model_caps.zig` `MODEL_CAPS_PATH`-well-formed test follow the new filename. The cryptic path-key prefix is unchanged. Pre-launch: no alias; the old path 404s through the existing router miss.
- **Dimension 1.1** — `GET /_um/<key>/cap.json` → 200; `GET /_um/<key>/model-caps.json` → 404. → Test `integration(cap_json): renamed path serves, old path 404s`

### §2 — Global non-auth `rates` + `billing` block
Extend `ResponseBody` with `rates { run_nanos_per_sec, event_nanos }` + `billing { starter_credit_nanos, free_trial_end_ms, free_trial_stage_nanos }`, read from `tenant_billing` via a single `pub` accessor. `models[]` byte-unchanged.
- **Dimension 2.1** — the block equals the `tenant_billing.zig` constants. → Test `integration(cap_json): global block matches billing constants`
- **Dimension 2.2** — `models[]` shape (id/provider/cap/3 rates) is unchanged vs the pre-rename response. → Test `integration(cap_json): model rows unchanged`

### §3 — Reconcile docs + cross-repo prose
In-repo architecture docs + the two skill repos + `docs/cli/zombiectl.mdx` updated to `cap.json`. `changelog.mdx`: a new entry (append-only).
- **Dimension 3.1** — no in-repo doc references the old `model-caps.json` path. → Verify: `git grep -n 'model-caps.json' -- docs/` → 0

---

## Interfaces

```
GET /_um/<MODEL_CAPS_PATH_KEY>/cap.json[?model=<urlencoded>]
-> 200 {
     version: "YYYY-MM-DD",
     models: [{ id, provider, context_cap_tokens,
                input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok }],   // UNCHANGED
     rates:   { run_nanos_per_sec, event_nanos },                                              // NEW
     billing: { starter_credit_nanos, free_trial_end_ms, free_trial_stage_nanos }              // NEW
   }
-> 404 for the old /model-caps.json path and any wrong key
-> 503 empty catalogue (unchanged)

tenant_billing.zig (NEW pub surface — one accessor, not per-const pub):
  pub fn publicConfig() PublicConfig    // { run_nanos_per_sec, event_nanos, starter_credit_nanos, free_trial_end_ms, free_trial_stage_nanos }
```

The `?model=` filter applies to `models[]` only; the global block is always present.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Old path hit | client still on `model-caps.json` | 404 (router miss) — pre-launch, no alias; consumers are prose-only or use doctor/authed API, so no runtime break. |
| Empty catalogue | migration didn't seed | 503 (unchanged); the global block is not a substitute for an empty catalogue. |
| Constant drift | someone edits `types.ts` rates but not Zig | `audit-cross-tier-rates` still pins Zig↔TS mirrors (this spec does not remove them); the public block reads Zig, so it can't drift from the enforcer. |

---

## Invariants

1. `models[]` is byte-for-byte the same shape before and after — enforced by `integration(cap_json): model rows unchanged`.
2. The `rates`/`billing` values equal the `tenant_billing.zig` constants at all times (read, not copied) — enforced by `integration(cap_json): global block matches billing constants`.
3. The old `model-caps.json` path returns 404, no alias — enforced by the rename test (pre-launch clean break, RULE NLG).
4. New pub surface on `tenant_billing.zig` is a single accessor, not five `pub const` — PUB gate.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | integration | `renamed path serves, old path 404s` | `cap.json` → 200; `model-caps.json` → 404. |
| 2.1 | integration | `global block matches billing constants` | response `rates`/`billing` == `tenant_billing.publicConfig()`. |
| 2.2 | integration | `model rows unchanged` | a model row has id/provider/cap + 3 rate fields, same as pre-rename. |
| 1.1 | unit | `cap path constant well-formed` | `MODEL_CAPS_PATH` ends `/cap.json`, key len 32. |
| — | cross-compile | `zig build -Dtarget=x86_64-linux && -Dtarget=aarch64-linux` | both green. |

**Regression:** existing `model_caps` integration assertions (version format, empty-catalogue 503, `?model=` filter) stay green.

---

## Acceptance Criteria

- [ ] `cap.json` serves catalogue + `rates`/`billing`; `model-caps.json` 404s — verify: `make test-integration` (cap_json cases)
- [ ] global block == `tenant_billing` constants — verify: the matching integration test
- [ ] `make lint` (lint-zig) + `make memleak` clean; cross-compile both linux targets
- [ ] `git grep -n 'model-caps.json' -- docs/ src/` → 0 (in-repo); cross-repo prose updated on their own branches
- [ ] `gitleaks detect` clean

---

## Eval Commands (post-implementation)

```bash
# E1: rename complete in-repo
git grep -n 'model-caps.json' -- docs/ src/ && echo "CHECK(empty=pass)"
# E2: integration
make test-integration 2>&1 | tail -8
# E3: cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo OK
# E4: leak + lint
make memleak 2>&1 | tail -3 && make lint 2>&1 | tail -3
```

---

## Dead Code Sweep

**Orphaned references** — after the rename:

| Removed | Grep | Expected |
|---------|------|----------|
| `model-caps.json` path string (in-repo) | `git grep -n 'model-caps.json' -- src/ docs/` | 0 |

---

## Discovery (consult log)

- `/plan-eng-review` (Jun 07): this spec exists because the M86_001 wizard needs the model catalogue client-side (D1); serving it from the public `cap.json` (D2) resolves the gap without the authed-config cathedral Indy rejected.
- Indy directive (Jun 07): `cap.json` is the alternative to `model_cap.json` (a rename, not a second endpoint); it carries `model_cap` + rates + global items that require no auth; served via zombied; the Next.js server fetches it for the wizard. Both M86_001 + M86_002 land on PR #373 / `feat/dashboard-launch-polish`, this one first.
- `key_prefix` is NOT served here — the wizard's provider detection stays a client-side key-format heuristic (`detect-provider.ts`), per D1.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Integration cases for rename + global block; cross-compile proof. |
| After tests pass | `/review` | Clean OR every finding dispositioned. |
| After push | `/review-pr` + `kishore-babysit-prs` | Greptile to two empty polls. |

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Integration | `make test-integration` | {paste} | |
| Cross-compile | `zig build -Dtarget=…-linux` ×2 | {paste} | |
| Leak + lint | `make memleak && make lint` | {paste} | |
| Rename complete | `git grep model-caps.json src/ docs/` | {paste} | |

---

## Out of Scope

- Killing the `types.ts` rate mirror / rewiring the dashboard billing display to read `cap.json` — a later follow-up; this spec only *serves* the data. The `audit-cross-tier-rates` pin (Zig↔website↔app↔zombiectl) stays intact.
- `key_prefix` as a served field or DB column — the wizard's prefix heuristic stays client-side (D1).
- Any change to `core.model_caps` rows or per-model rates.
- Endpoint auth / rotation of the cryptic path-key.
