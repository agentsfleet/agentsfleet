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

# M160_001: The acceptance-e2e-dev suite goes green — seven root-caused failure clusters fixed at their source

**Prototype:** v2.0.0
**Milestone:** M160
**Workstream:** 001
**Date:** Aug 11, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — two shipped product defects (workspace switcher label, secret rotation) plus a dev-environment data hole are user-visible; the suite is the deploy gate's only end-to-end proof
**Categories:** API, UI
**Batch:** B1 — all seven sections are independent and run concurrently
**Branch:** feat/m159-otlp-runtime-metrics
**Test Baseline:** unit=3512 integration=589
**Depends on:** none (rides the same branch and Pull Request (PR) as M159_001 per Indy's instruction; no code dependency in either direction)
**Provenance:** LLM-drafted (Claude Fable 5, Aug 11, 2026) from a root-cause investigation of deploy-dev runs 31457489402 and 31458941845 (log + artifact + Playwright network-trace evidence; each cluster verified in source on `main`)
**Canonical architecture:** `docs/architecture/observability.md` §The four signal paths (unaffected surfaces; no architecture change)

---

## Overview

**Goal (testable):** the acceptance-e2e-dev job of the first deploy-dev run containing this merge reports 0 failed, with every fix landed at the defect's source rather than by loosening an assertion.

**Problem:** ten of sixty-four acceptance journeys fail against the development environment. Behind them sit seven distinct causes, four of which are real product or environment defects an operator can hit today: creating a workspace leaves the header switcher on a placeholder, rotating a secret answers "name already exists", the billing page error-loops for any tenant whose wallet row predates the current bootstrap, and every Clerk session-keeper tick fires its Frontend API (FAPI) calls twice.

**Solution summary:** one Zig change makes the signup replay converge the wallet invariant it already claims; three dashboard changes fix the switcher label, the rotate verb, and the session-keeper effect churn; three test-layer changes retire stale contracts (a bundle fixture missing the required `tools` key, an events-list field that moved to the detail route in m154, a `networkidle` wait plus a mutation-time blank-frame audit that both measure states no user ever sees).

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(api,app,e2e): green the acceptance suite by fixing its seven root causes
- **Intent (one sentence):** the acceptance suite becomes a trustworthy deploy gate again because the product bugs it caught are fixed and the assertions that measured nothing real are replaced with ones that do.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.
- **Restatement (Orly, at PLAN):** every one of the ten failures gets fixed where it actually broke — four in shipped product/server code, three in test contracts that had gone stale — so a green suite means the product works, not that the tests look away. `ASSUMPTIONS I'M MAKING:` (1) Indy's "same PR" instruction overrides the prior handoff's separate-stream note; (2) the dev wallet hole is healed through the replay code path, never by manual data repair; (3) the full-suite proof can only land on the post-merge deploy-dev run, so R6 grades after merge.

## Implementing agent — read these first

1. `src/agentsfleetd/state/signup_bootstrap.zig` — `replayExisting` returns without touching the wallet; `tenant_billing.insertStarterGrant` is idempotent (`ON CONFLICT (tenant_id) DO NOTHING` in `state/sql.zig`), which is what makes the replay-heal safe.
2. `ui/packages/app/components/layout/WorkspaceSwitcherMenu.tsx` — the `visibleWorkspaces` merge over `WorkspaceCreationProvider.createdWorkspaces` is the exact pattern the always-mounted trigger must adopt; commit `817c9a6e1` shows where the pre-split component did this.
3. `ui/packages/app/app/(dashboard)/settings/models/actions.ts` — `replaceSecretAction` there is the migrated PUT pattern (`b5e8e2430`) the secrets page never received.
4. `src/agentsfleetd/fleet_runtime/config_parser.zig` — `parseToolsField` proves `tools` is a required TRIGGER.md frontmatter key; the daemon's own fixtures in `config_markdown.zig` all carry it.
5. `src/agentsfleetd/http/handlers/fleets/events_payload_free_integration_test.zig` — asserts the events list carries no `request_json`; the e2e test must assert through the detail route instead.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/state/signup_bootstrap.zig` | EDIT | replay path converges the wallet invariant via the idempotent starter grant |
| `src/agentsfleetd/state/signup_bootstrap_test.zig` | EDIT | replay-heal coverage: missing wallet is restored, existing balance untouched |
| `src/agentsfleetd/state/tenant_billing.zig` | EDIT | gains the heal entry point the replay path calls |
| `src/agentsfleetd/state/tenant_billing_store.zig` | EDIT | the conflict-safe insert reports whether it inserted |
| `ui/packages/app/tests/secrets-actions.test.ts` | EDIT | thin-forwarder coverage for the replace action (istanbul 100% gate) |
| `ui/packages/app/components/layout/WorkspaceSwitcher.tsx` | EDIT | always-mounted trigger merges optimistically created workspaces before resolving its label |
| `ui/packages/app/components/layout/WorkspaceSwitcher.test.tsx` | CREATE | label resolution coverage incl. the routed-but-not-yet-listed workspace |
| `ui/packages/app/components/layout/WorkspaceCreationProvider.tsx` | EDIT | gains the read-only created-workspaces hook the trigger consumes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/actions.ts` | EDIT | gains the replace (rotate) server action wrapping the existing PUT client |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/EditSecretDialog.tsx` | EDIT | Rotate calls replace, not create; stale "create upsert" comment corrected |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/EditSecretDialog.test.tsx` | EDIT | asserts the replace action is called; conflict-on-rotate becomes a regression case |
| `ui/packages/app/lib/auth/client.ts` | EDIT | session-keeper effect keyed on session identity, not `user` object identity |
| `ui/packages/app/lib/auth/client.test.tsx` | EDIT | regression: a reload that returns a fresh user object schedules no second refresh |
| `ui/packages/app/tests/e2e/acceptance/fixtures/seed.ts` | EDIT | exports the shared bundle fixture builders (single source for TRIGGER.md/SKILL.md strings) |
| `ui/packages/app/tests/e2e/acceptance/runner-detail.spec.ts` | EDIT | drops its local trigger fixture (missing required `tools`) for the shared builder |
| `ui/packages/app/tests/e2e/acceptance/install-fleet-cli.spec.ts` | EDIT | drops its duplicated trigger fixture for the shared builder |
| `ui/packages/app/tests/e2e/acceptance/logs-detail.spec.ts` | EDIT | polls the events list by fields it still carries; message text asserted via the detail surface |
| `ui/packages/app/tests/e2e/acceptance/dashboard-performance.spec.ts` | EDIT | `networkidle` wait replaced by a content gate; blank-frame audit samples at paint boundaries |
| `ui/packages/app/tests/e2e/acceptance/fixtures/blank-frame-audit.ts` | CREATE | the paint-boundary audit, extracted so the unit lane drives the same logic |
| `ui/packages/app/tests/blank-frame-audit.test.ts` | CREATE | unit lane for the audit (vitest excludes tests/e2e/** as test files) |
| `ui/packages/app/tests/acceptance-suite-hygiene.test.ts` | CREATE | grep-backed invariants: shared fixture keys, no local builders, no networkidle |
| `ui/packages/app/tests/secrets-components.test.ts` | EDIT | dismiss-guard suite follows the rotate action to replace |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (no dead code: the retired local fixtures leave no orphans), **NLR** (touch-it-fix-it: the stale upsert comment and duplicated fixtures are corrected in the files being touched), **UFS** (the TRIGGER.md fixture string becomes one shared builder instead of three drifting copies), **ORP** (orphan sweep over the deleted local builders), **TST-NAM** (new test identifiers stay milestone-free).
- `dispatch/write_zig.md` — the replay-heal edit and its test: memory ownership of the replayed rows is unchanged; no new allocation paths.
- `dispatch/write_ts_adhere_bun.md` — dashboard edits stay inside existing component and server-action patterns; no raw-HTML or token drift.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` — the replay-heal logs through the existing `signup_replay` scope; any new record registers its event name.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — `signup_bootstrap.zig` and its test | no new allocations; existing transaction shape untouched; cross-compile both linux targets |
| PUB / Struct-Shape | no — no new public Zig surface; the heal is internal to an existing pub fn | not applicable |
| File & Function Length (≤350/≤50/≤70) | yes — several edited files sit near limits | edits are subtractive or few-line; verify with the harness, split only if a cap is actually crossed |
| UFS (repeated/semantic literals) | yes — the bundle fixture strings | one exported builder in `fixtures/seed.ts`; spec-local copies deleted |
| UI Substitution / DESIGN TOKEN | yes — `.tsx` edits | no new visual surface; existing design-system primitives only |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING yes (replay-heal record); others no | reuse the `state` scope with a named event; no error-code, lifecycle, or schema change |

## Prior-Art / Reference Implementations

- **Reference:** `ui/packages/app/app/(dashboard)/settings/models/actions.ts` + its dialogs — the completed POST→PUT rotate migration this spec extends to the secrets page.
- **Reference:** `ui/packages/app/components/layout/WorkspaceSwitcherMenu.tsx` — the `createdWorkspaces` merge the trigger re-adopts; the pre-`817c9a6e1` switcher is the behavioral baseline.
- **Reference:** `src/agentsfleetd/db/test_fixtures_provider.zig` — how test bootstrap composes `insertStarterGrant`, mirrored by the replay-heal test.

## Sections (implementation slices)

### §1 — The bundle fixture satisfies the validator it is testing against

`runner-detail.spec.ts` seeds fleet libraries with a TRIGGER.md missing the `tools` key that `parseToolsField` requires, so both runner-detail journeys die at arrangement with `UZ-BUNDLE-001`. Three near-identical trigger builders exist across the e2e tree; two of them differ only by accident. One exported builder ends the drift.

**Implementation default:** `fixtures/seed.ts` exports the trigger and skill builders it already owns (the empty-body skill variant moves in beside them); spec-local copies are deleted rather than patched.

- **Dimension 1.1** — the shared trigger builder carries every frontmatter key the daemon requires, proven against the parser's requirement set → Test `test_shared_trigger_fixture_satisfies_required_keys` — **DONE**
- **Dimension 1.2** — no acceptance spec defines a private trigger/skill markdown builder → Test `test_no_spec_local_bundle_builders_remain` — **DONE**

### §2 — The signup replay converges the wallet invariant

`bootstrapPersonalAccount`'s replay path returns existing rows without touching `billing.tenant_wallet`, so a tenant whose wallet row is missing (the dev fixture tenant lost its row across the m154 schema rebuild) 500s on `GET /v1/tenants/me/billing` forever — the billing page error-loops and two journeys fail. The e2e global-setup already replays the signup webhook every run, so a replay that heals makes the environment self-repairing with no new surface.

**Implementation default:** call the existing idempotent starter grant inside the replay path (both the fast path and the unique-violation race path resolve through `replayExisting`); the fail-loud 500 in the billing handler stays — the invariant is converged at bootstrap, not silenced at read.

- **Dimension 2.1** — replaying signup for a tenant with no wallet row restores the starter grant → Test `test_replay_heals_missing_wallet` — **DONE**
- **Dimension 2.2** — replaying signup for a tenant with a spent-down wallet leaves the balance untouched → Test `test_replay_never_tops_up_existing_wallet` — **DONE**
- **Dimension 2.3** — the heal logs a distinct event so an operator can see the invariant was converged → Test `test_replay_heal_emits_named_record` — **DONE**

### §3 — The switcher label survives the navigation that workspace creation triggers

The optimistic merge of a just-created workspace lives only in the lazily-mounted menu; the `router.push` that creation fires unmounts that menu, and the shared dashboard layout never re-fetches its workspace list, so the always-mounted trigger resolves the new workspace id against a stale list and falls back to "Current workspace" (regression introduced by the `817c9a6e1` component split). Two journeys and every real user creating a workspace hit this.

**Implementation default:** the trigger consumes the same `WorkspaceCreationProvider` context the menu already reads and merges `createdWorkspaces` before resolving its label — restoring the pre-split behavior without adding a fetch or a refresh race.

- **Dimension 3.1** — a routed workspace id found only in `createdWorkspaces` resolves to that workspace's name, not the placeholder → Test `test_trigger_label_resolves_created_workspace` — **DONE**
- **Dimension 3.2** — a routed id in the server-provided list keeps resolving as today → Test `test_trigger_label_resolves_listed_workspace` — **DONE**
- **Dimension 3.3** — a routed id known to neither list still shows the placeholder (deep link to a foreign workspace stays honest) → Test `test_trigger_label_placeholder_for_unknown_id` — **DONE**

### §4 — Rotating a secret replaces its value instead of claiming its name

Secret creation stopped upserting in `33fd024c9`; the models-settings dialogs were migrated to the PUT replace endpoint the next day (`b5e8e2430`), but the standalone secrets page's edit dialog still rotates via the create action, so every rotation of an existing secret answers `UZ-VAULT-005` and the dialog never closes.

- **Dimension 4.1** — the secrets page owns a replace server action wrapping the existing PUT client → Test `test_secrets_replace_action_calls_put` — **DONE**
- **Dimension 4.2** — Rotate in the edit dialog invokes replace, and a success closes the dialog → Test `test_rotate_invokes_replace_and_closes` — **DONE**
- **Dimension 4.3** — the name-conflict error surface remains reachable only from creation, never from rotation of an unchanged name → Test `test_rotate_cannot_conflict_on_own_name` — **DONE**

### §5 — The logs journey asserts through the surface that carries the payload

m154 (`cbd7a945b`) deliberately removed `request_json` from the events list (a daemon integration test asserts its absence); the payload moved to the single-event detail route. `logs-detail.spec.ts` still reads the field off list items and throws before asserting anything.

- **Dimension 5.1** — the journey locates the seeded event via fields the list still carries → Test `the logs journey (repaired in place, same test title)` — **DONE**
- **Dimension 5.2** — the message text is asserted through the detail surface the operator actually opens → covered by the same repaired journey's detail-dialog assertions — **DONE**

### §6 — The session keeper refreshes once per tick, and the test waits on content, not silence

`AuthSessionKeeper`'s effect depends on the `user` object identity; `user.reload()` produces a fresh identity, re-running the effect and doubling every FAPI call (network trace: paired `/touch`, `/me`, `/tokens` requests at each 45 s tick). Separately, `waitForLoadState("networkidle")` is structurally unreachable under `@clerk/testing`, whose route interceptor holds retried FAPI requests in-flight (trace shows three never-completed requests), and the heading gate above it is satisfied by the loading skeleton, which renders the same h1.

**Implementation default:** the effect re-arms on the stable session/user identifier (object held in a ref), preserving the documented 45 s cadence; the e2e wait becomes a content-specific locator from the loaded catalog plus the existing script-request reset.

- **Dimension 6.1** — a reload returning a new user object schedules no additional immediate refresh → Test `test_keeper_single_refresh_despite_identity_churn` — **DONE**
- **Dimension 6.2** — the 45 s visible-tab cadence and unmount cleanup are unchanged → Test `existing keeper suite stays green unmodified in intent` — **DONE**
- **Dimension 6.3** — the intent-loading journey gates on rendered catalog content, with no `networkidle` wait anywhere in the acceptance tree → Test `test_no_networkidle_in_acceptance_suite` — **DONE**

### §7 — A blank frame means a painted frame with no content

The blank-frame audit counts DOM states observed at mutation microtasks — states between React commits that the compositor never paints — so back-to-back navigations intermittently "fail" with a blank the user cannot see (`:79` in run 31411517048, `:133` in run 31458941845, both green in between). Sampling at paint boundaries keeps the real invariant (a painted empty `main` still fails) and removes the phantom.

- **Dimension 7.1** — the audit samples `main` at animation-frame timing; a textless state that never reaches a frame does not count → Test `both blank-frame journeys assert 0 via the paint-boundary audit` — **DONE**
- **Dimension 7.2** — a genuinely blanked `main` (content removed and left empty across a frame) still increments the count → Test `test_paint_boundary_audit_detects_real_blank` — **DONE**

## Interfaces

```
No wire surface changes.
  POST /v1/auth/identity-events/clerk — response shape unchanged (created:false
    on replay); the replay now additionally converges billing.tenant_wallet.
  PUT /v1/workspaces/{ws}/secrets/{name} — existing endpoint; the secrets page
    edit dialog becomes its second client (settings/models is the first).
  GET /v1/tenants/me/billing — unchanged; keeps failing loud on a missing row.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Replay heal races a concurrent replay | two webhook replays for one tenant | both inserts hit `ON CONFLICT DO NOTHING`; exactly one wallet row exists; both replays answer `created:false` |
| Replay for a healthy spent-down tenant | routine webhook redelivery | grant insert is a no-op; balance provably unchanged (`test_replay_never_tops_up_existing_wallet`) |
| Rotate against a deleted secret | secret removed between list and rotate | PUT answers not-found; dialog surfaces the error and stays open — asserted in the dialog unit suite |
| Created workspace absent from context | creation raced a full page reload | trigger falls back to the server list, then the placeholder — never a wrong name (Dimension 3.3) |
| Seeded event never arrives | queue delay in dev | the repaired logs poll times out loudly on the list read, naming the fleet — no TypeError masks the wait |
| A real blank frame ships | future shell regression | the paint-boundary audit still counts it and the journeys fail (Dimension 7.2) |

## Invariants

1. Every tenant that has completed signup bootstrap — first delivery or replay — has exactly one wallet row: enforced by the create transaction plus the replayed idempotent insert; proven by `test_replay_heals_missing_wallet` and the conflict-safe SQL.
2. The acceptance tree contains no `networkidle` wait: enforced by `test_no_networkidle_in_acceptance_suite` (a grep-backed test, failing on reintroduction).
3. TRIGGER.md fixture content exists in exactly one builder: enforced by the orphan sweep greps plus `test_no_spec_local_bundle_builders_remain`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| replay-heal log record (named at implementation, `state` scope) | ops | a signup replay inserts a wallet row that was missing | tenant id, balance granted | no email, token, or subject claim in the record | `test_replay_heal_emits_named_record` |

No product analytics event changes; no funnel changes, so no analytics playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_shared_trigger_fixture_satisfies_required_keys` | the exported builder's frontmatter parses with name, triggers, tools, budget all present |
| 1.2 | unit | `test_no_spec_local_bundle_builders_remain` | no acceptance spec file defines its own trigger/skill markdown builder |
| 2.1 | integration | `test_replay_heals_missing_wallet` | bootstrap, delete the wallet row, replay → row restored at starter balance, `created:false` |
| 2.2 | integration | `test_replay_never_tops_up_existing_wallet` | bootstrap, debit, replay → balance still the debited value |
| 2.3 | integration | `test_replay_heal_emits_named_record` | the heal path logs its named event exactly when a row was inserted |
| 3.1 | unit | `test_trigger_label_resolves_created_workspace` | routed id only in creation context → label is the created name |
| 3.2 | unit | `test_trigger_label_resolves_listed_workspace` | routed id in server list → label is the listed name |
| 3.3 | unit | `test_trigger_label_placeholder_for_unknown_id` | routed id known nowhere → placeholder text |
| 4.1 | unit | `test_secrets_replace_action_calls_put` | the action invokes the PUT client with workspace, name, and data |
| 4.2 | unit | `test_rotate_invokes_replace_and_closes` | Rotate submit → replace called, create never called, dialog closes on success |
| 4.3 | unit | `test_rotate_cannot_conflict_on_own_name` | rotate error path renders the PUT error, not the name-conflict copy |
| 5.1–5.2 | e2e | repaired `logs-detail` journey | seeded message found via list identity fields; text asserted in the detail surface |
| 6.1 | unit | `test_keeper_single_refresh_despite_identity_churn` | reload resolving a fresh user object → exactly one refresh per trigger |
| 6.2 | unit | existing keeper cadence suite | 45 s interval, visibility gating, and cleanup unchanged |
| 6.3 | unit | `test_no_networkidle_in_acceptance_suite` | zero `networkidle` occurrences under `tests/e2e/acceptance/` |
| 7.1 | e2e | both blank-frame journeys | paint-boundary audit reports 0 across the navigation walks |
| 7.2 | unit | `test_paint_boundary_audit_detects_real_blank` | emptying `main` across a frame increments the audit exactly once |
| regression | integration | existing signup bootstrap suite | fresh bootstrap still creates tenant, user, membership, workspace, wallet atomically |
| regression | unit | existing secrets dialog suites | create and rename flows keep their current verbs and assertions |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Shared fixture used everywhere; no local builders (§1) | `command grep -rn 'function triggerMd' ui/packages/app/tests/e2e/acceptance/ \| wc -l` | `1` | P0 | ✅ `1` (the shared builder in fixtures/seed.ts) |
| R2 | Replay heals the wallet hole (§2) | `make test-integration` (suite includes the replay-heal tests) | exit 0 | P0 | ✅ exit 0 (replay-heal tests in the DB-backed lane) |
| R3 | No networkidle wait survives (§6) | `command grep -rn 'networkidle' ui/packages/app/tests/e2e/acceptance/ \| wc -l` | `0` | P0 | ✅ `0` |
| R4 | Dashboard unit suites cover the three UI fixes (§3, §4, §6) | `cd ui/packages/app && bun run test` | exit 0 | P0 | ✅ `222 passed (222)` / `2248 passed` at 100% coverage |
| R5 | Diff stays inside Files Changed (union with M159_001's table on the shared branch) | `git diff --name-only origin/main...HEAD` | 0 paths missing from the two Files Changed tables | P0 | ✅ 0 paths missing (union with M159_001's table) |
| R6 | acceptance-e2e-dev goes green post-merge | `gh run list --workflow=deploy-dev.yml -L 1 --json databaseId,conclusion` then the acceptance job of that run | acceptance-e2e-dev conclusion `success` | P1 | ⏳ post-merge by design — graded via kishore-babysit-prs on the next deploy-dev run |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `✓ All unit lanes passed` (exit 0) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ `✓ All lint checks passed` (exit 0) |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | ✅ `✓ [agentsfleetd] Full integration suite passed` (exit 0) |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | ✅ all four greps 0 matches |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery. R6 is grade-after-merge by design: the suite only runs against the deployed development environment, deploy-dev fires on pushes to main, and branch runs never deploy — it is graded via `kishore-babysit-prs` on the post-merge run.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted; the retired builders are functions inside files that remain.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| spec-local `triggerMd` in runner-detail | `command grep -n 'function triggerMd' ui/packages/app/tests/e2e/acceptance/runner-detail.spec.ts` | 0 matches |
| spec-local `triggerMd` in install-fleet-cli | `command grep -n 'function triggerMd' ui/packages/app/tests/e2e/acceptance/install-fleet-cli.spec.ts` | 0 matches |
| `request_json` read in the events-list poll | `command grep -n 'request_json' ui/packages/app/tests/e2e/acceptance/logs-detail.spec.ts` | 0 matches |
| `waitForLoadState("networkidle")` | `command grep -rn 'networkidle' ui/packages/app/tests/e2e/acceptance/` | 0 matches |

## Out of Scope

- The two cli-acceptance-dev failures on run 31458941845 (`api-key delete` expectation, `steer-live` timeout) — different job, different tests; surfaced to Indy for a separate stream.
- Refreshing the dashboard layout's workspace list after creation (server-side revalidation) — the optimistic merge fixes the user-visible defect; a revalidation design touches the documented refresh/push race and earns its own review.
- The dev-environment wallet row itself — no manual data repair; the next CI run's global-setup replay heals it through §2's code path.
- Retiring `@clerk/testing`'s route-interception noise ("FAPI request failed" log spam) — harness-vendor behavior, not ours to patch here.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator creates a workspace and the header pill reads its name the moment the dialog closes; they rotate a secret and the dialog closes with the new value live; the billing page renders a balance.
2. **Preserved user behaviour** — workspace switching between existing workspaces, secret creation and rename, the billing page for healthy tenants, and every currently-passing journey stay exactly as they are.
3. **Optimal-way check** — each fix lands at the layer that broke: bootstrap convergence server-side, label resolution in the one component that renders it, the rotate verb at the call site, stale test contracts in the tests. The gap to unconstrained-optimal is the layout's stale workspace list (masked, not refreshed) — accepted and named in Out of Scope.
4. **Rebuild-vs-iterate** — iterate; every defect is a small drift from an intact pattern that already exists elsewhere in the codebase (the menu's merge, the models page's PUT action, the create-path grant).
5. **What we build** — one replay-heal, one context merge, one server action + dialog rewire, one effect-dependency fix, one shared fixture builder, two repaired test contracts, one paint-boundary audit.
6. **What we do NOT build** — no billing-read fallback, no layout revalidation scheme, no new endpoints, no fixture-tenant manual repair, no networkidle replacement waits elsewhere.
7. **Fit with existing features** — restores trust in the deploy gate that every other milestone (including M159 on this same branch) relies on; must not destabilize the models-settings secret dialogs that share the PUT path.
8. **Surface order** — UI-first, justified: the failing journeys are dashboard journeys; the one API change is invisible except through the dashboard it heals.
9. **Dashboard restraint** — nothing new is shown; the switcher shows a name it can prove, and the placeholder remains the honest fallback for unknown ids.
10. **Confused-user next step** — an operator who still sees "Current workspace" after this fix has a reproducible bug with a test naming the exact resolution order; the billing 500 keeps its explicit "bootstrap invariant violated" body pointing at the tenant.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** seven sections mapped one-to-one onto verified root causes, each independently landable and testable; no section blocks another.
- **Alternatives considered:** silencing the billing 500 with a zero-balance fallback (rejected — hides a real invariant violation); `router.refresh()` after workspace creation (rejected for now — races the push, documented hazard); relaxing the blank-frame assertion to `<= 1` (rejected — measures nothing).
- **Patch-vs-refactor verdict:** this is a **patch** set, because every defect is a local drift from a sound pattern; the one structural debt (layout workspace-list staleness) is named as follow-up rather than mud-patched here.

## Discovery (consult log)

- **Consults** — Indy's session instruction placed this workstream on M159_001's branch and PR ("ensure the acceptance is fixed in the same PR as well"), overriding the prior handoff's separate-stream note. Root causes were verified against source, CI artifacts, and the Playwright network trace before this spec was authored (the trace disproved the initial "tight Clerk refresh loop" hypothesis — the loop is a bounded double-fire; the networkidle blocker is the testing proxy's held-open requests).
- **Test Delta** — shares the branch ledger with M159_001: unit 3512→3530 (+18); this workstream's own additions are the three replay-heal integration tests plus twelve dashboard/hygiene unit tests.
- **Metrics review** — one operational log record added (`signup_replay_wallet_healed`); no product analytics events, no funnel change, no analytics playbook update required.
- **Skill-chain outcomes** — unit and integration coverage authored inside the workstream (every fix carries its regression test; the istanbul 100% gate stays green); gstack `/review` run at REVIEW (findings below); `kishore-babysit-prs` runs post-push and grades R6 on the next deploy-dev run.
- **Out-of-scope surfaced to Indy** — the two cli-acceptance-dev failures on run 31458941845 (`api-key delete` UZ-APIKEY-003 expectation, `steer-live` timeout) are a different job and not covered here.
- **Deferrals** — none.
