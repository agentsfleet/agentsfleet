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

# M134_002: Retire a revoked runner, and make the model catalogue tell the truth

**Prototype:** v2.0.0
**Milestone:** M134
**Workstream:** 002
**Date:** Jul 19, 2026
**Status:** DONE
**Priority:** P1 — a fresh tenant is shown a green "Active" default that does not exist, and their first fleet run fails; revoked runners accumulate with no way to retire them
**Categories:** API, INFRA, UI
**Batch:** B1 — standalone; shares a branch with M134_001 (loader copy) but no files
**Branch:** feat/m134-loading-verbs
**Test Baseline:** unit=2798 integration=369
**Depends on:** none
**Provenance:** agent-generated (pre-spec, Indy chat session Jul 19, 2026 — runner-key trash affordance, Model Library platform-default report, seeding request)
**Canonical architecture:** `docs/architecture/` — no flow-defining change; no stream, queue, namespace or schema *shape* is introduced (the one migration is a privilege grant)

---

## Overview

**Goal (testable):** a revoked runner can be deleted from the dashboard and nowhere else; and a seeded model catalogue lets a tenant pick a named provider instead of being forced onto the OpenAI-compatible escape hatch.
**Problem:** two unrelated reports from the same session. (1) Revoking a runner leaves a dead row with zero affordances and no way to remove it, while API keys have had revoke-then-delete all along. (2) The Models page shows a green **Active** badge for a platform default that was never configured, suppressing the very warning that would have said so — and because `core.model_library` ships empty, a tenant cannot activate any named provider at all, only `openai-compatible`.
**Solution summary:** add `DELETE /v1/fleets/runners/{id}` gated on the runner already being revoked, mirroring the API-key lifecycle, plus the missing `DELETE` privilege the grant never had. Separately, correct the platform-default badge, hide the platform row from tenants it cannot serve, resolve a custom endpoint's context cap from the catalogue by model id, retitle unpriced tenant rows to "Billed by provider", and ship a curated allowlist plus `make seed-models` so the catalogue arrives populated instead of empty.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat: runner delete plus a model catalogue that is seeded and honest
- **Intent (one sentence):** stop the product lying about a configured default, stop forcing every tenant through the custom-endpoint hatch, and let an operator retire a runner they have already revoked.
- **Handshake** — restated: delete is the *lesser* action (revoke is the destructive one), so it reuses revoke's scope and only ever applies after revoke. The catalogue is a gate, not a display cache, so seeding it is a functional change, not cosmetic. `ASSUMPTIONS I'M MAKING: 1. Revoked-only is the whole delete guard — no lease check (Indy's call, verified: fence checks fail closed on a missing lease row). 2. Rates seed at the standard/upper tier where a model is dual- or context-tiered. 3. The seed is not a migration — rates are operational data. 4. Tenant-visible rates copy must not imply agentsfleet is charging a self-managed tenant.`

## Implementing agent — read these first

1. `src/agentsfleetd/http/handlers/api_keys/tenant.zig` — `innerDeleteApiKey` is the revoke-then-delete reference: the single-CTE idiom that separates 404 from 409 without a race.
2. `src/agentsfleetd/http/handlers/fleet/runner_patch.zig` — the runner operator plane's module shape, and why runner rows are scope-gated rather than tenant-scoped.
3. `src/agentsfleetd/state/tenant_provider_resolver.zig` + `handlers/tenant_provider.zig` — the two-branch resolution and the activation gate the catalogue backs.
4. `schema/003_model_library.sql` — the rate columns, the `(provider, model_id)` unique key, and the standing note that token rates apply only under platform-managed posture.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/031_fleet_runners_delete_grant.sql` | CREATE | The serve tier had SELECT/INSERT/UPDATE only; DELETE would 500 on a privilege error. |
| `schema/embed.zig` | EDIT | Register migration 031. |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | `UZ-RUN-016` — active runner must be revoked first. |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Named constant for the new code. |
| `src/agentsfleetd/http/handlers/fleet/runner_delete.zig` | CREATE | The handler; revoked-only CTE guard. |
| `src/agentsfleetd/http/handlers/fleet/runner_delete_test.zig` | CREATE | Guard-coupling and registry-shape unit tests. |
| `src/agentsfleetd/http/route_table_invoke_runner.zig` | EDIT | PATCH/DELETE method switch on the existing route variant. |
| `src/agentsfleetd/state/model_rate_cache.zig` | EDIT | `contextCapForModel` — cap by model id across providers. |
| `src/agentsfleetd/http/handlers/tenant_provider.zig` | EDIT | Custom endpoints borrow the catalogue cap instead of pinning zero. |
| `src/agentsfleetd/state/model_library_seed_integration_test.zig` | CREATE | Proves the seeded catalogue satisfies the platform-default gate. |
| `src/agentsfleetd/tests.zig` | EDIT | Register the new integration test. |
| `ui/packages/app/lib/api/runners.ts` | EDIT | `deleteRunner` client. |
| `ui/packages/app/app/(dashboard)/admin/runners/actions.ts` | EDIT | `deleteRunnerAction`, scoped `runner:write`. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerListCells.tsx` | EDIT | Trash affordance on revoked rows; retire the stale "no runner delete" comment. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerDialogs.tsx` | EDIT | Confirm copy shape factored so delete reuses it without gaining a PATCH action. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` | EDIT | Delete state, confirm, and refetch-on-success. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable.tsx` | EDIT | Correct `isDefaultLive`; hide the platform row when it cannot serve. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx` | EDIT | "Billed by provider" for unpriced tenant rows. |
| `ui/packages/app/tests/runners-list-actions.test.ts` | EDIT | Delete affordance, success, and refusal coverage. |
| `ui/packages/app/tests/models-registry-table.test.tsx` | EDIT | Re-pin the platform-row contract; add the fresh-tenant regression. |
| `scripts/model-library-allowlist.json` | CREATE | The curated catalogue input — 77 rows, 16 providers, rates and provenance. |
| `scripts/seed-models.mjs` | CREATE | Fetch, diff against live, emit and apply. |
| `samples/fixtures/model-library/pioneer.json` | CREATE | Committed API snapshot so the integration lane needs no network. |
| `samples/fixtures/model-library/openrouter.json` | CREATE | Same, and the per-token conversion path. |
| `samples/fixtures/model-library/seed.sql` | CREATE | Committed, byte-stable SQL the Zig seed tests self-apply — CI's zig container has neither node nor psql. Regenerated via `--emit-fixture-sql`. |
| `ui/packages/app/lib/api/runners.test.ts` | EDIT | Cover `deleteRunner` (204 + refusal passthrough) — the app enforces a 100% coverage gate. |
| `ui/packages/app/tests/runners-actions.test.ts` | EDIT | Cover `deleteRunnerAction` fail-closed and happy paths — same gate. |
| `make/dev.mk` | EDIT | `make seed-models`. |
| `make/test-integration.mk` | EDIT | Notes that catalogue seeding is self-serve in the Zig tests; no lane seed step (the CI zig container has no node/psql). |
| `public/openapi/paths/fleet.yaml` | EDIT | Document the new DELETE operation (204/404/409, `UZ-RUN-016`). |
| `src/agentsfleetd/http/fleet_runner_events_integration_test.zig` | EDIT | Delete lifecycle over live HTTP: 409 while live → revoke → 204 + cascade → 404. |
| `src/agentsfleetd/http/handlers/admin/model_library_admin_integration_test.zig` | EDIT | Suite-private provider names (`m100fw`/`m100an`) so its cleanup stops wholesale-deleting seeded catalogue rows. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **UFS** (error codes, copy strings and rate constants are named, never inline), **NDC** (no dead code), **NLR** (the stale "there is no runner delete" comment is corrected on touch, not left), **ORP** (orphan sweep).
- **`dispatch/write_zig.md`** — memory safety and `PgQuery` drain discipline on the new handler; cross-compile both linux targets.
- **`dispatch/write_sql.md`** — Schema Table Removal Guard on migration 031 (a GRANT, no shape change).
- **`dispatch/write_ts_adhere_bun.md`** — design-system primitives for the new trash affordance; no arbitrary utilities.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — the new DELETE verb and its 204/404/409 shape.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | `PgQuery.from(...)` + `defer q.deinit()` on the one query; cross-compile both linux targets. |
| PUB / Struct-Shape | yes | One `pub fn` entry point plus a private `Outcome` enum — a tagged result, not a bool pair. |
| File & Function Length (≤350/≤50/≤70) | yes | New handler is well under 350; every fn under 50. |
| UFS (repeated/semantic literals) | yes | `ERR_RUNNER_MUST_REVOKE_FIRST`, `RATES_NOT_APPLICABLE`, `SEEDED_ROW_COUNT` all named. |
| UI Substitution / DESIGN TOKEN | yes | `IconAction` + `Trash2Icon`, existing `destructive` variant; no new class strings. |
| ERROR REGISTRY | yes | `UZ-RUN-016` registered via `eu()` (dashboard-reachable, carries a user message). |
| SCHEMA | yes | Migration 031 is a GRANT only; `schema/embed.zig` updated; no table or column touched. |
| LOGGING / LIFECYCLE | yes | `log.debug` on the delete path, scoped `.fleet_runner_delete`. |

## Prior-Art / Reference Implementations

- **Reference:** `handlers/api_keys/tenant.zig` — the revoke-then-delete lifecycle, the CTE that distinguishes 404 from 409, and the `BanIcon`/`Trash2Icon` glyph split its UI established.
- **Reference:** `route_table_invoke_api_keys.zig` — one route variant, method fanned out in invoke; avoids touching the exhaustive `route_admission.zig` classifier.
- **Divergence:** the API-key delete requires a strictly higher scope than its PATCH. Runner delete does **not** — revoke is already terminal and already `runner:write`, so gating the lesser action higher would be backwards.

## Sections (implementation slices)

### §1 — A revoked runner can be retired

Delete exists only after revoke, so the destructive decision stays with revoke and delete merely removes the record. **Implementation default:** one CTE round-trip rather than SELECT-then-DELETE, because an operator revoking concurrently would race the two statements.

- **Dimension 1.1** — DONE — the serve role can delete a runner row at all → Test `test_delete_grant_present`
- **Dimension 1.2** — DONE — a revoked runner deletes and returns 204 → Test `test_delete_revoked_runner`
- **Dimension 1.3** — DONE — a live runner is refused with the revoke-first conflict → Test `test_delete_live_runner_conflicts`
- **Dimension 1.4** — DONE — an unknown id is 404, distinct from the 409 → Test `test_runner_not_found_distinct_from_conflict`
- **Dimension 1.5** — DONE — the SQL guard stays coupled to the enum spelling → Test `test_revoked_tag_matches_guard`

### §2 — The dashboard offers delete exactly where it works

**Implementation default:** delete gets its own config rather than joining `ACTION_CONFIG`, whose key type is the three PATCH verbs; widening it would loosen an exhaustive type two call sites rely on.

- **Dimension 2.1** — DONE — trash appears on revoked rows only, never beside revoke → Test `test_delete_affordance_revoked_only`
- **Dimension 2.2** — DONE — a successful delete refetches so page counts stay honest → Test `test_delete_refetches_page`
- **Dimension 2.3** — DONE — a refusal keeps the row and surfaces the reason → Test `test_delete_refusal_keeps_row`

### §3 — The platform default stops lying

A fresh tenant on a fresh install saw a green **Active** badge for a default that could not exist, and the badge suppressed the warning. **Implementation default:** the badge tests both conditions — wins resolution *and* exists.

- **Dimension 3.1** — DONE — no active entry and no default warns rather than claiming Active → Test `test_warns_instead_of_claiming_active`
- **Dimension 3.2** — DONE — a self-managed tenant with no default sees no platform row at all → Test `test_platform_row_hidden_when_unusable`

### §4 — Rates copy stops implying a charge we do not make

- **Dimension 4.1** — DONE — an unpriced tenant row reads "Billed by provider", not a failure → Test `test_self_managed_rate_copy`

### §5 — A custom endpoint learns its context window

A context window belongs to the model; rates belong to the (host, model) pair. **Implementation default:** resolve the cap by model id across any provider, and never borrow the rate with it.

- **Dimension 5.1** — DONE — a custom endpoint naming a catalogued model gets that model's cap → Test `test_custom_endpoint_borrows_cap`
- **Dimension 5.2** — DONE — an uncatalogued model still activates on the sentinel → Test `test_unknown_custom_model_still_activates`

### §6 — The catalogue arrives populated

The catalogue is the provider list, the model list, and the activation gate — an empty one forces every tenant onto `openai-compatible`. **Implementation default:** a script, not a migration, because rates are mutable operational data and migrations are immutable history.

- **Dimension 6.1** — DONE — the seed produces the allowlisted rows → Test `test_seed_row_count`
- **Dimension 6.2** — DONE — the platform-default gate accepts every shipped combination → Test `test_subscribed_combos_seeded`
- **Dimension 6.3** — DONE — the gate still refuses an uncatalogued pair → Test `test_gate_still_refuses_unknown`
- **Dimension 6.4** — DONE — rates land in nanos, including the per-token source → Test `test_rate_units`
- **Dimension 6.5** — DONE — re-running corrects drift without duplicating → Test `test_seed_idempotent`

## Interfaces

```
DELETE /v1/fleets/runners/{id}          scope: runner:write
  204  deleted
  404  UZ-RUN-014  no runner with this id
  409  UZ-RUN-016  active runner must be revoked before deletion
  (PATCH on the same path is unchanged)

// state/model_rate_cache.zig
pub fn lookup_context_cap(model: []const u8) ?u32   // cap only, any provider

// scripts/seed-models.mjs
node scripts/seed-models.mjs [--fixtures] [--apply]
  default            diff against the live catalogue, report, write nothing
  --apply            apply via psql (needs DATABASE_URL)
  --fixtures         read api-source providers from samples/fixtures/model-library
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Delete a live runner | Operator skips revoke | 409 `UZ-RUN-016`; row stays, confirm dialog holds the reason. |
| Delete an unknown id | Stale list, concurrent delete | 404 `UZ-RUN-014`, distinct from the conflict. |
| Missing DELETE privilege | Migration 031 not applied | Would surface as a bare 500 — which is why the grant ships with the handler. |
| Concurrent revoke during delete | Two operators | Single CTE decides atomically; no SELECT-then-DELETE window. |
| Cascade removes billing residue | A revoked runner still holding a lease row | Accepted: the final partial slice cannot settle, under-billing in the tenant's favour on an already-dead runner. Documented, not silently absorbed. |
| Seed applied, daemon not restarted | Direct SQL does not rebuild the rate cache | The script prints the restart requirement; until then the gate reads the old catalogue. |
| Seed run without DATABASE_URL | Fresh-install mode | Diffs against an empty catalogue and writes nothing; `--apply` fails loudly. |
| Upstream model retired | Allowlisted id vanished from an API source | Reported per-row and skipped, never silently dropped. |

## Invariants

1. Only a revoked runner is deletable — enforced in SQL (`c.admin_state = $2`), not by the caller, so a direct API call cannot bypass the UI.
2. The delete guard's SQL literal tracks the Zig enum — enforced by `test_revoked_tag_matches_guard`, which fails on a rename rather than letting the guard silently match nothing.
3. The announced platform-default state matches reality — enforced by testing both `isDefaultLive` conditions, with a regression test for the fresh-tenant case.
4. A cap borrowed from the catalogue never carries a rate with it — enforced by `lookup_context_cap` returning `?u32`, structurally unable to return a `ModelRate`.
5. Re-seeding is idempotent — enforced by `ON CONFLICT (provider, model_id) DO UPDATE` plus a row-count assertion after two passes.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `runner_deleted` (log.debug, `.fleet_runner_delete`) | ops | a runner record is retired | runner_id | no token or credential material | `test_delete_revoked_runner` |
| not applicable — model catalogue | not applicable | copy and gating corrections add, rename and remove no product event | none | none | none |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_delete_grant_present` | Migration 031 applied; a DELETE as `api_runtime` does not raise a privilege error. |
| 1.2 | integration | `test_delete_revoked_runner` | Revoked runner → 204; row and its cascaded events are gone. |
| 1.3 | integration | `test_delete_live_runner_conflicts` | `admin_state=active` → 409 `UZ-RUN-016`; row survives. |
| 1.4 | unit | `test_runner_not_found_distinct_from_conflict` | `UZ-RUN-014` is 404, `UZ-RUN-016` is 409, and they are different codes. |
| 1.5 | unit | `test_revoked_tag_matches_guard` | `@tagName(AdminState.revoked) == "revoked"`. |
| 2.1 | unit | `test_delete_affordance_revoked_only` | Active/cordoned/draining/drained rows have no Delete and do have Revoke; the revoked row inverts both. |
| 2.2 | unit | `test_delete_refetches_page` | On success the list refetches; the deleted host disappears, the survivor stays. |
| 2.3 | unit | `test_delete_refusal_keeps_row` | `UZ-RUN-016` → dialog stays open, row still rendered. |
| 3.1 | unit | `test_warns_instead_of_claiming_active` | No entries + no default → no "Active" badge, warning shown. |
| 3.2 | unit | `test_platform_row_hidden_when_unusable` | Active entry + no default → no platform row, no warning, no Use-default button. |
| 4.1 | unit | `test_self_managed_rate_copy` | An unpriced tenant row renders "Billed by provider". |
| 5.1 | integration | `test_custom_endpoint_borrows_cap` | `openai-compatible` + a catalogued model id resolves that model's cap, not 0. |
| 5.2 | integration | `test_unknown_custom_model_still_activates` | `openai-compatible` + an unknown model still activates on the sentinel. |
| 6.1 | integration | `test_seed_row_count` | Per-provider row counts match the allowlist (anthropic floor-asserted — another suite legitimately inserts under it); an empty catalogue is a hard failure, not a skip. |
| 6.2 | integration | `test_subscribed_combos_seeded` | `capFor` answers for pioneer/fireworks/openrouter/kimi/glm combinations. |
| 6.3 | integration | `test_gate_still_refuses_unknown` | `capFor` returns null for an uncatalogued provider and for an uncatalogued model. |
| 6.4 | integration | `test_rate_units` | `kimi/kimi-k3` input = 3_000_000_000 nanos; `openrouter/anthropic/claude-opus-4.8` = 5_000_000_000, proving the per-token ×1e6 chain. |
| 6.5 | integration | `test_seed_idempotent` | Seed → injected drift → re-seed leaves per-provider counts unchanged, and the rate-units row proves the UPSERT corrected the drift. |
| regression | unit | `runners-list-actions.test.ts` (existing) | Cordon/drain/revoke and the activity dialog are unchanged. |
| regression | unit | `models-registry-table.test.tsx` (existing) | Sorting, pinning and entry rendering unchanged. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Delete is offered only on revoked runners (§2) | `cd ui/packages/app && bunx vitest run tests/runners-list-actions.test.ts` | exit 0 | P0 |  ✅ `Tests 18 passed (18)` (runners-list-actions) |
| R2 | The default badge matches reality (§3, §4) | `cd ui/packages/app && bunx vitest run tests/models-registry-table.test.tsx` | exit 0 | P0 |  ✅ `Tests 29 passed (29)` (models-registry-table) |
| R3 | The seeded catalogue satisfies the platform-default gate (§6) | `make test-integration` | exit 0 | P0 |  ✅ `All integration tests passed` (exit 0; incl. drift-injection refresh + delete lifecycle) |
| R4 | Seeding is diff-first and writes nothing by default | `node scripts/seed-models.mjs --fixtures` | reports a diff, exits 0, no DB write | P1 |  ✅ `77 new · 0 changed`, exit 0, no write |
| R5 | Diff stays inside Files Changed | `git diff --cached --name-only origin/main` | 0 paths missing from the Files Changed table | P0 |  ✅ staged diff = Files Changed table + the two specs |
| S1 | Zig unit suite passes | `make test-unit-agentsfleetd` | exit 0 | P0 |  ✅ `unit=2802`, reachability 475 files |
| S2 | App + design-system suites pass | `cd ui/packages/app && bunx vitest run` | exit 0 | P0 |  ✅ app `1642 passed`, design-system `461 passed` |
| S3 | Lint clean | `make lint-apps-ds-ctl` | exit 0 | P0 |  ✅ `Lint passed` ×3 |
| S4 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 |  ✅ both linux targets build |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 |  ✅ `no leaks found` |
| S6 | CONFORM gates green | `make harness-verify` | ALL GATES GREEN | P0 |  ✅ `ALL GATES GREEN` (9 gates) |
| S7 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 |  ✅ `0 matches` both greps |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| the "there is no runner delete" claim | `grep -rn "no runner delete" ui/ src/` | 0 matches |
| the sentinel-only custom cap path | `grep -rn "CUSTOM_ENDPOINT_CAP_UNKNOWN" src/ \| grep -v "orelse CUSTOM_ENDPOINT_CAP_UNKNOWN\|const CUSTOM_ENDPOINT_CAP_UNKNOWN"` | 0 matches |

## Out of Scope

- **A tier column on `core.model_library`.** Three seeded models double their rate above a context threshold and the table holds one triple; they are seeded at the upper tier and the gap is recorded in the allowlist. A tier-aware schema is its own spec.
- **Rate provenance columns.** Source URLs and the verification date live in the allowlist rather than the table; adding columns is a schema change this spec does not need.
- **Settling a runner's final billing slice before delete.** The cascade drops an unsettled tail slice, under-billing in the tenant's favour on an already-revoked runner.
- **Tenant/organization terminology.** A separate, larger surface with a breaking API half.
- **`PROVIDER_LABELS` entries for the newly seeded providers.** They render as raw slugs, which is cosmetic.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a new tenant opens Models, picks **kimi** from a real dropdown, pastes a key, and runs a fleet — without ever learning what "OpenAI-compatible" means. And an operator who revoked a runner last week can finally clear the row.
2. **Preserved user behaviour** — cordon/drain/revoke unchanged; existing custom endpoints keep working, including uncatalogued models; platform-managed billing unchanged; already-seeded catalogues are never clobbered by a re-run.
3. **Optimal-way check** — the direct path. The gap to optimal is the tiered-rate models, seeded at the upper tier because under-billing is silent and unrecoverable while over-billing is visible and refundable.
4. **Rebuild-vs-iterate** — iterate. The resolution logic and lifecycle are right; what was missing was a populated catalogue, one honest boolean, and one HTTP verb.
5. **What we build** — a delete endpoint plus its grant and affordance; a corrected badge; a cap lookup; honest rates copy; and a curated allowlist with a diff-first seeder.
6. **What we do NOT build** — tier columns, provenance columns, terminology rename, pretty provider labels, lease-settling on delete.
7. **Fit with existing features** — compounds with the API-key lifecycle whose vocabulary it borrows, and with the platform-default flow, which becomes reachable once the catalogue is non-empty. Must not destabilise platform-managed billing.
8. **Surface order** — UI and API together: the endpoint is useless without the affordance, and the affordance cannot exist without the endpoint.
9. **Dashboard restraint** — no control appears before it can succeed: delete shows only where the daemon will accept it, and the platform row hides where it cannot serve.
10. **Confused-user next step** — every refusal names its own remedy: the 409 says revoke first, the empty-default note says a default is not configured.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections split by surface — endpoint, affordance, badge truth, copy, cap resolution, catalogue contents. Each is independently testable and independently revertible.
- **Alternatives considered:** (a) a new `runner:admin` scope for delete — rejected: revoke is already terminal at `runner:write`, and a new scope means re-granting every existing operator in Clerk. (b) a new `.fleet_runner_delete` route variant — rejected: the method switch avoids touching the exhaustive `route_admission.zig` classifier. (c) seeding the catalogue via a migration — rejected: rates change monthly, and immutable history is the wrong home for mutable operational data. (d) blocking delete while leases are live — rejected after verifying mutual exclusion lives in `runner_affinity.fencing_seq` and every fence check fails closed on a missing lease row.
- **Patch-vs-refactor verdict:** a **patch**. No structure changes: one new handler beside an existing one, one new boolean condition, one new cache lookup, and data that was always meant to be there.

## Discovery (consult log)

- **Consults** — Lease-safety consult with Indy (Jul 19, 2026): Indy challenged the proposed lease guard on the grounds that revocation already makes leases inert. Verified and Indy was right, with a sharper justification: exclusion lives in `runner_affinity.fencing_seq`, not the lease row, and every fence check drives off the lease row via INNER JOIN, so a missing row is uniformly a rejection. Guard reduced to revoked-only. Scope consult: `runner:write` reused rather than adding `runner:admin`. Rates consult: standard (not introductory) Sonnet 5 pricing and upper-tier seeding for context-tiered models, on the under-vs-over-billing asymmetry.
- **Metrics review** — no analytics/funnel playbook update required: one operational `log.debug` added, no product event added, renamed or removed.
- **Skill-chain outcomes** — pending: `/write-unit-test`, `/review`, `kishore-babysit-prs`. An independent Fable 5 adversarial review ran on the full diff before PR; dispositions below.
- **Fable 5 review dispositions** — (1) *Delete strands the in-flight event + debit* (raised P1): **refuted by code** — `assign.zig acquireFresh` reads the stable consumer's PEL first, explicitly for "sweep-recovered strands", so the un-acked event is re-delivered on the next claim; delete costs only the reclaim fast-path in the pre-sweep window, and the billing-tail residue was already an acked Failure Mode. (2) *Global `COUNT(*)` order-coupled* (P1): **confirmed by the first lane run and fixed** — count scoped to allowlist providers; the admin suite's cleanup renamed to suite-private providers so it stops deleting seeded rows. (3) *Double-seed vacuous* (P2): **confirmed, fixed** — the lane now injects rate drift between passes and the rate-units test proves the UPSERT corrected it. (4) *Position-derived uid collides across refreshes* (P2): **confirmed, fixed** — uid is now a content hash of `(provider, model_id)` alone. (5) *`contextCapForModel` arbitrary on cross-host disagreement* (P2): **confirmed, fixed** — deterministic `@min` across matches; under-budgeting wastes headroom, over-budgeting fails mid-run. (6) *DELETE scope deviates from the api-key precedent* (P2): **kept deliberately** — no `runner:admin` scope exists; minting one is a 5-file sweep plus re-granting every operator in Clerk. Deviation surfaced to Indy in-session (Jul 19, 2026) with the precedent named; `runner:write` stands unless Indy asks for the tier. (7) *OpenAPI missing the delete op; no executed handler coverage* (P2): **confirmed, fixed** — `delete_fleet_runner` documented; live-HTTP lifecycle test added. (8) P3s: emit now writes only drifted rows (preserving `updated_at_ms` as a signal); fixtures mode logs honestly; `UZ-RUN-016` user copy tightened. The entry-row `libraryRateFor` fallback is retained: Indy ruled the informational display fine ("the user has selected it, so its not an issue").
- **CI cycle (post-PR)** — two red jobs on the first push, both owned and fixed. (1) `test-unit-app`: the 100% coverage gate caught four uncovered paths this PR added (`deleteRunner` client, `deleteRunnerAction`, the delete-dialog dismiss closure, the verb-picker fallback arm, plus `formatRates(null)` left reachable only from the admin path) — all covered, gate green locally. (2) `test-integration`: `node: not found` in the ci-zig container — the lane's seed step assumed dev-machine tooling. Redesigned: the seed tests SELF-apply a committed byte-stable SQL fixture (`--emit-fixture-sql`, timestamp pinned to the allowlist's verified_at) over their own pg connection, the drift-injection proof moved inside the rate-units test, and the lane's node/psql/docker coupling was deleted outright. Also fixed en route: the per-provider count assertions replaced a summed count that absorbed another suite's legitimate `(anthropic, claude-sonnet-4-6)` row, and two inline queries were helper-scoped after a pg ConnectionBusy (deferred PgQuery drains holding the connection across statements).
- **Deferrals** — none.
