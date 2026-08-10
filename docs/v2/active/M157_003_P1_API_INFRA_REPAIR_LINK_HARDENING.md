<!--
SPECIFICATION AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill; after filling, DELETE every template guidance comment.
- No time estimates, effort columns, completion percentages, implementation
  dates, or assigned owners.
- Priority is the only sizing signal; Dependencies are the only sequencing
  signal.
-->

# M157_003: Repair linkage survives ingress, replay, and merge

**Prototype:** v2.0.0
**Milestone:** M157
**Workstream:** 003
**Date:** Aug 10, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the default GitHub App ingress misses repair linkage, preview runs overwrite each other, and one approval can fund unbounded token mints
**Categories:** API, INFRA
**Batch:** B1 — shared linkage and provenance; B2 — immutable run and merge history; B3 — approval spend ceiling
**Branch:** `feat/m157-repair-loop`
**Public docs branch:** `chore/m157-repair-loop-changelog` in `~/Projects/docs`
**Base branch:** `main` in both repositories
**Test Baseline:** unit=3512 integration=589
**Depends on:** M157_002 (ships the write mint, slot 830 linkage, and `incident-repairer`)
**Provenance:** agent-generated from Pull Request (PR) #591 Session notes and Indy's Aug 10, 2026 direction
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md`

---

## Overview

**Goal (testable):** A repair PR opened through either GitHub route records one provenance-checked incident link; every repair-branch workflow result is retained independently; a merged PR records GitHub's exact merged commit hash; and one approval funds no more token mints than its declared ceiling.

**Problem:** The shared GitHub App ingress drops repair traffic before linkage. Slot 830 then stores only one mutable deploy value, so a later lint run can replace a deploy result and an early run disappears. The branch prefix is trusted without installation, repository, or author proof. Separately, an approved gate can mint on every retry.

**Solution summary:** Both GitHub routes call one linkage arm before repair traffic is dropped. The arm resolves the repair Fleet from the incident event and verifies provenance. Slot 831 stores append-only workflow history, slot 832 records the exact `merge_commit_sha` returned by a merged-PR webhook, and slot 833 makes approval spend atomic and bounded. Preview rows remain evidence; only M157_004 may correlate the merged commit with production.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m157): close repair incidents on correlated production evidence`
- **Intent:** make repair history trustworthy before any verifier is allowed to act on it.
- **Orly restatement:** A branch run is preview evidence. This workstream records it without overwriting, proves whose repair PR opened, pins the provider-returned merged commit, and caps approval-funded mints; it does not declare production fixed.
- **ASSUMPTIONS I'M MAKING:** GitHub remains the repository authority; the human still merges; slot 830 is frozen migration history; M157_004 consumes the merged hash but owns production-result routing and the verifier.

## Implementing agent — read these first

1. `docs/architecture/scenarios/production-deploy-repair.md` — canonical three-Fleet order and closure boundary.
2. `src/agentsfleetd/http/handlers/webhooks/github_repair_link.zig` — current per-Fleet interception and mutable stamp.
3. `src/agentsfleetd/http/handlers/ingress/github.zig` — shared ingress where linkage must run before normal routing.
4. `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_app.zig` — repair traffic stays excluded from ordinary Fleet wake-up.
5. `src/agentsfleetd/http/handlers/runner/credentials_mint_write_gate.zig` — durable approval read that gains atomic spend.
6. `schema/830_repair_pr_links.sql` — frozen shipped history; new migrations extend behavior without editing it.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/831_repair_run_results.sql` | CREATE | Append-only workflow rows with provider run and head-commit identity |
| `schema/832_repair_pr_merge_correlation.sql` | CREATE | Add merged commit hash and merged time to slot 830 rows |
| `schema/833_fleet_approval_gates_spend.sql` | CREATE | Add nullable spend count and ceiling for provisioned databases |
| `schema/embed.zig` | EDIT | Register slots 831–833 in both migration lists |
| `src/agentsfleetd/state/repair_pr_links.zig` | EDIT | Insert link, record merged commit once, remove mutable deploy stamp |
| `src/agentsfleetd/state/repair_run_results.zig` | CREATE | Insert immutable workflow results idempotently |
| `src/agentsfleetd/state/sql.zig` | EDIT | Statements for repair links and run history |
| `src/agentsfleetd/http/handlers/webhooks/github_repair_link.zig` | EDIT | Handle PR opened, PR merged, and branch workflow results |
| `src/agentsfleetd/http/handlers/webhooks/repair_link_provenance.zig` | CREATE | Verify installation, base repository, author, and incident owner |
| `src/agentsfleetd/http/handlers/ingress/github.zig` | EDIT | Invoke shared linkage before ordinary target routing |
| `src/agentsfleetd/http/handlers/ingress/repair_fleet_resolve.zig` | CREATE | Resolve the owning Fleet from the branch incident identifier |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_write_gate.zig` | EDIT | Spend approval atomically and refuse past ceiling |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Register provenance and spend-ceiling refusals |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | Expose both runtime errors |
| `src/agentsfleetd/http/webhook_http_integration_test.zig` | EDIT | Prove both ingress routes, run history, merge correlation, and provenance |
| `src/agentsfleetd/http/handlers/runner/credentials_mint_write_gate_integration_test.zig` | EDIT | Prove spend and concurrency behavior |
| `src/agentsfleetd/db/pool_test.zig` | EDIT | Prove runtime privileges for new history rows |
| `docs/architecture/scenarios/production-deploy-repair.md` | EDIT | Separate preview evidence from production closure |

## Applicable Rules

- `~/Projects/dotfiles/docs/greptile-learnings/RULES.md` — No Dead Code (NDC), No Legacy Retained (NLR), String Literals Are Constants (UFS), Cross-layer Orphan Sweep (ORP), File and Function Length Limits (FLL), pre-v2.0 Schema Removal (SCH), and Integration Tests use real Fixtures (ITF).
- `~/Projects/dotfiles/dispatch/write_zig.md` — database drain, tagged-union, lifecycle, length, and both Linux cross-compiles.
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — slots 831–833 are additive, single-concern migrations; slot 830 stays frozen.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` and `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md` — structured record/refuse events and owned store lifetimes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| Zig / public shape / lifecycle | yes | shape verdict per new file, drains, cleanup, both Linux targets |
| File and function length | yes | provenance and owner resolution stay separate from near-cap handlers |
| Named literals | yes | provider fields, statuses, and error names each have one constant per file |
| Error registry | yes | typed provenance and spend-ceiling refusals with negative tests |
| Schema | yes | slots 831–833, embed, migration array, provisioned-database tests |
| User interface and design token | no | no user interface file changes |

## Prior-Art / Reference Implementations

- `schema/830_repair_pr_links.sql` supplies the history-layer purge and immutability shape, but is not edited.
- `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_app.zig` supplies the repair-traffic drop that remains after shared linkage.
- `schema/811_fleet_approval_gates_binding.sql` supplies the nullable additive-migration precedent for a provisioned database.
- GitHub's Pull Requests API supplies `merge_commit_sha`; merge, squash, and rebase strategies must use that provider value rather than reconstructing default-branch state.

## Sections (implementation slices)

### §1 — Both GitHub routes record the same link

Shared ingress resolves the incident event's owning Fleet and calls the same arm as the signed per-Fleet route before the existing repair-traffic drop.

- **Dimension 1.1** — shared ingress PR-open records one link → `test_ingress_repair_pr_records_linkage`
- **Dimension 1.2** — incident owner wins over a grant-matching Fleet → `test_ingress_resolves_incident_owner`
- **Dimension 1.3** — unknown incident records nothing and is acknowledged → `test_ingress_unknown_incident_is_ignored`
- **Dimension 1.4** — repair traffic wakes no Fleet → `test_repair_traffic_never_wakes`

### §2 — Every branch run remains evidence

Completed repair-branch workflows append by provider run identifier even when they arrive before the PR-open event. Workflow name and head commit distinguish deploy, lint, and preview evidence.

- **Dimension 2.1** — three completed workflows leave three rows → `test_repair_runs_append_independently`
- **Dimension 2.2** — run-before-PR remains joinable → `test_early_repair_run_is_retained`
- **Dimension 2.3** — replay changes no row count → `test_repair_run_replay_is_idempotent`
- **Dimension 2.4** — no production caller mutates slot 830 deploy status → `test_mutable_deploy_stamp_has_no_caller`

### §3 — Linkage proves provenance and merge identity

PR-open verifies installation, author, repository, and non-fork base. PR-closed records a merged hash only when `merged=true` and only once.

- **Dimension 3.1** — foreign installation, author, or fork records nothing → `test_foreign_repair_pr_is_refused`
- **Dimension 3.2** — own PR still links → `test_own_repair_pr_links`
- **Dimension 3.3** — merged PR stores exact provider hash → `test_merged_pr_records_provider_hash`
- **Dimension 3.4** — closed-unmerged or hashless PR cannot correlate → `test_unmerged_pr_records_no_hash`

### §4 — Approval funds bounded mints

The mint decision increments spend in the same transaction that confirms approval. The card states the ceiling.

- **Dimension 4.1** — mint increments spend once → `test_mint_spends_approval_once`
- **Dimension 4.2** — mint past ceiling returns the registered refusal → `test_mint_past_ceiling_refuses`
- **Dimension 4.3** — at least 100 concurrent mints cannot exceed ceiling → `test_concurrent_mints_hold_ceiling`
- **Dimension 4.4** — approval card names ceiling → `test_approval_card_states_ceiling`

## Interfaces

```text
core.repair_run_results (slot 831)
  workspace_id, fleet_id, event_id, repository, branch,
  workflow_name, provider_run_id, head_commit_sha,
  conclusion, completed_at, created_at
  UNIQUE (fleet_id, repository, provider_run_id)

core.repair_pr_links additions (slot 832)
  merged_commit_sha TEXT NULL, merged_at BIGINT NULL
  transition: NULL -> exact provider hash once; content otherwise frozen

core.fleet_approval_gates additions (slot 833)
  spend_count BIGINT NULL, spend_ceiling BIGINT NULL
  NULL ceiling resolves to an application constant, never a schema literal
```

No public route is added. Slot 830's old deploy columns remain frozen history and lose all production callers.

## Failure Modes

| Mode | Handling |
|---|---|
| Unknown incident | acknowledge with named ignore reason; write nothing |
| Foreign installation, author, or fork | typed refusal; write nothing |
| Run arrives before PR | retain it by Fleet, repository, and branch |
| Provider redelivers run or merge | unique/one-time writes absorb replay |
| PR closes without merge hash | keep link unmerged; never guess current `main` |
| Concurrent token mints | row lock plus in-transaction spend holds ceiling |
| Database unavailable | fail closed; delivery retry is safe |

## Metrics & Observability

- Structured counters: link inserted/refused, run inserted/replayed, merge correlated/ignored, mint spent/refused.
- Logs carry workspace, Fleet, incident, repository, provider identifier, and registered error name; never credential values or webhook bodies.
- Existing operator analytics are unchanged because no new user surface ships here.

## Invariants

- Repair traffic is recorded before it is dropped and never wakes a Fleet.
- Preview workflows are immutable evidence, never proof of production recovery.
- The merged commit hash comes from the merged-PR webhook and changes at most once.
- Provenance failure writes no linkage or run row.
- Approval spend cannot exceed its ceiling under concurrent mint attempts.

## Test Specification (tiered)

| Dimension | Tier | Test | Concrete assertion |
|---|---|---|---|
| 1.1–1.4 | integration | four §1 tests | shared and per-Fleet routes agree; no wake-up |
| 2.1–2.4 | integration | four §2 tests | immutable, early, idempotent history; stamp caller absent |
| 3.1–3.4 | integration | four §3 tests | provenance fails closed; exact merge hash stored once |
| 4.1–4.4 | integration | four §4 tests | spend visible, card accurate, concurrency ceiling holds |
| migration | integration | `test_831_833_apply_to_provisioned_database` | existing slot 830 and gate rows survive |
| regression | integration | `test_m157_002_write_gate_unchanged` | prior approval refusals keep their meanings |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | shared ingress links own repair | `make test-integration` | exit 0 | P0 | |
| R2 | run history is append-only and replay-safe | `make test-integration` | exit 0 | P0 | |
| R3 | exact merged hash is stored, never inferred | `make test-integration` | exit 0 | P0 | |
| R4 | 100 concurrent mints hold the ceiling | `make test-integration` | exit 0 | P0 | |
| R5 | prior write-gate behavior remains | `make test-integration` | exit 0 | P0 | |
| S1 | repository unit suite | `make test-unit-all` | exit 0 | P0 | |
| S2 | conformance | `make harness-verify` | exit 0 | P0 | |
| S3 | no leaks | `make memleak` | exit 0 | P0 | |
| S4 | version consistency | `make check-version` | exit 0 | P0 | |
| S5 | secret scan | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol:** run each command verbatim. Every row must be graded; every P0 row must pass before close.

## Dead Code Sweep

- Remove production calls to `stampDeploy`, its status constants, and its statement.
- Confirm repair history readers use slot 831 and merge correlation uses slot 832.
- Delete no file outside Files Changed; report any newly orphaned symbol before removal.

## Out of Scope

- Deciding whether production recovered; M157_004 owns that result.
- Adding `incident-verifier`, Vercel ingress, an operator result card, or live-repository acceptance.
- Automatic merge, deployment, rollback, or multi-repository repair.
- A stored crew entity or coordinator Fleet.

---

## Product Clarity (authoring record)

1. **Successful user moment** — repair evidence is complete and the merged bytes are pinned for later production verification.
2. **Preserved behavior** — one draft PR, human merge, repair traffic never loops, existing refusal meanings remain.
3. **Optimal-way check** — provider-returned merge identity plus append-only events is the direct path; guessing current `main` is rejected.
4. **Rebuild-vs-iterate** — iterate on slot 830 with additive migrations and retire only its mutable writer.
5. **What we build** — shared linkage, provenance, immutable runs, merge correlation, bounded spend.
6. **What we do not build** — verifier, vendor ingress beyond GitHub, dashboard closure, automatic repository action.
7. **Fit** — hardens M157_002 and supplies M157_004's trusted correlation input.
8. **Surface order** — storage and ingress first; no public interface changes.
9. **Dashboard restraint** — no production-success claim is exposed from preview history.
10. **Confused-user next step** — named ignore/refusal reasons distinguish missing incident, failed provenance, replay, and missing merge hash.

## Decomposition & alternatives

- **Chosen:** M157_003 stops at trusted, bounded repair history. M157_004 consumes it for production closure on the same branch and milestone PR.
- **Rejected:** keep the verifier in this file. It hid provider intake, event routing, and the operator read surface behind one vague section.
- **Patch-vs-refactor:** shared ingress, provenance, merge correlation, and spend are patches; replacing the mutable run writer with immutable history is the contained refactor.

## Discovery (consult log)

- **Branch lookup:** product work is `feat/m157-repair-loop`; public docs work is `chore/m157-repair-loop-changelog`; both merge to `main`.
- **Architecture call:** preview evidence cannot close the incident. Only repository plus exact provider-returned merged commit can correlate production.
- **Crew call:** responder, repairer, and verifier remain independently installed Fleets; Grafana and Elasticsearch are shared read-only evidence sources.
- **Review:** separate Orly Chief Technology Officer adversarial review runs after this split and the architecture/public-doc update.
- **Inherited deferral acknowledgement:** Indy directed the PR #591 items into M157_003 on Aug 10, 2026; M157_004 is a scope split, not a deferral from the milestone PR.
