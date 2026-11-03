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

**Solution summary:** §1: the report body carries the run's memory deltas; the daemon captures them, fenced as the settle is, before the settle commits, so the runner's memory push goes. The completion publish keeps its place after the biller calls and now answers its instant. §2: the lease reply carries the fleet's memory window, read after the issue under the fleet's slot, so it holds the previous run's memory and the runner reads none. §3: a runner's heartbeat asks to be held; the daemon holds it until its doorbell rings or the heartbeat interval passes, and a woken runner polls at once. §4: `fleet.delivery` spans event creation to `event_complete` with one attribute per segment, so Grafana answers p95 and p99 per fleet from Tempo; the existing `DeliveryStage` histogram gains the same boundaries fleet-wide. §5: the runner's lease reply body and the memory deltas it captures zero on drop. §6: the acceptance lane's credential mint retries a Cloudflare edge error, which turned the Nov 03 deploy red.

**Design flow:**

```text
message ─► admission commits ─► doorbell rings ─► held heartbeat answers woken ─► one idle worker polls now
        ─► lease reply {policy, history, memory window} ─► first model call (no memory read)
        ─► answer streamed ─► ONE reports post {result, usage, memory deltas, segments}
        ─► daemon: memory captured (fenced) ─► settle commits ─► billing ─► event_complete ─► audit
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
| `rustd/crates/afd_wire/src/report/mod.rs` | EDIT | the report's optional memory deltas and `telemetry.segments` (`RunSegments`) |
| `rustd/crates/afd_wire/src/lease/payload.rs` | EDIT | the lease payload's optional memory window |
| `rustd/crates/afd_wire/src/runner/mod.rs` | EDIT | the heartbeat's `wake` and the reply's `woken` |
| `rustd/crates/afd_core/src/limits.rs` | EDIT | `HELD_BEATS_MAX` |
| `rustd/crates/afd_core/src/timing.rs` | EDIT | `WAKE_HOLD_MAX`, asserted at or below `HEARTBEAT_INTERVAL` |
| `rustd/crates/afd_dragonfly/src/doorbell/mod.rs` | EDIT | the ring generation (a `watch`), the held-beat count and its cap, `wait_for_ring` |
| `rustd/crates/afd_dragonfly/src/doorbell/tests.rs` | EDIT | ring, deadline, cap, buffered-fleet, close and contended-cap proofs |
| `rustd/crates/afd_dragonfly/src/streams/tail.rs` | EDIT | the tail publish answers whether the frame landed |
| `rustd/crates/afd_fleet/src/lease/bracket.rs` | EDIT | the completion publish answers whether it landed |
| `rustd/crates/agentsfleetd/src/serve/drain/mod.rs`, `rustd/crates/agentsfleetd/src/serve/drain/tests.rs`, `rustd/crates/agentsfleetd/src/serve/mod.rs` | EDIT | the drain closes the doorbell when it stops accepting; proof; serve wires it |
| `rustd/crates/afd_api/src/openapi/stability/mod.rs` | EDIT | `HeartbeatResponse.woken` and `LeasePayload.memory` publish as beta |
| `rustd/crates/afr_supervisor/src/client/tests/mod.rs` | EDIT | pins the reply body's zeroing buffer |
| `rustd/crates/afd_fleet/Cargo.toml` | EDIT | `futures-util` moves to dependencies, for the overlapped post-issue reads |
| `rustd/crates/afd_fleet/src/error/classify.rs` | EDIT | `is_fence_refusal`, so a refused capture logs nothing twice |
| `rustd/crates/afd_fleet/src/lease/mod.rs` | EDIT | re-exports `Waited` |
| `rustd/crates/afd_fleet/src/lease/memory.rs` | EDIT | the fenced capture split into `capture_for`, which the report verb reuses |
| `rustd/crates/afd_fleet/src/lease/store.rs` | EDIT | `Leases::wait_for_work` over the doorbell |
| `rustd/crates/afd_fleet/src/lease/settle.rs` | EDIT | the loaded lease carries its own and its event's creation instants |
| `rustd/crates/afd_fleet/src/lease/sql/select_lease_for_report.sql` | EDIT | selects the two instants |
| `rustd/crates/afd_fleet/src/lease/answer/mod.rs` | EDIT | the rendered lease carries a `Context`: history and memory window |
| `rustd/crates/afd_fleet/src/lease/answer/tests.rs` | EDIT | renders with an empty context |
| `rustd/crates/afd_fleet/src/lease/deliver/mod.rs` | EDIT | reads the thread and the window after the issue, overlapped |
| `rustd/crates/afd_fleet/src/lease/deliver/window.rs` | CREATE | the window read and its logged failure |
| `rustd/crates/afd_fleet/src/lease/report/mod.rs` | EDIT | captures memory before the commit; answers the completion instant |
| `rustd/crates/afd_fleet/src/lease/report/memory.rs` | CREATE | the report's capture, fenced on the lease's token, logged on failure |
| `rustd/crates/afd_fleet/src/lease/report/steps.rs` | EDIT | the publish's position pinned and logged; answers its instant |
| `rustd/crates/afd_fleet/src/lease/report/timing.rs` | CREATE | the three completion stages on the daemon's clock |
| `rustd/crates/afd_fleet/src/lease/report/timing_tests.rs` | CREATE | each stage recorded once |
| `rustd/crates/afd_fleet/src/lease/report/tests.rs` | EDIT | the loaded-lease fixture's instants |
| `rustd/crates/afd_http/src/services/leasing/mod.rs` | EDIT | `Leasing::wait_for_work` |
| `rustd/crates/afd_http/src/services/leasing/plane.rs` | EDIT | the production plane forwards it |
| `rustd/crates/afd_api_runner/src/handler/runner/heartbeat.rs` | EDIT | the hold after the beat's writes; `woken` on every reply |
| `rustd/crates/afd_api_runner/src/handler/runner/wake.rs` | CREATE | a held beat's answer, count and refusal log; the permit-share assertion |
| `rustd/crates/afd_api_runner/src/handler/runner/wake_tests.rs` | CREATE | answer and cap proofs |
| `rustd/crates/afd_api_runner/src/handler/runner/mod.rs` | EDIT | registers `wake` |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs` | EDIT | the span's start, end and segment attributes; two segment proofs |
| `rustd/crates/afd_api/tests/harness/stubs_runner.rs` | EDIT | the stub plane's hold and extended settle answer |
| `rustd/crates/afd_api/tests/integration_runner_heartbeat.rs` | EDIT | a held beat answers `woken` after its liveness |
| `rustd/crates/afd_observability/src/delivery/mod.rs` | EDIT | the span starts at event creation and carries capped `Segments` |
| `rustd/crates/afd_observability/src/delivery/tests.rs` | EDIT | start, fallback, milliseconds and cap proofs |
| `rustd/crates/afd_observability/src/lib.rs` | EDIT | exports `Segments` |
| `rustd/crates/afd_observability/src/semconv/mod.rs` | EDIT | the seven segment keys |
| `rustd/crates/afd_observability/src/metrics/label/fleet.rs` | EDIT | three `DeliveryStage` members; the `WakeAnswer` set |
| `rustd/crates/afd_observability/src/metrics/label/tests.rs` | EDIT | the new family's label product |
| `rustd/crates/afd_observability/src/metrics/declared/fleet.rs` | EDIT | `agentsfleet_runner_wake_answers_total` |
| `rustd/crates/afd_observability/src/producers/fleet/mod.rs` | EDIT | `wake_answered` |
| `rustd/crates/afd_observability/tests/producers_drive.rs` | EDIT | drives the new producer and every stage |
| `docs/metrics.census.tsv` | EDIT | the new family; the stage ceiling 4 → 7 |
| `public/openapi.json` | EDIT | regenerated for the three changed wire types |
| `rustd/crates/afr_supervisor/src/client/mod.rs` | EDIT | the reply body as `Zeroizing<Vec<u8>>` |
| `rustd/crates/afr_supervisor/src/client/http.rs` | EDIT | the held-beat versus call-timeout assertion |
| `rustd/crates/afr_supervisor/src/memory/mod.rs` | EDIT | `CapturedMemory`, zeroed on drop; the push from borrowed views |
| `rustd/crates/afr_supervisor/src/memory/tests.rs` | EDIT | zeroing proofs |
| `rustd/crates/afr_supervisor/src/report/mod.rs` | EDIT | the builder's new fields |
| `rustd/crates/afr_supervisor/src/lease_loop/mod.rs` | EDIT | the window seeds the run; the memory read only without one |
| `rustd/crates/afr_supervisor/src/lease_loop/settle.rs` | EDIT | the one-post settle and the shape-refusal fallback |
| `rustd/crates/afr_supervisor/src/lease_loop/segments.rs` | EDIT | the segments a report carries |
| `rustd/crates/afr_supervisor/src/lease_loop/lessee.rs` | EDIT | the lessee holds the pool's `Wake` |
| `rustd/crates/afr_supervisor/src/lease_loop/tests/settlement.rs` | EDIT | one post, older daemon and fallback proofs |
| `rustd/crates/afr_supervisor/src/lease_loop/tests/timing.rs` | EDIT | the memory-read segment with and without a window |
| `rustd/crates/afr_supervisor/src/report_spool/mod.rs` | EDIT | a rejected delivery carries its refusal; `reshaped`; `forget_rejected` |
| `rustd/crates/afr_supervisor/src/report_spool/tests.rs` | EDIT | the rejected variant's payload |
| `rustd/crates/afr_supervisor/src/drainer/mod.rs` | EDIT | the drain re-posts a shape-refused one-post body without its memory |
| `rustd/crates/afr_supervisor/src/drainer/tests.rs` | EDIT | reshape proofs |
| `rustd/crates/afr_supervisor/src/heartbeat/mod.rs` | EDIT | asks to be held, reads `woken`, paces the next beat |
| `rustd/crates/afr_supervisor/src/heartbeat/tests.rs` | EDIT | asking and pacing proofs |
| `rustd/crates/afr_supervisor/src/worker_pool/mod.rs` | EDIT | `Wake`: idle count and wake-one; idle pauses end on it |
| `rustd/crates/afr_supervisor/src/worker_pool/tests.rs` | EDIT | woken-worker and hint-fallback proofs |
| `rustd/crates/afr_supervisor/src/lib.rs` | EDIT | the beat gets the pool's wake |
| `rustd/crates/afr_supervisor/src/test_util.rs`, `src/test_util_tests.rs`, `src/test_support/rig.rs`, `src/lease_telemetry_tests/mod.rs`, `src/client/tests/routing.rs` | EDIT | the test daemon answers `woken`; rigs build the wake; literals carry the new fields |
| `rustd/crates/afd_runner/src/heartbeat/mod.rs` | EDIT | `NO_REPORT` carries `wake: false` |
| `rustd/crates/afd_runner/tests/integration_runner_plane/faults.rs` | EDIT | beat literal |
| `rustd/crates/afd_bench/src/lane/lease/drain/runner.rs`, `rustd/crates/afd_bench/src/lane/lease/seed/mod.rs` | EDIT | report and beat literals |
| `rustd/crates/afd_fleet/tests/integration_report_memory.rs` | CREATE | the report's memory: lands, superseded, oversize, failed capture |
| `rustd/crates/afd_fleet/tests/integration_lease_gates/chat.rs` | CREATE | the chat-lease harness, moved out of `history.rs` so the window tests reuse it |
| `rustd/crates/afd_fleet/tests/integration_lease_gates/window.rs` | CREATE | the window on the lease, under the slot, and a failed read |
| `rustd/crates/afd_fleet/tests/integration_lease_gates/history.rs` | EDIT | reads the shared harness |
| `rustd/crates/afd_fleet/tests/integration_lease_gates/mod.rs` | EDIT | registers `chat` and `window` |
| `rustd/crates/afd_fleet/tests/integration_memory_capture.rs`, `rustd/crates/afd_fleet/tests/support/fleet_report_seed.rs` | EDIT | `supersede` moves to the shared `Held` |
| `rustd/crates/afd_fleet/tests/fleet_suite.rs` | EDIT | registers the report-memory suite |
| `rustd/crates/afd_fleet/tests/{integration_held_sandbox/{mod,report}.rs,integration_lease_gates/money.rs,integration_report_owes_destination/mod.rs,integration_runner_beat/{mod,recovery}.rs,lease_renewal/fixture.rs,support/{fleet_lease_seed,view_heartbeat}.rs}` | EDIT | report and beat literals carry the new fields |
| `cli/test/acceptance/fixtures/retryable.ts` | CREATE | the status sets, backoff and retry loop shared by mint and revoke |
| `cli/test/acceptance/fixtures/cli-credential-revoke.ts`, `cli/test/acceptance/fixtures/cli-credential-revoke.test.ts`, `cli/test/acceptance/global-teardown.ts` | EDIT | read the shared module |
| `cli/test/acceptance/fixtures/workspace-hydration.ts`, `cli/test/acceptance/fixtures/workspace-hydration.test.ts` | EDIT | the mint retries; four whole-outcome proofs |
| `docs/architecture/lease_flow.md`, `docs/architecture/runner_fleet.md`, `docs/architecture/runner_execution.md`, `docs/architecture/scaling.md`, `docs/architecture/observability.md` | EDIT | the one-post settle, the window, the held heartbeat, the round-trip span and the chat-path signal table |
| `playbooks/operations/observability/002_round_trip_percentiles.md`, `playbooks/README.md` | CREATE, EDIT | the per-fleet and fleet-wide queries; inventory line |
| `VERSION`, `cli/package.json`, `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | 0.73.0 |
| `docs/v2/done/M238_001_P1_API_DOCS_INFRA_OBS_CHAT_ROUND_TRIP_DROPS_THREE_WAITS.md` | EDIT | this spec, moved through `active/` to `done/` |
| `~/Projects/docs/changelog.mdx` | EDIT | own branch `chore/m238-chat-round-trip-changelog`, after merge |

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

### §1 — A chat reply settles in one post, and its memory lands before the settle commits

`ReportRequest` gains `memory: Vec<MemoryDelta>`, default empty. The daemon's report verb loads the lease as this runner's, then captures the deltas through the push verb's own fenced capture (`Plane::capture_for`), fenced on the lease row's token as the settle is, then commits the settle. The capture runs in its own transaction under the memory role (`SET LOCAL ROLE memory_runtime`), so nothing crosses the role boundary, and it is boxed so its state does not ride every report future. It precedes the commit, so a lease issued after the commit reads it (§2). A lease the fleet has moved past writes neither memory nor settle. A capture that fails is logged under `memory_capture_failed` and the report still settles: withholding the answer would lose it too. A capture refused because the lease is no longer live (a repeated report, whose first post wrote the memory) is logged at debug as `memory_capture_skipped`. The capture reads the fence and writes in separate transactions, as the push verb it reuses always has, so a reclaim landing between the two leaves that run's memory written while its settle is refused: the window the runner's own push had on `fde585971`, not a new one. `admit` cuts deltas past `MAX_PUSH_BYTES` and counts them, as the push does; the report is never refused for its memory.

The completion publish stays where it was on `fde585971`, now pinned by a comment, and the tail's publish now answers whether the frame landed; only a frame that landed is logged as `completion_published` and timed. It sits after the two biller calls (`money_gates` reads the payer's balance through the biller, so an open hold could park the owner's follow-up), before `record_released`, `queue_owed_answer` and `run_tokens`. The settle answers the publish instant in `Reconciled.completed_at` for §4. The saving on the runner is the memory push: one round trip after the answer.

Skew is handled on three paths. `ReportRequest` denies unknown fields, so a runner sends `memory` and `telemetry.segments` only when the lease carried a window, which only a daemon at this release sends (§2). If a rolling deploy hands the report to an old replica, the settle answers the shape refusal (400 `UZ-REQ-001`) by pushing the memory and posting the report again without the new fields, once, removing the refused copy from the spool. If the first post was kept and the drain later meets an old replica, the drain re-posts the body without its memory and segments, so the answer is kept and the memory is logged as dropped (`report_memory_dropped`). Tool records and the renewal flush stay as they were: a chat posts neither.

- **Dimension 1.1** — DONE — Memory on a report is in the store when the report answers → Test `test_report_memory_lands_before_the_settle` (integration): a report with two deltas settles; the store holds both; the admission is settled; `completion_published` names the lease and `completed_at` is at or after the verb's instant.
- **Dimension 1.2** — DONE — A lease the fleet moved past writes no memory and settles nothing → Test `test_superseded_report_writes_no_memory` (integration): after `supersede`, the report answers `RUN_STALE_FENCING_TOKEN`, the store is empty and the admission unsettled. False pass rejected: fencing on the request's token would let this report settle while its capture was refused, which the first draft of this test caught.
- **Dimension 1.3** — DONE — Memory past the push's cap is cut, counted, and the report settles → Test `test_oversize_report_memory_is_cut_and_counted` (integration): twenty 16 KiB deltas → the first fifteen kept, `agentsfleet_memory_capture_truncated_total` up by five, the admission settled.
- **Dimension 1.4** — DONE — The settle publishes `event_complete` and answers the instant, inside the call → Test `test_report_memory_lands_before_the_settle` (integration): `completed_at` lies between wall-clock readings taken before and after the report call; `completion_published` names the fleet and the lease.
- **Dimension 1.5** — DONE — A capture that fails leaves the report settling → Test `test_failed_capture_still_settles` (integration): the memory store unreachable; the report settles; one `memory_capture_failed` line naming the lease.
- **Dimension 1.6** — DONE — The runner makes one `reports` post on a lease that carried a window → Test `test_settle_is_one_post_on_a_window_lease` (unit): no `Hydrate` or `Capture` call; the report carries the run's memory and a `settle_ms` segment.
- **Dimension 1.7** — DONE — On a lease without a window the runner reads memory, pushes it, and sends neither new field → Test `test_settle_without_a_window_posts_as_before` (unit): `Hydrate`, `Capture`, `Report`; no `memory` key, no segments.
- **Dimension 1.8** — DONE — A one-post report refused for its shape falls back once → Test `test_shape_refused_report_falls_back_once` (unit): `Report`, `Capture`, `Report`; the first body carries memory and segments, the last neither; no report file left; one `report_shape_fell_back` line.
- **Dimension 1.9** — DONE — The drain re-posts a shape-refused one-post body without its memory → Test `test_drain_reposts_a_shape_refused_report_without_its_memory` (unit): the last body has no `memory` and no `telemetry.segments`; nothing left in the spool; one `report_memory_dropped` line. `test_only_a_shape_refused_one_post_body_is_reshaped` proves any other refusal, a plain body, or unreadable bytes are not reshaped.

### §2 — The lease reply carries the fleet's memory window

`LeasePayload.memory: Option<MemoryHydrateResponse>` carries what `GET /v1/runners/me/memory/{fleet_id}` answers, read by `issue_ready` after `Leases::issue` returns, overlapped with the thread read (`future::join`), and bounded as the verb is. A daemon at this release always sends it, an empty window included. The ordering invariant rests on the fleet slot: `release_through` frees it inside the settle transaction, and the candidate query skips a held slot, so no lease issues for a fleet between a report's capture and its commit. A window read that fails is logged as `lease_window_read_failed` and the lease issues with `memory: None`. The runner seeds its run from the payload; it reads memory only when the lease carries no window.

- **Dimension 2.1** — DONE — A fresh fleet's lease carries an empty window, and the next carries what the report remembered → Test `test_lease_carries_the_memory_window` (integration).
- **Dimension 2.2** — DONE — A second message waits while the first holds the slot, and its lease carries the first's memory → Test `test_window_follows_the_slot` (integration).
- **Dimension 2.3** — DONE — A window read that fails leaves the lease issuable → Test `test_window_read_failure_issues_without_a_window` (integration): the store unreachable; the lease issues with no window; one log line.
- **Dimension 2.4** — DONE — No memory read on a window lease → Test `test_no_memory_read_on_a_window_lease` (unit): no `Hydrate`; `memory_ms` absent.
- **Dimension 2.5** — DONE — One timed memory read without a window → Test `test_memory_read_without_a_window` (unit): one `Hydrate`; `memory_ms` equals the read's 40 ms.

### §3 — A held heartbeat wakes one idle worker

`HeartbeatRequest` gains `wake: bool`; `HeartbeatResponse` gains `woken: Option<bool>`, `Some` on every reply from this release. The runner asks only after a reply carried `woken`, while the assignment is `Ok` with workers, and while a worker is idle (`Wake::any_idle`). The daemon records liveness, capability and holds, then waits through `Leasing::wait_for_work` up to `WAKE_HOLD_MAX` (= `HEARTBEAT_INTERVAL`, 10 s) unless `HELD_BEATS_MAX` (64) beats are held in this process. The cap's count and the ring signal live in the `Doorbell`: an `AtomicUsize` reserved with `try_update` and released by a guard on drop, and a `watch` generation `ring` moves before its hint dedup, so a ring between a hold's arrival and its await is kept. A ring answers `woken: true`; a deadline or the cap `false`, the cap counted and logged as `runner_wake_refused`. Cross-replica rings arrive through the existing hub subscription.

On the runner a woken reply calls `notify_one` on the pool's `Wake`, which stores a permit, so one idle worker polls at once and the others keep their stagger. The next beat follows after `MIN_POLL_PAUSE` (250 ms) if woken, after what is left of the interval if held, after the interval otherwise, so a burst of admissions cannot turn beats into a busy loop. `WAKE_HOLD_MAX < CALL_TIMEOUT` is asserted in `client/http.rs`. `NO_WORK_RETRY_AFTER` stays 1 s as the fallback for a lost ring.

A held beat never delays the runner's or the daemon's own transitions. On the runner, a holds-saturation release or leasing stopping while a beat is held drops that beat (its writes are done) and sends the next at once. On the daemon, the drain closes the doorbell the moment it stops accepting (`Drain::with_doorbell`), which ends every held wait as at its deadline and holds no more, so a deploy is not held open by a hold. The wait subscribes to the ring generation before it checks the closed flag, so a close between the two is never slept through.

- **Dimension 3.1** — DONE — A ring answers a held wait, and a ring is what wakes → Tests `a_ring_answers_a_held_wait` (unit, `afd_dragonfly`) and `test_held_beat_answers_by_how_its_wait_ended` (unit, `afd_api_runner`).
- **Dimension 3.2** — DONE — With no ring a wait ends at its deadline and gives its place back → Test `a_silent_wait_answers_at_its_deadline` (unit).
- **Dimension 3.3** — DONE — A wait past the cap is refused at once, counted and logged → Tests `a_wait_past_the_cap_is_refused_at_once`, `test_held_beats_are_capped` (the log carries the cap and the runner) and `test_held_beats_are_counted_by_how_they_ended` (one count per reason; a beat that did not ask is not counted).
- **Dimension 3.4** — DONE — A ring for a buffered fleet still wakes a wait → Test `a_ring_for_a_buffered_fleet_still_wakes_a_wait` (unit).
- **Dimension 3.5** — DONE — A beat that asks to be held is answered with `woken` through the live route, its liveness recorded → Test `test_a_beat_asking_to_be_held_answers_woken_after_its_liveness` (integration). The route's lease plane is the harness stub, which ends the hold at once; the order (writes, then hold) is the handler's statement order.
- **Dimension 3.6** — DONE — A woken beat polls exactly one idle worker at once → Test `test_woken_beat_polls_one_worker_at_once` (unit, paused clock): polls at 0, 250, 500, 750 and the woken one at 900 ms.
- **Dimension 3.7** — DONE — A woken beat wakes a worker, and the next beat is paced by how the last ended → Test `test_woken_beat_wakes_a_worker_and_paces_the_next` (unit): 250 ms, interval less the hold, the full interval.
- **Dimension 3.8** — DONE — With no wake a worker polls on the hint → Test `test_lost_ring_falls_back_to_the_hint` (unit): polls at 0, 1000 and 2000 ms.
- **Dimension 3.9** — DONE — The runner asks only when proved, idle and told to keep working → Test `test_wake_is_asked_only_when_proved_and_idle` (unit): absent, absent, `true`, absent (busy), absent (told to drain), `true`.
- **Dimension 3.10** — DONE — The drain ends held beats at once → Tests `a_held_heartbeat_ends_when_the_drain_stops_accepting` (unit, `agentsfleetd`) and `a_closed_doorbell_ends_and_refuses_held_waits` (unit, `afd_dragonfly`).
- **Dimension 3.11** — DONE — A saturation release or a leasing stop does not wait out a held beat → Tests `test_a_saturation_release_does_not_wait_out_a_held_beat` and `test_a_leasing_stop_does_not_wait_out_a_held_beat` (unit, paused clock).
- **Dimension 3.12** — DONE — The cap holds under contention → Test `the_cap_holds_under_contention` (unit, four worker threads): 100 waits, 64 held, 36 refused, all held ones rung, every place given back.

### §4 — Every segment of every chat is read in Grafana at p95 and p99, per fleet

`fleet.delivery` starts at the event's creation (`event_created_at`, selected with the lease for the report) and ends at the publish instant `Reconciled.completed_at` carries. It keeps `gen_ai.agent.id` and gains `agentsfleet.delivery.queue_wait_ms` and `complete_ms`, measured on the daemon, and `memory_ms`, `model_ms`, `tools_ms`, `sandbox_wait_ms` and `settle_ms` from the runner's `telemetry.segments`, every one capped at `MAX_RUN`. A start the span cannot place falls back to the end less `wall_ms`. Fleet-wide, `DeliveryStage` gains `event_to_complete`, `lease_to_report` and `report_to_complete`, recorded at the publish; the census ceiling rises to seven. The playbook page holds the per-fleet TraceQL queries, with a search query for a Tempo without a metrics generator. No fleet, workspace or tenant identity reaches a metric.

- **Dimension 4.1** — DONE — The span runs from event creation to the publish and carries both clocks' segments → Tests `the_delivery_span_starts_at_the_events_creation`, `an_unplaceable_creation_falls_back_to_the_reported_duration`, `segments_are_exported_in_milliseconds_and_absent_ones_omitted` (unit, `afd_observability`) and `test_span_joins_daemon_and_runner_segments` (unit, `afd_api_runner`).
- **Dimension 4.2** — DONE — A runner-supplied segment past `MAX_RUN` is capped → Test `an_implausible_segment_is_capped` (unit).
- **Dimension 4.3** — DONE — A published completion records each new stage once → Test `test_completion_records_three_stages` (unit): 900, 660 and 200 ms; nothing for a report that published nothing.
- **Dimension 4.4** — The per-fleet queries answer on dev → Test `test_round_trip_queries_answer_on_dev` (manual, after deploy): ten chats, the queries from `002_round_trip_percentiles.md` in Grafana Explore, the answering query and a screenshot path recorded here. Disposition in Discovery.

### §5 — The runner's lease reply body and captured memory deltas zero on drop

`Body` holds its bytes in a `zeroize::Zeroizing<Vec<u8>>`, taken from the transport with `Vec::from` so no second clear copy is left, and the lease payload and memory window borrow from it. The run's deltas travel in `CapturedMemory`, whose `Drop` overwrites every owned string in place and clears the entries, in the `zero_on_drop!` shape `afd_crypto` uses. The claim is scoped to the runner's own heap copies: the bytes handed to the HTTP client and the report spool's on-disk copy are not covered, and `runner_execution.md` says so.

- **Dimension 5.1** — DONE — The body's buffer and the deltas are zeroing types, and the deltas' debug form says nothing they hold → Tests `test_reply_body_is_a_zeroing_buffer` (reverting `Body.bytes` to `Bytes` stops it compiling) and `test_sensitive_buffers_zeroize` (unit).
- **Dimension 5.2** — DONE — Every owned string is zeroed in place, its allocation kept until the clear, and zeroizing drops the entries → Test `test_captured_memory_zeroizes_its_strings` (unit): each string stays owned, emptied, with its capacity; deleting the `zeroize` call fails it.

### §6 — The acceptance lane survives a Cloudflare edge error

`mintCliCredential` gains `options: { sleep, random }` and retries through `withRetries`, the loop extracted from the revoke fixture into `retryable.ts` with its backoff (doubling from 500 ms, jittered, `Retry-After` honoured up to 10 s) and its cap (five attempts). The mint is a `POST` that writes a credential, so it retries only `UNPROCESSED_STATUSES` (425, 429, 503), a 502 the edge answered (an HTML page, not the daemon's JSON problem document: `edgeAnswered`), and a connection that never opened. A 502 the daemon answered, a 500, a 504 or a timeout may follow a mint that landed and throws at once, which is the rule `cli/src/lib/http-retry.ts` holds for every POST. The revoke keeps the wider `RETRYABLE_STATUSES` set, because its `DELETE` is idempotent. The last failure throws the last status and detail. The Nov 03 deploy went red on one 502 from the Cloudflare edge while the dev daemon restarted (Discovery); the lane's 170 other live tests passed.

- **Dimension 6.1** — DONE — An edge 502 followed by a 200 mints → Test `mints after an edge 502` (unit, `workspace-hydration.test.ts`): a fetch stub answering 502 then 200 → the whole outcome `{minted, error: null, calls: 2, slept: [500]}`.
- **Dimension 6.2** — DONE — Five retryable answers throw the last status → Test `throws after five retryable answers` (unit): 503 every time → `{minted: null, error: "CLI credential mint 503: … (gave up after 5 attempts)", calls: 5, slept: [500, 1000, 2000, 4000]}`.
- **Dimension 6.3** — DONE — A rejection is not retried → Test `does not retry a rejection` (unit): 401 → one call, no sleep, the status in the error.
- **Dimension 6.4** — DONE — A gateway timeout is not retried, because it may follow a mint that landed → Test `does not retry a gateway timeout, which may follow a mint that landed` (unit): 504 → one call, no sleep.
- **Dimension 6.5** — DONE — A 502 the daemon answered and a timed-out attempt are not retried; a connection that never opened is, and a `Retry-After` is waited out → Tests `does not retry a 502 the daemon answered`, `does not retry a timeout, which may follow a mint that landed`, `retries a connection that never opened` and `waits out a Retry-After before retrying a shed mint` (unit, whole outcomes).

## Interfaces

- `POST /v1/runners/me/reports` body gains `memory: [MemoryDelta]` (default empty) and `telemetry.segments: RunSegments` (`memory_ms`, `model_ms`, `tools_ms`, `sandbox_wait_ms`, `settle_ms`, each optional), sent only when the lease carried `memory`.
- `POST /v1/runners/me/leases` reply: `lease.memory` is a `MemoryHydrateResponse`, on every lease from this release, absent when the window read failed.
- `POST /v1/runners/me/heartbeats` body gains `wake: bool` (default false); the reply gains `woken: bool`, present on every reply from this release.
- `Leasing::wait_for_work(deadline, max) -> Waited` (`afd_http`); `Doorbell::wait_for_ring` (`afd_dragonfly`); `Waited` ∈ `Rung`, `Deadline`, `Capped`.
- `Reconciled` gains `event_created_at`, `lease_created_at`, `completed_at`; `Reported` gains `lease_created_at`, `event_created_at`.
- `fleet.delivery` span: event creation to publish; attributes `agentsfleet.delivery.{queue_wait,memory,model,tools,sandbox_wait,settle,complete}_ms`.
- `DeliveryStage` gains `event_to_complete`, `lease_to_report`, `report_to_complete`. New family `agentsfleet_runner_wake_answers_total{reason}`, `reason` ∈ `ring`, `deadline`, `capped`.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Superseded lease reports | failure | a reclaim moved the fleet's sequence past the lease | capture refused by the fence; settle `stale_fence`; nothing written | `test_superseded_report_writes_no_memory` |
| Oversize memory on the report | capacity | deltas past `MAX_PUSH_BYTES` | cut and counted; the report settles | `test_oversize_report_memory_is_cut_and_counted` |
| Memory capture fails on the report | failure | the memory store refuses the write | logged; the report settles; the next run starts without those items | `test_failed_capture_still_settles` |
| Window read fails at issue | failure | the memory store refuses the read | the lease issues without a window; one log line; the runner reads memory | `test_window_read_failure_issues_without_a_window` |
| Older daemon, newer runner | failure | a lease without `memory` | the runner reads and pushes memory and sends no new field | `test_settle_without_a_window_posts_as_before` |
| Newer runner, older replica mid-deploy | failure | a one-post report meets a daemon that denies the new fields | settle: push memory, re-post once; drain: re-post without memory | `test_shape_refused_report_falls_back_once`, `test_drain_reposts_a_shape_refused_report_without_its_memory` |
| Held beats past the cap | capacity | more than 64 held beats in one process | answered at once, `woken: false`, counted, logged | `test_held_beats_are_capped` |
| Ring lost | failure | `Received::Lagged` or `Gap` on the hub | the held beat answers at the deadline; the 1 s hint leases | `test_lost_ring_falls_back_to_the_hint` |
| A held beat fails on the runner | failure | a stall past `CALL_TIMEOUT` or a refusal | the beat loop's existing backoff; the last assignment stands | `a_failed_beat_retries_and_a_stop_ends_leases_in_flight` (existing) |
| A burst of rings | capacity | many admissions while workers idle | one re-beat per ring at most every 250 ms per host | `test_woken_beat_wakes_a_worker_and_paces_the_next` |
| Runner-supplied duration out of range | security | a runner reports an absurd segment | capped at `MAX_RUN` on the span | `an_implausible_segment_is_capped` |
| Edge 502 at credential mint | failure | daemon restarting behind the edge | bounded retries; the last status thrown | `throws after five retryable answers` |
| Mint answered 504 or timed out | failure | the mint may have landed | thrown at once; no second credential minted | `does not retry a gateway timeout, which may follow a mint that landed` |

## Invariants

1. A lease issued after a report's commit carries that report's memory. The fleet slot is the authority: `release_through` frees it inside the settle transaction, the candidate query skips a held slot, and the capture precedes the commit in the report verb.
2. A report's capture is fenced on the same token as its settle, the lease row's. `capture_reported` passes `lease.fence`; Dimension 1.2 fails if a lease superseded before the report writes either. A reclaim between the capture's fence read and the settle's commit is the window the push verb always had (§1).
3. Fleet identity never reaches a metric. `DeliveryStage` and `WakeAnswer` are closed sets and the only labels; the census rows declare no other key.
4. `WAKE_HOLD_MAX ≤ HEARTBEAT_INTERVAL` and `WAKE_HOLD_MAX < CALL_TIMEOUT`, and `HELD_BEATS_MAX × 4 ≤ DEFAULT_MAX_IN_FLIGHT`. `const` assertions in `afd_core/src/timing.rs`, `afr_supervisor/src/client/http.rs` and `afd_api_runner/src/handler/runner/wake.rs`.
5. A held beat's place under the cap is always given back. The `Held` guard decrements on drop, a dropped request included; Dimensions 3.1 to 3.3 read `held()` back at zero.
6. The lease body and the captured deltas zero on drop. `Zeroizing<Vec<u8>>` and `CapturedMemory`'s `Drop`, pinned by Dimension 5.1.
7. A runner never loses a report to a field the daemon refuses. The new fields ride only on a window lease, the settle falls back once, and the drain reshapes; Dimensions 1.7 to 1.9. `wake` rides only after a reply carried `woken`; Dimension 3.9.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `fleet.delivery` (span, extended) | operator | a report settles | segment durations, `gen_ai.agent.id`, event and workspace ids | identifiers and durations only; no content | `the_delivery_span_starts_at_the_events_creation` |
| `agentsfleet_fleet_delivery_stage_seconds{stage}` (three new stages) | operator | `event_complete` publishes | `stage` from `DeliveryStage` | closed label set | `test_completion_records_three_stages` |
| `agentsfleet_runner_wake_answers_total{reason}` | operator | a held beat answers | `reason` ∈ `ring`, `deadline`, `capped` | closed label set | `producers_drive` |
| `runner_wake_refused` (log) | operator | a beat past the cap | `runner_id`, `cap` | no token | `test_held_beats_are_capped` |
| `lease_window_read_failed` (log) | operator | the window read fails at issue | `fleet_id`, `error_code` | no content | `test_window_read_failure_issues_without_a_window` |
| `memory_capture_failed` (log) | operator | a report's capture fails | `fleet_id`, `lease_id`, `error_code` | no content | `test_failed_capture_still_settles` |
| `completion_published` (log, debug) | operator | `event_complete` publishes | `fleet_id`, `lease_id` | none | `test_report_memory_lands_before_the_settle` |
| `report_shape_fell_back`, `report_memory_dropped` (runner logs) | operator | an older replica refused the one-post body | `lease_id` | no content | `test_shape_refused_report_falls_back_once`, `test_drain_reposts_a_shape_refused_report_without_its_memory` |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The report's memory lands before the settle, fenced and cut (§1) | `git grep -l test_superseded_report_writes_no_memory rustd/crates/afd_fleet/tests` | one path | P0 | |
| R2 | The lease carries the window under the slot (§2) | `git grep -l test_window_follows_the_slot rustd/crates/afd_fleet/tests` | one path | P0 | |
| R3 | A held wait answers a ring, its deadline and its cap (§3) | `git grep -l a_wait_past_the_cap_is_refused_at_once rustd/crates/afd_dragonfly/src` | one path | P0 | |
| R4 | A woken beat polls one worker at once (§3) | `git grep -l test_woken_beat_polls_one_worker_at_once rustd/crates/afr_supervisor/src` | one path | P0 | |
| R5 | The span covers event creation to the publish with capped segments (§4) | `git grep -l the_delivery_span_starts_at_the_events_creation rustd/crates/afd_observability/src` | one path | P1 | |
| R6 | The playbook holds the per-fleet queries (§4) | `grep -c quantile_over_time playbooks/operations/observability/002_round_trip_percentiles.md` | `2` | P1 | |
| R7 | The lease body and the deltas zero on drop (§5) | `git grep -l test_captured_memory_zeroizes_its_strings rustd/crates/afr_supervisor/src` | one path | P1 | |
| R8 | The credential mint retries an edge error (§6) | `git grep -l "throws after five retryable answers" cli/test/acceptance/fixtures` | one path | P1 | |
| R9 | The census carries the new family (§4) | `grep -c agentsfleet_runner_wake_answers_total docs/metrics.census.tsv` | `1` | P1 | |
| S1 | Declared lint green | `make lint-all` | exit 0 | P0 | |
| S2 | Declared unit lane green | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version files agree | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |
| S6 | Rust patch coverage at or above 99% per touched file | `make test-coverage-rustd-merge RUSTD_PATCH_BASE=HEAD^1` (the CI coverage lane) | exit 0 | P0 | |
| S7 | TypeScript coverage 100% | `make test-coverage-all` | exit 0 | P0 | |

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
- **Implementation notes** (Nov 04, 2026, on `feat/m238-chat-round-trip`):
  - *Built against the draft.* The capture is fenced on the lease row's token, not the request's: the settle authorizes on the row, and the first run of `test_superseded_report_writes_no_memory` showed a request-token fence refusing the memory while the report settled. The completion publish already sat after the biller calls and before the audit row on `fde585971`; the draft's "moves" was wrong, and the build pins and logs it instead. The drain-side reshape (Dimension 1.9) was added after the design review named the kept-then-drained gap. `RunSegments` nests under `telemetry.segments` so each report literal adds one field.
  - *Refactors taken, per Indy's 10:30 PM ask:* the push verb's fenced capture split into `capture_for`, the one write path both the push and the report use; the revoke fixture's retry loop extracted to `retryable.ts` and shared with the mint; the chat-lease integration harness moved to `integration_lease_gates/chat.rs` and shared by the history and window suites; `supersede` moved to the shared `Held`; the post-issue thread and window reads overlapped with `future::join`; the report verb's capture boxed so the report future stays under the large-future bound.
  - *Refactors left, and why:* splitting `afd_fleet` (its `report/` and `deliver/` modules sit near the file cap) is a milestone of its own; the report spool's clear-text copy needs a security spec; the ring wakes every held beat whatever its labels, bounded by host count and revisited above 50 hosts; authoring leases are never rung (as on `fde585971`), so authoring pickup stays on the 1 s hint.
  - *Races checked:* a ring between a hold's arrival and its await (the `watch` generation is taken at arrival); a ring for a fleet whose hint is buffered (the generation moves before the dedup); a held beat dropped mid-wait (the cap's guard releases on drop); a worker entering its pause just after a wake (`notify_one` stores a permit); a lease issued between a report's capture and its commit (the slot forbids it).
- **Ship reviews** (Nov 04, 2026): gstack `/review` (native, this session) and `orly-standards-reviewer` (Claude Opus 5.5, fresh context; 30 findings against 26 rules). Fixed: a held beat holding the drain open past Fly's 5 s kill, which would cut the shutdown telemetry flush (the drain now closes the doorbell); a close between the closed check and the subscribe (subscribe first); a holds-saturation release and a leasing stop waiting out a held beat (both now cut it short); the completion timed even when its publish was dropped (the tail publish now answers whether it landed); a lost-lease capture logged as a failure (now a debug skip); drain-side failures swallowed and the cause code missing on the fallback logs; the held-beat refusal logged at info on a per-host path (now debug); vacuous zeroing tests (now pin the body field and prove in-place zeroing); the wake counter and the truncation counter unasserted; single-field log assertions (now whole tuples with codes); the completion instant unbounded (now bounded by the call); the wake ask's drain branch untested; the new reply fields published as stable (now beta); a derived `Debug` printing captured memory (now redacted); misplaced and stale doc comments; mutable-default patterns. Declined, with reasons: the mint's 502 retry kept for edge-answered pages only, because no route lists CLI credentials so a replay cannot be made idempotent, and the residual lost-reply risk falls on the dev fixture account (recorded for Indy below); `retryable.ts` kept as the fixture's own loop, extracted from the revoke fixture rather than added, because `cli/src/lib/http-retry.ts` refuses every POST replay on a 5xx and is Effect-based; folding the fixtures into it is a later change; `reshaped` edits the JSON value by field name rather than decoding `ReportRequest`, because the drain must strip fields an older daemon refuses without the runner's own version of the type deciding what survives; `capture_reported` and `window_for` log and carry on like the sibling `step()`, because neither may refuse the report or the lease; the span's `queue_wait_ms` and `event_to_complete` count a reclaimed event's earlier attempt, which is the round trip its owner waited, where `runner_lease_issued`'s `queue_wait_ms` counts only fresh entries (documented in `observability.md`); files already over the 350-line cap on `fde585971` (`deliver/mod.rs`, the heartbeat and pool test files) are not split here.
- **For Indy** — the one residual risk this PR carries: an acceptance-lane mint that the daemon writes but whose reply the edge loses as a 502 is retried, leaving one unrevoked CLI credential on the dev fixture account. It needs a list-or-revoke-by-machine route to remove; no such route exists.
- **Open blockers** — none.
- **Deferrals and transfers** — Each one carries its owner quote:
  > Owner (Indy, Nov 03, 2026 10:07 AM): "All three (Recommended)" — context: the M237 follow-up (per-step timers, warm slots, shorter poll wait) is a separate spec; this spec's §3 takes the poll wait.
  > Owner (Indy, Nov 03, 2026 09:40 PM): "Also ensure the metrics are captured and send to grafana as well so i can have another agent fix the grafana dashboard later. essentially the P95 and P99 performance must be captured in grafana on a end to end fleet level usecase" — context: §4 lands the span attributes, the histogram stages and the playbook queries; the dashboard JSON is the other agent's.
  > Owner (Indy, Nov 03, 2026 09:20 PM): "Report split + hydrate-in-lease (Recommended), Per-runner wake hold" — context: scope choice; warm slots and measure-first not chosen.
  > Owner (Indy, Nov 03, 2026 09:55 PM): "Fold the spec into the implementation PR (one PR), Edit .github/workflows if the acceptance-cli fix lives there, Push the ~/Projects/docs changelog branch and open its PR after merge" — context: the spec lands in the implementation PR rather than its own; §6's fix is fixture-level, so no workflow edit is needed.
  > Owner (Indy, Nov 03, 2026 09:50 PM): "Indy is going to bed. Ensure you spec out, comment the implementation using /orly-build and then /orly-ship conduct reviews, gates pass push the PR. Ensure the CI gates are green, with greptile feedback resolved. Then merge the PR. Prior to the push fix the acceptance-cli to pass." — context: authorises the commit, push, PR and merge of this milestone in this session, with the six Sections in one PR.
  > Owner (Indy, Nov 03, 2026 10:30 PM): "When touching code analyze for a refactor to see if the crates can be made smaller, performant, better concurrent and avoid races." — context: each Section's commit records, in the PR Session Notes, the refactor it took (folded when it completes the Section) and the ones it left with their reason.
  > Owner (Indy, Nov 03, 2026 09:40 PM): "so i can have another agent fix the grafana dashboard later" — context: Dimension 4.4, the Grafana check of the per-fleet queries, runs after deploy with the dashboard work; this PR lands the span, the stages and the queries.
  > Owner (Indy, Nov 03, 2026 10:50 PM): "also avoid singular field assertion test cases, you need make sure the ts,tsx are 100%, rust rs is 99>% coverage on the patch diff" — context: every new test compares the whole value or outcome it checks; rubric rows S6 and S7 hold the coverage floors the CI lanes already enforce (`scripts/rustd_coverage.py --per-file-floor 99`, `cli/bunfig.toml`).
  > Owner (Indy, Nov 03, 2026 10:35 PM): "avoid duplicate code and handrolled code, but rather look for other places where you may have implemented the same or using any batle tests crates" — context: every mechanism here names the existing code it reuses (Prior-Art); the build searches the workspace before writing a helper and prefers a workspace crate already in `Cargo.lock` over new code.
