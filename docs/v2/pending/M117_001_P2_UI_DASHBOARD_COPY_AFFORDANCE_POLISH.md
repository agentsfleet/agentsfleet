<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M117_001: Dashboard copy trim, Models affordance clarity, and connector-fetch error surfacing

**Prototype:** v2.0.0
**Milestone:** M117
**Workstream:** 001
**Date:** Jul 06, 2026
**Status:** PENDING
**Priority:** P2 — post-deploy dashboard polish: verbose copy, a mislabeled affordance that reads like a privilege it isn't, and a swallowed fetch error that hides the connectors failure.
**Categories:** UI
**Batch:** B1 — standalone UI workstream; independent of the connectors runtime bug and the M116 error-registry work.
**Depends on:** none.
**Provenance:** agent-generated (pre-spec, this session's post-deploy dashboard review)
**Canonical architecture:** no architecture change — UI copy + presentation only. Scope model of record: `docs/AUTH.md` §scopes (confirms the tenant provider switch is correctly tenant-scoped).
**Branch:** {feat/mNN-name — added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`

---

## Overview

**Goal (testable):** The Create-API-Key dialog copy is one tight sentence with no duplicated "shown once"; the tenant Models card reads as "you are on the platform default — add your own key to override," never as editing the global default; and a failed server-side connector-catalog fetch logs its `ApiError` before degrading to an empty list.

**Problem:** Three post-deploy dashboard nits. (1) `CreateApiKeyDialog` says "The raw key is shown once. Name it so you can recognise it later in the list." while the reveal step already says "shown once" — verbose and redundant. (2) On the tenant Models page the active card is titled "Platform default model" with an "Add key & model" button; it reads as if a tenant admin edits the *global* platform default — even though the action (`setProviderSelfManagedAction`) is correctly tenant-scoped (the tenant sets their *own* provider). (3) `integrations/page.tsx` wraps the catalog fetch in `.catch(() => [])`, so any failure is silently swallowed and rendered as "Couldn't load connectors" with no server-side trace — the exact reason the current connectors failure is undiagnosable.

**Solution summary:** Tighten the API-key dialog copy (and align the AddRunner sibling if still verbose). Relabel the tenant Models affordance so the platform-default-vs-own-key distinction is unambiguous — no authorization change, because the action is already correctly tenant-scoped. Replace the silent `.catch(() => [])` with a catch that logs the `ApiError` (status + code + detail) server-side before degrading closed. No backend change.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(m117): trim API-key copy, clarify tenant Models affordance, log swallowed connector-fetch error
- **Intent (one sentence):** Three dashboard clarity/observability fixes — tighter copy, an honestly-labeled Models card, and a connector-fetch failure that leaves a trace.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx` (~line 100) — the verbose copy; the reveal copy (~line 165) already carries "shown once", so the pre-create line should not repeat it.
2. `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelRow.tsx` (`title` at ~line 64) + `ProviderSwitchList.tsx` — the affordance. `setProviderSelfManagedAction`/`resetProviderAction` are the tenant-scoped actions; confirm no control here calls the platform-admin route before deciding label vs guard.
3. `ui/packages/app/app/(dashboard)/integrations/page.tsx` (line ~36, `getConnectorCatalog(...).catch(() => [])`) — the swallow; `lib/api/client.ts` `request()` already throws a typed `ApiError` (status + user_message/detail) that the catch currently discards.
4. `ui/packages/app/app/(dashboard)/admin/models/page.tsx` (`hasScope(SCOPE.MODEL_READ)` redirect) — the existing scope-gating precedent, in case §2 confirms a guard is warranted.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx` | EDIT | Trim the pre-create copy; drop the duplicated "shown once" |
| `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx` | EDIT | Align the sibling one-time-secret copy if still verbose |
| `ui/packages/app/app/(dashboard)/settings/models/components/ActiveModelRow.tsx` | EDIT | Relabel so "platform default" vs "your own key" is unambiguous |
| `ui/packages/app/app/(dashboard)/settings/models/components/ProviderSwitchList.tsx` | EDIT | Same affordance clarity where the switch/add copy renders |
| `ui/packages/app/app/(dashboard)/integrations/page.tsx` | EDIT | Log the `ApiError` server-side before degrading the catalog to `[]` |
| `**/*.test.tsx` (co-located) | EDIT/CREATE | Assert the new copy + the error-logging path |

## Applicable Rules

- **`dispatch/write_ts_adhere_bun.md`** — TS/Bun discipline; UI copy strings as named constants where reused (`ADD_KEY_AND_MODEL_LABEL` is the existing pattern); no raw-HTML substitution needed (design-system primitives already in use).
- **`docs/greptile-learnings/RULES.md`** — NLR (touch-it-fix-it on the copy files edited), UFS (a repeated copy string becomes one constant, not duplicated literals).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI Substitution / DESIGN TOKEN | no | no new components, no arbitrary utilities; copy + presentation only |
| File & Function Length (≤350/≤50/≤70) | yes | edits are in-place string/label swaps; no growth |
| UFS | yes | any reused copy string is a named constant (mirror `ADD_KEY_AND_MODEL_LABEL`) |
| ZIG / SCHEMA / LOGGING (Zig) / ERROR REGISTRY | no | no backend, no schema, no Zig |

## Prior-Art / Reference Implementations

- **Reference (copy-constant pattern):** `ActiveModelRow.tsx` `ADD_KEY_AND_MODEL_LABEL` — reuse the named-constant idiom for any shared string.
- **Reference (scope gate, if §2 needs one):** `admin/models/page.tsx` `hasScope(SCOPE.MODEL_READ)` redirect — the precedent if a control is found to hit the admin route.
- **Reference (server-side logging):** the existing `request()`/`ApiError` shape in `lib/api/client.ts` — log its fields, don't invent a new error type.

## Sections (implementation slices)

### §1 — Trim the one-time-secret copy

Removes verbose/duplicated copy from the secret-creation dialogs. **Implementation default:** the pre-create line drops the "shown once" it repeats at reveal and keeps only the naming guidance — e.g. "Name it so you can find it later." Verify the AddRunner sibling and tighten only if still verbose (its `FormDescription` may already be concise).

- **Dimension 1.1** — the Create-API-Key pre-create copy is one clause, no "shown once" duplication → Test `test_api_key_copy_trimmed`
- **Dimension 1.2** — the AddRunner one-time-secret copy is consistent (concise, no redundant "shown once") → Test `test_add_runner_copy_consistent`

### §2 — Clarify the tenant Models "platform default" affordance

Makes the tenant Models card honestly represent what it does. **Implementation default:** this is a **labeling change, not an authorization change** — `setProviderSelfManagedAction`/`resetProviderAction` are correctly tenant-scoped (the tenant sets their *own* provider; the global platform default is edited only under `admin/models`, `MODEL_ADMIN`-gated). Relabel so the card reads "Running on the platform default — add your own key to override," never implying the tenant edits the global default. Only if a control here is found to call the platform-admin route → add a `hasScope(MODEL_ADMIN)` guard (Discovery records which).

- **Dimension 2.1** — the tenant Models card/button copy distinguishes "on the platform default" from "use your own key" without implying global-default editing → Test `test_models_affordance_copy_clear`
- **Dimension 2.2** — no tenant-surface control mutates the global platform default (grep: no `admin/platform-keys` call from `settings/models`) → Test `test_no_platform_default_mutation_from_tenant`

### §3 — Surface the swallowed connector-fetch error

Stops the catalog fetch failure from vanishing. **Implementation default:** replace `.catch(() => [])` with a catch that logs the caught `ApiError` (status, `error_code`, detail, request path) via the server-side logger before returning the empty degrade — the empty-list UX stays, but the cause is now one grep away.

- **Dimension 3.1** — a failing `getConnectorCatalog` logs the error server-side (status + code + detail) before degrading to `[]`; the page still renders the empty-state, not a crash → Test `test_connector_fetch_error_logged`

## Interfaces

```
No API/route change. Server actions unchanged (setProviderSelfManagedAction, resetProviderAction).
integrations/page.tsx degrade contract UNCHANGED: a failed catalog fetch still yields [] →
  IntegrationsConnectors renders "Couldn't load connectors". The ONLY delta is a server-side
  log line emitted before the degrade. No new user-visible error surface.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Copy change breaks a test asserting old string | Snapshot/text test pinned to the verbose copy | Update the co-located test to the new copy in the same commit |
| Over-correcting §2 into a gate | Mistaking the tenant self-provider action for a platform-default edit | §2.2 proves the action is tenant-scoped first; default is relabel, not gate |
| Log leaks a secret | Logging the whole response body including a token | Log only status + `error_code` + detail + path — never body/headers/token |
| Logging throws | Logger unavailable in the RSC context | The log is best-effort; the `[]` degrade still returns so the page renders |

## Invariants

1. No tenant-surface control mutates the global platform default — §2.2 grep assertion, not review.
2. The connector-fetch degrade still returns `[]` on failure (page never crashes) — §3.1 test.
3. No secret material in any new log line — logged fields are an allowlist (status/code/detail/path).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| connector-catalog fetch error log | ops | server-side `getConnectorCatalog` throws | status, `error_code`, detail, request path | no response body / token / header material | `test_connector_fetch_error_logged` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_api_key_copy_trimmed` | rendered dialog copy has no second "shown once"; is a single naming clause |
| 1.2 | unit | `test_add_runner_copy_consistent` | AddRunner one-time-secret copy carries no redundant "shown once" |
| 2.1 | unit | `test_models_affordance_copy_clear` | tenant Models card copy contains the platform-default-vs-own-key distinction; no "edit the platform default" phrasing |
| 2.2 | unit | `test_no_platform_default_mutation_from_tenant` | grep `settings/models/**` → 0 calls to the `admin/platform-keys` route |
| 3.1 | integration | `test_connector_fetch_error_logged` | inject a `getConnectorCatalog` rejection → a server log with status+code emitted; return value is `[]` (no throw) |
| — | regression | `test_integrations_empty_state_unchanged` | with `catalog=[]` the page still renders "Couldn't load connectors" (degrade UX preserved) |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | API-key copy trimmed (§1) | `grep -c 'shown once' 'ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx'` | ≤1 | P2 | |
| R2 | Models affordance not authz-changed, only relabeled (§2) | `grep -rn 'platform-keys' 'ui/packages/app/app/(dashboard)/settings/models/'` | no output | P2 | |
| R3 | Connector-fetch error is logged (§3) | inspect `integrations/page.tsx` catch | logs before returning `[]`; no bare `.catch(() => [])` | P2 | |
| R4 | Diff inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the table | P0 | |
| S1 | UI unit tests pass | `make test-unit-agentsfleet` (or the ui package test runner) | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S4 | No oversize file | `git diff --name-only origin/main \| grep '\.tsx\?$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P2 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| N/A — no symbols removed | — | — |

## Out of Scope

- **The connectors runtime root cause** (catalog 500 / `NEXT_PUBLIC_API_URL`) — a separate bug; §3 only makes it diagnosable.
- **The M116 error-registry work** — the "catalogue"→"library" copy and de-mudball live there.
- **Any backend/scope change** — the tenant provider action is already correctly scoped; §2 is presentation only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user reads a one-line API-key hint, understands at a glance on the Models page that they're running on the platform default and can add their own key (not editing a global setting), and — when connectors fail — an operator finds the cause in the logs instead of guessing.
2. **Preserved user behaviour** — Key creation, provider switching, and the connectors empty-state all behave exactly as today; only copy, a label, and a log line change.
3. **Optimal-way check** — Yes: copy + one log line is the most direct fix; no refactor buys more.
4. **Rebuild-vs-iterate** — Iterate. Presentation-only edits; determinism untouched.
5. **What we build** — trimmed copy (2 dialogs), a clearer Models label, one server-side error log.
6. **What we do NOT build** — the connectors root fix, any scope/authz change, the model copy rename (M116).
7. **Fit with existing features** — Compounds with the existing `hasScope` gating and `ApiError` plumbing; must not destabilize the tenant provider switch.
8. **Surface order** — UI-only; no CLI/API surface.
9. **Dashboard restraint** — §2 is the restraint fix itself: a card must not imply a privilege the user doesn't have; §3 adds a signal (a log) behind a real failure, not a vanity control.
10. **Confused-user next step** — Clearer copy is the self-serve move; the new log is the operator's.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One workstream, three small Sections by surface (copy / Models affordance / error surfacing); each independently verifiable, all "post-deploy dashboard clarity."
- **Alternatives considered:** (a) Fold into M116 — rejected: M116 is backend error-registry; these are UI. (b) Gate the Models affordance with `hasScope` — rejected as default: the action is correctly tenant-scoped, so relabel, not restrict (guard only if §2.2 finds an admin-route call).
- **Patch-vs-refactor verdict:** a **patch** — presentation + one log line; no architecture move.

## Discovery (consult log)

- **Consults** — Indy directed each item this session: API-key copy verbose ("this is verbose"); Models platform-default reads as tenant-editable ("which is incorrect?") — resolved to labeling, backend correctly scoped; connectors swallow surfaced during the RSC trace. §2 authz-vs-label verdict recorded at EXECUTE (2.2 result).
- **Metrics review** — one ops log added (§3); no analytics/funnel playbook change.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs`: {empty at creation}
- **Deferrals** — {empty at creation}
