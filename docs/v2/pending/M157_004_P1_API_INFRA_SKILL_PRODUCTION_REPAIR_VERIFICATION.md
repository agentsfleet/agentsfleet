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
**Batch:** B1 — durable production intake and correlation; B2 — verifier Fleet and standard result proof
**Branch:** `feat/m157-repair-loop`
**Public docs branch:** `chore/m157-repair-loop-changelog` in `~/Projects/docs`
**Base branch:** `main` in both repositories
**Test Baseline:** unit=3512 integration=589
**Depends on:** M157_003 (shared provenance-checked repair link plus exact `merged_commit_sha`)
**Provenance:** agent-generated from Indy's Aug 10, 2026 three-Fleet clarification and merged-commit direction
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md`

---

## Overview

**Goal (testable):** A terminal GitHub production deployment status wakes a separately installed, read-only `incident-verifier` only when its repository and commit hash exactly match a merged repair Pull Request (PR), regardless of webhook order; the verifier reads Grafana and Elasticsearch and stores its verdict in standard Fleet event history.

**Problem:** M157_002 stops at a draft PR and M157_003 stops at trusted history. Repair-branch runs are previews. Looking at the default branch later can inspect different bytes. A production result can also arrive before the merged-PR webhook and vanish unless intake is durable. No verifier Fleet or order-independent production correlation exists.

**Solution summary:** The signed GitHub ingress stores normalized deployment status in slot 834 before correlation; Vercel qualifies only through GitHub. One reconciler runs after either the production insert or M157_003 merged-hash write. An exact match creates one due-time-bound slot 835 attempt; the dispatcher later emits `repair_production_result`. The verifier subscribes only to that event, reads the exact commit plus Grafana and Elasticsearch, and writes `cleared`, `not_cleared`, or `inconclusive` to standard Fleet history. Repository integration tests prove both arrival orders and replay without a custom incident card or live-repository target.

## PR Intent & comprehension handshake

- **PR title (shared with M157_003):** `feat(m157): close repair incidents on correlated production evidence`
- **Intent:** let an operator trust a verifier result because it is tied to the exact merged bytes and post-deploy telemetry, never to branch-name coincidence.
- **Orly restatement:** Store production evidence first, reconcile it with M157_003 from either arrival order, then give the third read-only Fleet exact incident and commit context through one proof-qualified event.
- **ASSUMPTIONS I'M MAKING:** one milestone PR carries M157_003 and M157_004; the public docs PR remains separate; provider payloads missing repository, production environment, or commit identity fail closed; several matching verifier Fleets may each receive a result because installations remain independent.

## Golden path

```text
responder detects symptom
        -> repairer opens draft PR after approval
        -> human reviews and merges
        -> M157_003 records exact merged commit
        -> GitHub reports terminal production deployment status
        -> slot 834 stores production result
        -> reconciler matches workspace + repository + exact commit
        -> slot 835 records one verification attempt and fixed due time
        -> dispatcher waits for the complete evidence window
        -> daemon emits repair_production_result once
        -> installed incident-verifier receives proof-qualified event
        -> verifier reads Grafana + Elasticsearch + exact commit
        -> standard Fleet event stores result
        -> operator reads cleared / not cleared / inconclusive in Fleet history
```

There is no unknown lookup or secret handoff in the path. Provider webhook secrets and Grafana/Elasticsearch credentials use existing vault key-name resolution. The daemon supplies linkage context; the Fleet never queries internal tables.

## Implementing agent — read these first

1. `docs/architecture/scenarios/production-deploy-repair.md` — canonical ordering, correlation rule, and no-crew-row decision.
2. M157_003 active spec — exact merged-hash and run-history inputs.
3. `src/agentsfleetd/http/handlers/ingress/github.zig` — signed shared-ingress routing and fan-out.
4. `.github/workflows/smoke-post-deploy.yml` — existing Vercel-through-GitHub deployment-status caller.
5. `src/agentsfleetd/state/fleet_events_store.zig` — standard Fleet result history used as the first operator surface.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `schema/834_repair_production_results.sql` | CREATE | Retain normalized terminal production results before correlation |
| `schema/835_repair_verifications.sql` | CREATE | Retain one due-time-bound verifier attempt per repair, result, and Fleet |
| `schema/embed.zig` | EDIT | Register slots 834–835 in both migration lists |
| `src/agentsfleetd/state/repair_production_results.zig` | CREATE | Insert production results idempotently and reconcile either arrival order |
| `src/agentsfleetd/state/repair_verifications.zig` | CREATE | Insert dispatch intents, complete event links once, and scan due rows |
| `src/agentsfleetd/state/sql.zig` | EDIT | Correlation and verification-link statements |
| `src/agentsfleetd/queue/redis_fleet.zig` | EDIT | Atomically append once per dispatch intent and return the original stream identifier on retry |
| `src/agentsfleetd/fleet/repair_verification_dispatcher.zig` | CREATE | Retry a bounded batch of pending slot 835 intents |
| `src/agentsfleetd/cmd/serve_background.zig` | EDIT | Start and join the bounded verification dispatcher |
| `src/agentsfleetd/cmd/serve_lifecycle_integration_test.zig` | EDIT | Prove the new dispatcher starts and joins during shutdown |
| `src/agentsfleetd/http/handlers/ingress/github.zig` | EDIT | Normalize GitHub deployment status without weakening existing ingress |
| `src/agentsfleetd/http/handlers/ingress/production_repair_result.zig` | CREATE | Exact merge correlation and proof-qualified synthetic event emission |
| `src/agentsfleetd/http/handlers/webhooks/github_repair_link.zig` | EDIT | Invoke the same reconciler after a merged-hash write |
| `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_deployment.zig` | CREATE | Normalize terminal GitHub production deployment status |
| `src/agentsfleetd/errors/error_registry.zig` | EDIT | Register production-correlation refusal |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | Expose runtime refusal metadata |
| `library/incident-verifier/SKILL.md` | CREATE | Exact-commit, Grafana, and Elasticsearch verification instructions |
| `library/incident-verifier/TRIGGER.md` | CREATE | Subscribe only to `repair_production_result` with read-only bindings |
| `src/agentsfleetd/http/handlers/ingress/github_integration_test.zig` | EDIT | Preserve GitHub ingress and prove production correlation |
| `src/agentsfleetd/fleet/repair_verification_dispatcher_integration_test.zig` | CREATE | Inject crashes around Redis and prove one verifier event |
| `src/agentsfleetd/http/handlers/library/onboard_integration_test.zig` | EDIT | Prove verifier catalogue onboarding |
| `src/agentsfleetd/db/pool_test.zig` | EDIT | Prove runtime privileges for slots 834–835 |
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT | Add deployment-status subscription, Deployments read permission, and live proof |
| `docs/AUTH.md` | EDIT | Document deployment-status intake before synthetic verifier routing |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | Keep shared GitHub App registration requirements complete |
| `docs/architecture/scenarios/production-deploy-repair.md` | EDIT | Canonical three-Fleet production flow |

## Applicable Rules

- `~/Projects/dotfiles/docs/greptile-learnings/RULES.md` — No Dead Code (NDC), No Legacy Retained (NLR), String Literals Are Constants (UFS), Cross-layer Orphan Sweep (ORP), File and Function Length Limits (FLL), pre-v2.0 Schema Removal (SCH), and Integration Tests use real Fixtures (ITF).
- `~/Projects/dotfiles/dispatch/write_zig.md` — database drain, tagged results, lifecycle, size, and Linux cross-compiles.
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — slots 834–835 are additive and single-concern; slot 835 permits only the fenced `NULL` to event-identifier completion.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` and `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md` — structured failures and owned resources.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| Zig / public shape / lifecycle | yes | shape verdicts, drains, cleanup, both Linux targets |
| File and function length | yes | provider dispatch, normalization, and correlation stay separated |
| Error registry and logging | yes | one typed fail-closed correlation refusal; structured ignore metrics |
| Schema | yes | slots 834–835 plus embed, migration array, privilege and provisioned-database tests |

## Prior-Art / Reference Implementations

- `src/agentsfleetd/http/handlers/ingress/github.zig` supplies signature-before-parse, replay, target selection, and fan-out behavior.
- `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_app.zig` supplies pure provider normalization returning normalized body or ignore reason.
- `src/agentsfleetd/state/fleet_events_store.zig` supplies the existing result-history reader.
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

Slot 834 stores every normalized result before matching. One reconciler runs after either that insert or M157_003's merged-hash write. Exact workspace, repository, and commit equality creates one slot 835 dispatch intent per subscribed verifier Fleet with `verify_after = completed_at + OBSERVATION_WINDOW_MS`. The fixed window is fifteen minutes. Composite indexes serve the exact merge/result lookup and the bounded due-intent scan. Redis enqueue-once keys on the intent and returns the original stream event identifier on retry.

- **Dimension 2.1** — result-first then merge emits once → `test_result_before_merge_reconciles_once`
- **Dimension 2.2** — merge-first then result emits once → `test_merge_before_result_reconciles_once`
- **Dimension 2.3** — preview, missing hash, or mismatch remains stored and emits nothing → `test_unproven_result_stays_unmatched`
- **Dimension 2.4** — provider and merge replay leave one result and attempt → `test_reconciliation_replay_is_idempotent`
- **Dimension 2.5** — second workspace cannot observe or claim correlation → `test_repair_correlation_is_workspace_scoped`
- **Dimension 2.6** — crashes before and after Redis converge on one event → `test_verifier_dispatch_crash_retries_once`
- **Dimension 2.7** — intent stays pending until its fixed window completes → `test_verifier_dispatch_waits_for_evidence_window`
- **Dimension 2.8** — each matching verifier gets one independent attempt → `test_matching_verifiers_each_receive_once`
- **Dimension 2.9** — dispatcher starts and joins with the daemon → `test_verification_dispatcher_lifecycle_is_bounded`
- **Dimension 2.10** — correlation and due scans have exact indexes → `test_repair_verification_indexes_exist`

### §3 — Install and run the read-only verifier

`incident-verifier` subscribes to `repair_production_result`, rejects raw `deployment_status`, receives the linked incident and repair evidence, reads the exact merged commit plus Grafana and Elasticsearch over the completed fixed window, and returns one named outcome. It has no write permission and no database tool.

- **Dimension 3.1** — bundle onboards through normal library path → `test_incident_verifier_onboards`
- **Dimension 3.2** — minted repository permission is read-only → `test_incident_verifier_token_is_read_only`
- **Dimension 3.3** — instructions use event merge hash, never current default branch → `test_verifier_uses_event_commit_hash`
- **Dimension 3.4** — absent/contradictory telemetry yields `inconclusive` → `test_verifier_does_not_guess_without_evidence`

## Interfaces

```text
production_result
  provider, provider_deployment_id, repository, environment,
  commit_sha, conclusion, completed_at

repair_production_result
  incident: { workspace_id, fleet_id, event_id, linked_evidence }
  repair: { pr_number, pr_url, merged_commit_sha, merged_at }
  production: { provider, deployment_id, conclusion, completed_at }
  evidence_window: { start_at: completed_at, end_at: verify_after }

core.repair_production_results (slot 834)
  id, workspace_id, provider, provider_deployment_id, repository,
  environment, commit_sha, conclusion, completed_at, created_at
  UNIQUE (workspace_id, provider, provider_deployment_id)

core.repair_verifications (slot 835)
  id, workspace_id, production_result_id, repair_link_id,
  verifier_fleet_id, verify_after,
  verifier_event_id NULL until dispatched, created_at
  UNIQUE (production_result_id, repair_link_id, verifier_fleet_id)
```

The slot 835 identifier is the Redis enqueue-once key. Redis atomically appends or returns the stream identifier recorded for that key. Only that returned identifier may complete `verifier_event_id`, once. The verifier event request carries the same repair and production context. `response_text` remains the human-readable Fleet result; no daemon code parses it into a second status.

## Failure Modes

| Mode | Handling |
|---|---|
| Signature missing or invalid | reject before body parsing or routing |
| App lacks deployment subscription or permission | development live proof fails; production setup does not proceed |
| Environment is not production | record named ignore metric; queue nothing |
| Repository or commit missing | fail closed; queue nothing |
| Production result or merge arrives first | retain the first side; second side runs the same reconciler |
| Commit does not match merged repair | retain unmatched result; never emit verifier closure |
| Several repairs match unexpectedly | refuse correlation and alert; do not guess |
| Verifier Fleet absent | retain durable production result; no verifier event |
| Grafana/Elasticsearch unavailable | verifier reports `inconclusive` with missing evidence |
| Verifier run fails | linked event shows failure; no cleared label |
| Process stops before Redis append | slot 835 remains pending; bounded dispatcher retries it |
| Process stops after Redis append | enqueue-once returns the original stream identifier; no second event is created |
| Observation window is incomplete | dispatcher leaves the intent pending; no Fleet run waits or guesses |
| Several verifier Fleets match | each gets its own slot 835 intent and standard event |

## Metrics & Observability

- Counters: provider result accepted/ignored by reason, correlation matched/missed/ambiguous, dispatch pending/retried, synthetic event emitted/replayed, verifier queued/completed.
- Histograms: production completion to verifier queue and queue to verifier completion.
- Logs include workspace, repository, provider deployment, commit hash prefix, repair link, and verifier event; never webhook body or credentials.

## Invariants

- Only an exact workspace, repository, production environment, and merged commit match can wake verification.
- Production-first, merge-first, and replayed delivery converge on one stored result and verification attempt.
- Every slot 835 intent reaches zero or one Fleet event; retry after any write boundary returns the same event identifier.
- `OBSERVATION_WINDOW_MS` is fixed at fifteen minutes; M157_004 adds no timing setting or baseline engine.
- Raw `deployment_status` never wakes the verifier; correlation must emit `repair_production_result` first.
- The registration playbook requires deployment-status events, Deployments read-only permission, and a signed development delivery proof.
- Provider data missing commit identity fails closed.
- Trigger wiring selects verifier Fleets; no Fleet name or crew row is an identity boundary.
- Several trigger matches intentionally yield several independent responses; no resolver guesses a preferred Fleet.
- The verifier receives repair context in its event and never reads internal database tables.
- The verifier has read-only repository access and cannot merge, revert, or deploy.
- Pending, failed, and inconclusive verification are never presented as cleared.

## Test Specification (tiered)

| Dimension | Tier | Test | Concrete assertion |
|---|---|---|---|
| 1.1–1.4 | unit + integration | four §1 tests | GitHub shape; Vercel-through-GitHub parity; unready results ignored; registration complete |
| 2.1–2.10 | integration | ten §2 tests | order, due time, crashes, fan-out, lifecycle, indexes, replay, and workspace scope stay deterministic |
| 3.1–3.4 | unit + integration | four §3 tests | synthetic trigger only, normal onboarding, read-only token, exact-hash prompt |
| load | integration | `test_production_correlation_100_parallel` | at least 100 deliveries do not serialize globally |
| migration | integration | `test_834_835_apply_to_provisioned_database` | existing repair rows remain readable |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | GitHub deployment status normalizes production identity | `make test-unit-all` | exit 0 | P0 | |
| R2 | only exact merged commit wakes verifier | `make test-integration` | exit 0 | P0 | |
| R3 | verifier has read-only repository permission | `make test-integration` | exit 0 | P0 | |
| R4 | both arrival orders and replay converge once | `make test-integration` | exit 0 | P0 | |
| R5 | 100 parallel results show no global serialization | `make test-integration` | exit 0 | P0 | |
| S1 | conformance | `make harness-verify` | exit 0 | P0 | |
| S2 | repository integration | `make test-integration` | exit 0 | P0 | |
| S3 | no leaks | `make memleak` | exit 0 | P0 | |
| S4 | version consistency | `make check-version` | exit 0 | P0 | |
| S5 | secret scan | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol:** run commands verbatim. Every row must be graded; every P0 row must pass before close.

## Dead Code Sweep

- Confirm no verifier instruction or correlation path reads current `main` as a substitute for the event hash.
- Confirm provider-specific vocabulary stops at normalizers.
- Confirm the slot 834 writer and slot 835 writer/readers are called by the reconciler or verifier linkage path.
- Delete no file outside Files Changed; report newly orphaned code before removal.

## Out of Scope

- Automatic merge, rollback, or another repository write after verification.
- A stored crew entity, coordinator Fleet, or Grafana/Elasticsearch vendor Fleet.
- Repair across multiple repositories for one incident.
- Declaring source-code correctness from model opinion; the verdict is production symptom state.
- Provider payloads that cannot prove exact repository, environment, and commit identity; direct Vercel webhook ingestion or signature handling.
- A custom incident card, event-detail API extension, or disposable-repository acceptance target.

---

## Product Clarity (authoring record)

1. **Successful user moment** — standard Fleet history shows a telemetry-backed verifier result for the exact deployed repair.
2. **Preserved behavior** — human approval, review, and merge remain; no Fleet gains automatic merge or rollback.
3. **Optimal-way check** — correlate the provider's exact deployment commit; current-branch inference is cheaper but wrong.
4. **Rebuild-vs-iterate** — reuse trigger routing, Fleet history, and library onboarding; add one result ledger and one attempt ledger.
5. **What we build** — durable GitHub intake, order-independent correlation, synthetic event, verifier bundle, standard result.
6. **What we do not build** — direct Vercel ingress, crew coordinator, vendor-specific Fleets, auto-merge/revert, multi-repository orchestration.
7. **Fit** — responder detects, repairer changes, verifier judges; one synthetic event reuses existing Fleet routing without a special Fleet role.
8. **Surface order** — backend and standard Fleet history first; no new dashboard component.
9. **Dashboard restraint** — no custom incident card until the verifier spine has production evidence.
10. **Confused-user next step** — unmatched results name the missing hash, environment, or merge link in standard history.

## Decomposition & alternatives

- **Chosen:** GitHub deployment-status result, exact merged-hash gate, standard target selection, one verification link, standard Fleet response.
- **Chosen verifier routing:** exact correlation schedules `repair_production_result`; the due dispatcher emits it and the verifier subscribes to that type. This gives tests one fixture-in/event-out seam without adding Fleet roles.
- **Rejected:** classify Fleets with a verifier role. It adds stored identity and onboarding behavior when a proof-qualified event already supplies the safe routing boundary.
- **Rejected:** identify verifier by Fleet name. Installers may rename a bundle, so normal trigger selection is the durable identity.
- **Rejected:** add a crew table. It adds lifecycle and consistency problems without helping event routing.
- **Rejected:** let verifier query internal repair rows. The daemon already owns correlation and can provide a smaller, safer event.
- **Reduced-scope call:** standard Fleet history is the first operator surface; deterministic repository integration replaces a new live target and custom card.

## Discovery (consult log)

- **Branch lookup:** product work is `feat/m157-repair-loop`; public docs work is `chore/m157-repair-loop-changelog`; both merge to `main`.
- **Crew decision:** one logical incident crew is three independent Fleets in event order: responder, repairer, verifier.
- **Evidence decision:** Grafana and Elasticsearch are read-only evidence sources for all three Fleets, not separate members.
- **Correlation decision:** only exact provider-returned merged commit plus production environment can wake verification; preview and current-default-branch inference are excluded.
- **Provider decision:** Indy chose GitHub deployment status, including Vercel-through-GitHub, with no direct Vercel ingress; 3A adds the App subscription, Deployments read-only permission, and live proof.
- **Verifier-routing decision:** `> Indy (Aug 10, 2026: 08:42 PM): "2A i want a simpler approach to get this tested"` — exact correlation schedules `repair_production_result`; raw deployment status never selects the verifier.
- **Evidence-window decision:** `> Indy (Aug 10, 2026: 09:14 PM): "I want to keep the scope simple, so dont keep overengineering, if there is a simple way to do so follow that."` — reuse linked Fleet evidence, hold slot 835 for one fixed fifteen-minute window, then run the verifier once; no new setting, baseline engine, or structured target.
- **Simple-result decision:** standard trigger fan-out and human-readable Fleet responses remain the surface; no crew resolver or parsed verifier-status column.
- **Arrival-order decision:** Indy asked Orly to continue with the recommended durable ledger and shared reconciler; both webhook orders must produce the same attempt.
- **Review:** separate Orly Chief Technology Officer adversarial review runs after architecture, both specs, and public docs are updated.
- **User direction:** Indy approved the M157_003/M157_004 split on Aug 10, 2026 while keeping one branch and milestone PR.
