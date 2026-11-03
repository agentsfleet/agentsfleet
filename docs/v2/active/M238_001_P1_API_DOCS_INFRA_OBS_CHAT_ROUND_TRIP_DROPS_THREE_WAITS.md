<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M238_001: A chat reply settles in one post, the lease carries the fleet's memory, and the runner is woken instead of polling

**Prototype:** v2.0.0

**Milestone:** M238

**Workstream:** 001

**Date:** Nov 03, 2026

**Status:** IN_PROGRESS

**Priority:** P1, every chat on dev spends 240–419 ms after its answer before `event_complete`, 83–105 ms on a second round trip before its first model call, and up to `NO_WORK_RETRY_AFTER / workers` before any runner looks at it

**Categories:** API, DOCS, INFRA, OBS

**Batch:** B1, one Workstream; the spec lands in the implementation Pull Request (PR) on Indy's direction (Discovery)

**Depends on:** none. M233_001 is IN_PROGRESS and names `afd_observability/src/semconv/mod.rs` and `afr_supervisor/src/lease_loop/segments.rs`, which this spec also edits; its §1 and §2 are unstarted, so this spec lands first and M233_001 rebases onto the attribute keys declared here

**Provenance:** large language model (LLM) drafted (Claude Fable 5.1, Nov 03, 2026) from the dev runner journal on `zombie-dev-worker-ant`, the primitives timed on that host, and the source on `origin/main` at `fde5859717c73ea52d02d84d2408b8f00efcae82`

**Canonical architecture:** `docs/architecture/lease_flow.md` §2 (the runner's side of one lease); `docs/architecture/runner_fleet.md` §Memory continuity, §The control protocol; `docs/architecture/observability.md` §Traces, §Label registry; `docs/architecture/scaling.md` §Idle request volume

---

## Overview

**Goal (testable):** `test_settle_is_one_post_on_a_window_lease` and its companions: a run that ends processed settles with one `reports` post carrying the memory deltas; a lease reply carries the fleet's memory window, so no memory read precedes the first model call; a runner whose held heartbeat is answered by the doorbell polls within one round trip of the admission.

**Problem:** A fleet owner sends a chat and waits about a second for a one-line answer. The dev runner's `lease_completed` lines on Nov 03, 2026 (`journalctl -u agentsfleet-runner`) read `wall_ms` 883–1,025 with `model_ms` 380–555, `memory_ms` 83–105 (one at 282) and `report_ms` 240–419.

Two of the three non-model segments are serial HTTP round trips from the runner to the daemon. Before the first model call the runner reads memory with `GET /v1/runners/me/memory/{fleet_id}`. After the answer it makes three posts in order: `tool-calls` (only when the run called a tool), `memory/{fleet_id}` (the memory push) and `reports`. The daemon publishes `event_complete` only after the report settles and its billing steps run.

Before any of that, a fresh message waits for a worker's next idle poll. M225's doorbell makes the daemon find the work on its next poll, but the runner still asks on a timer. The worst wait is `NO_WORK_RETRY_AFTER / workers`: 62 ms on dev's 16 workers, the full second on a one-worker host (`DEFAULT_WORKERS`, `afd_core/src/limits.rs`).

An operator reads none of this per fleet in Grafana. The runner's segments reach only its host's journal, and the one span that joins daemon and runner (`fleet.delivery`) starts at the first model call and ends at the report.

**Solution summary:** §1: the report body carries the run's memory deltas; the daemon writes them before the settle commits and publishes `event_complete` as soon as it commits, before billing and the audit event. §2: the lease reply carries the fleet's memory window, read after the claim commits, so it holds the previous run's memory and the runner reads none. §3: a runner's heartbeat asks to be held; the daemon holds it until its doorbell rings or the heartbeat interval passes, and a woken runner polls at once. §4: `fleet.delivery` spans event creation to `event_complete` with one attribute per segment, so Grafana answers p95 and p99 per fleet from Tempo; the existing `DeliveryStage` histogram gains the same boundaries fleet-wide. §5: the runner's lease reply body and the memory deltas it captures zero on drop. §6: the acceptance lane's credential mint retries a Cloudflare edge error, which turned the Nov 03 deploy red.

**Design flow:**

```text
message ─► admission commits ─► doorbell rings ─► held heartbeat answers woken ─► one idle worker polls now
        ─► lease reply {policy, history, memory window} ─► first model call (no memory read)
        ─► answer streamed ─► ONE reports post {result, usage, memory deltas, segments}
        ─► daemon: memory written ─► settle commits ─► event_complete ─► billing ─► audit
        ─► fleet.delivery span: event created → event_complete, one attribute per segment ─► Tempo
```

## PR Intent & comprehension handshake

- **PR title (eventual):** perf(lease): one settle post, memory on the lease, a woken runner
- **Intent (one sentence):** A chat's reply reaches its owner without the runner's two serial daemon round trips and without an idle-poll wait, and an operator reads where every chat's time went, per fleet, in Grafana.
- **Authoring handshake:** Indy asked (Nov 03, 2026) where the code could save time after M237's sandbox start, and chose the one-post settle, the memory window on the lease and a per-runner wake over warm-slot wiring. He added five requirements: p95 and p99 per fleet in Grafana for a later dashboard agent; a record of the pending specs; a record of the merged PR's acceptance failure; zeroing of dropped sensitive buffers; and an audit of logs, metrics and traces across the handlers on the chat path.
  ASSUMPTIONS I'M MAKING:
  1. Memory continuity (the next lease sees this run's memory) outranks the saved milliseconds, so memory is written before the settle commits, never after it.
  2. The daemon deploys before any runner (Continuous Integration (CI) deploys the daemon on merge; a runner deploys by playbook later), so a new wire field is optional on the wire and a runner sends it only after the daemon has proved it accepts it.
  3. Per-fleet percentiles come from Tempo on a span attribute, never from a metric label, because `observability.md` §Label registry forbids fleet identity on a metric.
  4. The runner has no collector on dev (M233 §2 is unbuilt), so every figure Grafana must show rides the report to the daemon and is exported from there.
- **Implementer handshake:** recorded in the PR Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afr_supervisor/src/lease_loop/settle.rs` — the settle today: records post, memory push, renewal flush, spool, report post. §1 removes the memory push.
2. `rustd/crates/afd_fleet/src/lease/report/mod.rs` and `rustd/crates/afd_fleet/src/lease/report/steps.rs` — load, `commit_report`, `settle_usage`, `release_usage`, `announce`. §1 writes memory before the commit and publishes right after it.
3. `rustd/crates/afd_fleet/src/lease/memory.rs` and `rustd/crates/afd_fleet/src/lease/fence/mod.rs` — the fence: a memory push is refused once the lease row leaves `LEASE_STATUS_ACTIVE`, which is why memory cannot follow the report.
4. `rustd/crates/afd_dragonfly/src/doorbell/mod.rs` — the ring is a Dragonfly sharded publish every replica's hub hears; `Doorbell` is each replica's hint buffer. §3 adds a waiter there.
5. `rustd/crates/afr_supervisor/src/heartbeat/mod.rs` and `rustd/crates/afr_supervisor/src/worker_pool/mod.rs` — the beat loop that will ask to be held, and the per-worker pause the wake ends.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_wire/src/report/mod.rs` | EDIT | the report body's optional memory deltas and segment durations |
| `rustd/crates/afd_wire/src/lease/payload.rs` | EDIT | the lease payload's optional memory window |
| `rustd/crates/afd_wire/src/runner/mod.rs` | EDIT | the heartbeat's hold request and woken answer |
| `rustd/crates/afd_fleet/src/lease/report/mod.rs` | EDIT | memory written before the commit; completion published right after it; the completion instant answered |
| `rustd/crates/afd_fleet/src/lease/report/steps.rs` | EDIT | the publish leaves `announce`; the audit event stays |
| `rustd/crates/afd_fleet/src/lease/report/tests.rs` | EDIT | the order proofs |
| `rustd/crates/afd_fleet/src/lease/settle.rs` | EDIT | the loaded lease carries the event's and the lease's creation instants |
| `rustd/crates/afd_fleet/src/lease/sql/select_lease_for_report.sql` | EDIT | selects the two instants |
| `rustd/crates/afd_fleet/src/lease/deliver/mod.rs` | EDIT | the memory window read after the claim commits, on the payload |
| `rustd/crates/afd_fleet/src/lease/store.rs` | EDIT | the doorbell waiter reachable from the plane |
| `rustd/crates/afd_fleet/tests/integration_report_commit.rs` | EDIT | memory lands before the settle; a fenced settle leaves memory upserted; the publish precedes billing |
| `rustd/crates/afd_fleet/tests/integration_lease_issue.rs` | EDIT | the window on the lease; the window after a report; no lease while the slot is held |
| `rustd/crates/afd_fleet/tests/integration_memory_hydrate_order.rs` | EDIT | the window read failure leaves the lease issuable |
| `rustd/crates/afd_fleet/tests/fleet_suite.rs` | EDIT | registers the new cases |
| `rustd/crates/afd_dragonfly/src/doorbell/mod.rs` | EDIT | every ring notifies waiters before the hint dedup |
| `rustd/crates/afd_dragonfly/src/doorbell/tests.rs` | EDIT | a waiter is woken by a ring for an already-buffered fleet |
| `rustd/crates/afd_api_runner/src/handler/runner/heartbeat.rs` | EDIT | the hold after liveness is recorded; the cap; the woken answer |
| `rustd/crates/afd_api_runner/src/handler/runner/wake.rs` | CREATE | the held-beat count, its cap and the wait on the doorbell |
| `rustd/crates/afd_api_runner/src/handler/runner/wake_tests.rs` | CREATE | ring, deadline, cap and liveness-order proofs |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs` | EDIT | the span's start and end instants and segment attributes; the extended histogram |
| `rustd/crates/afd_api/tests/harness/stubs_runner.rs` | EDIT | builds the extended `Reconciled` |
| `rustd/crates/afd_cron/src/service.rs` | EDIT | builds the extended `Reconciled` |
| `rustd/crates/afd_observability/src/delivery/mod.rs` | EDIT | the span starts at event creation, ends at the completion instant, carries the segment attributes, caps every runner duration |
| `rustd/crates/afd_observability/src/semconv/mod.rs` | EDIT | the segment attribute keys |
| `rustd/crates/afd_observability/src/metrics/label/fleet.rs` | EDIT | `DeliveryStage` gains three boundaries; the `WakeAnswer` set |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs` | EDIT | `agentsfleet_runner_wake_answers_total` |
| `rustd/crates/afd_observability/src/producers/fleet/mod.rs` | EDIT | the wake-answer producer |
| `rustd/crates/afd_observability/tests/producers_drive.rs` | EDIT | drives the new producer and the new stages |
| `docs/metrics.census.tsv` | EDIT | the new family; the stage ceiling 4 → 7 |
| `rustd/crates/afd_core/src/limits.rs` | EDIT | `HELD_BEATS_MAX` |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | `WAKE_HOLD_MAX`, asserted at or below `HEARTBEAT_INTERVAL` |
| `public/openapi.json` | EDIT | regenerated for the three changed wire types |
| `rustd/crates/afr_supervisor/src/lease_loop/settle.rs` | EDIT | memory rides the report; the push stays for a lease without a window; a shape refusal falls back once |
| `rustd/crates/afr_supervisor/src/report_spool/mod.rs` | EDIT | a shape refusal of a one-post body is answered to the settle, not quarantined |
| `rustd/crates/afr_supervisor/src/lease_loop/mod.rs` | EDIT | the memory read made only when the lease carries no window |
| `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` | EDIT | the segments handed to the report builder |
| `rustd/crates/afr_supervisor/src/report/mod.rs` | EDIT | memory deltas and segment durations on the body, only when the lease carried a window |
| `rustd/crates/afr_supervisor/src/memory/mod.rs` | EDIT | the captured deltas as a zeroing type |
| `rustd/crates/afr_supervisor/src/heartbeat/mod.rs` | EDIT | the beat asks to be held while leasing; a woken answer wakes one idle worker and beats again |
| `rustd/crates/afr_supervisor/src/heartbeat/tests.rs` | EDIT | held, woken, timed-out and not-held beats |
| `rustd/crates/afr_supervisor/src/worker_pool/mod.rs` | EDIT | an idle pause ends on the wake; the stagger stays |
| `rustd/crates/afr_supervisor/src/worker_pool/tests.rs` | EDIT | one woken worker polls at once, the rest keep their pause |
| `rustd/crates/afr_supervisor/src/lib.rs` | EDIT | the wake signal shared by the beat loop and the pool |
| `rustd/crates/afr_supervisor/src/client/mod.rs` | EDIT | the reply body zeroes on drop |
| `rustd/crates/afr_supervisor/src/client/http.rs` | EDIT | the compile-time proof that a held beat fits the call timeout |
| `rustd/crates/afr_supervisor/src/client/lease_verbs/mod.rs` | EDIT | the report verb takes the zeroing body |
| `rustd/crates/afr_supervisor/src/test_support/mod.rs` | EDIT | the fake daemon answers through the zeroing body |
| `rustd/crates/afr_supervisor/src/lease_loop/tests/settlement.rs` | EDIT | one post on a window lease; the push on a lease without one |
| `rustd/crates/afr_supervisor/src/lease_loop/tests/timing.rs` | EDIT | no memory read on a window lease |
| `rustd/crates/afr_supervisor/src/memory/tests.rs` | CREATE | the zeroing proofs |
| `cli/test/acceptance/fixtures/retryable.ts` | CREATE | the retryable and unprocessed status sets, the backoff and the retry loop, shared by mint and revoke |
| `cli/test/acceptance/fixtures/cli-credential-revoke.ts` | EDIT | reads the shared retry machinery |
| `cli/test/acceptance/fixtures/cli-credential-revoke.test.ts` | EDIT | reads the attempt cap from the shared module |
| `cli/test/acceptance/global-teardown.ts` | EDIT | reads the worst-case budget from the shared module |
| `cli/test/acceptance/fixtures/workspace-hydration.ts` | EDIT | the credential mint retries through it |
| `cli/test/acceptance/fixtures/workspace-hydration.test.ts` | EDIT | the four retry proofs, each comparing the call's whole outcome |
| `docs/architecture/lease_flow.md` | EDIT | the one-post settle and the window on the lease |
| `docs/architecture/runner_fleet.md` | EDIT | the held heartbeat in the control protocol; memory continuity through the report |
| `docs/architecture/runner_execution.md` | EDIT | the runner's settle and wake |
| `docs/architecture/scaling.md` | EDIT | held beats beside the idle poll volume |
| `docs/architecture/observability.md` | EDIT | the extended span and histogram; the chat-path coverage table |
| `playbooks/operations/observability/002_round_trip_percentiles.md` | CREATE | the TraceQL queries a dashboard agent pastes, per fleet and fleet-wide |
| `playbooks/README.md` | EDIT | the inventory row |
| `VERSION` | EDIT | 0.73.0 |
| `cli/package.json` | EDIT | 0.73.0 |
| `rustd/Cargo.toml` | EDIT | 0.73.0 |
| `rustd/Cargo.lock` | EDIT | 0.73.0 |
| `docs/v2/pending/M238_001_P1_API_DOCS_INFRA_OBS_CHAT_ROUND_TRIP_DROPS_THREE_WAITS.md` | EDIT | this spec, marked IN_PROGRESS in `active/` at open and DONE in `done/` at close |
| `~/Projects/docs/changelog.mdx` | EDIT | on branch `chore/m238-chat-round-trip-changelog`: replies settle one round trip after the answer; the lease and report shapes |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE for the held beat's and the settle's error paths; RS-RACE for the doorbell waiter and the held-beat count.
- **`domains/http.md`** — HTTP-GARDE and HTTP-VALID: the report's `memory` field reuses `MemoryDelta`'s bounds; the daemon admits the total through `afd_memory::admit`, as the push does.
- **`domains/http-surface.md`** — the OpenAPI document regenerated with `--no-banner`; the new fields documented as optional.
- **`domains/logging.md`** — segment figures are span attributes and histogram samples, not new log lines; the one new log event is the held beat's refusal.
- **`domains/sql.md`** — the report's select gains two columns; no schema change.
- **`tests.md`** — TCF, TEST-PARTIAL, FLL; every failure mode's negative test is named below.
- **`docs.md`** — the architecture pages describe the design as the design (SPEC-FRAME); DOC-05 bans `hydrate` as prose, so the pages say "memory read" and keep `hydrate` only as the verb's name.
- **`changelog.md`** — CL-USER and CL-EARNED for the docs-repository entry.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `orly gate work` conform at commit | yes — Rust and TypeScript change | `make lint-all` before each commit |
| `make check-version` | yes — wire shapes change | the four version files move together |
| OpenAPI regeneration | yes — three wire types change | regenerate; never hand-edit |
| Label registry (fleet identity never on a metric) | yes — per-fleet figures requested | fleet identity rides the span attribute; the histogram's only label stays the closed `DeliveryStage` |
| Census registry test | yes — one new family, one ceiling change | one census row, one `Declared` constant, one producer |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes — `report/mod.rs`, `settle.rs` and `heartbeat.rs` are near the cap | the hold lands in `wake.rs`; the memory write in its own function |
| gitleaks | no — no secret material | none |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_fleet/src/lease/history.rs` — a bounded payload the lease already carries; the memory window follows the same shape, with `afd_memory::window::select` as its cut.
- **Reference:** `rustd/crates/afd_memory/src/admit.rs` — the push's cut-and-count at `MAX_PUSH_BYTES`; the report's memory goes through the same function, so an oversize set is truncated and counted, never refused.
- **Reference:** `rustd/crates/afr_supervisor/src/heartbeat/mod.rs` — the beat already answers at once when holds saturate (`holds.saturated().notified()`); the held beat is the daemon-side mirror of that pattern, and M225's rejected design (a held request per worker) is not it: one held beat per host.
- **Reference:** `rustd/crates/afd_observability/src/delivery/mod.rs` — the synthesised span; §4 moves its start and end and adds attributes rather than adding a second span.
- **Reference:** `rustd/crates/afd_crypto/src/secret/mod.rs` — `SecretBytes` with `zero_on_drop!`, the derive-less zeroing shape this workspace uses; §5 reuses `zeroize::Zeroizing` for the body and that shape for the deltas.
- **Reference:** `cli/test/acceptance/fixtures/cli-credential-revoke.ts` — `RETRYABLE_STATUSES`, jittered backoff honouring `Retry-After`, injectable `sleep` and `random`; §6 extracts it rather than writing a second schedule.

## Sections (implementation slices)

### §1 — A chat reply settles in one post, and `event_complete` publishes when the settle commits

`ReportRequest` gains `memory: Vec<MemoryDelta>`, default empty. The daemon's report verb, after it loads the lease as this runner's, calls `Memories::capture` with the deltas, then `commit_report`. The capture runs in its own transaction under the memory role (`SET LOCAL ROLE memory_runtime`, `afd_memory/src/postgres`), so nothing crosses the role boundary; it precedes the commit, so a lease issued after the commit reads it (§2). The capture is an idempotent upsert, so a report the daemon then refuses (`Fenced`) or loses (the runner's spool retries) writes the same rows again without duplication. A capture that fails is logged under `memory_capture_failed` and the report still settles, the decision `settle.rs` records on `fde585971`: withholding the answer would lose it too and let the lease lapse into a re-run. `admit` truncates deltas past `MAX_PUSH_BYTES` and counts them, as the push does; the report is never refused for its memory. The completion publish moves ahead of the Postgres steps that follow the commit: `publish_completion` runs right after `settle_usage` and `release_usage` (two biller calls, kept ahead of it because `money_gates` reads the payer's balance through the biller and an open hold could park the owner's follow-up) and before `record_released`, `queue_owed_answer` and `run_tokens`.

Skew is handled on two sides. `ReportRequest` denies unknown fields, so a runner sends `memory` and the §4 segment durations only when the lease reply carried a `memory` window, which only a daemon at this release sends (§2). A rolling deploy can still hand a lease from a new replica and the report to an old one, so a report refused for its shape (400 `UZ-REQ-001`) on a one-post body is answered to the settle rather than quarantined: the runner pushes the deltas through the memory push verb, re-posts the report without the new fields once, and logs `report_shape_fell_back`. A lease without a window is settled as on `fde585971`: the memory push, then the report. Tool records stay as they are: posted before the report, only when the run called a tool, and absent from a chat. The renewal flush stays: it posts only usage lines that do not fit the report's `requests` field, which a chat never reaches.

- **Dimension 1.1** — Memory on a report is written before the settle commits → Test `test_report_memory_lands_before_the_settle` (integration): a report with two deltas settles; `select_all_for_fleet` reads both; a lease issued to the fleet afterwards carries both in its window. False pass rejected: a daemon that wrote memory after the commit would show the same rows here, so the test also issues the next lease inside the same request's completion and reads the window from it.
- **Dimension 1.2** — A settle the fence refuses leaves memory upserted and the lease active, and the runner's retry settles once → Test `test_fenced_report_keeps_memory_and_settles_on_retry` (integration): the deltas are read back once after the refusal and once after the retry, identical; one settlement row.
- **Dimension 1.3** — Deltas past `MAX_PUSH_BYTES` are truncated and counted, and the report settles → Test `test_oversize_report_memory_is_cut_and_counted` (integration): 257 KiB of deltas → `Captured.truncated` ≥ 1, the report settles, the newest entries are kept.
- **Dimension 1.4** — `event_complete` publishes before the post-commit Postgres steps → Test `test_completion_publishes_before_the_audit_event` (integration, log capture): the `completion_published` event precedes the `record_released` finalize step in the capture, and the completion frame is on the fleet's stream when the report call returns. False pass rejected: a publish after the audit event would still leave the frame on the stream, so the order is read from the capture, not the stream.
- **Dimension 1.5** — A capture that fails leaves the report settling → Test `test_failed_capture_still_settles` (integration): the memory store's pool is closed; the report settles; one `memory_capture_failed` line; `Captured` absent.
- **Dimension 1.6** — The runner makes one `reports` post on a lease that carried a window → Test `test_settle_is_one_post_on_a_window_lease` (unit): the fake daemon records one `Report` call and no `Capture` call; the body's `memory` holds the run's deltas.
- **Dimension 1.7** — On a lease without a window the runner pushes memory and sends a report with neither `memory` nor segment durations → Test `test_settle_without_a_window_posts_as_before` (unit): one `Capture` then one `Report`; the report JSON has no `memory` key and no segment keys.
- **Dimension 1.8** — A one-post report refused for its shape falls back once → Test `test_shape_refused_report_falls_back_once` (unit): the fake daemon answers the first `Report` with 400 `UZ-REQ-001`; the runner makes one `Capture`, then one `Report` whose JSON has no `memory` key; a second 400 quarantines as before; one `report_shape_fell_back` line.

### §2 — The lease reply carries the fleet's memory window

`LeasePayload.memory: Option<MemoryHydrateResponse>` carries what `GET /v1/runners/me/memory/{fleet_id}` answers today, read by `issue_ready` after `Leases::issue` returns and bounded as the verb is (`HYDRATE_WINDOW_BYTES` 256 KiB plus `HYDRATE_SHARED_BYTES` 64 KiB). A daemon at this release always sends `Some`, an empty window included, so the runner reads the field's presence as the daemon's version. The ordering invariant rests on the fleet slot: `release_through` frees the slot inside the settle transaction, the candidate query skips a held slot, so no lease issues for a fleet between a report's memory write and its commit. A window read that fails is logged under `lease_window_read_failed`, the lease issues with `memory: None`, and the runner reads memory as before. The runner seeds its run from the payload and makes no memory read when the window is present; the verb stays for a lease without one.

- **Dimension 2.1** — A lease reply carries the window the verb answers → Test `test_lease_carries_the_memory_window` (integration): a fleet with three entries is leased; the payload's `memory` equals the verb's answer.
- **Dimension 2.2** — No lease issues for a fleet whose slot a running lease holds, and the lease issued after that lease's report carries the report's memory → Test `test_window_follows_the_slot` (integration): a poll during the run answers no work; the poll after the report leases and its window holds the report's deltas.
- **Dimension 2.3** — A window read that fails leaves the lease issuable → Test `test_window_read_failure_issues_without_a_window` (integration): the memory store's pool is closed before the poll; the lease issues with `memory: None`; one `lease_window_read_failed` line.
- **Dimension 2.4** — The runner makes no memory read when the lease carries a window → Test `test_no_memory_read_on_a_window_lease` (unit): the fake daemon records no `Hydrate` call; `memory_ms` is absent from `lease_completed`.
- **Dimension 2.5** — The runner reads memory when the lease carries no window → Test `test_memory_read_without_a_window` (unit): one `Hydrate` call; `memory_ms` present.

### §3 — A held heartbeat wakes one idle worker

`HeartbeatRequest` gains `wake: bool`, default false; `HeartbeatResponse` gains `woken: bool`, present on every reply a daemon at this release sends. The runner sets `wake` only when an earlier reply carried `woken` (so an older daemon, which would read the unknown field as no report at all and drop the hold list, is never asked), the assignment is `Ok` with workers above zero, and a worker is idle; with every worker busy the beat is not held, so the hold-saturation path the beat loop already has is untouched. The daemon records liveness and reconciles holds as before, then, when `wake` is set and fewer than `HELD_BEATS_MAX` (64) beats are held in this process, awaits a ring up to `WAKE_HOLD_MAX`, which equals `HEARTBEAT_INTERVAL` (10 s), so a held beat returns no later than the next beat was due and liveness is unchanged. The ring signal is a `tokio::sync::watch` generation counter the `Doorbell` increments on every ring before its hint dedup: a hold reads the generation at arrival and awaits a change, so a ring between arrival and await is kept, and a second admission for a fleet whose hint is still buffered wakes a holder. A ring answers `woken: true`; the deadline answers `woken: false`; a beat past the cap is not held and answers `woken: false` at once, counted under `reason="capped"` and logged once under `runner_wake_refused`. The ring itself is a Dragonfly sharded publish every replica's hub hears (`doorbell::publish`, `Doorbell::listen`), so a beat held on one replica is woken by an admission committed on another; a `Received::Lagged` or `Gap` rings nothing, and the runner's 1 s hint is the fallback. The held beat consumes one in-flight permit of `DEFAULT_MAX_IN_FLIGHT` (256); the cap keeps 192 for requests.

On the runner, the beat loop runs as before; a `woken: true` sets the next pause to zero and calls `notify_one` on a `tokio::sync::Notify` the worker pool's idle pause selects on (`notify_one` stores a permit, so a worker entering its pause after the wake still takes it), so exactly one idle worker polls at once and the stagger of the others stands. `WAKE_HOLD_MAX` is asserted below the client's `CALL_TIMEOUT` (20 s) beside the existing assertion in `client/http.rs`. `NO_WORK_RETRY_AFTER` stays at 1 s: it is the fallback for a lost ring, and a lost ring must not wait five seconds. **Implementation default:** every admission, gate confirmation and approval resolution rings, and every ring wakes every held beat, whatever the runner's labels; the woken worker's one poll filters. This is bounded by held-beat count, costs one authenticated poll per runner host per ring, and is revisited above 50 runner hosts.

- **Dimension 3.1** — A ring answers a held beat → Test `test_ring_answers_the_held_beat` (unit, paused clock): a beat with `wake: true` is held; `Doorbell::ring` fires; the reply reads `woken: true` within `READY_WAKE_DEADLINE` (100 ms) of the ring.
- **Dimension 3.2** — A beat with no ring answers at the deadline → Test `test_silent_held_beat_answers_at_the_deadline` (unit, paused clock): 10 s pass; `woken: false`; `reason="deadline"` counted.
- **Dimension 3.3** — The 65th held beat is not held → Test `test_held_beats_are_capped` (unit): 64 beats held; the 65th answers at once with `woken: false`; `reason="capped"` counted; one `runner_wake_refused` line with the cap.
- **Dimension 3.4** — A ring for an already-buffered fleet still wakes a holder, and a ring between a hold's arrival and its await is kept → Test `test_ring_generation_survives_dedup_and_arrival` (unit): the fleet is rung twice without a `take` between, and a waiter that read its generation before the second ring and awaited after it is woken.
- **Dimension 3.5** — Liveness is recorded before the hold → Test `test_liveness_precedes_the_hold` (unit, paused clock): the `seen` producer fires before the hold's deadline passes.
- **Dimension 3.9** — The runner asks to be held only after a reply carried `woken`, and only with an idle worker → Test `test_wake_is_asked_only_when_proved_and_idle` (unit): the first beat carries `wake: false`; after a reply with `woken: false`, the next beat carries `wake: true`; with every worker busy it carries `wake: false`.
- **Dimension 3.6** — A woken runner wakes exactly one idle worker → Test `test_woken_beat_polls_one_worker_at_once` (unit, paused clock): sixteen workers mid-pause; `woken: true`; one `Lease` call before 1 ms more passes; the other fifteen poll at their staggered instants.
- **Dimension 3.7** — A beat that times out on the runner is retried on the client's backoff and the workers keep their hint → Test `test_timed_out_held_beat_backs_off` (unit, paused clock): the fake daemon stalls past `CALL_TIMEOUT`; the next beat lands after `RETRY_FIRST_DELAY`; the workers' next poll lands at the hint.
- **Dimension 3.8** — With no ring the worker leases on the hint → Test `test_lost_ring_falls_back_to_the_hint` (unit, paused clock): no `woken`; the next poll lands at `NO_WORK_RETRY_AFTER` and leases.

### §4 — Every segment of every chat is read in Grafana at p95 and p99, per fleet

`fleet.delivery` starts at the event's creation (`event_created_at`, selected with the lease for the report) and ends at the instant `publish_completion` returns, which `Reconciled` carries to the handler. It keeps `gen_ai.agent.id` (the fleet) and gains one attribute per segment: `agentsfleet.delivery.queue_wait_ms` (event creation to lease issue, both daemon instants on the lease row), `agentsfleet.delivery.memory_ms`, `.model_ms`, `.tools_ms`, `.sandbox_wait_ms` and `.settle_ms` (the runner's segments, carried in `ReportTelemetry`; `settle_ms` is hold-or-destroy to the report post's start), and `agentsfleet.delivery.complete_ms` (report received to `event_complete`). Every runner-supplied duration is capped at `MAX_RUN` as `wall` is today. `time_to_first_token_ms` already rides the body and is not duplicated. The segments are not a partition of the span (`sandbox_wait_ms` runs beside the model call), so no sum is asserted.

Per fleet, the playbook page holds two queries against Tempo in Grafana Explore: a TraceQL search over `fleet.delivery` filtered by `gen_ai.agent.id`, which every Tempo answers and whose duration column Grafana aggregates to p95 and p99 with a transformation, and a TraceQL metrics query (`quantile_over_time`) that needs Tempo's metrics generator; Dimension 4.4 records which of the two dev answered. `fleet.delivery` is one span per settled report, so the lease-poll request spans that share `SPAN_QUEUE` (2048) with it are the only thing that could drop it under load; §3 adds one poll per ring, not one per worker. Fleet-wide, `DeliveryStage` gains `EventToComplete`, `LeaseToReport` and `ReportToComplete`, recorded at the publish on `agentsfleet_fleet_delivery_stage_seconds`, whose census ceiling rises to seven. No fleet, workspace or tenant identity reaches a metric. The dashboard JSON is not edited here (Discovery).

- **Dimension 4.1** — The span's start, end and segment attributes come from the loaded lease and the report → Test `test_delivery_span_spans_event_to_complete` (unit): a `Reconciled` with `event_created_at` T, `lease_created_at` T+40 ms, `completed_at` T+900 ms and a report with `memory_ms` 90, `model_ms` 500, `settle_ms` 30 → start T, end T+900 ms, `queue_wait_ms` 40, the three figures, `complete_ms` = T+900 ms minus the receipt instant.
- **Dimension 4.2** — A runner-supplied duration past `MAX_RUN` is capped → Test `test_segment_attributes_are_capped` (unit): `model_ms` = `u64::MAX` → the attribute reads `MAX_RUN` in milliseconds.
- **Dimension 4.3** — The three new stages record on the existing histogram under closed labels only → Test `test_completion_records_three_stages` (unit): one publish → three samples, labels in `DeliveryStage::ALL`, no other label key.
- **Dimension 4.4** — The two TraceQL queries answer per-fleet quantiles on dev → Test `test_round_trip_queries_answer_on_dev` (manual): after ten chats, the two queries from `002_round_trip_percentiles.md` are pasted into Grafana Explore against dev Tempo; the screenshot path is recorded in Discovery.

### §5 — The runner's lease reply body and captured memory deltas zero on drop

`Body` holds its bytes in a `zeroize::Zeroizing<Vec<u8>>`, so the lease payload and the memory window borrowed from it zero when the lease run drops; `RunnerApi::send` answers that type and the fake daemon converts its `Bytes` once. The deltas a run produces travel in `CapturedMemory`, a newtype over `Vec<MemoryDelta<'static>>` whose `Zeroize` impl zeroes every owned string and whose `Drop` calls it, in the `zero_on_drop!` shape `afd_crypto` uses. The claim is scoped to the heap copies the runner owns at a lease's end. Not covered, and said so in `runner_execution.md`: the bytes handed to `reqwest` for a post, which the client frees unzeroed; and the report spool's copy, which `ReportSpool::hold` writes to disk in clear and keeps as `Bytes` until delivery. Widening the spool's storage is a later spec. **Implementation default:** no type is added for the report body, because its sensitive content is the deltas, which `CapturedMemory` covers before they are serialised.

- **Dimension 5.1** — The body and the deltas are zeroing types → Test `test_sensitive_buffers_zeroize` (unit): `fn pins<T: Zeroize>()` instantiated for `Body`'s bytes and `CapturedMemory` compiles and runs.
- **Dimension 5.2** — `zeroize` empties every owned delta string → Test `test_captured_memory_zeroizes_its_strings` (unit): two deltas with owned key, content and category; `zeroize()`; every field is empty and the vector is empty.

### §6 — The acceptance lane survives a Cloudflare edge error

`mintCliCredential` gains `options: { sleep, random }` and retries through `withRetries`, the loop extracted from the revoke fixture into `retryable.ts` with its backoff (doubling from 500 ms, jittered, `Retry-After` honoured up to 10 s) and its cap (five attempts). The mint is a `POST` that writes a credential, so it retries only `UNPROCESSED_STATUSES` (425, 429, 502, 503), the answers that prove nothing was minted, and a connection that never opened; a 500, 504 or timeout may follow a mint that landed and throws at once, so a retry never leaves a second live credential behind the first. The revoke keeps the wider `RETRYABLE_STATUSES` set, because its `DELETE` is idempotent. The last failure throws the last status and detail. The Nov 03 deploy went red on one 502 from the Cloudflare edge while the dev daemon restarted (Discovery); the lane's 170 other live tests passed.

- **Dimension 6.1** — DONE — An edge 502 followed by a 200 mints → Test `mints after an edge 502` (unit, `workspace-hydration.test.ts`): a fetch stub answering 502 then 200 → the whole outcome `{minted, error: null, calls: 2, slept: [500]}`.
- **Dimension 6.2** — DONE — Five retryable answers throw the last status → Test `throws after five retryable answers` (unit): 503 every time → `{minted: null, error: "CLI credential mint 503: … (gave up after 5 attempts)", calls: 5, slept: [500, 1000, 2000, 4000]}`.
- **Dimension 6.3** — DONE — A rejection is not retried → Test `does not retry a rejection` (unit): 401 → one call, no sleep, the status in the error.
- **Dimension 6.4** — DONE — A gateway timeout is not retried, because it may follow a mint that landed → Test `does not retry a gateway timeout, which may follow a mint that landed` (unit): 504 → one call, no sleep.

## Interfaces

- `POST /v1/runners/me/reports` body gains `memory: [MemoryDelta]` (default empty; admitted through `afd_memory::admit` at `MAX_PUSH_BYTES`) and `telemetry` gains `memory_ms`, `model_ms`, `tools_ms`, `sandbox_wait_ms`, `settle_ms`, each optional. A runner sends them only when its lease carried `memory`.
- `POST /v1/runners/me/leases` reply: `lease.memory` is a `MemoryHydrateResponse`, present on every lease a daemon at this release issues, absent when the window read failed.
- `POST /v1/runners/me/heartbeats` body gains `wake: bool` (default false); the reply gains `woken: bool`, present on every reply. A beat with `wake` is held up to `WAKE_HOLD_MAX` or until a ring.
- `fleet.delivery` span: start at event creation, end at `event_complete`; attributes `agentsfleet.delivery.{queue_wait,memory,model,tools,sandbox_wait,settle,complete}_ms`.
- `DeliveryStage` gains `event_to_complete`, `lease_to_report`, `report_to_complete`. New family `agentsfleet_runner_wake_answers_total{reason}` with `reason` ∈ `ring`, `deadline`, `capped`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Report fenced after its memory was written | failure | a reclaim won the fence between the write and the commit | the capture stands (idempotent upsert); the report answers `stale_fence`; the runner's retry rewrites the same rows and settles once | `test_fenced_report_keeps_memory_and_settles_on_retry` |
| Oversize memory on the report | capacity | deltas past `MAX_PUSH_BYTES` | truncated newest-first and counted; the report settles | `test_oversize_report_memory_is_cut_and_counted` |
| Memory capture fails on the report | failure | the memory store refuses the write | logged; the report settles; the fleet's next run starts without those items | `test_failed_capture_still_settles` |
| Audit or owed-answer step stalls after the commit | failure | a slow Postgres step after the biller calls | `event_complete` is already published; the step is logged if it fails | `test_completion_publishes_before_the_audit_event` |
| Newer runner, older replica mid-deploy | failure | a one-post report meets a daemon that denies the new fields | 400 answered to the settle; the runner pushes memory, re-posts without the fields once | `test_shape_refused_report_falls_back_once` |
| Window read fails at issue | failure | the memory store refuses the read | the lease issues with `memory: None`; one log line; the runner reads memory as before | `test_window_read_failure_issues_without_a_window` |
| Older daemon, newer runner | failure | a lease without `memory` | the runner pushes memory and sends a report without the new fields | `test_settle_without_a_window_posts_as_before` |
| Held beats past the cap | capacity | more than 64 held beats in one process | the beat answers at once, `woken: false`, counted, logged once | `test_held_beats_are_capped` |
| Ring lost | failure | `Received::Lagged` or `Gap` on the hub | the held beat answers `woken: false` at the deadline; the 1 s hint leases | `test_lost_ring_falls_back_to_the_hint` |
| Held beat times out on the runner | failure | a stall past `CALL_TIMEOUT` | the beat is retried on the client's backoff; the last assignment stands; workers keep the hint | `test_timed_out_held_beat_backs_off` |
| Runner-supplied duration out of range | security | a runner reports an absurd segment | capped at `MAX_RUN` on the span | `test_segment_attributes_are_capped` |
| Edge 502 at credential mint | failure | daemon restarting behind the edge | bounded retries; the last status thrown | `throws after five retryable answers` |
| Mint answered 504 or timed out | failure | the mint may have landed | thrown at once; no second credential minted | `does not retry a gateway timeout, which may follow a mint that landed` |

## Invariants

1. A lease issued after a report's commit carries that report's memory. The authority is the fleet slot: `release_through` frees it inside the settle transaction, the candidate query skips a held slot, and the capture precedes the commit in the report verb.
2. `event_complete` publishes before the post-commit Postgres steps. `publish_completion` follows the two biller calls and precedes `record_released`, `queue_owed_answer` and `run_tokens`; Dimension 1.4 reads the order from the log capture.
3. Fleet identity never reaches a metric. `DeliveryStage` and `WakeAnswer` are closed sets and the only labels; the census rows declare no other key.
4. `WAKE_HOLD_MAX ≤ HEARTBEAT_INTERVAL < CALL_TIMEOUT`. `const` assertions in `afd_core/src/timing.rs` and `afr_supervisor/src/client/http.rs`.
5. A held beat never starves requests. The per-process held-beat count is an `AtomicUsize` in `wake.rs`, capped at `HELD_BEATS_MAX` = 64 of the 256 permits; Dimension 3.3 reads the 65th answered at once.
6. The lease body and the captured deltas zero on drop. `Zeroizing<Vec<u8>>` and `CapturedMemory`'s `Drop`, pinned by Dimension 5.1.
7. A runner never loses a report to a field the daemon refuses. `memory` and the segment durations ride the report only when the lease carried `memory`, and a shape refusal re-posts without them once; Dimensions 1.7 and 1.8. `wake` rides the beat only after a reply carried `woken`; Dimension 3.9.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `fleet.delivery` (span, extended) | operator | a report settles | segment durations, `gen_ai.agent.id`, event and workspace ids | identifiers and durations only; no content | `test_delivery_span_spans_event_to_complete` |
| `agentsfleet_fleet_delivery_stage_seconds{stage}` (three new stages) | operator | `event_complete` publishes | `stage` from `DeliveryStage` | closed label set | `test_completion_records_three_stages` |
| `agentsfleet_runner_wake_answers_total{reason}` | operator | a held beat answers | `reason` ∈ `ring`, `deadline`, `capped` | closed label set | `producers_drive` |
| `runner_wake_refused` (log) | operator | a beat past the cap | `runner_id`, `cap` | no token | `test_held_beats_are_capped` |
| `lease_window_read_failed` (log) | operator | the window read fails at issue | `fleet_id`, `error_code` | no content | `test_window_read_failure_issues_without_a_window` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The one-post settle and its order proofs exist in tracked sources (§1) | `git grep -l test_report_memory_lands_before_the_settle rustd/crates/afd_fleet/tests` | one path | P0 | |
| R2 | The window on the lease and its ordering proofs exist (§2) | `git grep -l test_window_follows_the_slot rustd/crates/afd_fleet/tests` | one path | P0 | |
| R3 | The held beat's ring, deadline and cap proofs exist (§3) | `git grep -l test_held_beats_are_capped rustd/crates/afd_api_runner/src` | one path | P0 | |
| R4 | The runner wakes one worker and falls back to the hint (§3) | `git grep -l test_woken_beat_polls_one_worker_at_once rustd/crates/afr_supervisor/src` | one path | P0 | |
| R5 | The span covers event creation to completion with capped segments (§4) | `git grep -l test_delivery_span_spans_event_to_complete rustd/crates/afd_api_runner/src` | one path | P1 | |
| R6 | The playbook holds the two per-fleet queries (§4) | `grep -c quantile_over_time playbooks/operations/observability/002_round_trip_percentiles.md` | `2` | P1 | |
| R7 | The lease body and the deltas zero on drop (§5) | `git grep -l test_captured_memory_zeroizes_its_strings rustd/crates/afr_supervisor/src` | one path | P1 | |
| R8 | The credential mint retries an edge error (§6) | `git grep -l "throws after five retryable answers" cli/test/acceptance/fixtures` | one path | P1 | |
| R9 | The census carries the new family and the raised ceiling (§4) | `grep -c agentsfleet_runner_wake_answers_total docs/metrics.census.tsv` | `1` | P1 | |
| S1 | Declared lint green | `make lint-all` | exit 0 | P0 | |
| S2 | Declared unit lane green | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version files agree | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |
| S6 | Rust patch coverage at or above 99% per touched file (the CI coverage lane's merge, graded with the PR's base) | `make test-coverage-rustd-merge RUSTD_PATCH_BASE=HEAD^1` | exit 0 | P0 | |
| S7 | TypeScript coverage 100% (CLI, app, website, design system) | `make test-coverage-all` | exit 0 | P0 | |

## Dead Code Sweep

N/A: no files deleted. The `memory/{fleet_id}` read and push verbs stay for a lease without a window; their removal is a later milestone's sweep, after both sides run this release.

## Out of Scope

- Warm-slot wiring and prepare sub-step timers: the M237 follow-up spec Indy chose on Nov 03, not yet written; the cold start is 42 ms and runs beside the model call.
- The runner collector and runner spans in Grafana: M233_001 §2.
- Edge `Server-Timing` and browser marks: M233_001 §3.
- The Grafana dashboard JSON: not planned here; the playbook page is its input (Discovery).
- Tool records on the report body: not planned; a chat posts none, and a run with tool calls posts them before the report as before.
- Removing the memory read and push verbs: a later milestone, after both sides run this release.
- The toolbox image's contents: a P3 successor to M221_001, not yet written (Discovery holds the figures).
- Model time, prompt size, provider choice: not planned.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A chat reply's "done" state appears one round trip after the answer is written, before billing runs, and a message sent to an idle fleet starts running within one round trip of its admission.
2. **Preserved user behaviour** — Every chat, steer, approval and report flow keeps its shape; memory continuity across runs is unchanged; the memory read and push verbs still answer.
3. **Optimal-way check** — The most direct path is a push channel from daemon to runner; the held heartbeat is that channel with no new route, and one post is the fewest requests a settle can take.
4. **Rebuild-vs-iterate** — Iterate: every change rides an existing verb, span or constant.
5. **What we build** — Three optional wire fields (`memory` on the report, `memory` on the lease, `wake`/`woken` on the beat), five segment durations, one span extension, three histogram stages, one counter family, two zeroing types, one shared retry helper.
6. **What we do NOT build** — A new route (SPEC-BOUNDARY); a per-worker long poll (rejected Oct 12); a per-label doorbell; a second stage histogram; a runner collector (M233 §2); warm slots (M237 follow-up).
7. **Fit with existing features** — Compounds with M225's doorbell and M237's beside-the-model build; must not destabilise memory continuity, which is why memory precedes the commit.
8. **Surface order** — API first; no command-line interface (CLI) or user interface (UI) change.
9. **Dashboard restraint** — No panel is added here; the playbook holds the queries until a dashboard agent lands them against real traces.
10. **Confused-user next step** — An operator reading a slow chat opens `002_round_trip_percentiles.md` and pastes the per-fleet query.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six Sections. §1 and §2 share the memory-continuity invariant and are built first; §3 is independent; §4 consumes §1's telemetry; §5 and §6 are independent and small.
- **Alternatives considered:** pushing memory after the report (rejected: the fence refuses a settled lease); writing memory inside the settle transaction (rejected: the memory store runs under its own role on its own connection); a new wake route (rejected: SPEC-BOUNDARY, and the heartbeat already is the per-host request); a per-worker long poll (rejected by Indy on Oct 12); raising the idle hint to 5 s (rejected: a lost ring would wait five times longer); a second stage histogram (rejected: `DeliveryStage` exists); a per-fleet metric label (rejected: `observability.md` §Label registry); tool records on the report (rejected: a chat posts none, and a long run's records exceed one body).
- **Patch-vs-refactor verdict:** a **patch** because every mechanism extends a verb, span or constant that exists.

## Discovery (consult log)

- **Consults** —
  - *Where the time goes* (Nov 03, 2026, `fde585971`): dev runner `lease_completed`, five chats: `wall_ms` 883–1,025; `memory_ms` 83–105 (one 282); `model_ms` 380–555; `first_token_ms` 346–702; `report_ms` 240–419; `tools_ms` 0; no sandbox segment, and `sandbox_prepare_completed` appears 0 times in three days of journal. Dev runner has 16 workers (`worker=15` highest), so the idle-poll pickup wait is at most 62 ms there. Daemon-side verb times are known only from M233's throttled run (lease 1.67 s over 36 statements, report 1.60 s over 28).
  - *Cold sandbox floor* (Nov 03, `zombie-dev-worker-ant`, three runs each, shell-level): `mke2fs` 7.3–7.6 ms, loop `mount` 24–25 ms, `bwrap --unshare-all /bin/true` 5.0–5.3 ms, cgroup mkdir+freeze 2.4–2.7 ms. A per-lease filesystem has a floor near 30 ms, so a start under 10 ms needs the warm slots `afr_sandbox` already has (1.17 ms claim, M237_002). Out of scope by Indy's choice.
  - *Observability audit* (`fde585971`; `tracing::` events / metric producers / spans per file): `message_steer/mod.rs` 0/0/0; `runner/lease/mod.rs` 0/0/0; `runner/memory.rs` 0/0/0; `runner/heartbeat.rs` 0/0/0; `runner/report.rs` 0/3/1; `lease/report/mod.rs` 1/0/1; `lease/deliver/mod.rs` 2/1/0 (`runner_lease_issued` with `queue_wait_ms`); runner `settle.rs` 5/0/0, `activity/mod.rs` 2/0/0, `heartbeat/mod.rs` 3/0/0, `memory/mod.rs` 0/0/0; `afd_admission/src/wake.rs` 0/0/0. The runner's `OTEL_EXPORTER_OTLP_ENDPOINT` is optional and unset on dev, so its twelve metric families reach no backend there. Daemon histograms: `fleet_delivery_stage_seconds`, `lease_history_bytes`. Five `heartbeat_failed` (`UZ-INTERNAL-003`) on the dev runner in the day before this spec; the daemon logs the same beat under `runner_heartbeat_failed`, so no counter is added. §4 lands the figures where export exists; the coverage table in `observability.md` records the rest.
  - *Toolbox image* (Indy, Nov 03: "is this the optimum size of the debian image? do we have stuff that we donot need?"): the mounted toolbox is 358 MB of files in a 162,066,432 B Enhanced Read-Only File System (EROFS) image (LZ4HC 9, 4 KiB blocks), 144 packages, built by `scripts/toolbox/build.sh` from `scripts/toolbox/manifest.txt` with docs, man, locale and info excluded (M221_001). Largest by `Installed-Size`: `libnode115` 54 MB, `git` 49 MB, `libicu76` 37 MB, `libperl5.40` + `perl-modules-5.40` 49 MB (pulled by `git`), `coreutils` 18 MB, `libc6` 13 MB, Python 3.13 about 21 MB, `apt` + `libapt-pkg7.0` 8.4 MB, `dpkg` 6.6 MB, `passwd` 4.8 MB. Removable without losing a listed tool: `apt`, `libapt-pkg7.0`, `dpkg`'s database and `passwd`, about 20 MB. Removable with a trade: `perl` (breaks `git add -i`, `git send-email`, `git svn`). Size affects host page cache and staging (1.98 s), not sandbox start. Verdict: a P3 successor to M221_001. Side finding: fifteen `linux-image-6.12.*` packages (about 1.6 GB) are installed on the dev host itself.
  - *Pending work discussed and not started* (Indy, Nov 03: "Are there any pending specs that we discussed but havent proceeded?"): `M236_001` goal-driven durable repair (P0, `pending/`); `M222_001` outbound proxy declared not inherited (P2, `pending/`); `M233_001` chat round-trip stage timing (`active/`, §0 and §5 IN_PROGRESS, §1–§4 not started); the M237 follow-up Indy chose on Nov 03 10:07 AM (per-step prepare timers, warm slots for default-sized leases, shorter lease-poll wait), no spec yet, its third item taken by this spec's §3; `M225_001` §4 dev pickup measurements, deferred to Indy and never taken; the token counting proposal `docs/reviews/tokenizer-approach-proposal.md`, awaiting Indy's sign-off and not yet committed; the workspace-setup L8 follow-up (scheduler callback destination and registration sweep), recorded in `docs/reviews/workspace-setup/decisions.md` on Oct 26 and not yet committed.
  - *Merged PR's red lanes* (Indy, Nov 03: "The previously merged PR had a failure in acceptance-cli or so, in your spec record all these"): PR #798 (`fde585971`, merged Nov 03, 2026 15:25 Coordinated Universal Time (UTC)) passed every required check. After merge, `deploy (dev)` run 41663777137 went red in `acceptance / acceptance-cli`: the live lane read 170 passed, 1 failed, the failure `CLI credential mint 502: <html>…502 Bad Gateway…cloudflare` thrown by `mintCliCredential` (`cli/test/acceptance/fixtures/workspace-hydration.ts:105`) from `tenant-provider-mutation.spec.ts:109`; the edge answered for a daemon still restarting; §6 fixes it. The deterministic lane read 209 passed. `smoke (post-deploy)` run 41663866589 went red in `acceptance-e2e-prod` with `getaddrinfo ENOTFOUND api.agentsfleet.net`, and the same job failed the same way on `2f8c878c0` on Nov 02 (run 41440294059): the production smoke's API host does not resolve from the GitHub runner. Out of scope here; recorded for the next operator pass.
  - *Why the heartbeat and not a route* (M225 Discovery, Indy, Oct 12, 2026: "New spec, wake hint (Recommended)"): a held request per worker was rejected. §3 holds one request per runner host, the heartbeat the runner already sends, counted and capped. The scope was put to Indy on Nov 03 as "Per-runner wake hold — one held request per runner host, not per worker (what you rejected Oct 12)" and chosen. The EGM zoom-out review (below) turned the first draft's new route into the held heartbeat.
  - *M233_001 overlap*: M233 §1 adds per-verb request-span attributes and §3 edge `Server-Timing`; this spec's §4 extends `fleet.delivery` and `DeliveryStage`. The two are complementary; M233's unstarted Sections rebase onto the keys declared here, and M233 §4's planned `002_round_trip_playbook.md` becomes a Section of `002_round_trip_percentiles.md` created here.
- **Model reviews** — Elephant-Goldfish Model (EGM) review, Claude Fable 5.1, Nov 03, 2026, six reviewers on the first draft: structural 16 findings, comprehension 33, zoom-out 16, style 38, implementation readiness 24 (3 blockers), design 24 (2 blockers). Accepted from the design review on top of the list below: a capture failure keeps today's "the report still settles" decision; the publish follows the two biller calls because `money_gates` reads the balance through the biller; a shape-refused one-post report falls back once (the rolling-deploy skew); the ring signal is a generation counter so no ring is lost between a hold's arrival and its await; the beat is held only with an idle worker, so the saturation path stands; the runner asks to be held only after a reply carried `woken`; §5's claim is scoped to the runner's own heap copies, with the spool's clear-text copy named as not covered; the TraceQL metrics query is paired with a search query that needs no metrics generator. Declined: dropping `deny_unknown_fields` on the report types (the pre-port runner refusal depends on it; the fallback covers skew instead). Accepted and applied in this revision from the other five: no new route (the heartbeat is held); the idle hint stays at 1 s; `DeliveryStage` extended instead of a second histogram; memory written through `Memories::capture` before the commit instead of inside the settle transaction; `event_created_at` read off the lease row; tool records left as they are; the 413 branch replaced by `admit`'s cut-and-count; §6 (coverage audit) folded into Discovery and §4; the report path corrected to `/v1/runners/me/reports`; the retry extracted from the revoke fixture; the zeroing test replaced by a `zeroize()` proof; every path in Files Changed resolved on disk; globs, compound actions, em dashes and `hydrate`-as-prose removed; acronyms expanded. Declined: splitting into four Workstreams and PRs (Indy's one-PR direction below); dropping §5 (Indy asked for it, scope kept minimal); deferring §4 to M233 (complementary, recorded above).
- **Open blockers** — none.
- **Deferrals and transfers** — Each one carries its owner quote:
  > Owner (Indy, Nov 03, 2026 10:07 AM): "All three (Recommended)" — context: the M237 follow-up (per-step timers, warm slots, shorter poll wait) is a separate spec; this spec's §3 takes the poll wait.
  > Owner (Indy, Nov 03, 2026 09:40 PM): "Also ensure the metrics are captured and send to grafana as well so i can have another agent fix the grafana dashboard later. essentially the P95 and P99 performance must be captured in grafana on a end to end fleet level usecase" — context: §4 lands the span attributes, the histogram stages and the playbook queries; the dashboard JSON is the other agent's.
  > Owner (Indy, Nov 03, 2026 09:20 PM): "Report split + hydrate-in-lease (Recommended), Per-runner wake hold" — context: scope choice; warm slots and measure-first not chosen.
  > Owner (Indy, Nov 03, 2026 09:55 PM): "Fold the spec into the implementation PR (one PR), Edit .github/workflows if the acceptance-cli fix lives there, Push the ~/Projects/docs changelog branch and open its PR after merge" — context: the spec lands in the implementation PR rather than its own; §6's fix is fixture-level, so no workflow edit is needed.
  > Owner (Indy, Nov 03, 2026 09:50 PM): "Indy is going to bed. Ensure you spec out, comment the implementation using /orly-build and then /orly-ship conduct reviews, gates pass push the PR. Ensure the CI gates are green, with greptile feedback resolved. Then merge the PR. Prior to the push fix the acceptance-cli to pass." — context: authorises the commit, push, PR and merge of this milestone in this session, with the six Sections in one PR.
  > Owner (Indy, Nov 03, 2026 10:30 PM): "When touching code analyze for a refactor to see if the crates can be made smaller, performant, better concurrent and avoid races." — context: each Section's commit records, in the PR Session Notes, the refactor it took (folded when it completes the Section) and the ones it left with their reason.
  > Owner (Indy, Nov 03, 2026 10:50 PM): "also avoid singular field assertion test cases, you need make sure the ts,tsx are 100%, rust rs is 99>% coverage on the patch diff" — context: every new test compares the whole value or outcome it checks; rubric rows S6 and S7 hold the coverage floors the CI lanes already enforce (`scripts/rustd_coverage.py --per-file-floor 99`, `cli/bunfig.toml`).
  > Owner (Indy, Nov 03, 2026 10:35 PM): "avoid duplicate code and handrolled code, but rather look for other places where you may have implemented the same or using any batle tests crates" — context: every mechanism here names the existing code it reuses (Prior-Art); the build searches the workspace before writing a helper and prefers a workspace crate already in `Cargo.lock` over new code.
