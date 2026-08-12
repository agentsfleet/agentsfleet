<!-- SPECIFICATION AUTHORING RULES: Body order is execution order. No estimates,
percentages, assigned owners, or implementation dates. Priority sizes work;
Dependencies sequence it. -->

# M157_003: Correlated production evidence closes repair incidents

**Prototype:** v2.0.0
**Milestone:** M157
**Workstream:** 003
**Date:** Aug 10, 2026
**Status:** DONE
**Priority:** P1 — preview success cannot prove that the merged repair reached production or cleared the incident
**Categories:** API, Infrastructure (INFRA), skill (SKILL)
**Batch:** B1 — trusted repair history and bounded write access; B2 — production correlation and verifier closure
**Branch:** `feat/m157-repair-loop`
**Public docs:** not written; Indy prohibited writes to `~/Projects/docs` for this workstream
**Base branch:** `main` in both repositories
**Test Baseline:** unit=3512 integration=589
**Test Delta (VERIFY):** unit=3589 integration=612 before merging `origin/main` — +77 unit and +23 integration; the post-merge tree reads unit=3633 integration=611 because it also contains tests landed on `main`
**Depends on:** Milestone 157 Workstream 002 (M157_002), which ships the write mint, initial linkage, and `incident-repairer`
**Provenance:** agent-generated from Pull Request (PR) #591 Session Notes and Indy's Aug 10–11, 2026 direction
**Canonical architecture:** `docs/architecture/scenarios/production-deploy-repair.md`

## Overview

**Goal (testable):** Either GitHub route records one provenance-checked repair; every branch run remains evidence; the exact merged commit correlates with a terminal production deployment regardless of webhook order; and a separately installed read-only verifier records `cleared`, `not_cleared`, or `inconclusive` in standard Fleet history.
**Problem:** Shared App ingress misses repair linkage, mutable preview state overwrites earlier runs, branch text lacks durable authority, an approved write gate can mint without a ceiling, and current-default-branch inspection can verify bytes different from the deployed repair.
**Solution summary:** The user-authored `TRIGGER.md` declares the repository and trusted Pull Request base, while `SKILL.md` owns exact GitHub ref and draft-PR reconciliation. The daemon binds the approved gate, Fleet event, repository, base, and repair branch into generic HTTP request rules. The runner evaluates those rules without GitHub types or process-local progress. Slots 831–833 retain immutable runs, merge identity, and atomic write spend. Slots 834–835 retain every provider status and fenced verifier intent. One evidence service reconciles either arrival order. A bounded dispatcher claims due rows without holding a database connection across Redis input/output, completes the durable event link, and safely retries deletion of each transient Redis once-key before `incident-verifier` reads the exact commit plus Grafana and Elasticsearch.
**Golden path:** responder detects → repairer receives the daemon branch → human merges → GitHub reports production status → exact workspace, repository, and commit correlate → dispatcher emits one proof-qualified event → verifier reads the completed evidence window → standard Fleet history carries the result.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m157): close repair incidents on correlated production evidence`
- **Intent:** let an operator trust a verifier result because it is tied to exact merged bytes and post-deploy telemetry.
- **Orly restatement:** record trustworthy repair history, correlate deployed bytes once, then let a read-only Fleet judge production evidence.
- **ASSUMPTIONS I'M MAKING:** one workstream and milestone PR carry product changes; external documentation stays untouched per Indy's restriction; missing repository, environment, or commit fails closed; every push-capable identity in the mapped repository is inside the first-spine status-producer boundary; matching verifier Fleets receive independent results.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `docs/{AUTH,architecture/scenarios/github-pr-reviewer,architecture/scenarios/production-deploy-repair}.md` | EDIT | Document trusted intake and the three-Fleet flow |
| `library/{incident-repairer,incident-verifier}/{SKILL,TRIGGER}.md` | EDIT/CREATE | Reconcile exact GitHub progress and install the verifier |
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT | Add subscription and live proof |
| `schema/{831_repair_run_results,832_repair_pr_merge_correlation,833_fleet_approval_gates_spend,834_repair_production_results,835_repair_verifications}.sql`, `schema/embed.zig` | CREATE/EDIT | Add and register evidence ledgers |
| `src/agentsfleetd/git/{repair_branch,repair_trusted_context}.zig` | CREATE | Own daemon Git repair identity |
| `src/agentsfleetd/cmd/serve_background.zig` | EDIT | Own the bounded dispatcher |
| `src/agentsfleetd/db/{index_usage_integration_test,pool_test,test_fixtures}.zig` | EDIT | Prove privileges, indexes, and fixtures |
| `src/agentsfleetd/errors/{error_entries_runtime,error_registry}.zig` | EDIT | Register typed refusals |
| `src/agentsfleetd/fleet/{approval_gate,event_rows,fleet_session,service,service_billing,service_execution_policy,service_report,service_repository,sql}.zig` | EDIT/CREATE | Issue version-safe trusted context and record completion |
| `src/agentsfleetd/fleet/{repair_verification_dispatcher,repair_verification_dispatcher_integration_test,*integration_test}.zig` | EDIT/CREATE | Prove fences, compatibility, billing, lifecycle, and live dependencies |
| `src/agentsfleetd/fleet_runtime/{approval_gate,approval_gate_constants,approval_gate_db,approval_gate_slack}.zig` | EDIT | Carry atomic spend and ceiling |
| `src/agentsfleetd/fleet_runtime/{config,config_context,config_helpers,config_parser,config_repositories,config_repositories_test,config_types,config_types_test,crew_bundle_test,repository_binding_json,sql}.zig` | EDIT/CREATE | Carry the trusted repository base, separate stored parsing, and keep config parsing below the file limit |
| `src/agentsfleetd/http/handlers/ingress/github.zig` | EDIT | Route repair and deployment events |
| `src/agentsfleetd/http/handlers/ingress/github/{deployment_result,production_repair_result,repair_gate_resolve,repair_link,repair_link_provenance}.zig` | CREATE | Normalize, prove, and correlate |
| `src/agentsfleetd/http/handlers/ingress/github_integration_test.zig` | EDIT | Prove App ingress and correlation |
| `src/agentsfleetd/integration_tests.zig` | EDIT | Register live-dependency suites |
| `src/agentsfleetd/http/handlers/{library/onboard_integration_test,fleets/*}.zig` | EDIT | Prove verifier onboarding and validate authored config |
| `src/agentsfleetd/http/handlers/runner/{assigned_policy_integration_test,credentials_mint,credentials_mint_integration_test,credentials_mint_scope,credentials_mint_write_gate,lease,sql}.zig` | EDIT | Negotiate lease versions and reserve bounded write requests |
| `src/agentsfleetd/http/*integration_test.zig` | EDIT | Prove ingress, runner, and lease behavior |
| `src/agentsfleetd/http/handlers/webhooks/github.zig` | EDIT | Use shared repair interception |
| `src/agentsfleetd/http/handlers/webhooks/github_repair_link.zig` | DELETE | Remove the route-local arm |
| `src/agentsfleetd/http/webhook_http_integration_test.zig` | EDIT | Prove per-Fleet ingress behavior |
| `src/agentsfleetd/observability/{metrics_runner,metrics_repair_verification,otel_instruments,otel_metric_meta,otel_metrics,otel_metrics_attribution,otel_metrics_census_test,otel_metrics_dims,otel_metrics_families,semantic_schema_test}.zig`, `docs/architecture/observability.md` | EDIT/CREATE | Define and wire bounded verifier signals through the closed OTLP registry |
| `src/agentsfleetd/queue/redis_repair_verification.zig` | CREATE | Enqueue each intent once |
| `src/agentsfleetd/state/{repair_pr_links,sql,repair_evidence,repair_evidence_integration_test,repair_production_results,repair_run_results,repair_sql,repair_verification_fanout,repair_verifications,repair_verifications_test}.zig` | EDIT/CREATE | Own evidence, reconciliation, statements, and attempts |
| `src/agentsfleetd/tests.zig` | EDIT | Register daemon unit coverage |
| `src/agentsfleetd/types/{id_format,id_format_test}.zig` | EDIT | Validate compact identifiers |
| `src/lib/contract/{contract,execution_policy,protocol,protocol_lease_v1,protocol_test}.zig` | EDIT/CREATE | Export provider-neutral HTTP request rules and mixed-version lease negotiation |
| `src/runner/daemon/{control_plane_client,control_plane_client_lease,control_plane_client_test}.zig` | EDIT/CREATE | Negotiate and tolerate mixed-version daemon leases |
| `src/agentsfleetd/git/repository_http_policy.zig` | CREATE | Build the daemon-owned repository HTTP allowlist |
| `src/runner/engine/runtime/{credential_placement,http_request_policy,http_request_policy_test,policy_http_request,policy_http_read_only_test,policy_http_request_test,request_args}.zig` | EDIT/CREATE | Enforce canonical generic request rules and centralize arena-backed request substitution |
| `src/runner/engine/tool_builders_test.zig` | EDIT | Prove the split request-policy module remains reachable |
| `tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md` | EDIT | Keep the write-binding fixture authorable with an explicit base |
| `cli/test/{json-contract.test.ts,acceptance/help-and-errors.spec.ts}` | EDIT | Keep unauthenticated checks isolated from operator credentials |
| `ui/packages/app/lib/events/{event-summary,event-summary.test}.ts` | EDIT | Render terminal refusal causes and stored recovery guidance before generic approval copy |
| `tests/bench/micro.zig` | EDIT | Keep the credential-broker benchmark aligned with binding-aware minting |

## Applicable Rules

- `~/Projects/dotfiles/docs/greptile-learnings/RULES.md` — No Dead Code (NDC), No Legacy Retained (NLR), String Literals Are Constants (UFS), Cross-layer Orphan Sweep (ORP), File and Function Length Limits (FLL), pre-v2.0 Schema Removal (SCH), and Integration Tests use real Fixtures (ITF).
- `~/Projects/dotfiles/dispatch/write_zig.md` — query drains, public shape, lifecycle, test discovery, and both Linux cross-compiles.
- `~/Projects/dotfiles/dispatch/write_ts_adhere_bun.md` — Bun test isolation and typed import discipline.
- `~/Projects/dotfiles/docs/SCHEMA_CONVENTIONS.md` — additive single-concern slots 831–835.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` and `~/Projects/dotfiles/docs/LIFECYCLE_PATTERNS.md` — structured outcomes and owned dispatcher resources.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| Zig, public shape, lifecycle | yes | domain-owned modules, drains, cleanup, both Linux targets |
| File and function length | yes | separate Git, ingress, state, queue, and metric modules |
| Named literals and errors | yes | one constant per wire value; typed fail-closed refusals |
| Schema | yes | slots 831–835, grants, embed, reapply and index tests |
| User interface and design token | yes | existing event-summary branch only; no new markup, styles, or tokens |

## Prior-Art / Reference Implementations

- `schema/830_repair_pr_links.sql` supplies immutable linkage history and stays frozen.
- `src/agentsfleetd/fleet_runtime/webhook/normalizer/github_app.zig` supplies pure provider normalization.
- `src/agentsfleetd/state/fleet_events_store.zig` supplies standard Fleet results.
- `library/incident-responder/` supplies read-only Grafana, Elasticsearch, and GitHub declarations.
- GitHub supplies `merge_commit_sha` and deployment commit identity; neither is reconstructed from current branch state.

## Sections (implementation slices)

### §1 — Issue and resolve one trusted repair branch — DONE

The daemon's `git/` modules encode one approved write-gate Universally Unique Identifier version 7 (UUIDv7) into a compact branch and resolve it back to authoritative Fleet context. The user-authored trigger supplies the trusted Pull Request base. The daemon binds repository, base, and branch into provider-neutral HTTP rules for exact methods, paths, and locked top-level JSON fields. The runner evaluates those immutable rules and carries no GitHub code or repair-progress state. Before any write, the repairer searches GitHub for the exact repository, branch, base, and all-state Pull Request; it returns the existing Pull Request, creates only the missing Pull Request for a validated existing ref, or creates the missing Git objects, ref, and draft Pull Request. An ambiguous response is resolved by rereading GitHub before another write. Both GitHub routes intercept repair traffic before normal routing.

- **Dimension 1.1 — DONE** — daemon supplies the compact branch → `test_repair_branch_uses_compact_gate_reference`
- **Dimension 1.2 — DONE** — both routes resolve one Fleet-plus-event owner → `test_ingress_resolves_gate_event_owner`
- **Dimension 1.3 — DONE** — malformed, unknown, unapproved, foreign, or forked repairs write nothing → `test_invalid_or_unapproved_repair_reference_is_ignored` and `test_foreign_repair_pr_is_refused`
- **Dimension 1.4 — DONE** — repair traffic records evidence without waking a Fleet → `test_repair_traffic_never_wakes`
- **Dimension 1.5 — DONE** — generic rules accept only the exact host, repository paths, ref, head, base, and draft flag; restart reconciliation uses GitHub state rather than runner memory → runner request-policy tests and repairer skill review

### §2 — Retain immutable run and merge evidence — DONE

Every completed repair workflow remains append-only by provider run; a merged PR records the provider-returned commit once. Preview evidence never closes production.

- **Dimension 2.1 — DONE** — independent runs remain independent → `test_repair_runs_append_independently`
- **Dimension 2.2 — DONE** — early delivery and replay retain one row per run → `test_early_repair_run_is_retained`
- **Dimension 2.3 — DONE** — merged PR records the exact provider hash once → `test_merged_pr_records_provider_hash`
- **Dimension 2.4 — DONE** — unmerged or hashless PR cannot correlate and mutable deploy stamping has no caller → `test_unmerged_or_hashless_pull_request_never_records_merge` plus the dead-code sweep

### §3 — Bound write-credential spending — DONE

Each write-credential request reserves one of 32 uses before vault or provider access. Cache hits and failures consume a reservation because authority covers the request.

- **Dimension 3.1 — DONE** — request reserves before secret access → `test_write_request_reserves_before_vault_load`
- **Dimension 3.2 — DONE** — failed request retains its reservation → `test_failed_write_request_still_spends`
- **Dimension 3.3 — DONE** — request past 32 returns the registered refusal → `test_write_request_past_ceiling_refuses`
- **Dimension 3.4 — DONE** — 100 concurrent requests cannot exceed 32 → `test_concurrent_write_requests_hold_ceiling`
- **Dimension 3.5 — DONE** — approval card states the same ceiling → `test_approval_card_states_ceiling`

### §4 — Normalize production identity — DONE

GitHub deployment status retains deployment and status identifiers, repository, production environment, commit, conclusion, and completion. Provider status identifier is the append identity, so a later terminal status for one deployment remains evidence instead of colliding with an earlier status. Vercel qualifies only through GitHub.

- **Dimension 4.1 — DONE** — terminal GitHub production status normalizes → `test_github_production_status_normalizes`
- **Dimension 4.2 — DONE** — Vercel-through-GitHub uses the same shape → `test_vercel_github_status_normalizes`
- **Dimension 4.3 — DONE** — non-terminal, non-production, or incomplete status queues nothing → `test_unready_deployment_status_is_ignored` and `test_incomplete_deployment_identity_is_ignored`
- **Dimension 4.4 — DONE** — App registration records event, permission, producer boundary, and live proof → release-gate playbook review
- **Dimension 4.5 — DONE** — failure followed by success for one deployment retains both provider statuses and correlates the success → GitHub ingress integration test

### §5 — Correlate and dispatch exactly once — DONE

Both arrival paths call one evidence service under a transaction-scoped lock keyed by workspace, repository, and commit. A merged close delivery upserts the immutable Pull Request link when its opened delivery was missed. Exact correlation creates every verifier intent with one set-based insert. The dispatcher takes fenced short claims with `FOR UPDATE SKIP LOCKED`, releases the database connection before Redis input/output, and completes only the claim token it owns. Redis replay and stale-claim recovery converge on the original event identifier.

- **Dimension 5.1 — DONE** — result-first, merge-first, simultaneous arrival, and replay converge once → `integration: production results reconcile in either arrival order and emit verifier once`
- **Dimension 5.2 — DONE** — mismatch, missing identity, another workspace, or ambiguity emits nothing → same correlation integration test
- **Dimension 5.3 — DONE** — Redis-before-completion replay returns the original event → same correlation integration test
- **Dimension 5.4 — DONE** — due time and multi-verifier fan-out remain independent → same correlation integration test
- **Dimension 5.5 — DONE** — dispatcher starts, joins, and uses matching indexes → daemon lifecycle and index-use integration tests
- **Dimension 5.6 — DONE** — 100 distinct keys progress while one exact key waits → `integration: distinct production correlations admit one hundred requests concurrently`
- **Dimension 5.7 — DONE** — a merged close delivery without an earlier opened delivery creates and merges one link → GitHub ingress integration test
- **Dimension 5.8 — DONE** — 100 concurrent dispatchers claim one intent once, stale claims recover, and a poison row does not starve later work → dispatcher integration tests
- **Dimension 5.9 — DONE** — Redis failure occurs with no database connection held and retries through the same fence → dispatcher integration tests

### §6 — Run the read-only verifier — DONE

`incident-verifier` subscribes only to `repair_production_result`, reads the exact commit and completed telemetry window, and writes one human-readable result to standard Fleet history.

- **Dimension 6.1 — DONE** — bundle onboards normally and rejects raw deployment status → `test_incident_verifier_onboards`
- **Dimension 6.2 — DONE** — repository token is read-only → `test_incident_verifier_token_is_read_only`
- **Dimension 6.3 — DONE** — instructions use event merge hash, never current default branch → `test_verifier_uses_event_commit_hash`
- **Dimension 6.4 — DONE** — missing or contradictory telemetry yields `inconclusive` → `test_verifier_does_not_guess_without_evidence`
- **Dimension 6.5 — DONE** — bundle parse, lease wire, and runner enforce read-only methods and exact query paths → `test_lease_read_only_http_restrictions_survive_the_runner_wire`

## Interfaces

```text
repair branch: agentsfleet-repair/<22-character approved-gate reference>
TRIGGER.md repository_base + repository -> approved gate -> trusted request rules
repair reference -> approved gate -> workspace_id + fleet_id + event_id + repository + trusted PR base + repair branch
slot 831: immutable workflow runs, unique by fleet + repository + provider run
slot 832: merged_commit_sha and merged_at, written once on slot 830 linkage
slot 833: spend_count and spend_ceiling, atomically reserved before mint
slot 834: normalized production result, unique by workspace + provider + provider status
slot 835: production result + repair link + verifier fleet + verify_after + fenced short claim + optional event id
repair_production_result: linked incident + repair + production + evidence window
```

The slot 835 identifier is the Redis enqueue-once key. Only the returned stream identifier may complete the event link. After completion, `redis_once_key_cleared_at` records deletion of the transient key. Cleanup is retryable and cannot reopen a completed intent. `response_text` remains the human-readable result; daemon code does not parse a second status.

## Failure Modes

| Mode | Handling |
|---|---|
| Invalid branch, gate, event, installation, author, or fork | named refusal; write nothing |
| Run arrives before PR or provider replays | retain idempotently |
| Merged close arrives without opened delivery | upsert the exact link, then record its merge |
| PR closes unmerged or hashless | retain unmerged; never infer current `main` |
| Spend exhausted or concurrent | row lock holds 32-request ceiling |
| Vault or GitHub fails after reservation | reservation remains spent |
| Runner stops before or after a GitHub write | repairer rereads the exact ref and all-state Pull Request before another write |
| Signature invalid | reject before parse or routing |
| App subscription or permission absent | signed development proof blocks setup |
| Push-capable mapped actor writes status | accept; Session Notes record expected and received identity |
| Non-production, missing identity, or commit mismatch | retain bounded ignore reason; emit nothing |
| Both sides arrive simultaneously | shared correlation lock serializes the exact key |
| Later terminal status follows an earlier status for one deployment | retain each provider status; correlate the qualifying status |
| Several repairs match | refuse and alert; never guess |
| Verifier absent or several match | retain result; zero or one independent intent per match |
| Process stops around Redis append | stale fenced claim recovers; only its owner can complete; Redis replay returns the original event identifier |
| Process stops during Redis once-key cleanup | durable completion prevents another dispatch; idempotent deletion and its cleanup marker retry |
| Evidence window incomplete | leave pending; no Fleet waits or guesses |
| Telemetry unavailable or verifier fails | record `inconclusive` or failure; never `cleared` |

## Metrics & Observability

- Counters cover intake, bounded ignore reasons, correlation, dispatch, replay, and verifier completion.
- Gauges expose oldest due-intent age and a bounded due sample.
- Histograms cover production-to-queue and queue-to-completion latency with the fixed evidence window represented.
- Logs carry scoped identifiers and commit prefixes; never webhook bodies, full repair references, or credentials.

## Invariants

- Repair traffic records before being dropped and never wakes the repairer.
- Repair identity resolves one approved gate and exact Fleet event or writes nothing.
- The repair lease carries provider-neutral request rules bound to repository, branch, and trusted Pull Request base; runner-local flags carry no authority.
- GitHub procedure belongs to the user-authored skill; the runner imports no GitHub repair module.
- Runs are immutable preview evidence; merge identity comes only from GitHub and changes at most once.
- Write spend never exceeds 32.
- Only exact workspace, repository, production environment, and merged commit wake verification.
- Either arrival order, simultaneous arrival, and replay converge on one result and intent.
- Missing opened delivery does not prevent a later trusted merged delivery from creating the immutable link.
- Each provider status is retained independently; deployment identifier remains correlation evidence, not append identity.
- Every intent reaches zero or one event; retry returns the same event identifier.
- No database connection or row lock is held during Redis input/output.
- Redis once-keys are removed only after durable event completion; cleanup retries cannot reopen the intent.
- The evidence window is fixed; no timing setting or baseline engine is added.
- Raw deployment status never selects the verifier.
- The verifier reads event context, not internal tables, and cannot write, merge, revert, or deploy.
- Pending, failed, and inconclusive verification are never presented as cleared.

## Test Specification (tiered)

| Dimension | Tier | Test | Concrete assertion |
|---|---|---|---|
| 1.1–1.5 | unit + integration | repair branch, generic runner policy, skill reconciliation, and both-ingress tests | exact grammar, trusted base, provider-neutral enforcement, restart reconciliation, provenance, owner, no wake-up |
| 2.1–2.4 | integration | repair history and merge tests | immutable, early, replay-safe, exact hash |
| 3.1–3.5 | integration | write-gate tests | pre-secret spend, failure spend, refusal, card, concurrency |
| 4.1–4.3, 4.5 | unit + integration | deployment normalizer and GitHub ingress tests | GitHub shape, Vercel parity, ignored input, successive statuses |
| 4.4 | release gate | App playbook plus signed delivery evidence | registration and producer audit recorded |
| 5.1–5.9 | integration | correlation, lifecycle, index, dispatch, and concurrency tests | order, missing delivery, replay, fencing, Redis cleanup, isolation, set-based fan-out, and 100-way progress |
| 6.1–6.5 | unit + integration | verifier bundle, mint, lease, and runner tests | trigger, exact hash, read-only policy, inconclusive result |
| migration + regression | integration | schema reapply and prior write-gate tests | slots apply idempotently; prior refusal meanings remain |

## Acceptance Rubric (single scoring surface)

All rows are priority zero (P0).

| # | Criterion | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | both routes retain trusted immutable repair history | `make test-integration` | exit 0 | P0 | ✅ full suite passed |
| R2 | write requests hold the 32-use ceiling | `make test-integration` | exit 0 | P0 | ✅ full suite passed |
| R3 | exact deployed bytes wake one verifier per match | `make test-integration` | exit 0 | P0 | ✅ full suite passed |
| R4 | verifier lease remains read-only | `make test-unit-all` | exit 0 | P0 | ✅ all unit lanes passed |
| R5 | 100 independent correlations make progress | `make test-integration` | exit 0 | P0 | ✅ full suite passed |
| S1 | conformance | `make harness-verify` | exit 0 | P0 | ✅ all gates green |
| S2 | no leaks | `make memleak` | exit 0 | P0 | ✅ all leak lanes passed |
| S3 | version consistency | `make check-version` | exit 0 | P0 | ✅ 0.26.2 matches |
| S4 | both Linux targets compile | `make _prepare_prebuilt_linux_binaries` | exit 0 | P0 | ✅ x86-64 and ARM64 built |
| S5 | no secrets | `gitleaks detect` | exit 0 | P0 | ✅ 4,298 commits scanned; no leaks |

**Grading protocol:** run each command verbatim. Every row must be graded and pass before close.

## Dead Code Sweep

- Remove the route-local repair-link file, mutable deploy writer, status constants, and statement.
- Confirm no old common repair codec/export or old trusted-context path remains.
- Confirm slot 831, 834, and 835 writers and readers have production callers.
- Confirm verifier code never substitutes current `main` for the event hash.

## Out of Scope

- Automatic merge, rollback, deploy, or another repository write after verification.
- Direct Vercel ingress or provider payloads lacking exact repository, environment, and commit.
- Stored crew identity, coordinator Fleet, or vendor-specific Grafana and Elasticsearch Fleets.
- Multi-repository repair or source-code correctness declared from model opinion.
- Producer-specific deployment-status allowlists or daemon-side Vercel attestation.
- Custom incident card, event-detail API extension, or disposable-repository acceptance target.

## Product Clarity (authoring record)

1. **Successful user moment** — Fleet history shows a telemetry-backed result for the exact deployed repair.
2. **Preserved behavior** — one draft PR, human review and merge, and existing refusal meanings remain.
3. **Built surface** — daemon repair identity, immutable history, bounded spend, production correlation, verifier bundle, and standard results; no automatic repository action or dashboard card.
4. **Clarity** — provider identities are direct proof, and named refusals expose missing authority, hash, environment, or verifier.

## Decomposition & alternatives

- **Chosen:** one active workstream owns trusted repair history through production verification because one user outcome and PR cross the same correlation spine.
- **Chosen ownership:** the user skill owns GitHub procedure, the daemon owns trusted repair identity and generic request rules, the runner owns provider-neutral enforcement, and the evidence service plus dispatcher own production closure. HTTP handlers parse and delegate; they do not own transactions.
- **Chosen repair progress:** exact GitHub refs and draft Pull Requests are durable state; process-local booleans are removed.
- **Chosen dispatch:** PostgreSQL owns fenced short claims and Redis owns transport; no connection crosses the network input/output boundary.
- **Rejected:** parallel active M157_003 and prior workstream 004 (M157_004) specs; the lifecycle gate requires one stream per worktree.
- **Rejected:** Fleet roles, Fleet-name identity, a crew table, verifier database reads, current-branch inference, or mutable preview stamps.
- **Patch-vs-refactor:** Indy selected the larger ownership refactor because it removes provider code and process-local repair state from the runner, handler-owned transactions, per-row fan-out, and connection-held network input/output in one coherent design.

## Discovery (consult log)

- **Branch lookup:** product work is `feat/m157-repair-loop` and merges to `main`; external documentation stayed untouched per Indy's standing restriction.
- **Spec consolidation:** Indy approved folding M157_004 into M157_003 on Aug 11, 2026 after `orly gate work` rejected two active specs.
- **Git ownership:** Indy directed daemon-only Git helpers to `src/agentsfleetd/git/`; shared helpers would live under `src/lib/common/git/`, and runner-only helpers under `src/runner/git/`.
- **Crew decision:** responder, repairer, and verifier remain independent Fleets; Grafana and Elasticsearch are shared evidence sources.
- **Correlation decision:** only exact provider-returned merge and production commit identity wake verification.
- **Provider decision:** GitHub deployment status includes Vercel-through-GitHub; direct Vercel ingress stays outside scope.
- **Verifier routing:** `> Indy (Aug 10, 2026: 08:42 PM): "2A i want a simpler approach to get this tested"` — exact correlation emits `repair_production_result`.
- **Evidence window:** `> Indy (Aug 10, 2026: 09:14 PM): "I want to keep the scope simple, so dont keep overengineering, if there is a simple way to do so follow that."` — one fixed evidence window; no new setting or baseline engine.
- **Spend decision:** each write request reserves one of 32 uses before vault or GitHub access; failures consume the reservation.
- **Refactor decision:** `> Indy (Aug 12, 2026): "Okay I select A"` — use GitHub as durable repair progress, one evidence reconciler, and fenced short dispatcher claims; deployment workflow and Fly configuration stay untouched.
- **Deployment exclusion:** `> Indy (Aug 12, 2026): "No i donot approve edits to release.yml or fly/** since its pointless and continue"` — `.github/workflows/release.yml` and `deploy/fly/**` remain untouched; no release ordering claim depends on them.
- **Ownership decision:** `> Indy (Aug 12, 2026): "Yes agree on the decision"` — GitHub procedure stays in `SKILL.md`, repository and trusted base stay in `TRIGGER.md`, the daemon authors generic HTTP rules, and the runner contains no GitHub repair code or process-local progress.
- **Security review:** Chief Security Officer (CSO) review found no P0. Repair mode now rejects credential placeholders recursively in bodies, URLs, and non-`Authorization` headers before substitution or minting; it permits only the exact Elasticsearch query POST and provider-validated GitHub writes. Slack write/report authority was removed.
- **In-PR cleanup:** Indy approved folding touched-code cleanup into this PR. Request argument substitution is now one stateless arena-backed builder; it preserves failure ordering, adds no new heap owner, and keeps credential resolution isolated from policy evaluation and network dispatch.
- **Metrics review:** operational repair-verification signals change; no analytics funnel or user-interface event changes.
- **Skill-chain outcomes:** `/write-unit-test`, `/write-integration-test`, and `/review` completed; all valid findings were fixed and the final adversarial pass returned no findings.
- **Deferrals:** none.
