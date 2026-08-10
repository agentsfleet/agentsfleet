<!--
SPECIFICATION AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill; after filling, DELETE every template guidance comment.
- No time estimates, effort columns, completion percentages, implementation
  dates, or assigned owners.
- Priority is the only sizing signal; Dependencies are the only sequencing
  signal.
-->

# M157_004: Production evidence closes the repair loop

**Prototype:** v2.0.0
**Milestone:** M157
**Workstream:** 004
**Date:** Aug 10, 2026
**Status:** PENDING
**Priority:** P1 — a green preview or the current default branch cannot prove that the repaired bytes reached production or cleared the incident
**Categories:** API, INFRA, SKILL
**Batch:** B1 — provider normalization and correlation; B2 — verifier Fleet and durable result link; B3 — operator surface and live proof
**Branch:** `feat/m157-repair-loop`
**Public docs branch:** `chore/m157-repair-loop-changelog` in `~/Projects/docs`
**Base branch:** `main` in both repositories
**Test Baseline:** unit=3512 integration=589
**Depends on:** M157_003 (shared provenance-checked repair link plus exact `merged_commit_sha`)
**Provenance:** agent-generated from Indy's Aug 10, 2026 three-Fleet clarification and merged-commit direction
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md`

---

## Overview

**Goal (testable):** A terminal GitHub production deployment status wakes a separately installed, read-only `incident-verifier` only when its repository and commit hash exactly match a merged repair Pull Request (PR); the event carries the original incident and evidence window; the verifier reads Grafana and Elasticsearch and leaves its result linked to the incident for an operator.

**Problem:** M157_002 stops at a draft PR and M157_003 stops at trusted history. Repair-branch runs are previews. Looking at the default branch later can inspect different bytes. No verifier Fleet, GitHub production-result correlation, or incident-to-verifier result reader exists.

**Solution summary:** The existing signed GitHub ingress gains deployment-status normalization. Vercel deployments qualify when Vercel surfaces them through GitHub; no direct Vercel webhook is added. Before Fleet trigger matching, the daemon requires production environment and exact workspace + repository + merged commit equality. A match emits one internal `repair_production_result`; the verifier subscribes only to that proof-qualified event. Slot 834 links the queued verifier event to the repair. The bundle reads the exact merged commit plus Grafana and Elasticsearch, then reports `cleared`, `not_cleared`, or `inconclusive` through the standard Fleet result. Event detail exposes the repair arc; a live disposable repository proves it.

## PR Intent & comprehension handshake

- **PR title (shared with M157_003):** `feat(m157): close repair incidents on correlated production evidence`
- **Intent:** let an operator trust a verifier result because it is tied to the exact merged bytes and post-deploy telemetry, never to branch-name coincidence.
- **Orly restatement:** Install a third read-only Fleet. Route only exact production results for M157_003's merged hash, give the Fleet incident context without database access, and join its standard result back to the repair.
- **ASSUMPTIONS I'M MAKING:** one milestone PR carries M157_003 and M157_004; the public docs PR remains separate; provider payloads missing repository, production environment, or commit identity fail closed; several matching verifier Fleets may each receive a result because installations remain independent.

## Golden path

```text
responder detects symptom
        -> repairer opens draft PR after approval
        -> human reviews and merges
        -> M157_003 records exact merged commit
        -> GitHub reports terminal production deployment status
        -> daemon matches workspace + repository + exact commit
        -> daemon emits repair_production_result
        -> installed incident-verifier receives proof-qualified event
        -> verifier reads Grafana + Elasticsearch + exact commit
        -> standard Fleet event stores result
        -> slot 834 joins result to incident and PR
        -> operator sees cleared / not cleared / inconclusive
```

There is no unknown lookup or secret handoff in the path. Provider webhook secrets and Grafana/Elasticsearch credentials use existing vault key-name resolution. The daemon supplies linkage context; the Fleet never queries internal tables.

## Implementing agent — read these first

1. `docs/architecture/scenarios/production-deploy-repair.md` — canonical ordering, correlation rule, and no-crew-row decision.
2. M157_003 active spec — exact merged-hash and run-history inputs.
3. `src/agentsfleetd/http/handlers/ingress/github.zig` — signed shared-ingress routing and fan-out.
4. `.github/workflows/smoke-post-deploy.yml` — existing Vercel-through-GitHub deployment-status caller.
5. `src/agentsfleetd/state/fleet_event_detail_store.zig` — standard operator detail row and response surface.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/834_repair_verifications.sql` | CREATE | Link a correlated production result and verifier event to one repair |
| `schema/embed.zig` | EDIT | Register slot 834 in both migration lists |
| `src/agentsfleetd/state/repair_verifications.zig` | CREATE | Insert and read verification links idempotently |
| `src/agentsfleetd/state/sql.zig` | EDIT | Correlation and verification-link statements |
| `src/agentsfleetd/http/handlers/ingress/github.zig` | EDIT | Normalize GitHub deployment status without weakening existing ingress |
| `src/agentsfleetd/http/handlers/ingress/production_repair_result.zig` | CREATE | Exact merge correlation and proof-qualified synthetic event emission |
| `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_deployment.zig` | CREATE | Normalize terminal GitHub production deployment status |
| `src/agentsfleetd/http/handlers/fleets/event_detail.zig` | EDIT | Return linked repair and verification result |
| `src/agentsfleetd/state/fleet_event_detail_store.zig` | EDIT | Join incident, PR, production result, and verifier event |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Register production-correlation refusal |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | Expose runtime refusal metadata |
| `library/incident-verifier/SKILL.md` | CREATE | Exact-commit, Grafana, and Elasticsearch verification instructions |
| `library/incident-verifier/TRIGGER.md` | CREATE | Subscribe only to `repair_production_result` with read-only bindings |
| `src/agentsfleetd/http/handlers/ingress/github_integration_test.zig` | EDIT | Preserve GitHub ingress and prove production correlation |
| `src/agentsfleetd/http/handlers/fleets/event_detail_integration_test.zig` | EDIT | Prove operator repair arc and isolation |
| `src/agentsfleetd/http/handlers/library/onboard_integration_test.zig` | EDIT | Prove verifier catalogue onboarding |
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT | Add deployment-status subscription, Deployments read permission, and live proof |
| `docs/AUTH.md` | EDIT | Document deployment-status intake before synthetic verifier routing |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | Keep shared GitHub App registration requirements complete |
| `ui/packages/app/lib/api/events.ts` | EDIT | Type linked repair verification detail |
| `ui/packages/app/components/domain/EventDetailsDialog.tsx` | EDIT | Render PR, merged hash, production result, and verifier response |
| `ui/packages/app/tests/event-details-dialog.test.tsx` | EDIT | Prove labels, links, empty states, and isolation |
| `tests/acceptance/repair_live.zig` | CREATE | Disposable-repository end-to-end arc and cleanup |
| `make/acceptance.mk` | EDIT | On-demand caller for live repair proof |
| `docs/architecture/scenarios/production-deploy-repair.md` | EDIT | Canonical three-Fleet production flow |

## Applicable Rules

- `~/Projects/dotfiles/docs/greptile-learnings/RULES.md` — No Dead Code (NDC), No Legacy Retained (NLR), String Literals Are Constants (UFS), Cross-layer Orphan Sweep (ORP), File and Function Length Limits (FLL), pre-v2.0 Schema Removal (SCH), and Integration Tests use real Fixtures (ITF).
- `~/Projects/dotfiles/dispatch/write_zig.md` — database drain, tagged results, lifecycle, size, and Linux cross-compiles.
- `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — TypeScript shape, component substitution, and design tokens.
- `~/Projects/dotfiles/dispatch/write_http.md` plus `~/Projects/dotfiles/docs/REST_API_DESIGN_GUIDELINES.md` — event-detail response remains tenant-scoped and additive.
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — slot 834 is additive, single concern, immutable except purge cascade.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md`, `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md`, and `~/Projects/dotfiles/docs/DESIGN_SYSTEM.md` — structured failures, owned resources, and existing user-interface primitives.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| Zig / public shape / lifecycle | yes | shape verdicts, drains, cleanup, both Linux targets |
| TypeScript / user interface / design token | yes | existing dialog primitives and semantic tokens only |
| File and function length | yes | provider dispatch, normalization, and correlation stay separated |
| Error registry and logging | yes | one typed fail-closed correlation refusal; structured ignore metrics |
| Schema | yes | slot 834 plus embed, migration array, privilege and provisioned-database tests |
| HTTP and tenant isolation | yes | additive event-detail field; cross-workspace result remains not found |

## Prior-Art / Reference Implementations

- `src/agentsfleetd/http/handlers/ingress/github.zig` supplies signature-before-parse, replay, target selection, and fan-out behavior.
- `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_app.zig` supplies pure provider normalization returning normalized body or ignore reason.
- `src/agentsfleetd/state/fleet_event_detail_store.zig` supplies the payload-bearing single-event reader.
- `library/incident-responder/` supplies Grafana, Elasticsearch, GitHub read-only, network, and budget declarations.
- GitHub Pull Requests API supplies `merge_commit_sha`; GitHub deployment status supplies deployed commit identity, including Vercel deployments surfaced through GitHub.

## Sections (implementation slices)

### §1 — Normalize GitHub production results

GitHub deployment-status events normalize to provider, deployment identifier, repository, environment, commit hash, conclusion, and completion time. The same path accepts Vercel deployments surfaced through GitHub. Direct Vercel webhook ingestion remains outside this workstream.

- **Dimension 1.1** — terminal GitHub production status normalizes → `test_github_production_status_normalizes`
- **Dimension 1.2** — Vercel-through-GitHub status uses the same normalized shape → `test_vercel_github_status_normalizes`
- **Dimension 1.3** — non-terminal or non-production status queues no verifier → `test_unready_deployment_status_is_ignored`
- **Dimension 1.4** — App registration declares event, permission, and development live proof → `test_github_app_registration_carries_deployment_status`

### §2 — Correlate exact merged bytes

Before ordinary Fleet trigger matching, the daemon matches each normalized result to M157_003 by workspace, repository, and exact merged commit. One match emits one internal `repair_production_result` carrying the repair and production context. Normal routing then selects Fleets subscribed to that synthetic type and inserts slot 834 with the returned verifier event identifier.

- **Dimension 2.1** — exact production commit emits one synthetic event → `test_exact_merged_commit_emits_repair_result`
- **Dimension 2.2** — preview environment, missing hash, or mismatch emits nothing → `test_unproven_result_emits_nothing`
- **Dimension 2.3** — later default-branch commit cannot verify earlier repair → `test_later_default_commit_does_not_correlate`
- **Dimension 2.4** — provider replay leaves one synthetic event and verifier link → `test_production_result_replay_is_idempotent`
- **Dimension 2.5** — second workspace cannot observe or claim correlation → `test_repair_correlation_is_workspace_scoped`

### §3 — Install and run the read-only verifier

`incident-verifier` subscribes to `repair_production_result`, rejects raw `deployment_status`, reads the exact merged commit plus Grafana and Elasticsearch, and returns one named outcome with evidence. It has no write permission and no database tool.

- **Dimension 3.1** — bundle onboards through normal library path → `test_incident_verifier_onboards`
- **Dimension 3.2** — minted repository permission is read-only → `test_incident_verifier_token_is_read_only`
- **Dimension 3.3** — instructions use event merge hash, never current default branch → `test_verifier_uses_event_commit_hash`
- **Dimension 3.4** — absent/contradictory telemetry yields `inconclusive` → `test_verifier_does_not_guess_without_evidence`

### §4 — Show the linked result to the operator

Incident event detail adds a nullable `repair` object containing PR, merge, production result, verifier event status, and standard response text. The existing dialog renders it without introducing a separate incident dashboard.

- **Dimension 4.1** — incident detail returns linked arc in one tenant-scoped read → `test_incident_detail_returns_repair_verification`
- **Dimension 4.2** — pending verifier is explicit, never shown as cleared → `test_pending_verifier_has_no_success_label`
- **Dimension 4.3** — dialog links PR and displays exact abbreviated hash and response → `test_dialog_renders_repair_verification`
- **Dimension 4.4** — unrelated event detail shape is unchanged → `test_event_without_repair_omits_repair_field`

### §5 — Prove the arc against a live repository

An on-demand acceptance target uses a disposable repository and real GitHub delivery to drive failure, approval, draft PR, merge, production correlation, verifier result, and cleanup.

- **Dimension 5.1** — full arc ends with one linked verifier result → `test_live_repair_arc_closes_on_exact_commit`
- **Dimension 5.2** — cleanup leaves no branch, open PR, repository, or test rows → `test_live_repair_arc_cleans_up`

## Interfaces

```text
production_result
  provider, provider_deployment_id, repository, environment,
  commit_sha, conclusion, completed_at

repair_production_result
  incident: { workspace_id, fleet_id, event_id }
  repair: { pr_number, pr_url, merged_commit_sha, merged_at }
  production: { provider, deployment_id, conclusion, completed_at }
  evidence_window

core.repair_verifications (slot 834)
  id, workspace_id, repair_link_id, verifier_fleet_id,
  verifier_event_id, provider, provider_deployment_id,
  environment, commit_sha, conclusion, completed_at, created_at
  UNIQUE (verifier_fleet_id, provider, provider_deployment_id)

GET /v1/workspaces/{workspace}/fleets/{fleet}/events/{event}
  repair?: {
    pr_number, pr_url, merged_commit_sha, merged_at,
    production: { provider, deployment_id, conclusion, completed_at },
    verification: { fleet_id, event_id, status, response_text }
  }
```

The verifier event request carries this same repair and production context. `response_text` remains the standard Fleet result; the bundle's first line names `cleared`, `not_cleared`, or `inconclusive` for human scanning.

## Failure Modes

| Mode | Handling |
|---|---|
| Signature missing or invalid | reject before body parsing or routing |
| App lacks deployment subscription or permission | development live proof fails; production setup does not proceed |
| Environment is not production | record named ignore metric; queue nothing |
| Repository or commit missing | fail closed; queue nothing |
| Commit does not match merged repair | queue ordinary subscribers only; never verifier closure |
| Several repairs match unexpectedly | refuse correlation and alert; do not guess |
| Verifier Fleet absent | retain normalized provider result metric; no Fleet event |
| Grafana/Elasticsearch unavailable | verifier reports `inconclusive` with missing evidence |
| Verifier run fails | linked event shows failure; no cleared label |
| Database or queue fails between writes | idempotent retry converges on one link and event |

## Metrics & Observability

- Counters: provider result accepted/ignored by reason, correlation matched/missed/ambiguous, synthetic event emitted/replayed, verifier queued/completed.
- Histograms: production completion to verifier queue and queue to verifier completion.
- Logs include workspace, repository, provider deployment, commit hash prefix, repair link, and verifier event; never webhook body or credentials.
- Product analytics: event-detail repair section viewed and PR link opened. No funnel beyond existing event detail.

## Invariants

- Only an exact workspace, repository, production environment, and merged commit match can wake verification.
- Raw `deployment_status` never wakes the verifier; correlation must emit `repair_production_result` first.
- The registration playbook requires deployment-status events, Deployments read-only permission, and a signed development delivery proof.
- Provider data missing commit identity fails closed.
- Trigger wiring selects verifier Fleets; no Fleet name or crew row is an identity boundary.
- The verifier receives repair context in its event and never reads internal database tables.
- The verifier has read-only repository access and cannot merge, revert, or deploy.
- Pending, failed, and inconclusive verification are never presented as cleared.

## Test Specification (tiered)

| Dimension | Tier | Test | Concrete assertion |
|---|---|---|---|
| 1.1–1.4 | unit + integration | four §1 tests | GitHub shape; Vercel-through-GitHub parity; unready results ignored; registration complete |
| 2.1–2.5 | unit + integration | five §2 tests | one fixture produces one synthetic event; mismatch, replay, and cross-workspace cases stay deterministic |
| 3.1–3.4 | unit + integration | four §3 tests | synthetic trigger only, normal onboarding, read-only token, exact-hash prompt |
| 4.1–4.4 | integration + user interface | four §4 tests | additive detail and honest pending/complete rendering |
| 5.1–5.2 | end-to-end | two §5 tests | live exact-commit closure and complete cleanup |
| load | integration | `test_production_correlation_100_parallel` | at least 100 deliveries do not serialize globally |
| migration | integration | `test_834_applies_to_provisioned_database` | existing repair rows remain readable |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | GitHub deployment status normalizes production identity | `make test-unit-all` | exit 0 | P0 | |
| R2 | only exact merged commit wakes verifier | `make test-integration` | exit 0 | P0 | |
| R3 | verifier has read-only repository permission | `make test-integration` | exit 0 | P0 | |
| R4 | incident detail shows linked verifier result | `make test-integration` | exit 0 | P0 | |
| R5 | user-interface dialog states pending and result honestly | `make test-unit-all` | exit 0 | P0 | |
| R6 | live disposable repository proves full arc and cleanup | `make acceptance-e2e` | exit 0 | P1 | |
| R7 | 100 parallel results show no global serialization | `make test-integration` | exit 0 | P0 | |
| S1 | conformance | `make harness-verify` | exit 0 | P0 | |
| S2 | repository integration | `make test-integration` | exit 0 | P0 | |
| S3 | no leaks | `make memleak` | exit 0 | P0 | |
| S4 | version consistency | `make check-version` | exit 0 | P0 | |
| S5 | secret scan | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol:** run commands verbatim. Every row must be graded; every P0 row must pass before close. The live P1 row requires an Indy-acknowledged deferral quote if the external environment cannot run it.

## Dead Code Sweep

- Confirm no verifier instruction or correlation path reads current `main` as a substitute for the event hash.
- Confirm provider-specific vocabulary stops at normalizers.
- Confirm every slot 834 writer has an event-detail reader and every new dialog field has an API source.
- Delete no file outside Files Changed; report newly orphaned code before removal.

## Out of Scope

- Automatic merge, rollback, or another repository write after verification.
- A stored crew entity, coordinator Fleet, or Grafana/Elasticsearch vendor Fleet.
- Repair across multiple repositories for one incident.
- Declaring source-code correctness from model opinion; the verdict is production symptom state.
- Provider payloads that cannot prove exact repository, environment, and commit identity; direct Vercel webhook ingestion or signature handling.

---

## Product Clarity (authoring record)

1. **Successful user moment** — the incident detail shows the repair PR, exact merged hash, production result, and telemetry-backed verifier response together.
2. **Preserved behavior** — human approval, review, and merge remain; no Fleet gains automatic merge or rollback.
3. **Optimal-way check** — correlate the provider's exact deployment commit; current-branch inference is cheaper but wrong.
4. **Rebuild-vs-iterate** — reuse trigger routing, Fleet events, event detail, and library onboarding; add one link table.
5. **What we build** — GitHub deployment-status normalization, exact correlation, verifier bundle, linked event detail, live proof.
6. **What we do not build** — direct Vercel ingress, crew coordinator, vendor-specific Fleets, auto-merge/revert, multi-repository orchestration.
7. **Fit** — responder detects, repairer changes, verifier judges; one synthetic event reuses existing Fleet routing without a special Fleet role.
8. **Surface order** — ingress and durable link, verifier bundle, API detail, then existing dialog.
9. **Dashboard restraint** — no cleared label before a completed verifier response; inconclusive stays visibly inconclusive.
10. **Confused-user next step** — missing correlation names missing hash/environment/link; verifier failures link to their event detail.

## Decomposition & alternatives

- **Chosen:** GitHub deployment-status result, exact merged-hash gate, standard target selection, one verification link, standard Fleet response.
- **Chosen verifier routing:** exact correlation emits `repair_production_result`; the verifier subscribes to that type. This gives tests one fixture-in/event-out seam without adding Fleet roles.
- **Rejected:** classify Fleets with a verifier role. It adds stored identity and onboarding behavior when a proof-qualified event already supplies the safe routing boundary.
- **Rejected:** identify verifier by Fleet name. Installers may rename a bundle, so normal trigger selection is the durable identity.
- **Rejected:** add a crew table. It adds lifecycle and consistency problems without helping event routing.
- **Rejected:** let verifier query internal repair rows. The daemon already owns correlation and can provide a smaller, safer event.
- **Completeness call:** the operator reader and live acceptance stay in scope; without them the backend would write an unobservable, fixture-only result.

## Discovery (consult log)

- **Branch lookup:** product work is `feat/m157-repair-loop`; public docs work is `chore/m157-repair-loop-changelog`; both merge to `main`.
- **Crew decision:** one logical incident crew is three independent Fleets in event order: responder, repairer, verifier.
- **Evidence decision:** Grafana and Elasticsearch are read-only evidence sources for all three Fleets, not separate members.
- **Correlation decision:** only exact provider-returned merged commit plus production environment can wake verification; preview and current-default-branch inference are excluded.
- **Provider decision:** Indy chose GitHub deployment status, including Vercel-through-GitHub, with no direct Vercel ingress; 3A adds the App subscription, Deployments read-only permission, and live proof.
- **Verifier-routing decision:** `> Indy (Aug 10, 2026: 08:42 PM): "2A i want a simpler approach to get this tested"` — exact correlation emits `repair_production_result`; raw deployment status never selects the verifier.
- **Review:** separate Orly Chief Technology Officer adversarial review runs after architecture, both specs, and public docs are updated.
- **User direction:** Indy approved the M157_003/M157_004 split on Aug 10, 2026 while keeping one branch and milestone PR.
