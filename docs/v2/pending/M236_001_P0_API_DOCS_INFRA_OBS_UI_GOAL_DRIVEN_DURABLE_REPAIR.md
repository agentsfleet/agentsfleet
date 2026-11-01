# M236_001: Goal-driven durable repair

**Prototype:** v2.0.0
**Milestone:** M236
**Workstream:** 001
**Date:** Nov 01, 2026
**Status:** PENDING
**Priority:** P0 — operational repairs must survive requests and worker replacement
**Categories:** API, DOCS, INFRA, OBS, UI
**Batch:** B1 — one unified development-autonomy implementation scope
**Branch:** pending, set at implementation start
**Baseline revision:** pending, set at implementation start
**Test Baseline:** pending, measured before implementation publication
**Baseline evidence:** pending, implementation has not started
**Depends on:** M235 supplies bounded GitHub repair; M230 supplies shared compaction; M224 separately verifies the reduced manual-first path
**Provenance:** agent-drafted from Indy's goal/Jev and durable repair decisions; scenario reviewed in two Sol Max rounds and an Astra completeness round
**Canonical architecture:** `docs/architecture/scenarios/durable-repair.md` owns the combined outcome; topic pages retain their existing mechanisms

## Overview

**Goal (testable):** An authorized repair continues toward its reviewed goal across later CI results and worker replacement, while human conversation neither restarts the task nor widens its authority.
**Problem:** A one-request repair loses cross-event ownership; another author's PR lacks admitted update authority; saved conversation alone cannot recover files or uncertain writes.
**Solution summary:** Add daemon-owned task identity and waits to the existing admission/lease path, task-aware human input, evidence-bound Jev decisions and compatible workspace recovery.
**Design flow:** accepted goal/task → action and evidence → Jev decision → continue, durable wait, verified completion or named blocker; every effect remains within native authority.

This is a pending implementation-spec draft, not implemented autonomy or ready-to-build approval.
The unresolved publication and Jev-provider decisions in Discovery must be resolved before runtime edits.
Production launch and a new Firecracker backend are excluded.

## PR Intent & comprehension handshake

- **PR title:** feat(fleets): continue goal-driven repairs across events and workers
- **Intent:** Implement one coherent task lifecycle, including an explicitly authorized external PR, rather than three unrelated features.
- **Authoring handshake:** ASSUMPTIONS I'M MAKING: initial repair work requires an addressed command or approved proposal; the admitted task can continue within its limits without approval on every inference. Scheduled observation can notify without authorizing a repair.
- **Implementer handshake:** Confirm that boundary and the final publication/recovery/provider mechanisms before code. Standing unattended repository-write authority needs a separate explicit decision.

## Implementing agent — read these first

1. `AGENTS.md`, `.orly/LOCAL.md`, `.orly/standards/spec.md` — scope and repository checks.
2. `docs/architecture/scenarios/durable-repair.md` — combined Linkwarden and Grafana outcomes, human input and failure presentation.
3. `docs/architecture/{direction,lease_flow,data_flow,fleet_definitions}.md` — one runtime, admission and setup authority.
4. `docs/architecture/{sandbox_evaluation,runner_execution,context_lifecycle,memory}.md` — distinct task, conversation, file and semantic-memory responsibilities.
5. `rustd/crates/afr_agent/src/{prompt/mod.rs,loop/mod.rs,loop/conversation.rs}` — current root prompt/loop boundary.
6. `rustd/crates/afd_gate/src/policy/{repair.rs,egress/write.rs}` — current event-owned authority, not arbitrary-PR permission.
7. `rustd/crates/afd_api_ingress/src/handler/webhook/{github/mod.rs,app_route.rs}`; `rustd/crates/afd_cron/src/fire/mod.rs` — filtering, scheduling and reply destination limitations.
8. `docs/reviews/development-autonomy/README.md` — actual review outcomes, not runtime acceptance.

## Files Changed (blast radius)

The authoring change adds this spec; implementation must refine the new-module paths below before code and allocate schema slots from freshly fetched main.
Topic mechanisms are linked instead of copied into additional scenario documents.

| Files | Action | Role |
| --- | --- | --- |
| `schema/2190_fleet_tasks.sql`; `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | proposed task/wait/input/effect state and registration; reallocate slot if main advances |
| `rustd/crates/afd_fleet/src/lease/`; new task lifecycle module in that crate and its tests | EDIT / CREATE | trusted task claims, current authority, waits, cumulative limits and input receipts |
| `rustd/crates/afd_wire/src/{lease,policy,report}/`; matching wire tests | EDIT | explicit task generation, goal revision and decision/evidence boundaries |
| `rustd/crates/afr_agent/src/{prompt,loop}/`; matching agent tests | EDIT | current intent, consumed human input and goal-aware action/decision loop |
| `rustd/crates/afr_providers/src/`; matching provider tests | EDIT | selected Jev adapter, accounting, bounded failures and injection-safe evidence input |
| `rustd/crates/afd_gate/src/policy/`; `rustd/crates/afd_credential/src/credential/github/`; matching policy tests | EDIT | reviewed task/existing-PR authority and selected publication precondition |
| `rustd/crates/afr_supervisor/src/{holds,lease_loop}/`; matching recovery tests | EDIT | compatible task-bound workspace recovery and effect reconciliation |
| `rustd/crates/afd_api_ingress/src/handler/webhook/`; `rustd/crates/afd_cron/src/fire/`; existing background-worker framework and tests | EDIT | signed completion routing, durable waits/fallbacks and monitoring occurrences |
| `rustd/crates/afd_api_tenant/src/handler/`; `rustd/crates/afd_api_wire/src/`; `rustd/crates/afd_http/src/`; `public/openapi.json` | EDIT if needed | expose authenticated task/input/status through reviewed existing resource ownership; exact new routes need Interfaces amendment |
| `ui/packages/app/components/domain/`; `ui/packages/app/lib/api/`; related application/browser tests | EDIT | task-aware conversation, progress, steering and distinct recoverable failures |
| `docs/architecture/` owning topics; this spec and eventual done destination; `docs/DESIGN_SYSTEM.md` | EDIT / MOVE | architecture and actual final evidence without duplicate owners |
| `rustd/crates/afd_outbound/src/`; its delivery tests | EDIT | correlate notification intents, native receipts and uncertain-delivery reconciliation |
| `tests/fixtures/fleets/goal-repair/cases.json`; `playbooks/operations/acceptance/baselines/M236_001-autonomy.md` | CREATE | pin the actual-model inputs/expectations and concise attributed evidence ledger, without a new bespoke checker |
| `docs/architecture/scenarios/durable-repair.md`; scenario index | EDIT | acceptance results and current/proposed distinction |

## Applicable Rules

- `.orly/standards/domains/architecture.md`: ARCH-GROUND, ARCH-LAND, ARCH-HOMES — one owner per runtime mechanism.
- `.orly/standards/domains/auth.md`: AUTH-DOC, AUTH-GUARD, WAUTH, AWO, AUTH-NOLOG, CTX — current workspace/grant checks, secret isolation and task ownership.
- `.orly/standards/languages/rust.md`: ERR-RS, RS-FROM, RS-BORROW, RS-RACE — native state/errors and deterministic contention proofs.
- `.orly/standards/tests.md`: TEST-REDGREEN, TEST-OUTCOME, TEST-ERROR, TEST-RESIDUAL, TEST-ACKLOSS — production entrypoints and negative outcomes.
- `.orly/standards/spec.md`: SPEC-MECH, SPEC-DIM, SPEC-FILES, SPEC-E2E — resolve open mechanisms before implementation and map every proof.

## Applicable Gates

Authoring is Markdown-only. Implementation retains `orly gate pr`, `make lint-all`, `make test-unit-all`, `make test-integration-rustd`, `make check-version` and gitleaks.
Run one native test lane at a time with the cloud Cargo settings; no test, coverage, authentication or TLS bypass is authorized.
User-visible source changes need browser checks and matching documentation in its owning repository.

## Prior-Art / Reference Implementations

Reuse existing admissions, lease fencing, supervised workers, native setup Review/Apply, provider transport and shared compaction.
The inspected Codex client illustrates pending-input/history and transport separation; its source does not establish hidden server behavior or a classifier to copy.
Use current native state as authority; semantic memory and model summaries cannot select a publication target.

## Sections (implementation slices)

### §1 — Admit one durable task under reviewed authority

The daemon owns task identity, workspace/fleet, accepted instruction revision, target binding, input/effect receipts, wait state and cumulative money/time/attempt limits.
New admissions reference that owner; they cannot select another PR or reset limits through prose.
A guarded database transition claims the current task generation before a lease is issued; settlement and wait registration commit before releasing its execution slot.

Keep one active lease per fleet and separate incidents/tasks within it.
Initially admit repair only through an authorized direct command or approved proposal, including the verified Slack/workspace identity boundary in the canonical approval design.
The exact store/schema/transition plan must be reviewed before this Section starts; no second queue or scheduler is added.

- **Dimension 1.1** — Repeated admission retains one task/target and cumulative limits → Test `task_admission_preserves_owner_and_limits`.
- **Dimension 1.2** — Stale/cross-workspace claims cannot execute or publish → Test `task_generation_and_workspace_fences_hold`.

### §2 — Separate human conversation from task execution

Interpret the newest human input against trusted task state and standing instructions.
Pure social replies, acknowledgements and status questions do not implicitly restart or reset a task; a status answer may use recorded evidence labelled with its freshness.
An explicit refresh or steer affects the addressed task under current authority; an ambiguous target asks for clarification before investigation.
Mixed greeting-plus-work input must preserve the actual requested work rather than matching a greeting prefix and discarding it.

Persist input receipt and consumption position so retries do not apply the same steer twice.
Consume queued input at defined safe boundaries between inference/tool groups without a second active lease; show queued versus consumed status.
Pin an admitted task's goal revision; changing its completion questions requires explicit reviewed revision adoption, and invalidates stale Jev decisions.

Define cancellation's publication cutoff: reject new effect intents after the committed stop generation, reconcile already-admitted effects, and never imply that an issued remote write was undone.
Social messages cannot revive terminal tasks; a new task needs a fresh admission.
Pause blocks new execution and records its queued/resumable state; deletion, cancellation, deadline/budget exhaustion and a closed/merged target stop late wakes and new effects.
Record blocked versus terminal outcomes explicitly, without resetting the original bounds when a prerequisite is later resolved.

- **Dimension 2.1** — Varied social replies make no investigation calls or task restart → Test `social_input_does_not_restart_a_task`.
- **Dimension 2.2** — Steers are consumed once and cannot silently replace goal/authority → Test `steering_preserves_goal_revision_and_authority`.
- **Dimension 2.3** — Stop prevents new effects while reconciling admitted ones → Test `cancellation_fences_new_effects_without_inventing_rollback`.

### §3 — Evaluate goal progress through Jev

The reviewed `instructions_md` remains the user-editable home of the goal and decision questions; onboarding chat changes it through Review/Apply rather than a hidden parallel question list.
Jev receives that accepted revision plus bounded, attributable current evidence and native task limits.
It returns a schema-validated continue/wait/done/blocked proposal bound to the task generation and assessed evidence/head.

Call Jev at completed action groups or a resumed task boundary, not on token deltas or routine social replies.
Native code verifies required evidence and current authority before accepting done; a successful tool status or model assertion alone cannot prove a passing regression.
Malformed, stale, timed-out, unavailable or unsupported decisions never default to success and share cumulative task cost/attempt limits.

Select and review the actual Jev provider/model/API, catalogue/credential integration and charge accounting before implementation.
No runtime Jev adapter exists by implication of earlier Orly judgment experiments, and no live provider call is authorized by this draft.

- **Dimension 3.1** — Current evidence drives a bounded goal transition → Test `goal_decision_uses_the_accepted_revision_and_evidence`.
- **Dimension 3.2** — Stale/injected/invalid/unavailable judgment cannot complete work → Test `unsupported_goal_decision_cannot_complete_a_task`.

### §4 — Bind existing-PR writes and later CI to that task

An externally opened PR, including one authored through Codex, needs explicit reviewed existing-PR mode distinct from event-owned draft creation.
Recheck the exact open same-repository PR/head/base and current grants on each admission; initially refuse fork heads, protected/default/base branches and workflow edits.
Review workflow/secret/environment eligibility so an admitted push cannot implicitly deploy or gain privileged workflow effects.

Record expected exact-head validation before publication; a signed completion or fallback wake only requests independent current-evidence reads.
Route matching repair/success events before the existing investigation-only filter, without broadening ordinary trigger fan-out.
Duplicate, stale, out-of-order and fast-completion wakes compete through one fenced wait transition; closed/merged/cancelled/exhausted tasks cannot restart.

Choose a publication mechanism that atomically enforces the expected old head before allowing this Section to build.
The current `force:false` rule is not an arbitrary-PR expected-head guarantee; a separate read followed by a write is insufficient.
Persist intent before remote effects, reconcile uncertain acknowledgements, preserve human changes and do not blindly repeat writes.

- **Dimension 4.1** — Human/Codex-opened PR is repaired only under its exact binding → Test `external_pr_repair_preserves_target_and_human_changes`.
- **Dimension 4.2** — Later CI/fallback resumes the same current wait once → Test `ci_wakes_resume_one_current_repair_wait`.
- **Dimension 4.3** — Lost acknowledgements and races cannot duplicate or overwrite effects → Test `publication_reconciles_uncertain_effects`.

### §5 — Recover compatible coding work on another worker

Restore the task's compatible conversation, file/Git generation and confirmed outcomes, not independently selected newest snapshots.
Use the canonical sandbox save/restore design for quiescence, staged/unstaged/untracked coverage, hostile files, isolation and fenced manifests.
API-only repairs require trusted task/effect recovery without a mandatory checkout or GitHub grant for repository-free fleets.

Expose the last recoverable generation after unsaved loss, and reconcile remote PR state before reconstructing local work.
Finalize persistence/retention/encryption/quota/crash-loss and restore-failure choices before implementation; compaction is not workspace persistence.
A new Firecracker backend remains separate from proving these task outcomes on the existing engine.

- **Dimension 5.1** — Replacement worker restores a compatible generation and same PR → Test `replacement_worker_resumes_compatible_task_state`.
- **Dimension 5.2** — Missing/stale/hostile/wrong-task recovery refuses safely → Test `incompatible_recovery_never_attaches_writable_state`.

### §6 — Present progress and prove the Grafana/scheduler outcome

Use short confirmed action/result summaries, explicit waiting/blocked state and expandable evidence; preserve those states across recovery without replaying the incident narrative.
Distinguish browser reconnection, inference interruption and uncertain tool effects; retries preserve task identity and reconcile native state first.
The exact UI labels and scheduler/recovery narrative remain owned by the canonical scenario and design-system guidance.
Bind notification intents to the task/generation/destination and retain native delivery receipts.
After an uncertain send, reconcile supported provider evidence before retry; if delivery cannot be established, show unconfirmed delivery rather than blindly resending or claiming exactly-once external posting.

Use a controlled development symptom visible through an actually configured Grafana data source, plus GitHub and a configured Slack destination.
Existing schedules observe quietly or correlate one incident/notification; diagnosis alone does not admit repair.
After authorized repair/CI and separately authorized development deployment, a fresh scheduled query verifies the symptom or reports insufficient data.

Prove duplicate ticks, overlapping/busy fleets, independent due fleets, pause/resume, cancellation/deletion and stale queued work against existing scheduler semantics.
Pin real query/window/threshold and measured timing tolerances before the demonstration; Elasticsearch and production are excluded.

- **Dimension 6.1** — Progress and recovery notices reflect confirmed native state → Test `task_progress_and_failure_recovery_remain_truthful`.
- **Dimension 6.2** — Scheduled Grafana observation correlates notification and fresh recovery evidence → Test `scheduled_grafana_incident_reaches_verified_development_outcome`.
- **Dimension 6.3** — Notification retries preserve intent and honest delivery state → Test `notification_retry_reconciles_native_delivery_state`.

## Interfaces

Retain one admission/lease/report path; task identity, generation, goal revision, authority and notification destination come from native state.
The Jev result is a bounded native decision proposal, never executable policy.
Exact wire fields, new routes and publication transport must be added here with their final design review before implementation; incompatible runners must refuse rather than silently run a task as a one-shot event.

## Failure Modes

Stale/cross-workspace ownership and revoked grants map to `task_generation_and_workspace_fences_hold`.
Ambiguous input, duplicate steer and stale goal revisions map to `steering_preserves_goal_revision_and_authority`.
Timeout/malformed/injected Jev results map to `unsupported_goal_decision_cannot_complete_a_task`.
Human pushes, lost acknowledgements and duplicate/out-of-order checks map to the Section 4 negative tests.
Unsaved loss, wrong generation, quota exhaustion and hostile restore map to `incompatible_recovery_never_attaches_writable_state`.
Provider/browser interruption and unknown remote effects map to `task_progress_and_failure_recovery_remain_truthful`; do not hide them as success.

## Invariants

Database claims and generation checks enforce one current owner; native grants and publication preconditions enforce target reach.
Persisted cumulative usage/attempt/deadline state prevents a wake from resetting limits.
A native proof check rejects unsupported Jev completion; task-bound manifests reject cross-task recovery.

## Metrics & Observability

Carry existing task/event/lease correlation without logging prompts, credentials or raw private evidence.
Specify any added transition/decision/wake/recovery signals with privacy bounds and tests before implementation; no unreviewed analytics stream is added.
Retain complete bounded call inventory and named evidence references independently of prompt compaction.

## Test Specification (tiered)

| Dimension | Tier | Test | Required observation |
| --- | --- | --- | --- |
| 1.1 | integration | `task_admission_preserves_owner_and_limits` | concurrent duplicate admission → one task/target and retained limits |
| 1.2 | integration | `task_generation_and_workspace_fences_hold` | stale/cross-workspace/revoked execution denied without publication |
| 2.1 | manual | `social_input_does_not_restart_a_task` | real-model varied greetings/thanks, status and explicit refresh → no incidental investigation; fresh checks only when asked; mixed work input is preserved |
| 2.2 | integration | `steering_preserves_goal_revision_and_authority` | retried steer consumed once; ambiguous/new goal cannot select stale task authority |
| 2.3 | integration | `cancellation_fences_new_effects_without_inventing_rollback` | pause/delete/cancel/deadline/budget and write races → admitted effects reconciled, new effects/late wakes refused |
| 3.1 | integration | `goal_decision_uses_the_accepted_revision_and_evidence` | current evidence/revision → permitted transition and accounted usage |
| 3.2 | integration | `unsupported_goal_decision_cannot_complete_a_task` | stale/injected/malformed/unavailable judgment → no false completion |
| 4.1 | integration | `external_pr_repair_preserves_target_and_human_changes` | exact authorized external PR succeeds; other/fork/protected/racing target refused |
| 4.2 | integration | `ci_wakes_resume_one_current_repair_wait` | completion/fallback/fast/duplicate/stale events → one current wake, no late restart |
| 4.3 | integration | `publication_reconciles_uncertain_effects` | committed-before-ack-loss and human push → safe readback, no overwrite/duplicate |
| 5.1 | integration | `replacement_worker_resumes_compatible_task_state` | worker replacement during CI wait → compatible state and same PR |
| 5.2 | integration | `incompatible_recovery_never_attaches_writable_state` | wrong/stale/hostile/over-quota restore → explicit refusal and preserved isolation |
| 6.1 | e2e | `task_progress_and_failure_recovery_remain_truthful` | dry-lane browser tests under `tests/e2e/` (outside acceptance/) show progress and distinct recovery without losing native identity/actions |
| 6.2 | manual | `scheduled_grafana_incident_reaches_verified_development_outcome` | real scheduler/queries/GitHub/Slack and replaced worker → combined recorded outcome |
| 6.3 | integration | `notification_retry_reconciles_native_delivery_state` | repeated/ack-lost notification intent → reconciled receipt or explicit unconfirmed state; no new repair or blind resend |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded |
| --- | --- | --- | --- | --- | --- |
| R1 | Native task, authority, wake and recovery boundaries | `make test-integration-rustd` | exit 0; named negative/concurrency proofs pass | P0 | |
| R2 | Goal and human-input behavior | Manual: freeze `goal-repair/cases.json`, admit each case through native chat/Slack, inspect goal decisions and complete call/task records; grade every row in `M236_001-autonomy.md` | zero false completion, unrelated investigation or authority widening | P0 | |
| R3 | Combined development scenario | Manual: configure the declared Grafana query/window and scheduled observer; use an authorized external Linkwarden PR, replace the worker during exact-head CI wait, then inspect Slack and the fresh post-deploy query in `M236_001-autonomy.md` | exact-head repair/CI, worker recovery, visible Slack and fresh symptom observation retained | P0 | |
| R4 | Rendered progress/recovery | `make dry-app` | exit 0 with the named browser outcomes | P1 | |
| S1 | Declared lint | `make lint-all` | exit 0 | P0 | |
| S2 | Declared unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Declared version | `make check-version` | exit 0 | P0 | |
| S5 | Required PR gate | `orly gate pr` | exit 0 at pushed head | P0 | |
| S6 | No credentials in history | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

### Behaviour evals

Before any live trigger, freeze the case set, accepted setup/model revisions, expected outcomes and evidence destinations in the declared fixture/ledger.
The case set covers pure social input, status versus refresh, mixed work input, ambiguous/new task, normal goal completion, unsupported/stale Jev result, external-PR race, later-CI/fallback races, pause/delete/cancel/budget/deadline, worker restore failure, notification uncertainty and Grafana recovery.
Grade every case for grounding/authority, expected useful result and truthful completion: all N/N with zero critical failures, preserving failed attempts and missing evidence rather than changing the denominator afterward.

The authorized observer records native event/task/head identities, complete tool references, Jev outcomes and visible Slack attribution in the ledger; deterministic fixture tests do not substitute for these observations.
The combined Linkwarden scenario must include an externally opened PR, later CI and worker replacement in one task; isolated passes do not substitute for it.
Codex may repair product defects but must not author the demonstration's fleet repair or provide its diagnosis.

## Dead Code Sweep

Replace overlapping task/incident narratives with links to the canonical scenario; keep current runtime facts in their owning topic pages.
Do not reuse semantic memory, model prose or old one-event branch derivation as task authority.
Keep useful current policy/repair tests; no blanket checker retirement belongs to this future implementation.

## Out of Scope

Production launch, automatic merge/deploy, new Firecracker backend, PR review posting and unreviewed standing repository-write authority are excluded.
The Grafana observer's quiet monitoring does not create an alert subscription or an implicit repairer handoff.
M224 closure and Orly 0.28 cleanup keep their separate owners.

## Product Clarity (authoring record)

1. **Successful user moment** — A repaired PR keeps progressing after CI and worker replacement; the person can ask a question without starting it over.
2. **Preserved user behaviour** — Existing reviewed setup, Slack addressing and human merge/deploy remain recognizable.
3. **Optimal-way check** — Extend the current generic runtime with trusted task ownership and bounded decisions, not a second scheduler.
4. **Rebuild-vs-iterate** — Iterate admission/lease/provider/recovery boundaries; sandbox replacement is separate.
5. **What we build** — One goal/Jev loop, durable PR repair, recovery, human input and combined development proof.
6. **What we do NOT build** — Automatic production actions or a second independent PR writer.
7. **Fit with existing features** — Reuses setup revisions, compaction and grants without treating memory as authority.
8. **Surface order** — Shared native task semantics, then existing chat/Slack surfaces; no separate CLI engine.
9. **Dashboard restraint** — Concise confirmed progress; raw tool/evidence details remain expandable.
10. **Confused-user next step** — Show the missing evidence, permission or decision and the action needed to resume.

## Decomposition & alternatives (patch vs refactor)

One scope contains the three failure boundaries of one task; split implementation by Sections, not competing task runtimes.
A prompt-only loop cannot supply durable waits, atomic publication or workspace recovery.
A backend rewrite cannot supply task authority; neither is a substitute for this design.

## Discovery (consult log)

- **Consults** — Fresh `origin/main` showed M235 as latest milestone and schema 2180 as latest slot; M236 is this pending draft and 2190 must be rechecked at implementation start.
- **Model reviews** — The linked scenario received two Sol Max rounds and one Astra completeness review; their findings were resolved. This newly expanded implementation draft still needs its own authoring review and is not implementation-ready.
- **Open blockers** — Select/review atomic publication transport, durable store/wait transitions, actual Jev adapter and accounting, exact lease/interface changes and recovery retention/loss limits before code. Final proof fixture inputs, manual procedures and authenticated-versus-dry browser coverage must also be frozen before implementation acceptance. These are design work, not permission to invent runtime guarantees.
- **Metrics review** — Event/signal additions remain pending the exact design; sensitive content stays out of telemetry.
- **Skill-chain outcomes** — Scenario intake reviewed; implementation spec drafting in progress; no runtime acceptance claimed.
- **Deferrals and transfers** — Production, Elasticsearch incident work and full Linux/development identity checks are outside reduced M224; the new sandbox backend and review-posting capability remain separate.

> Owner (Indy, Nov 01, 2026, time not recorded): "So i assume in 1 spect all the above + the goal driven JEV based autonomous agent will exists?" — one unified scope selected; writing a spec does not claim implemented autonomy.
