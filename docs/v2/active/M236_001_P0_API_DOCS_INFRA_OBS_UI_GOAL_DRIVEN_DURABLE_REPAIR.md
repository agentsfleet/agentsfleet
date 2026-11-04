# M236_001: Durable Tasks with configurable decisions and goal-driven repair

**Prototype:** v2.0.0
**Milestone:** M236
**Workstream:** 001
**Date:** Nov 01, 2026
**Status:** IN_PROGRESS
**Mechanism status:** APPROVED FOR IMPLEMENTATION; activation and acceptance remain pending
**Priority:** P0 — operational repairs must survive requests and worker replacement
**Categories:** API, DOCS, INFRA, OBS, UI
**Branch:** feat/m236-durable-tasks
**Baseline revision:** 2f8c878c09e9c4048da1724385ec58746d348ed4
**Test Baseline:** Rust unit=4972 after the approved Rig upgrade; JavaScript and integration baselines pending
**Baseline evidence:** playbooks/operations/acceptance/baselines/M236_001-autonomy.md
**Depends on:** M235 bounded repair and M230 compaction; PRs #798–#800 ancestry is retained and PR #802/main `1920ff345` is integrated. Combined-branch verification and final ship gates remain pending.
**Provenance:** agent-drafted from Indy's decisions; historical scenario reviews plus a fresh-context Astra design review and verified correction pass
**Canonical architecture:** `docs/architecture/scenarios/durable-repair.md` owns the combined outcome; topic pages retain their existing mechanisms
**Additional authorized work:** [Runner deadline correction](M236_002_P1_INFRA_OBS_RUNNER_ENDING_DEADLINE.md), [schedule activity](M236_003_P1_API_DOCS_OBS_UI_FLEET_SCHEDULE_ACTIVITY.md) and the [pending signal audit](../pending/M240_001_P1_INFRA_OBS_EXECUTOR_SIGNAL_ISOLATION_AUDIT.md) stay in PR #803, per Indy's “no new PR”.
## Overview

**Goal (testable):** An authorized repair continues toward its reviewed goal across later CI results and worker replacement, while human conversation neither restarts the task nor widens its authority.
**Problem:** A one-request repair loses cross-event ownership; another author's PR lacks admitted update authority; saved conversation alone cannot recover files or uncertain writes.
**Solution summary:** Add daemon-owned task identity and waits to the existing admission/lease path, task-aware human input, evidence-bound decision assessments and compatible workspace recovery.
**Design flow:** message admission → configured intent decision → native DT creation/association → action/evidence → goal assessment → native transition.

Indy approved these mechanisms for implementation on November 3, 2026.
No runtime behavior or passing acceptance is claimed.
Indy selected Durable Tasks (DTs), configurable decisions per runner, initial TypeSafe `jev-latest` and Cloudflare R2.
Concrete interfaces and default limits are defined below and in their owning architecture pages.
Runtime implementation is authorized; live activation requires the declared configuration and proof.
Production launch and a new Firecracker backend are excluded.

## Approved implementation scope

Approve implementation of one DT lifecycle inside the existing admission/lease/report path, with one active lease per fleet.
Configure one reviewed decision profile per runner with platform-admin Secrets; initial Jev shares one trait with three other native decision APIs.
Provider activation requires capability, version, rate and acceptance proofs; model output grants no authority and decision keys have no tenant/environment fallback.

Keep task history, input receipts, exact-head publication intents and incremental encrypted R2 recovery.
Cut separate client publication dispatch, duplicate task-message sends, verb-style control endpoints, additional model registries and provider SDKs without demonstrated benefit.
Use one bounded synchronous native publication call, standard resource methods/pagination and existing renewal/report settlement.

The numerical budgets and storage limits below are approved implementation defaults; measured performance remains unproved.
This authorizes source implementation only; deployment, merging and live external writes remain governed by their existing authorization boundaries.
The extra-high Astra review and correction check are recorded in Discovery; implementation and activation evidence remain separate.

## PR Intent & comprehension handshake

- **PR title:** feat(fleets): continue goal-driven repairs across events and workers
- **Intent:** Implement one coherent task lifecycle, including an explicitly authorized external PR, rather than three unrelated features.
- **Authoring handshake:** ASSUMPTIONS I'M MAKING: initial repair work requires an addressed command or approved proposal; the admitted task can continue within its limits without approval on every inference. Scheduled observation can notify without authorizing a repair.
- **Implementer handshake:** Matched: use existing admission/lease/provider boundaries and latest stable direct dependencies. Standing unattended repository-write authority needs a separate explicit decision. Indy authorizes publication and merge after PR #798 integration, green CI and resolved Greptile threads.

## Implementing agent — read these first

1. `AGENTS.md`, `.orly/LOCAL.md`, `.orly/standards/spec.md` — scope and repository checks.
2. `docs/architecture/scenarios/durable-repair.md` — combined Linkwarden and Grafana outcomes, human input and failure presentation.
3. `docs/architecture/{direction,lease_flow,data_flow,fleet_definitions}.md` — one runtime, admission and setup authority.
4. `docs/architecture/{sandbox_evaluation,runner_execution,context_lifecycle,memory}.md` — distinct task, conversation, file and semantic-memory responsibilities.
5. `rustd/crates/afr_agent/src/{prompt/mod.rs,loop/mod.rs,loop/conversation.rs}` — current root prompt/loop boundary.
6. `rustd/crates/afd_gate/src/policy/{repair.rs,egress/write.rs}` — current event-owned authority, not arbitrary-PR permission.
7. `rustd/crates/afd_api_ingress/src/handler/webhook/{github/mod.rs,app_route.rs}`; `rustd/crates/afd_cron/src/fire/mod.rs` — filtering, scheduling and reply destination limitations.
8. `docs/reviews/space-sandbox/next-specs-handoff.md` — accepted direction and work order; implementation remains pending.
9. `docs/architecture/durable_tasks.md`, `docs/architecture/goal_evaluation.md` — lifecycle and configurable decision providers.
10. `https://docs.typesafe.ai/llms.txt` — discover provider API, primitives, limits and current model meanings.
11. `docs/architecture/scenarios/durable-tasks-three-prs.md` — three-task caller/steer/trigger walkthrough with existing and proposed code boundaries.

## Files Changed (blast radius)

The documentation change amends this pending spec and its owning architecture pages.
The following module boundaries and interface definitions scope implementation; allocate schema numbering against fetched main at implementation start.
Topic mechanisms are linked instead of copied into additional scenario documents.

| Files | Action | Role |
| --- | --- | --- |
| `schema/{2190_fleet_task_items,2200_fleet_tasks,2210_fleet_task_inputs,2220_fleet_task_bindings,2260_runner_decision_model,2270_runner_lease_decisions,2280_runner_decision_calls,2290_runner_decision_payloads}.sql`; `rustd/crates/afd_db/src/{migration.rs,test_util/}`, `tests/{integration_migrate/,integration_admissions_queue.rs}`; `docs/architecture/postgres_schemas.md` | CREATE / EDIT | task ownership, runner model selection, retained decision accounting, test cleanup ownership and explicit migration/schema/role inventories; wait/effect/checkpoint slots follow |
| `rustd/crates/afd_fleet/src/{task/,error/}`; `lease/{claim,issue,reclaim,finalize,settle,context,decision,coverage,billing_sweep,holds,money,installed,mod}.rs`, `lease/{report,renew,usage}/`, `lease/sql/{meter.rs,meter_lease.sql,bill_expired_slice.sql}`; tests | CREATE / EDIT | guarded DT ownership, lifecycle scenarios with gated accepted-plan fixtures, bounded reads, effects, snapshots and settlement integration |
| `rustd/crates/afd_wire/src/{decision,lease,policy,report,task}/`; matching wire tests | EDIT | explicit task generation, goal revision and decision/evidence boundaries |
| `rustd/crates/afr_agent/src/{prompt,loop}/`; matching agent tests | EDIT | current intent, consumed human input and goal-aware action/decision loop |
| `rustd/crates/afr_agent/src/decision/{mod,questions,routing,goal}.rs`; `afr_providers/src/{decision/,lib.rs,error/,retry/}`, `afr_providers/tests/decisions/`; `afd_fleet_runtime/src/{config/,definition/}`; `afd_authoring/src/model/provider/{prompt.rs,prompt/*.md}`; existing authoring review/Apply owners and tests | CREATE / EDIT | one typed decision trait/client; customer-owned instructions/questions; Markdown authoring guidance; linkable starters reuse immutable Library copies and existing setup chat |
| `rustd/crates/afd_gate/src/policy/`; `rustd/crates/afd_credential/src/credential/github/`; matching policy tests | EDIT | reviewed task/existing-PR authority and selected publication precondition |
| `rustd/crates/afr_supervisor/src/{holds,lease_loop}/`; matching recovery tests | EDIT | compatible task-bound workspace recovery and effect reconciliation |
| `rustd/crates/afd_api_ingress/src/handler/webhook/{app_route.rs,github/}`; `afd_ingress/src/app/`; `afd_cron/src/fire/`; `agentsfleetd/src/sweepers/{mod.rs,billing.rs,tasks.rs}`, `serve/mod.rs` and supervisor inventory/tests | EDIT / CREATE | signed completion matching, existing scheduler observation and supervised bounded task maintenance; financial expiry shares the billing loop |
| `rustd/crates/afd_api_tenant/src/handler/{fleet/tasks,stream}/`; `afd_api_runner/src/{handler/runner/{tasks/,decisions.rs,mod.rs},lib.rs,openapi.rs}`; `afd_api_wire/src/fleet/`; `afd_wire/src/{lease,report,paths,usage}`; `afd_http/src/{route/,services/contract.rs}`; `afd_api/tests/{route_*,fleet_tasks.rs,openapi_stream_frames.rs,integration_fleet_tasks/,harness/,runner_plane/,runner_plane_suite.rs,tenant_plane_suite.rs}`; `agentsfleetd/src/plane/{mod,services}.rs`; `public/openapi.json` | CREATE / EDIT / GENERATE | authenticated task/lease APIs and capability negotiation defined by Interfaces |
| `package.json`, `bun.lock`, `ui/packages/{app,design-system,website}/package.json`; `ui/packages/app/components/domain/chat/{TaskPanel,TaskDetail,TaskControls,SetupQuestions,ChatThread,ChatComposerFrame,DraftConversation,UnfinishedSetups}.tsx`, `{use-task-read,chat-defaults}.ts`; `ui/packages/app/public/fleet-starters.json`; `ui/packages/app/components/{domain/SteerComposer,layout/{DraftChatList,DraftSidebarItem}}.tsx`; existing fleet thread, setup/review/history, streaming, `app/live/` and `lib/api/{tasks,decision-policy,fleet-paths,live-json-proxy}.ts`; related tests | CREATE / EDIT | retain setup/chat layout, spinner, theming/save flow; editable customer-selected starters, aligned Resume action, concise fleet-chat guidance, compact work summaries, criteria/revisions and recovery; omit routing rules from setup display |
| `docs/architecture/` owning topics; `docs/AUTH.md`; this spec and eventual done destination; `docs/DESIGN_SYSTEM.md` | EDIT / MOVE | architecture, credential custody and actual final evidence without duplicate owners |
| `rustd/crates/afd_outbound/src/` | REUSE | no default edits; DT records correlate existing obligations and Slack markers |
| `tests/fixtures/fleets/goal-repair/{cases,decision-policy}.json`; `playbooks/operations/acceptance/baselines/M236_001-autonomy.md`; `scripts/toolbox/build.sh`; `make/test-integration-rustd.mk`, `codecov.yml`, `.github/workflows/test-integration-rustd.yml` | CREATE / EDIT | shared editable questions, attributed evidence, visible bootstrap diagnostics, the 99.51% Rust patch floor and approved coverage-worker resource bounds |
| `docs/architecture/scenarios/durable-repair.md`; scenario index | EDIT | acceptance results and current/proposed distinction |
| `docs/architecture/{durable_tasks,goal_evaluation}.md` | CREATE | lifecycle and decision-provider owners |
| `docs/architecture/{README,data_flow,lease_flow,fleet_definitions,context_lifecycle,sandbox_evaluation}.md` | EDIT | owning mechanisms and explicit cross-lease handoff |
| `rustd/crates/afr_supervisor/src/recovery/{mod,capture,restore,store,manifest}.rs`; tests; Cargo dependency files | CREATE / EDIT | encrypted incremental R2 checkpoints through proposed object_store transport |
| `rustd/crates/afd_admission/src/{admission,admit}.rs`, `stage/` and SQL; tests | EDIT | task binding and shared single-use active-loop input claims |
| `cli/src/commands/{fleet_steer,fleet_steer_events,fleet-read}.ts`; `commands/chat/{render,protocol,setup-session,questions}.ts`; response decoders and tests | EDIT / CREATE | readable setup/question revisions and task acceptance; accepted identity across reconnects without required user DT identifiers |
| `rustd/{Cargo.toml,Cargo.lock}`; `rustd/crates/afd_crypto/src/{lib.rs,aad.rs,checkpoint.rs,envelope.rs,entropy.rs}` and tests; affected Cargo manifests | CREATE / EDIT | latest stable direct dependencies, domain-separated object encryption and guarded key buffers using existing AES-256-GCM; preserve vault bytes; no TLS-policy changes |
| `rustd/crates/afd_billing/{Cargo.toml,src/}`; `afd_core/src/catalogue.rs`; `afd_wire/src/usage/`; `afd_fleet/src/lease/usage/`; tests | EDIT | frozen decision-protocol price schedules and one call-linked billing line across receipt/renew/report |
| `rustd/crates/afd_core/src/{error_code,problem}/`; `afd_validate/{Cargo.toml,src/{lib.rs,serialized.rs}}`; registry, problem and bounded-serialization tests | CREATE / EDIT | UZ-DT errors and one allocation-free JSON size proof in the existing validation leaf, shared by decision config and wire |
| `docs/architecture/scenarios/durable-tasks-three-prs.md` | CREATE | three-picture implementation-readiness walkthrough requested by Indy |
| `rustd/crates/afd_wire/src/{task/,activity/,redact.rs}`; `afd_api_wire/src/tail/`; `afd_observability/src/{producers,metrics,semconv}/` and capture tests; `docs/metrics{,.runner}.census.tsv`; `afr_telemetry/src/` and tests; CLI/UI stream decoders and tests | CREATE / EDIT | typed task context, credential-safe diagnostics, frozen decisions, fair financial recovery, bounded attempt/age metrics and transfer readback |
| `rustd/crates/afr_tools/src/verbs/mod.rs`; `afr_supervisor/src/client/lease_verbs/`; matching tests | EDIT | extend existing native LeaseVerbs for task calls rather than adding another runtime interface |
| `rustd/crates/afd_runner/{Cargo.toml,src/{store,edit,record,decision}.rs,src/{capacity,policy,heartbeat,view,validate,error,sql}/,tests/}`; `afd_wire/src/runner/`; `afd_api_operator/src/handler/operator/`; `afd_api_wire/src/admin/`; runner edit/catalogue UI codecs and tests | EDIT / CREATE | reviewed decision-model selection through existing enrolment/edit/heartbeat, native catalogue checks and frozen lease profile |
| `rustd/crates/afd_wire/src/{inference.rs,decision/}`; `afd_admin/src/{model.rs,model_validation.rs,catalogue.rs,decision_revision.rs,sql/}`; `afd_api_wire/src/admin_catalogue.rs`; `afd_api_operator/src/handler/admin/models/`; model-catalogue readers/writers, fixtures and tests (`scripts/model-catalogue/`, `afd_db/src/test_util/sql/`), plus shared metadata; `afd_inference/src/`; `afd_credential/src/{provider/{mod.rs,decision.rs},vault/mod.rs,error/mod.rs}` and `tests/integration_rotation/decision_keys.rs`; `schema/{2230_model_decision_revisions,2240_model_decisions,2250_model_decision_prices}.sql` | EDIT / CREATE | reviewed profiles and protocol-specific rates; existing vault resolves platform-admin keys freshly for reserved calls |

## Applicable Rules

- `.orly/standards/domains/architecture.md`: ARCH-GROUND, ARCH-LAND, ARCH-HOMES — one owner per runtime mechanism.
- `.orly/standards/domains/auth.md`: AUTH-DOC, AUTH-GUARD, WAUTH, AWO, AUTH-NOLOG, CTX — current workspace/grant checks, secret isolation and task ownership.
- `.orly/standards/languages/rust.md`: ERR-RS, RS-FROM, RS-BORROW, RS-RACE — native state/errors and deterministic contention proofs.
- `.orly/standards/tests.md`: TEST-REDGREEN, TEST-OUTCOME, TEST-ERROR, TEST-RESIDUAL, TEST-ACKLOSS — production entrypoints and negative outcomes.
- `.orly/standards/spec.md`: SPEC-MECH, SPEC-DIM, SPEC-FILES, SPEC-E2E — resolve open mechanisms before implementation and map every proof.

## Applicable Gates

Implementation retains `orly gate pr`, `make lint-all`, `make test-unit-all`, `make test-integration-rustd`, `make check-version` and gitleaks.
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
Postgres owns `fleet.tasks`, `fleet.task_inputs`, `fleet.task_waits`, `fleet.task_effects` and `fleet.task_checkpoints`.
R2 owns encrypted immutable checkpoint bytes; Dragonfly retains existing wake hints.
[The lifecycle transition table](../../architecture/durable_tasks.md#state-authority-and-transaction-boundaries) defines guards and commit-before-release boundaries.

The daemon saves each human message through existing admission before routing.
The runner calls its assigned decision provider under its lease; the conversational model prepares a bounded objective when routing selects new work.
A guarded daemon mutation checks current authority, target and originating admission, then creates the DT and binds current execution together.
Use composite ownership keys, BIGINT versions/counters, fixed lock order and named application state constants from the owning relational model.
Mixed-message work uses an immutable interpretation plan and stable native item identities; deduplicate each admission/item pair.
Multiplicity at most 0.30 permits one request; at least 0.70 requires decomposition; uncertainty or more than four actions clarifies without truncation.

Several DTs may remain unfinished while one lease runs.
A waiting DT settles its execution admission and releases the fleet slot; its wait cannot block the queue head.
Ordinary exchanges retain the current path without compulsory DT creation.

- **Dimension 1.1** — Repeated admission retains one task/target and cumulative limits → Test `task_admission_preserves_owner_and_limits`.
- **Dimension 1.2** — Stale/cross-workspace claims cannot execute or publish → Test `task_generation_and_workspace_fences_hold`.

### §2 — Separate human conversation from task execution

Use the selected decision profile to interpret newest input against relevant conversation and trusted eligible DT summaries.
Native code validates associations; failed or uncertain routing retains the message and clarifies instead of selecting a write target.
Mixed requests retain original text and can produce separately validated actions.
Pure social replies, acknowledgements and status questions do not implicitly restart or reset a task; a status answer may use recorded evidence labelled with its freshness.
An explicit refresh or steer affects the addressed task under current authority; an ambiguous target asks for clarification before investigation.
Mixed greeting-plus-work input must preserve the actual requested work rather than matching a greeting prefix and discarding it.

Persist input receipt and consumption position so retries do not apply the same steer twice.
Consume input between complete action groups through one daemon claim shared with queued delivery.
A claimed admission cannot repeat queued delivery; its accepted meaning remains in canonical DT history.
Resume replays accepted input beyond the checkpoint's covered cursor and reconciles recorded effects before another action.
Users address the fleet without DT identifiers; show queued versus consumed status.
Pin an admitted task's goal revision; changing its completion questions requires explicit reviewed revision adoption, and invalidates stale decision assessments.

Define cancellation's publication cutoff: reject new effect intents after the committed stop generation, reconcile already-admitted effects, and never imply that an issued remote write was undone.
Social messages cannot revive terminal tasks; a new task needs a fresh admission.
Pause blocks new execution and records its queued/resumable state; deletion, cancellation, deadline/budget exhaustion and a closed/merged target stop late wakes and new effects.
Record blocked versus terminal outcomes explicitly, without resetting the original bounds when a prerequisite is later resolved.

- **Dimension 2.1** — Varied social replies make no investigation calls or task restart → Test `social_input_does_not_restart_a_task`.
- **Dimension 2.2** — Steers are consumed once and cannot silently replace goal/authority → Test `steering_preserves_goal_revision_and_authority`.
- **Dimension 2.3** — Stop prevents new effects while reconciling admitted ones → Test `cancellation_fences_new_effects_without_inventing_rollback`.
- **Dimension 2.4** — Uncertain routing and active-input replay cannot create unintended work → Test `intent_routing_refuses_uncertain_or_replayed_work`.
- **Dimension 2.5** — A crash after input acceptance retains its meaning without repeating effects → Test `accepted_input_survives_crash_before_checkpoint`.

### §3 — Evaluate intent and goal progress through the runner's decision model

Fleet `decision_policy` owns editable routing/completion questions and criteria; setup chat or CLI creates immutable revisions through Review/Apply. Setup shows success criteria; work requests and DT activity use existing fleet chat.
Add nullable `assigned_policy.decision_model_id` to the existing runner assignment and resolve its reviewed catalogue profile when issuing a lease.
Heartbeat delivers assignment changes; active calls retain their recorded profile, while revocation fences new dispatch and native acceptance.

One `DecisionEvaluator` trait lives in `afr_providers`; `afr_agent` owns decision timing and application policy.
One `DecisionClient` maps Jev, Perplexity Decisions, Cloudflare Clef and OpenAI Decisions through four small native wire adapters.
Native transport, secrets, accounting and validated inputs remain shared; native slicing checks the admitted source's bounds and UTF-8.

Provider capabilities declare supported native question types, distributions, usage and refusal semantics.
Jev's proposed thresholds belong to its profile; other providers require evaluated policies and cannot inherit calibration by API similarity.
Unsupported schemas, unproved context bounds or unknown model versions refuse activation rather than producing a guessed decision.

Evaluate human intent and goal progress at completed action groups or task resume; avoid extra assessment for unchanged status reads.
Signed events match native waits without classification; social input needs no incidental investigation or second goal assessment.
Native code verifies authority, exact-head evidence and cumulative bounds before accepting a transition.

Record profile/model/question revisions, assessed state/evidence bindings, native disposition and actual physical usage, including unknown outcomes.
Renew/report usage references `decision_call_id`; one canonical billing line uses its daemon-authorized decision-protocol schedule/profile/rates, while coding lines retain their current rate.
Resolve credentials through the existing vault/inference plane; never include keys in tools, model state or checkpoints.
Replacement runners can select another evaluated profile without changing the objective, receipts or cumulative allowance; invalidate cached assessments.

[Decision models](../../architecture/goal_evaluation.md) owns provider APIs, question types, thresholds, limits and community-crate findings.
Jev starts with the proposed 0.85 intent and 0.95 target/completion probabilities, one bounded retry and a 30-second logical deadline.
Each source permits ten physical calls; USD 0.02 reserves each call, while actual usage and reviewed rates remain authoritative.

- **Dimension 3.1** — Current evidence drives a bounded goal transition → Test `goal_decision_uses_the_accepted_revision_and_evidence`.
- **Dimension 3.2** — Stale/injected/invalid/unavailable judgment cannot complete work → Test `unsupported_goal_decision_cannot_complete_a_task`.
- **Dimension 3.3** — Alias drift or missing version blocks unreviewed decisions → Test `unevaluated_decision_version_cannot_drive_a_transition`.
- **Dimension 3.4** — Mixed-message uncertainty never discards independent work → Test `mixed_request_boundary_never_discards_work`.
- **Dimension 3.5** — Runner selection reuses one decision boundary without false calibration or mixed-model billing → Test `runner_decision_profiles_preserve_capabilities_and_accounting`.

### §4 — Bind existing-PR writes and later CI to that task

An externally opened PR, including one authored through Codex, needs explicit reviewed existing-PR mode distinct from event-owned draft creation.
Recheck the exact open same-repository PR/head/base and current grants on each admission; initially refuse fork heads, protected/default/base branches and workflow edits.
Review workflow/secret/environment eligibility so an admitted push cannot implicitly deploy or gain privileged workflow effects.

Record expected exact-head validation before publication; a signed completion or fallback wake only requests independent current-evidence reads.
Route matching repair/success events before the existing investigation-only filter, without broadening ordinary trigger fan-out.
Duplicate, stale, out-of-order and fast-completion wakes compete through one fenced wait transition; closed/merged/cancelled/exhausted tasks cannot restart.

Use a native GitHub `createCommitOnBranch` publisher with `expectedHeadOid` and narrowly bound credentials.
Reconstruct the supported candidate tree and compare it with the tested tree before publication.
Reserve the native effect ordinal and immutable request before dispatch; recovery lists effects beyond its checkpoint watermark even after first-response loss.
Unsupported changed-path modes, symlinks, submodule changes or provider-size limits block publication until explicitly supported.
Unchanged base files retain their modes; publication permits 32 paths and 1 MiB total decoded content through a 2-MiB request.
The current `force:false` rule is not an arbitrary-PR expected-head guarantee; a separate read followed by a write is insufficient.
Persist intent before remote effects, reconcile uncertain acknowledgements, preserve human changes and do not blindly repeat writes.

- **Dimension 4.1** — Human/Codex-opened PR is repaired only under its exact binding → Test `external_pr_repair_preserves_target_and_human_changes`.
- **Dimension 4.2** — Later CI/fallback resumes the same current wait once → Test `ci_wakes_resume_one_current_repair_wait`.
- **Dimension 4.3** — Lost acknowledgements and races cannot duplicate or overwrite effects → Test `publication_reconciles_uncertain_effects`.
- **Dimension 4.4** — First-response loss cannot hide a reserved or issued effect → Test `lost_effect_reservation_response_is_discoverable`.

### §5 — Recover compatible coding work on another worker

Restore the task's compatible conversation, file/Git generation and confirmed outcomes, not independently selected newest snapshots.
Use the canonical sandbox save/restore design for quiescence, staged/unstaged/untracked coverage, hostile files, isolation and fenced manifests.
API-only repairs require trusted task/effect recovery without a mandatory checkout or GitHub grant for repository-free fleets.

Expose the last recoverable generation after unsaved loss, and reconcile remote PR state before reconstructing local work.
Use encrypted immutable Cloudflare R2 objects with Postgres manifests and proposed Rust `object_store` transport.
Save a verified base plus incremental logical file/Git changes; do not upload an entire checkout on every save.
Capture requires verified writer quiescence; an unsettled kill cannot publish a checkpoint.

Inside M237_001's joined build, select validated restore or fresh checkout; ordinary checkout must not overwrite recovered files.
Select compatible conversation before coding inference; sandbox tools await validated ExecutorSlot readiness.
Toolbox bytes remain host-local by digest and never enter tenant checkpoints.

Propose a 16-GiB retained base, 1-GiB incremental save, 50-GiB workspace storage cap and 15-minute transfer deadlines.
The [storage protocol and limits](../../architecture/sandbox_evaluation.md#proposed-storage-authorization-and-encryption) define key ownership, scoped URLs, encryption, pins and retention.
These are resource ceilings; the six-GiB fixture must still measure scanning, initial transfer and cold restore.
Indy's 6-GB example is not an approved repository-size limit; enforce proposed capacity against actual measured bytes.
Use parallel independent encrypted chunks, not a redundant multipart session for each small object.
Failed capture/upload preserves the prior manifest and shows the last recoverable generation.
Garbage collection serializes tombstones with reader and publication pins; manifest commit converts live writer pins to durable references.
Numbered pages, URL refresh, pin renewal and exact-base verification have explicit APIs.
Native fixed key/nonce and immutable ciphertext descriptors make conditional-upload replay verifiable; tombstoned content gets a new object identity.
Separate versioned workspace/repository identity keys survive master-key rewrapping without becoming sandbox credentials.
A new Firecracker backend remains separate from proving these task outcomes on the existing engine.

- **Dimension 5.1** — Replacement worker restores a compatible generation and same PR → Test `replacement_worker_resumes_compatible_task_state`.
- **Dimension 5.2** — Missing/stale/hostile/wrong-task recovery refuses safely → Test `incompatible_recovery_never_attaches_writable_state`.
- **Dimension 5.3** — Large repositories reuse base/chunks across saves → Test `large_repository_checkpoints_upload_only_changed_content`.
- **Dimension 5.4** — Quota failures or unsettled kills retain the prior manifest → Test `checkpoint_quota_or_unsettled_kill_preserves_prior_generation`.
- **Dimension 5.5** — Collector races cannot invalidate newly published checkpoints → Test `checkpoint_publication_pins_fence_garbage_collection`.
- **Dimension 5.6** — Long transfer authorization and pin renewal preserve ownership → Test `checkpoint_authorizations_refresh_without_losing_pins`.
- **Dimension 5.7** — Conditional-upload replay preserves immutable bytes → Test `conditional_chunk_replay_preserves_ciphertext_identity`.
- **Dimension 5.8** — Identity-key rotation and deleted-content reintroduction preserve isolation → Test `dedup_key_rotation_and_reintroduction_preserve_isolation`.

### §6 — Present progress and prove the Grafana/scheduler outcome

Add compact Current work above the shared fleet-chat composer with DT title, state, next dependency and confirmed result links.
Expand local/published files, checks, attempts, subagents and evidence; subagents remain helpers within work.
Preserve composer access, selected reading position and confirmed progress across recovery.
Show the selected provider/model during routing and goal activity and transfer preparation/upload/verification stages in both CLI and browser, with bounded native progress events.
Distinguish browser reconnection, inference interruption and uncertain tool effects; retries preserve task identity and reconcile native state first.
The exact UI labels and scheduler/recovery narrative remain owned by the canonical scenario and design-system guidance.
Reuse outbound obligations, Slack answer markers and repeat-delivery checks; DT records bind task/generation/destination.
No `afd_outbound` source edit is assumed; a demonstrated missing behavior requires a spec amendment.
After an uncertain send, reconcile supported provider evidence before retry; if delivery cannot be established, show unconfirmed delivery rather than blindly resending or claiming exactly-once external posting.

Use a controlled development symptom visible through an actually configured Grafana data source, plus GitHub and a configured Slack destination.
Existing schedules observe quietly or correlate one incident/notification; diagnosis alone does not admit repair.
After authorized repair/CI and separately authorized development deployment, a fresh scheduled query verifies the symptom or reports insufficient data.

Prove duplicate ticks, overlapping/busy fleets, independent due fleets, pause/resume, cancellation/deletion and stale queued work against existing scheduler semantics.
Pin real query/window/threshold and measured timing tolerances before the demonstration; Elasticsearch and production are excluded.

- **Dimension 6.1** — Progress and recovery notices reflect confirmed native state → Test `task_progress_and_failure_recovery_remain_truthful`.
- **Dimension 6.2** — Scheduled Grafana observation correlates notification and fresh recovery evidence → Test `scheduled_grafana_incident_reaches_verified_development_outcome`.
- **Dimension 6.3** — Notification retries preserve intent and honest delivery state → Test `notification_retry_reconciles_native_delivery_state`.
- **Dimension 6.4** — CLI and browser show truthful decision/upload stages without duplicate counters → Test `decision_and_checkpoint_progress_is_visible_and_truthful`.

## Interfaces

[The native API table](../../architecture/durable_tasks.md#proposed-native-api) fixes methods, paths, request/response fields and guards.
Runner resources use plural `L/tasks/{bindings,inputs,input-receipts,decisions,effects,checkpoints}` under the existing lease.
Runner mutations require current lease/fence, expected task generation and operation identity; workspace reads/writes use existing FleetRead/FleetWrite capabilities.
Task detail GET returns ETag; PATCH requires If-Match, with missing/stale tags mapped to 428/412 and lifecycle conflicts to 409.
Task-bound leases require `durable_tasks_v1`; incompatible runners refuse those admissions while ordinary one-shot work remains compatible.

Lease task context carries identity/generation, accepted goal revision, native target, limits, input/effect cursors, checkpoint and decision policy.
Reports carry transition/evidence, input responses, covered cursor, checkpoint and confirmed-effect watermark.
The owning DT architecture page fixes the proposed structs, behavior traits, dispatch choices, error composition and tracked signals before their implementation.
Native code validates every claim; duplicate physical call or effect identities cannot charge or publish again.

The daemon alone creates native continuation admissions through `Producer::TaskContinuation` and task event kind.
The [input/scheduling protocol](../../architecture/durable_tasks.md#proposed-input-claims-and-scheduling) fixes FIFO boundaries, four-message claims and immutable mixed-action identities.
A ten-minute slice yields at a completed group and saves before release; issued uncertain effects retain reconciliation ownership.

The [relational model](../../architecture/durable_tasks.md#proposed-relational-model-for-signoff) defines tables, uniqueness, ownership and lock order.
The UZ-DT family fixes not-found/stale/replay/target/limit/decision/checkpoint/storage/effect/protocol outcomes with mapped HTTP status.
Generate OpenAPI after route inventory, bounds and served-route authorization tests; do not hand-edit generated JSON.

## Proposed defaults for signoff

| Decision | Proposed behavior |
|---|---|
| Lifetime and budget | Seven-day wall deadline, USD 5 cumulative model budget, two running hours, 12 leases and eight repair publications; lower existing limits win. |
| Concurrency | Up to 16 unfinished DTs per fleet, one active lease, FIFO continuation admissions and four-message safe-boundary input batches. |
| Decision model | Runner-assigned reviewed catalogue profile; initial Jev `jev-latest`; provider-specific capabilities, thresholds, rates and version evaluation. |
| R2 | `object_store` 0.14.2; daemon-held bucket credential; five-minute exact-object URLs; AES-256-GCM objects; wrapped per-object keys. |
| Recovery | Verified retained base plus incremental chunks, bounded readable manifests, three compatible generations per live DT, seven-day terminal retention, pinned reader/writer GC. |
| Encryption scope | AES-256-GCM at rest and existing HTTPS transport. Quantum-safe transport belongs to a separate specification and does not block M236. |
| Maintenance | Supervised ten-second due-work scan; fallback reads at 60/120/240/300-second spacing, capped at 24 per DT; no new scheduler service. |

These approved implementation defaults remain unmeasured operating limits; activation and acceptance require their declared proofs.
The architecture owners carry full bounds and negative behavior; implementation follows the approved values without another signoff loop.
The 60-second fallback is a backup read, not webhook delivery latency; unchanged status causes no agent lease, checkout restore or decision call.
Show the pending dependency and bounded recheck state without posting repeated unchanged reports.

## Failure Modes

Stale/cross-workspace ownership and revoked grants map to `task_generation_and_workspace_fences_hold`.
Ambiguous input, duplicate steer and stale goal revisions map to `steering_preserves_goal_revision_and_authority`.
Missing, late, failed, malformed and injected decision responses map to `unsupported_goal_decision_cannot_complete_a_task`; unknown physical outcomes retain reservations.
Human pushes, lost acknowledgements and duplicate/out-of-order checks map to the Section 4 negative tests.
Worker out-of-memory death, disk-full capture/restore, partial writes, quota exhaustion and hostile restore map to Section 5 failure tests.
Unserved ready tasks, expired owners and overdue waits map to Section 4 wake and Section 5 replacement scenarios, with bounded completion.
Provider/browser interruption and unknown remote effects map to `task_progress_and_failure_recovery_remain_truthful`; do not hide them as success.

## Invariants
Database claims and generation checks enforce one current owner; native grants and publication preconditions enforce target reach.
Persisted cumulative usage/attempt/deadline state prevents a wake from resetting limits.
A native proof check rejects unsupported model completion; task-bound manifests reject cross-task recovery.

## Metrics & Observability

Carry task/event/lease correlation without prompts, credentials or private evidence; record bounded failure causes, queue age, overdue waits and maintenance freshness.
Use operator events `dt_input_accepted`, `dt_transition`, `dt_decision_recorded`, `dt_wake_claimed`, `dt_checkpoint_published` and `dt_effect_uncertain`.
Allow only native task/lease/event IDs, state/reason codes, model/question revisions, durations, byte counts and usage; exclude raw prompts, paths, URLs and evidence.
Tests named in the corresponding Dimensions capture these events and assert sensitive content is absent; browser scenarios verify private task titles and expanded criteria stay within the existing analytics exclusion.
User activity uses `task_decision_started`, `task_decision_completed` and `task_transfer_progress` with bounded purpose/state/counters and task/admission identity.
History and native task/decision/checkpoint rows remain authoritative when ephemeral frames drop; saved/completed labels require confirmed settlement.
Retain call inventory and stage timings across compaction. Record baseline/candidate, runner/model profile, token/cache usage and cold/warm recovery evidence for later DT performance investigations.

## Test Specification (tiered)

Use modular production scenarios with independent expected outcomes and several assertions per invariant. Changed executable TS/TSX lines require 100% coverage; Rust patch coverage must exceed 99.5% (Indy, Nov 04, 2026; stricter interpretation). Existing native decision scenarios also check complete wire data and credential-safe diagnostics.
Avoid test-only production seams, coverage-padding field tests and duplicated proof; broader test cleanup belongs to Indy's other agent.
Complete one crate slice before running its focused gates; repeat only for concrete failures. Concurrency proofs use isolated fixtures and bounded channels/barriers; justify mutexes.

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
| 4.2 | integration | `ci_wakes_resume_one_current_repair_wait` | unchanged native reads cause no agent lease/model call; changed/completion/fallback races cause one current wake, no late restart |
| 4.3 | integration | `publication_reconciles_uncertain_effects` | committed-before-ack-loss and human push → safe readback, no overwrite/duplicate |
| 5.1 | integration | `replacement_worker_resumes_compatible_task_state` | worker replacement during CI wait → compatible state and same PR |
| 5.2 | integration | `incompatible_recovery_never_attaches_writable_state` | wrong/stale/hostile/over-quota or disk-full restore → explicit refusal, retained checkpoint and preserved isolation |
| 6.1 | e2e | `task_progress_and_failure_recovery_remain_truthful` | dry-lane browser tests under `tests/e2e/` (outside acceptance/) show progress and distinct recovery without losing native identity/actions |
| 6.2 | manual | `scheduled_grafana_incident_reaches_verified_development_outcome` | real scheduler/queries/GitHub/Slack and replaced worker → combined recorded outcome |
| 6.3 | integration | `notification_retry_reconciles_native_delivery_state` | repeated/ack-lost notification intent → reconciled receipt or explicit unconfirmed state; no new repair or blind resend |
| 6.4 | e2e | `decision_and_checkpoint_progress_is_visible_and_truthful` | CLI/browser show started/result/uncertainty and scan/upload/verify states; retries do not inflate bytes; saved appears only after manifest commit |
| 2.4 | integration | `intent_routing_refuses_uncertain_or_replayed_work` | mixed/ambiguous/malformed/stale routing cannot widen scope or apply input twice |
| 5.3 | integration | `large_repository_checkpoints_upload_only_changed_content` | retained six-gigabyte base plus repeated small edits avoids complete reupload; cold restore remains exact |
| 5.4 | integration | `checkpoint_quota_or_unsettled_kill_preserves_prior_generation` | disk-full, worker out-of-memory death, timeout, partial upload and live writers retain the prior checkpoint; no false saved state |
| 2.5 | integration | `accepted_input_survives_crash_before_checkpoint` | A crash after input acceptance retains its meaning without repeating effects; stale/replayed failure paths preserve native state |
| 3.3 | integration | `unevaluated_decision_version_cannot_drive_a_transition` | Alias drift or missing version blocks unreviewed decisions; stale/replayed failure paths preserve native state |
| 5.5 | integration | `checkpoint_publication_pins_fence_garbage_collection` | Collector races cannot invalidate newly published checkpoints; stale/replayed failure paths preserve native state |
| 3.5 | integration | `runner_decision_profiles_preserve_capabilities_and_accounting` | distinct runner profiles, unsupported capabilities, profile changes/replacement, one charge at its decision-protocol rates and normalized cache/token usage across PATCH/renew/report, and no cross-workspace credential access |
| 3.4 | integration | `mixed_request_boundary_never_discards_work` | 0.30/0.70 boundaries and more-than-four items clarify or decompose without effects on an unaccepted target |
| 4.4 | integration | `lost_effect_reservation_response_is_discoverable` | replacement with no returned effect ID discovers the immutable action and reconciles before dispatch |
| 5.6 | integration | `checkpoint_authorizations_refresh_without_losing_pins` | URL expiry, page replay/conflict, pin renewal and collector races refuse stale publication |
| 5.7 | integration | `conditional_chunk_replay_preserves_ciphertext_identity` | ack-loss and concurrent chunk requests verify identical remote bytes; mismatch refuses manifest |
| 5.8 | integration | `dedup_key_rotation_and_reintroduction_preserve_isolation` | rewrapping preserves dedup; identity epoch rotation retains old reads; tombstones never revive; another workspace cannot reuse objects |

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
The case set covers pure social input, status versus refresh, mixed work input, ambiguous/new task, normal goal completion, unsupported/stale decision result, external-PR race, later-CI/fallback races, pause/delete/cancel/budget/deadline, worker restore failure, notification uncertainty and Grafana recovery.
Grade every case for grounding/authority, expected useful result and truthful completion: all N/N with zero critical failures, preserving failed attempts and missing evidence rather than changing the denominator afterward.

The authorized observer records native event/task/head identities, complete tool references, decision outcomes and visible Slack attribution in the ledger; deterministic fixture tests do not substitute for these observations.
The combined Linkwarden scenario must include an externally opened PR, later CI and worker replacement in one task; isolated passes do not substitute for it.
Codex may repair product defects but must not author the demonstration's fleet repair or provide its diagnosis.

## Dead Code Sweep

Replace overlapping task/incident narratives with links to the canonical scenario; keep current runtime facts in their owning topic pages.
Do not reuse semantic memory, model prose or old one-event branch derivation as task authority.
Keep useful current policy/repair tests; no blanket checker retirement belongs to this future implementation.

## Out of Scope

Production launch, automatic merge/deploy, new Firecracker backend, PR review posting and unreviewed standing repository-write authority are excluded.
Post-quantum transport changes belong to a separate specification under Indy's scope decision.
The Grafana observer's quiet monitoring does not create an alert subscription or an implicit repairer handoff.
M224 closure and Orly 0.28 cleanup keep their separate owners.

## Product Clarity (authoring record)

1. **Successful user moment** — A repaired PR keeps progressing after CI and worker replacement; the person can ask a question without starting it over.
2. **Preserved user behaviour** — Existing reviewed setup, Slack addressing and human merge/deploy remain recognizable.
3. **Optimal-way check** — Extend the current generic runtime with trusted task ownership and bounded decisions, not a second scheduler.
4. **Rebuild-vs-iterate** — Iterate admission/lease/provider/recovery boundaries; sandbox replacement is separate.
5. **What we build** — One goal/decision loop, durable PR repair, recovery, human input and combined development proof.
6. **What we do NOT build** — Automatic production actions or a second independent PR writer.
7. **Fit with existing features** — Reuses setup revisions, compaction and grants without treating memory as authority.
8. **Surface order** — Shared native task semantics, then existing chat/Slack surfaces; no separate CLI engine.
9. **Dashboard restraint** — Preserve the approved status pill and compact inline work rows above the composer. Use bundled fonts, current spacing/colors, 14px chat, 13px rows and 12px metadata.
10. **Confused-user next step** — Show the missing evidence, permission or decision and the action needed to resume.

## Decomposition & alternatives (patch vs refactor)

One scope contains the three failure boundaries of one task; split implementation by Sections, not competing task runtimes.
A prompt-only loop cannot supply durable waits, atomic publication or workspace recovery.
A backend rewrite cannot supply task authority; neither is a substitute for this design.

## Discovery (consult log)

- **Consults** — This documentation worktree started at `2f8c878c0`. Indy parked M237 revision review with Claude Code. Unshipped task foundation slots 2190–2220 are allocated; later owners allocate forward slots.
- **Model reviews** — The Nov 02, 2026 Astra proposal review (high) verified five corrections closed. On Nov 04, requested `gpt-6-astra`, extra-high reasoning, found and verified a native receipt/issuance race fixed; its schema/query follow-up found no material residual issue. These were source reviews; gate results belong in the acceptance ledger. Serving revision and effective reasoning were not independently observable; no full Elephant-Goldfish Model workflow or live acceptance is claimed.
- **Review dispositions** — Earlier corrections covered recoverable effects, checkpoint APIs, conditional uploads, deduplication keys and mixed-request uncertainty. Dimensions 3.4, 4.4 and 5.6–5.8 name their negative tests. The three-task walkthrough's partial views were verified consistent.
- **Final review, Nov 03, 2026** — Requested `gpt-6-astra`, xhigh reasoning, read repository source and official provider documentation. Four material findings were corrected and verified closed: native OpenAI Decisions, decision-protocol billing, public task ETag checks and bounded manifest reads with restore-pin bootstrap. No material residual finding remained. Actual serving revision was not independently observable.
- **Final API/crate disposition** — Twenty new method/path pairs, existing caller/runner configuration reused, one native decision trait/client and four wire adapters. Reuse guarded reqwest and current coding-provider dependencies; propose `object_store` 0.14.2 for R2. No new decision SDK, task worker or publisher queue.
- **Selected mechanisms** — Postgres DT ownership, incremental R2 checkpoints, native expected-head publication and runner-configured decision profiles, initially TypeSafe `jev-latest`.
- **Signoff requested** — Approve the proposed APIs, input/FIFO mechanism, defaults, provider profiles, incremental R2 custody and encryption-at-rest posture. No unresolved product mechanism is delegated to an implementing model.
- **Approval boundary** — Indy approved source implementation on Nov 03, 2026. Credentials, live regression/storage checks and demonstration fixtures gate activation and acceptance, not document authorship. Indy: "Yes signedoff and approved on both Fleet setup chat visual and the Fleet detail chat with DT as well."
- **M237 coordination** — Indy: "i think park the m237 revision. its handled by claude code now." Preserve its supplied Engine/ExecutorSlot seams during implementation; do not reopen its milestone or edit its specs here.
- **Metrics review** — Metrics & Observability names operator events and user-visible decision/transfer states. Sensitive content stays out of telemetry; implementation tests must prove the declared fields and exclusions.
- **Skill-chain outcomes** — The revised proposal received the requested extra-high Astra review and correction check. All four material findings were verified closed with no material residual findings; numerical defaults remain proposals and runtime checks remain unrun. Earlier M237 feedback was handed to Claude Code and its review files remain deleted under Indy's instruction.
- **Documentation verification** — Changed-document language scan: zero findings. Local links: 353 checked, zero missing; two JSON examples parsed; 28 Dimensions match named test rows; spec below 500 lines. `git diff --check` and `gitleaks dir docs --redact --no-banner` passed. Runtime gates, live provider/storage checks and benchmarks remain unrun.
- **Deferrals and transfers** — Production, Elasticsearch incident work and full Linux/development identity checks are outside reduced M224; the new sandbox backend and review-posting capability remain separate. Conditional fleet handoffs follow M236; Indy: "or else i would want a prompt to start a new session after we merge this."

> Owner (Indy, Nov 01, 2026, time not recorded): "So i assume in 1 spect all the above + the goal driven JEV based autonomous agent will exists?" — one unified scope selected; writing a spec does not claim implemented autonomy.

### Owner decisions from this session

> Indy: "I would select Durable Tasks(DT) as the name."

> Indy: "Yes agree on jev-latest model to use." Later: "This shouldnt be platform owned"; "not editing the same old revision." — owners revise criteria through chat/CLI and immutable revisions. On displaying routing rules: "What is the value the user gets by seeing that?" — remove those details from setup display.

> Indy: "I will use R2 with some rust package"

> Indy: "Well i am good to have Jev based decision on intent routing, so keep that and update the proposal"

> Indy: "No go, and get signoff /approval from indy before you implement"

> Indy: "Upon updating the spec request you to review the M236 spec with Astra"

> Indy: "i thought about an usecase where the git clone or checkout may take 6GB or data?"

> Indy: "on m236 post your updates for signoff and approval on the pending mechanism and concrete implementation readiness"

> Indy: "i think we can do it as a separate spec for quantum safe" — post-quantum transport stays outside M236.

> Indy, Nov 03, 2026: "Keep the Jev one of the decision model which can be configured on a per runner basis"

> Indy, Nov 03, 2026: "And give me a final one to signoff"; "do a review with Astra extra high as well"

> Indy, Nov 03, 2026: "Yes approved,  use the latest crates i see rig-core has 0.46.0 and what is next?" — implementation and latest stable in-scope dependency selection approved.

> Indy, Nov 03, 2026: "And use the standard like `L/task-bindings` to `L/tasks/bindings` ?" — runner task resources use the nested plural namespace.

- **Opening dependency check** — Rig/rig-http 0.46.0 passed `make test-unit-rustd`: runner 1490, daemon 139, libraries 3343; exit 0. Environment assumptions and unrun DT acceptance are recorded in Baseline evidence.

> Indy, Nov 03, 2026: "Let me know before you start the abstractions you will buidl" — struct/trait/generic/error/observability boundaries are proposed in the owning DT architecture page before source edits.
