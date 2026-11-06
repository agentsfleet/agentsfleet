# M236_003: Show schedule activity in fleet chat

**Prototype:** v2.0.0
**Milestone:** M236
**Workstream:** 003
**Date:** Nov 04, 2026
**Status:** DONE
**Priority:** P1 — saved schedules lack readable lifecycle and sync history in fleet chat
**Categories:** API, DOCS, OBS, UI
**Branch:** feat/m236-durable-tasks
**Baseline revision:** 1920ff345; all existing behavior below was checked on this main revision
**Test Baseline:** pending for this workstream; existing milestone measurements remain separately attributed
**Baseline evidence:** playbooks/operations/acceptance/baselines/M236_001-autonomy.md
**Depends on:** existing activity receipts and schedule ownership; no new scheduler service
**Provenance:** Indy's explicit schedule-activity request; other event types remain inventory only
**Canonical architecture:** docs/architecture/data_flow.md and its existing schedule/history owners

## Owner closure — Nov 06, 2026

Closed with M236 under the [owner closure decision](M236_001_P0_API_DOCS_INFRA_OBS_UI_GOAL_DRIVEN_DURABLE_REPAIR.md#owner-closure--nov-06-2026).
Source is in existing PR #803. Automated merge checks remain required; live acceptance is parked for the separate NATS revamp.

## Overview

**Goal (testable):** Fleet chat shows readable schedule changes, registration results and fires linked to their confirmed run outcomes.
**Problem:** A saved schedule still says to save it; sync failures appear only in a banner; fires hide their message behind JSON.
**Solution summary:** Reuse transactional activity receipts, enrich the existing cron event and extend the existing flat chat renderer.

Activity is customer-facing history.
No dashboard, log viewer, decision-model call or additional work queue is added.
All implementation remains in existing PR #803; companion pages remain in docs PR #241.

## PR Intent & comprehension handshake

- **PR:** Existing M236 implementation PR #803.
- **Intent:** Explain what each schedule did in the fleet's conversation, with its human description, message, state and real result.
- **Handshake:** Implement schedules; inventory other missing activity without building it before Indy selects the scope.

## Implementing agent — read these first

1. `afd_events/src/activity.rs` — four existing receipt kinds, `record_on`, transactional commit and post-commit live notices.
2. `afd_cron/src/{service.rs,store/,fire/mod.rs,model.rs,run/mod.rs}` — every schedule mutation, sync fence, fire identity and existing description formatter.
3. `afd_http/src/services/schedule/` — manual and runner paths; `service.rs` also serves background reconciliation.
4. `afd_memory/src/access.rs`, `afd_authoring/src/creation/{reserve,activate}.rs`, `afd_vault/src/write.rs` — actual receipt callers.
5. `afd_api_wire/src/schedule/view.rs` — description, message, status, sync, last error, run state and next time already exist.
6. App `lib/events/{fleet-activity,event-summary}.ts`, `components/domain/{fleetMessageRenderers,FleetTriggerSummary}.tsx`, `components/domain/chat/ScheduleSync.tsx` — current rendering and the saved-state bug.

Rust crate paths share `rustd/crates/`; app paths share `ui/packages/app/`.
The receipt enum on inspected main has SetupSaved, SetupReady, AccessSaved and MemorySharingChanged only.
The vault already records generic access-saved history for writes; deletion and rotation-specific wording are different gaps.

## Files Changed (blast radius)

| Folder → files | Planned change |
|---|---|
| `afd_api_wire/src/activity/`; `afd_events/src/activity.rs`; existing receipt callers | One shared tenant-history wire definition, preserving existing receipt bytes, borrowed text and declared bounds; runner activity frames retain their existing owner. |
| `afd_cron/src/store/{create,guarded,fence,complete}.rs`, `edit/`, `history/`, corresponding statements, `model.rs`, manifest and tests | Store schedule mutation and its receipt in one transaction; register the existing live publisher in production composition. |
| `afd_cron/src/recipe/{replace,write,retire}.rs`; authoring transaction owners | Record setup-owned schedule changes on their existing transaction and append announcements to the existing authoring commit. |
| `afd_cron/src/{service.rs,fire/mod.rs,store/input.rs,store/target.rs}`; owning SQL and tests | Record confirmed sync outcomes under the existing fence; retain description/message snapshot on the accepted cron event. |
| `afd_events/src/history/` and its queued-admission statements; existing catch-up/notice owners | Include accepted schedule fires in queued chat history before a runner claims them, preserving the same admission identity through promotion. |
| `afd_events/src/sql/mod.rs`; `afd_approval/src/inbox/resolve/mod.rs`; shared event-insert fixtures | Bind one consistent JSONB type to the cached narrative insert; typed receipts must coexist with approval continuations and schedule-run history on reused connections. |
| `afd_http/src/services/schedule/`; `agentsfleetd` schedule composition; authoring schedule setup owners | Thread the existing receipt/live-stream capability through every current writer; preserve one scheduling path. |
| App `lib/events/`, `lib/api/{draft-types,fleet-detail,schedule-status,events}.ts`, `components/domain/{fleetMessageRenderers,FleetMessageRow,FleetTriggerSummary,EventsList,EventDetailsDialog,FleetRunDetails}.tsx`, `use-event-detail.ts`, `components/domain/chat/{ScheduleSync,SetupSummary}.tsx` and callers | Plain activity rows on shared 14px typography, visible fire message, saved-state summary, native run-details action and existing retry action. |
| Native storage/HTTP tests and app Playwright owners | Exercise atomic history, stream catch-up, privacy, duplicates, failure/recovery and rendered rows. |
| Architecture owners, this specification, acceptance baseline and generated OpenAPI when affected | Record actual behavior and evidence; public API changes require the existing docs-repository branch. |
| Docs repository `fleets/{running,tools,authoring}.mdx`, `changelog.mdx`, affected generated API pages | Explain visible schedule activity; new API reference follows the owning lead-repository merge. |

## Applicable Rules

Use the existing error shell, wire crates and garde bounds.
Read `docs/RUST_ERROR_STANDARD.md` before changing a fallible signature.
Use purpose-driven test fleet names such as “Support triage”.

Each new receipt kind needs its matching Zod decoder and UI renderer.
Native tests use real storage through `make test-integration-rustd`; rendered rows use the app's Make-driven Playwright lane.
Preserve the required Rust and TypeScript coverage floors without field-only padding tests.

## Applicable Gates

`orly gate work`, `orly gate verify`, `make test-unit-all`, `make test-integration-rustd` and `make dry-app`.
Run each crate/component's checks after completing it; final milestone gates remain separate.
No new Make target or test harness is needed.

## Prior-Art / Reference Implementations

`record_on` already writes history on the caller's connection and returns an unpublished announcement.
`activity::commit` commits before notifying live chat streams.
Reuse these facilities and the existing native schedule description formatter.

Choose an `afd_cron` dependency on `afd_events`, with writes inside the schedule store transactions.
Recording only in the HTTP caller would omit background reconciliation and separate the receipt from the schedule commit.
The current dependency graph permits the existing events/admissions owner without adding a service or queue.

## Sections (implementation slices)

### §1 — Record schedule changes atomically

1. Authenticate the existing caller and prove fleet/workspace ownership through the current schedule seam.
2. Validate current input through existing bounded types and claim its native fence.
3. Save the schedule change and a typed receipt on the same transaction.
4. Snapshot schedule identity, human description, saved message and desired status before a later edit or removal can change them.
5. Commit before notifying the existing chat stream; failed notification leaves reloadable history.

The store handles API, setup, CLI and runner-originated changes through existing owners.
Create, change, pause and requested removal produce distinct readable facts.
A stale token, failed precondition or rolled-back mutation produces no successful activity.

Setup-owned recipes also write through `recipe::{replace,write,retire}` on the author's existing transaction.
Their receipts join that transaction's announcement; storing them only in ordinary create/edit would omit setup changes.

Receipt identity binds the actual schedule mutation or held sync claim, never only a millisecond timestamp.
Repeating the same native operation cannot add another receipt.
Activity itself admits no paid run and creates no Durable Task.

- **Dimension 1.1** — Mutations and history commit together → `schedule_mutations_commit_history_with_their_rows`.
- **Dimension 1.2** — Refusal, rollback and foreign ownership leave no successful receipt → `refused_schedule_changes_leave_history_and_state_unchanged`.
- **Dimension 1.3** — Setup-owned recipes commit matching creation/change/removal history → `setup_schedule_revisions_commit_their_activity_atomically`.

### §2 — Explain registration and recovery

1. The existing reconciler sends its bounded upstream operation under the current claim.
2. `finalize_synced`, `finalize_failed` or `delete_claimed` checks that claim on the mutation transaction.
3. Save the resulting state and matching receipt together; superseded finalization emits no success.
4. Show “registered with the scheduler” only after confirmed active registration.
5. For failure, show the stored `last_error` sentence and “Will retry on the next sync.”

Paused or removed schedules must not claim active registration.
Failed external acknowledgement and database rollback retain the existing idempotent reconciliation behavior.
Existing retry controls remain available without a separate chat warning banner or raw diagnostic payload.

Requested removal precedes confirmed removal; `delete_claimed` commits its receipt with deletion using the held schedule snapshot.
A missed one-off records confirmed expiry/removal without claiming that it was registered or fired.

- **Dimension 2.1** — Failure explains the stored cause and later recovery once → `failed_schedule_sync_history_recovers_without_duplicate_success`.
- **Dimension 2.2** — Superseded sync cannot announce another revision's success → `superseded_schedule_sync_emits_no_successful_activity`.
- **Dimension 2.3** — Requested removal, confirmed deletion and missed one-offs retain truthful history → `schedule_removal_commits_confirmed_history_once` and `missed_one_off_expiry_keeps_its_message_without_a_run`.
- **Dimension 2.4** — History waits preserve session foreign-key progress → `schedule_history_wait_allows_a_first_session_to_commit`.

### §3 — Show a fire and the run it actually started

1. Verify the existing scheduler callback and load display facts from the resolved database schedule.
2. Admit through the existing schedule-fire key and keep its original event identity.
3. Include human description and message snapshot in that event; do not append a second runnable fire.
4. Include `ScheduleFire` alongside `Steer` in existing queued history and notify that existing catch-up path after admission commits.
5. Render “Schedule <description> fired: <message>” directly in the existing chat row, including when no runner is available.
6. Promote the same admission identity to its leased event without another row.
7. Read the run's confirmed state and failure cause through existing event updates; open that canonical event's native run details.

Keep queued or blocked work distinct from completion.
A duplicate callback cannot start a second run or rewrite its historical description.
Removed schedules retain their accepted fire history; callbacks that never admitted work cannot invent a run link.

Snapshots describe schedule state at acceptance, not an unproved scheduler revision.
Current guards reject missing/inactive targets and deduplicate accepted callbacks; they do not reject every first delivery from an earlier active revision.
Adding revision-bound upstream identities is outside activity scope; preserve first-payload-wins history without making that stronger claim.

The existing verifier signs the message identity/body; ingress selects the schedule through `Upstash-Schedule-Id`, outside that signature.
Database-derived display facts do not prove cryptographic binding to that schedule or revision.
This inherited boundary needs separately reviewed correction; activity must not claim to close it or widen accepted targets.

- **Dimension 3.1** — One fire retains its message and exact run outcome → `schedule_fire_history_keeps_one_event_and_its_confirmed_run_outcome`.
- **Dimension 3.2** — Duplicate, missing/inactive-target and untrusted callbacks preserve current admission guards/history → `schedule_fire_replays_preserve_the_original_history`.
- **Dimension 3.3** — Offline reload shows the queued fire; later leasing promotes one row → `queued_schedule_fire_history_survives_reload_and_single_row_promotion`.

### §4 — Render readable rows through reconnects

Reuse current flat activity rows, typography and the native run-details dialog/read seam.
Saved summaries describe the declared schedule and direct registration checks to fleet chat.
Current schedule controls read stored state; saved summaries no longer ask the owner to save again.
Historical rows describe their recorded snapshot, while current controls use the current schedule version.

Current chat payload links read external `url`/`run_url` fields; they are not native run identity.
Carry the canonical event identifier through chat/grouped rows and open `EventDetailsDialog` under current workspace/fleet ownership.
Withhold the run-details action until the corresponding `fleet.events` row exists; queued admissions remain readable without a fabricated run.

Public labels use the server's description and timezone.
Unknown payloads cannot become arbitrary markup, instructions, secret values or external links.
Reload and live catch-up converge on the same stable event identities.

- **Dimension 4.1** — Lifecycle, retry reason, fire and outcome are visible → `fleet_schedule_activity_rows_show_confirmed_facts` in Playwright.
- **Dimension 4.2** — Saved summaries and reloads remain truthful → `saved_schedule_summaries_and_activity_survive_reload` in Playwright; native stream catch-up remains separately measured.

The browser fixture extends the existing chat build and renders production rows, grouping, controls and native-details components.
Its Next Server Action read uses a declared HTTP stub; this proves rendering and selected run identity, not server credentials, scheduler execution or stream delivery.
Real storage scenarios cover accepted queued history and a seeded event promotion; daemon tests exercise native leasing separately.
Do not label these separate results as a live scheduler or end-to-end reconnect observation.

## Interfaces

Use the existing activity envelope and fleet history/stream APIs.
New typed schedule lifecycle and sync outcomes share one wire definition and one matching UI union.
The existing cron event gains trusted schedule display metadata; it remains the admitted run's event.

Schedule messages are customer-owned content displayed within their existing fleet boundary.
The receipt model's current metadata comment must acknowledge this permitted content while continuing to exclude credentials.
No generic client-authored activity endpoint is introduced.

## Other activity inventory — no implementation selected

This table inventories existing main writers and their current surfaces.
Indy selects any additions; schedules above are the only new activity authorized by this request.
Already-visible information must not be duplicated merely to add another row.

| Event | Main writer / stored fact | Today's UI surface and gap | Scope |
|---|---|---|---|
| Runner enrollment, heartbeat state, token rotation | `afd_runner/src/sql/runner/{insert_runner_with_event,heartbeat_with_transition_event,rotate_runner_token}.sql` → `runner.events` | Runner detail activity; no fleet-chat receipt. Only fleet-relevant transitions could belong there. | Inventory only |
| Lease acquired/released | `afd_fleet/src/lease/sql/{lease/insert_lease_with_event.sql,report.rs}`; runner history | Runner lease lists and run details; no fleet lifecycle receipt. | Inventory only |
| Credential removed/revoked; rotation-specific wording | `afd_vault/src/{delete,write}.rs` | Secrets/access settings; writes already emit generic AccessSaved, deletion does not. | Inventory only |
| Approval requested/resolved and grant changed | `afd_gate/src/gate/pause.rs`; `afd_approval/src/inbox/resolve/`, `grant_sql/revoke_grant.sql` | Approval controls/list and run continuation are visible; no corresponding generic activity receipt. | Inventory only |
| Connector installed/rebound/removed | `afd_connector/src/sql/{upsert_install,delete_workspace_installs,release_other_installs}.sql` | Connector/access setup; distinguish a real connection change from existing AccessSaved. | Inventory only |
| Admission blocked or later admitted | `afd_admission/src/{admit.rs,stage/}` | Queue/run state; no separate explanatory activity receipt. | Inventory only |
| Outbound delivery confirmed, retrying or abandoned | `afd_outbound/src/{obligation/,poster/}` | Reply/run delivery state and operational diagnostics; do not turn each retry into chat noise. | Inventory only |
| Setup saved/ready and memory sharing | `afd_authoring/src/creation/{reserve,activate}.rs`; `afd_memory/src/access.rs` | Existing fleet-chat receipts already cover these facts. | Retain existing coverage |

Events without trusted affected-fleet identity cannot be broadcast into arbitrary fleet conversations.
This inventory grants no new credential, approval, runner, admission or outbound behavior.

## Discovery

> Indy: "Activity only."

> Indy: "ALSO INVENTORY (one table in the spec, no build yet)"

> Indy: "I'll pick which ones join this milestone."

> Indy: "no new PR"

All listed main facts were checked against `1920ff345`; the renderer path is `components/domain/fleetMessageRenderers.tsx`, outside its `chat/` directory.
Implementation evidence, actual gates and review findings must be added before completion.

Astra's UI review found raw payload disclosure in grouped receipts and stale already-open run details after settlement.
Grouped recorded facts now suppress raw payloads, and existing status/update changes refresh the selected native record once.
The inherited runner-guard regression test also caught missing-parent lookup returning before an unheld guard; the same transaction now retains that check.

The once-run history scenario exposed mixed text/JSONB bindings sharing one cached statement; all callers now use SQLx's native JSON wrapper.
The corrected storage gate passed 1,524 regular and three exclusive tests; the affected library unit gate passed 3,398.

Astra also found that the new parent `FOR UPDATE` lock could block a session's foreign-key check while waiting on its lease.
Use `FOR NO KEY UPDATE` for the history scope; it preserves serialization and deletion exclusion while permitting `KEY SHARE`.
Prove that compatibility through native finalization, a separately observed row wait and the first real session insert under a bounded lock timeout.

This regression measures parent-lock compatibility, without claiming execution of the terminal-report/runner-guard composition.
The old strength produced Postgres `40P01` at native finalization; the corrected strength passes the same real-row scenario.

All four TypeScript package coverage gates passed at 100%; the app passed 4,669 unit tests and 58 desktop/mobile browser checks.
Evidence is retained in the [M236 acceptance baseline](../../../playbooks/operations/acceptance/baselines/M236_001-autonomy.md#schedule-implementation-and-documentation-publication).

## Out of scope

New dashboards, new scheduler services and unselected inventory events.
No automatic work admission from a history receipt, and no change to model routing or authority.
