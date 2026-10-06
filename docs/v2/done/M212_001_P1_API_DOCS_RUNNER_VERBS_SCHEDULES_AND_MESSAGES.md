<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M212_001: Two runner verbs let a fleet schedule its own follow-ups on the daemon's schedule plane and post a message to its thread mid-run, and the harness carries `cron_*`, `schedule` and `message` onto them

**Prototype:** v2.0.0
**Milestone:** M212
**Workstream:** 001
**Date:** Oct 02, 2026
**Status:** DONE
**Priority:** P1 — the tools the published page lists that the Zig runner refuses; without them a fleet cannot plan a follow-up or speak before it finishes
**Categories:** API, DOCS
**Batch:** B1 — the daemon half depends on nothing and can start beside M210_002; the runner half plugs into M210_002's catalog. One Pull Request
**Branch:** `feat/m212-001-runner-schedules-messages`
**Baseline revision:** `c5f7680f2ee4a475a9f6f6c8701c98262c6d5c8d`
**Test Baseline:** unit=3776 integration=4442 — unit 3776 passed, 0 failed (daemon libraries 3033 · runner 613 · daemon 130); integration and coverage 4442 passed, 0 failed (substrate 3844 + 2 exclusive · runner crates 410 · runner against the daemon 2 · daemon 184); Rust line coverage 98.8644% (52670 of 53275)
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M212_001-c5f7680f2.md`
**Depends on:** M210_002 (the catalog and router the three handlers plug into) — the daemon half (§1, §2) has no dependency and tests through `rustd/crates/agentsfleetd/tests/support/e2e_wire.rs`
**Provenance:** LLM-drafted (Claude Fable 5.1, Oct 02, 2026) from a source trace on `main` (`rustd/crates/afd_cron`, `rustd/crates/afd_outbound`) and Indy's decision "Yes, runner verb onto daemon schedules"
**Canonical architecture:** `docs/architecture/runner_execution.md` §"Tool catalog"; `docs/architecture/capabilities.md` §"2. The platform tools the fleet can call"; `docs/architecture/data_flow.md` §B. TRIGGER (QStash owns the clock)

---

## Overview

**Goal (testable):** `test_fleet_creates_its_own_schedule` — a runner post of `{ fencing_token, cron: "0 9 * * 1", timezone: "Asia/Kolkata", message: "weekly check" }` to the schedules verb stores a `core.fleet_schedules` row with source `fleet`, reconciles it to QStash (the fake records one upsert), and the lease's next `cron_list` returns it; a stale fence stores nothing, the seventeenth schedule is refused with `SCHEDULE_CAP_REACHED`, and a fleet cannot change a schedule a person created. `test_fleet_posts_a_message_mid_run` — a runner post to the messages verb reaches the event's Slack thread through the outbound poster before the run ends.
**Problem:** `cron_*` and `schedule` fail a lease on the Zig runner by design (`src/runner/engine/tool_bridge.zig:129`), because the runner owns no timer; the daemon's schedule plane (`rustd/crates/afd_cron`, `schema/520_fleet_schedules.sql`, the tenant routes in `rustd/crates/afd_http/src/route/fleet.rs:30-34`) exists but has no runner-facing verb. `message` is wired to NullClaw's channel tool, which has no channel credential inside a sandbox, so it cannot reach the thread; a fleet speaks only through its report. Indy wants every published tool real on the Rust runner, and chose a runner verb onto the daemon's schedules over a runner-local clock.
**Solution summary:** Two fenced runner verbs. `…/leases/{lease_id}/schedules` creates, lists, updates, deletes and runs-now schedules whose `fleet_id` is derived from the lease server-side, with a new source `fleet` beside `api` and `trigger`, a per-fleet cap, `afd_cron::validate` for the three fields, and the existing reconcile to QStash. `…/leases/{lease_id}/messages` hands a bounded, scrubbed text to the event's origin channel through `afd_outbound::poster::dispatch` as an interim post in the same thread. `afr_tools` maps `cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs`, `schedule` and `message` onto them.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(api): runner verbs for schedules and messages; cron_* and message become real tools
- **Intent (one sentence):** A fleet can plan its own follow-up and say something to the thread before it finishes, with the daemon still owning every timer and every channel credential.
- **Handshake** (PLAN, Oct 05, 2026): a running fleet asks `agentsfleetd`, through its own lease, to keep a schedule or to say one line in the thread it was asked from; the daemon stores the schedule and lets QStash keep time, and the daemon holds the Slack token and posts the line. The runner gains no clock and no channel credential. Matches the Intent.
- **ASSUMPTIONS I'M MAKING** (each read from source on `main` at `c5f7680f2`; recorded in Discovery):
  1. A `GET` and a `DELETE` carry `fencing_token` as a query parameter, never a body; `POST` and `PATCH` carry it in the body.
  2. A schedule fire records actor `cron:<schedule_id>`, replacing `schedule:qstash` (`rustd/crates/afd_cron/src/fire.rs:37`). The app already filters cron runs by `cron:*` (`ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/last-delivery.ts:8`), so the old spelling matched nothing there, and `runs` reads by this actor.
  3. `once` needs a column: nothing on the row survives a fire to say "delete me", and `source_key` is replaced by QStash's id at the first sync. Slot `928` adds it; `VERSION` is `0.56.0`, so this is a forward migration.
  4. The per-run message count lives on the lease row (slot `929`), so the fence, the count and the cap are one guarded `UPDATE`.
  5. The daemon masks with the runner's `afr_secrets::Scrub`, the one masker `docs/architecture/runner_execution.md` §Credentials names, over the fleet's declared credentials from the vault. A second masker would drift on the `«secret:NAME»` spelling.
  6. An interim post carries its own marker part, so a repeat of the final answer cannot mistake an interim line for the answer (`afd_connector::slack::answered`).
  7. The two new schedule codes sit beside their siblings in `error_code/request.rs`; the message codes sit in `fleet.rs`. A fourth code, `MESSAGE_LIMIT_REACHED`, names the per-run cap, which the spec's three codes left unnamed.

## Implementing agent — read these first

1. `rustd/crates/afd_cron/src/model.rs`, `rustd/crates/afd_cron/src/store.rs`, `rustd/crates/afd_cron/src/validate.rs`, `rustd/crates/afd_cron/src/service.rs` — `Source`, the store's `create`/`list`/`one`, the three validators and their caps, `reconcile`.
2. `rustd/crates/afd_api_tenant/src/handler/schedule/write.rs` — the tenant `create`, `patch`, `purge` and `sync` this verb mirrors, one fleet instead of one workspace.
3. `rustd/crates/afd_api_runner/src/handler/runner/memory.rs` — a fenced runner verb end to end; both verbs copy its fence check.
4. `rustd/crates/afd_outbound/src/poster.rs` — `dispatch` and `deliver_with_retry`: how a settled reply reaches a channel today.
5. `docs/REST_API_DESIGN_GUIDELINES.md` — §1 naming, §5 registry codes, §7 the four registration steps and the silent `ALL`-entry trap, §9 `x-stability`.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `schema/928_fleet_schedules_once.sql`, `schema/929_runner_leases_messages_posted.sql`, `schema/930_fleet_events_actor_index.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | `once` on a schedule; the per-run message count on a lease; one actor's events read by index |
| `rustd/crates/afd_wire/src/schedule_verb.rs`, `rustd/crates/afd_wire/src/message_verb.rs`, `rustd/crates/afd_wire/src/schedule.rs`, `rustd/crates/afd_wire/src/paths.rs`, `rustd/crates/afd_wire/src/lib.rs` | CREATE / EDIT | Request and response types, bounds, the path segments; a schedule view names its source and `once` |
| `rustd/crates/afd_http/src/route/runner.rs`, `rustd/crates/afd_http/src/services/*.rs`, `rustd/crates/afd_http/src/handler/mod.rs`, `rustd/crates/afd_http/src/handler/schedule.rs`, `rustd/crates/afd_http/src/handler/schedule/tests.rs`, `rustd/crates/afd_http/src/openapi/*.rs`, `rustd/crates/afd_http/Cargo.toml` | EDIT | Variants, `ALL` entries and `meta()`; the lease seam's two verbs, its mask and the event reads; one schedule renderer for both surfaces; a once schedule retires after it fires |
| `rustd/crates/afd_api_runner/src/handler/runner/schedule*.rs`, `rustd/crates/afd_api_runner/src/handler/runner/message.rs`, `rustd/crates/afd_api_runner/src/handler/runner/mod.rs`, `rustd/crates/afd_api_runner/src/lib.rs`, `rustd/crates/afd_api_runner/src/openapi.rs`, `rustd/crates/afd_api_runner/Cargo.toml` | CREATE / EDIT | Fence, narrow, cap, store, reconcile; edit; run-now gates and runs; fence, scrub, post under the deadline |
| `rustd/crates/afd_api_tenant/src/handler/schedule.rs`, `rustd/crates/afd_api_tenant/src/handler/schedule/*.rs`, `rustd/crates/afd_api_ingress/src/handler/webhook/qstash_route.rs`, `rustd/crates/afd_api/src/**`, `rustd/crates/afd_api/Cargo.toml` | EDIT | The tenant routes share the schedule renderer; the QStash route retires a `once` fire it drops; `x-stability` on the new fields |
| `rustd/crates/afd_cron/src/**`, `rustd/crates/afd_cron/Cargo.toml` | EDIT | `Source::Fleet`, `once`, the fleet-authored cap, admission and refusal reasons, retirement, the `cron:<schedule_id>` actor |
| `rustd/crates/afd_events/src/history/**` | CREATE / EDIT | Exact-actor statements and `page_of_actor`, so a schedule's runs read through slot 930 |
| `rustd/crates/afd_fleet/src/lease/**`, `rustd/crates/afd_fleet/src/error/*.rs`, `rustd/crates/afd_fleet/Cargo.toml` | CREATE / EDIT | The proved lease a verb names; the interim message's fence, count, destination, scrub and post; the installed secrets the mask reads; the message codes' problems |
| `rustd/crates/afd_connector/src/slack.rs`, `rustd/crates/afd_connector/src/slack/answered.rs`, `rustd/crates/afd_connector/src/test_util.rs` | EDIT | An interim stamp and a lease-scoped marker part, so an interim line is never read as the answer |
| `rustd/crates/afd_outbound/src/interim.rs`, `rustd/crates/afd_outbound/src/slack.rs`, `rustd/crates/afd_outbound/src/slack/*.rs`, `rustd/crates/afd_outbound/src/lib.rs` | CREATE / EDIT | An interim post into the event's origin thread, through the existing poster and retry |
| `rustd/crates/afd_core/src/error_code/fleet.rs`, `rustd/crates/afd_core/src/error_code/request.rs`, `rustd/crates/afd_core/src/error_code.rs`, `rustd/crates/afd_core/src/problem/*.rs`, `rustd/crates/afd_core/src/id.rs` | EDIT | `SCHEDULE_CAP_REACHED`, `SCHEDULE_NOT_FLEET_OWNED`, `SCHEDULE_NOT_RUNNABLE`, `MESSAGE_NO_CHANNEL`, `MESSAGE_LIMIT_REACHED` and their problems |
| `rustd/crates/afr_secrets/src/statics.rs` | EDIT | A view over a declared map, so the daemon masks with the one masker |
| `rustd/crates/agentsfleetd/src/plane.rs`, `rustd/crates/agentsfleetd/src/plane/*.rs`, `rustd/crates/agentsfleetd/src/outbound.rs`, `rustd/crates/agentsfleetd/src/preflight.rs`, `rustd/crates/agentsfleetd/src/preflight/*.rs`, `rustd/crates/agentsfleetd/src/serve/runtime.rs`, `rustd/crates/agentsfleetd/Cargo.toml` | EDIT | One Slack poster for the worker and the interim post; its base address knob, refused unless https or loopback http |
| `public/openapi.json` | EDIT | Regenerated; new fields declare `x-stability` |
| `rustd/crates/afr_tools/src/verbs.rs`, `rustd/crates/afr_tools/src/verbs/**`, `rustd/crates/afr_tools/src/lease.rs`, `rustd/crates/afr_tools/src/runtime.rs`, `rustd/crates/afr_tools/src/catalog.rs`, `rustd/crates/afr_tools/src/lib.rs`, `rustd/crates/afr_tools/src/testing.rs`, `rustd/crates/afr_tools/src/memory/shared_tests.rs`, `rustd/crates/afr_tools/Cargo.toml` | CREATE / EDIT | The lease-verb seam and the eight handlers onto it, masking what they send |
| `rustd/crates/afr_supervisor/src/**`, `rustd/crates/afr_agent/src/**`, `rustd/Cargo.lock` | CREATE / EDIT | The seam over the daemon's HTTP verbs, with `PATCH` and `DELETE`; handed to each run |
| `rustd/crates/agentsfleetd/tests/support/*.rs`, `rustd/crates/agentsfleetd/tests/integration_runner_schedules.rs`, `rustd/crates/agentsfleetd/tests/integration_runner_messages.rs`, `rustd/crates/*/tests/**`, `rustd/crates/**/tests.rs` | EDIT / CREATE | Runner-shaped posts, fake QStash and Slack, live-datastore proofs (`#[ignore]`d, run by `make test-integration-rustd`); unit proofs beside each change |
| `scripts/check_architecture_doc.sh`, `scripts/check_architecture_doc_citations.sh`, `scripts/check_architecture_doc_test.sh`, `scripts/check_architecture_doc_test_citations.sh` | EDIT / CREATE | The schedule-ownership check bans the stale sentences, not `cron_add` near a schedule (Indy-approved); split at the length cap |
| `docs/v2/*/M212_001_P1_API_DOCS_RUNNER_VERBS_SCHEDULES_AND_MESSAGES.md` | MOVE | This spec, `pending/` → `active/` → `done/` |
| `VERSION`, `build.zig.zon`, `cli/package.json`, `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | 0.56.0 → 0.57.0 at close |
| `docs/architecture/capabilities.md`, `docs/architecture/runner_execution.md`, `docs/architecture/data_flow.md` | EDIT | The fleet as a third schedule author; the interim post; the fire actor |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS + TFX (the cap, the message bound and the per-run message count are constants the tests and the runner import), NTP (both bodies narrowed at the verb), STS (the `source` column holds a word; `fleet` is a named constant, never a `CHECK`), ERR (the three codes are declared and referenced), OBS, ORP, TST-NAM, TCF, ECL (a QStash reconcile failure stores the row and reports `sync_status`, as the tenant route does).
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md`; `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` (§1, §5, §7, §9); `docs/LOGGING_STANDARD.md` — never log a message body or a schedule message.
- `docs/architecture/data_flow.md` §B. TRIGGER — QStash owns the clock; nothing here sleeps or ticks.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ERROR REGISTRY | yes | Five codes declared in `afd_core` and used by the verbs |
| SCHEMA GUARD | yes | `VERSION=0.56.0` (migrate): `migration:schema/928_fleet_schedules_once.sql`, `migration:schema/929_runner_leases_messages_posted.sql`, `migration:schema/930_fleet_events_actor_index.sql`. `source` stays `TEXT` with no `CHECK`; `fleet` is an application constant |
| UFS / LOGGING / MILESTONE-ID | yes | Constants once; scoped events with ids and counts only |
| Architecture consult | yes | `capabilities.md` §2 and `runner_execution.md` §"Tool catalog" already state these facts |
| File & Function Length (≤350/≤50/≤70) | yes | One verb per handler file; the interim poster in its own module |

## Prior-Art / Reference Implementations

- **Reference:** the memory push (`rustd/crates/afd_api_runner/src/handler/runner/memory.rs` → `rustd/crates/afd_fleet/src/lease/memory.rs`) — a fenced runner write whose fleet is derived from the lease, never from the body. Both verbs copy its shape.
- **Reference:** the tenant schedule routes (`rustd/crates/afd_api_tenant/src/handler/schedule/write.rs`) — create, patch, purge and sync over `afd_cron`; the runner verb is the same store with the fleet fixed and the source `fleet`.
- **Reference:** `rustd/crates/afd_outbound/src/poster.rs` — `dispatch` delivers a settled reply to the event's channel; the interim post reuses its posters and retry with a different body.

## Sections (implementation slices)

### §1 — A fleet owns schedules on the daemon's plane

`POST`, `GET`, `PATCH`, `DELETE` on `/v1/runners/me/leases/{lease_id}/schedules[/{schedule_id}]` and `POST …/schedules/{schedule_id}/runs` (run now, modelled as creating a run, since the REST guide bans a verb in a path), each carrying `fencing_token`. The fleet is the lease's; the body never names one. A create stores source `fleet`, keyed by its own minted schedule id, validates `cron`, `timezone` and `message` through `afd_cron::validate`, refuses the `FLEET_SCHEDULES_MAX`+1th schedule with `SCHEDULE_CAP_REACHED`, and reconciles to QStash as the tenant create does. List returns every schedule of the fleet with its source; update and delete refuse a schedule whose source is not `fleet` with `SCHEDULE_NOT_FLEET_OWNED`. Run-now admits one `schedule_fire` event for that schedule through the same producer QStash's callback uses, keyed `run:<event_id>` so a reclaimed lease replays the run. It applies the callback's gates first: a fleet that takes no work answers `UZ-AGT-012`, and a paused or deleting schedule, or a run a schedule started, answers `SCHEDULE_NOT_RUNNABLE`; each is a 409 naming `current_state`, so no schedule wakes its fleet in a loop. `GET …/schedules/{schedule_id}/runs` lists the fleet's events with actor `cron:<schedule_id>`, newest first, paged, through the slot-930 index.

- **Dimension 1.1** — A valid create stores a `fleet`-sourced row and reconciles once → Test `test_fleet_creates_its_own_schedule` → **DONE**
- **Dimension 1.2** — A stale fence is refused and stores nothing → Test `test_stale_fence_schedule_refused` → **DONE**
- **Dimension 1.3** — The cap refuses the next create with its code → Test `test_schedule_cap_refuses_with_code` → **DONE**
- **Dimension 1.4** — An `api`- or `trigger`-sourced schedule cannot be updated or deleted by the fleet → Test `test_fleet_cannot_touch_human_schedules` → **DONE**
- **Dimension 1.5** — An invalid cron, timezone or message is refused with the validator's reason → Test `test_schedule_fields_validated` → **DONE**
- **Dimension 1.6** — Run-now admits exactly one event with actor `cron:<schedule_id>` → Test `test_schedule_run_now_admits_one_event` → **DONE**
- **Dimension 1.7** — Runs lists that event, newest first → Test `test_schedule_runs_lists_events` → **DONE**
- **Dimension 1.8** — Delete sets the desired status and reconciles; the row leaves QStash → Test `test_fleet_deletes_its_schedule` → **DONE**
- **Dimension 1.9** — A `once` schedule retires after its first fire, from QStash or run-now → Test `test_once_schedule_retires_after_fire` → **DONE**
- **Dimension 1.10** — Run-now of a paused or deleting schedule is refused with `SCHEDULE_NOT_RUNNABLE` and its state → Test `test_run_now_refuses_a_schedule_that_would_not_fire` → **DONE**
- **Dimension 1.11** — Run-now for a fleet that takes no work is refused with `UZ-AGT-012` → Test `test_run_now_refuses_a_fleet_that_takes_no_work` → **DONE**
- **Dimension 1.12** — A run a schedule started cannot run a schedule now → Test `test_run_now_from_a_scheduled_run_is_refused` → **DONE**
- **Dimension 1.13** — One schedule's runs read through the actor index, never the fleet's whole history → Test `test_event_list_plans_use_the_index` → **DONE**
- **Dimension 1.14** — A run a schedule starts is leased, its body `{"message": …}` as every producer stores → Test `test_scheduled_run_is_leased_with_its_message` → **DONE**

### §2 — A fleet posts to its thread mid-run

`POST /v1/runners/me/leases/{lease_id}/messages { fencing_token, text }` delivers `text` to the event's origin channel in the same thread through the outbound posters, with retry. `text` is capped at `MESSAGE_MAX_BYTES` and scrubbed of the lease's secret values; a run may post at most `MESSAGES_PER_RUN_MAX`, counted in the same statement that proves the fence. Delivery stops at `MESSAGE_DELIVERY_DEADLINE` (12 s), inside the runner's call timeout, and answers `delivered: false`. An interim line is stamped `agentsfleet_interim` with a `{lease_id, line}` part, so neither a repeat of the answer nor a reclaimed lease's line 1 matches it. An event with no origin channel (an API steer, a webhook) is refused with `MESSAGE_NO_CHANNEL`, which the tool returns to the model.

- **Dimension 2.1** — A message reaches the Slack thread fake before the run ends → Test `test_fleet_posts_a_message_mid_run` → **DONE**
- **Dimension 2.2** — A stale fence posts nothing → Test `test_stale_fence_message_refused` → **DONE**
- **Dimension 2.3** — The per-run count and the byte cap refuse with their codes → Test `test_message_caps_refuse` → **DONE**
- **Dimension 2.4** — An event without a channel is refused with `MESSAGE_NO_CHANNEL` → Test `test_message_without_channel_refused` → **DONE**
- **Dimension 2.5** — A secret value in the text is masked before delivery → Test `test_message_is_scrubbed` → **DONE**
- **Dimension 2.6** — An interim line carries its own marker, so a repeat of the final answer still posts it → Test `test_interim_marker_is_not_the_answer` → **DONE**
- **Dimension 2.7** — A reclaimed lease's interim line never matches the dead lease's line → Test `test_reclaimed_lease_line_is_not_the_dead_lease_line` → **DONE**
- **Dimension 2.8** — Posts racing for the last slots never pass the cap → Test `test_concurrent_messages_never_pass_the_cap` → **DONE**
- **Dimension 2.9** — A stalled Slack answers `delivered: false` at the deadline → Test `test_message_to_a_stalled_slack_answers_at_the_deadline` → **DONE**
- **Dimension 2.10** — A line naming `<!channel>` reaches the thread as literal text and notifies nobody → Test `test_a_line_naming_the_channel_notifies_nobody` → **DONE**

### §3 — The harness carries the eight tools

`cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs` and `schedule` (NullClaw's one-shot: a cron that deletes itself after firing, expressed as a create with `once: true`) map onto §1; `message` maps onto §2. Each is a `Supervisor` handler in the catalog; a refusal's code reaches the model as the tool's error.

- **Dimension 3.1** — Each cron tool issues its verb with the lease's fence and returns the daemon's answer → Test `test_cron_tools_map_onto_the_verb` → **DONE**
- **Dimension 3.2** — `schedule` creates a once schedule that is deleted after its first fire → Test `test_schedule_tool_is_once` → **DONE**
- **Dimension 3.3** — `message` posts and returns the refusal code when there is no channel → Test `test_message_tool_posts_or_names_gap` → **DONE**

## Interfaces

```
POST   /v1/runners/me/leases/{lease_id}/schedules                 { fencing_token, cron, timezone?, message, once? } → schedule view
GET    /v1/runners/me/leases/{lease_id}/schedules?fencing_token=N  → { schedules: [{ schedule_id, source, once, cron, timezone, message, status, sync, … }] }
PATCH  /v1/runners/me/leases/{lease_id}/schedules/{id}            { fencing_token, cron?, timezone?, message?, paused? } → schedule view
DELETE /v1/runners/me/leases/{lease_id}/schedules/{id}?fencing_token=N  → 204, or the view while QStash has not agreed
POST   /v1/runners/me/leases/{lease_id}/schedules/{id}/runs       { fencing_token } → { event_id }  (run now: creates a run)
GET    /v1/runners/me/leases/{lease_id}/schedules/{id}/runs?fencing_token=N&limit=&starting_after=  → events with actor cron:<id>, newest first
POST   /v1/runners/me/leases/{lease_id}/messages                  { fencing_token, text } → { delivered: bool }

core.fleet_schedules.source ∈ { api, trigger, fleet }; core.fleet_schedules.once (slot 928)
source_key for a fleet create = its minted schedule_id until QStash's id replaces it
run-now idempotency key = run:<leased event_id>
interim marker = event_type agentsfleet_interim, part { lease_id, line }
fleet.runner_leases.messages_posted (slot 929)
A schedule fire's actor = cron:<schedule_id> (QStash and run-now alike)
Constants: FLEET_SCHEDULES_MAX 16 · MESSAGE_MAX_BYTES 4096 · MESSAGES_PER_RUN_MAX 8 · MESSAGE_DELIVERY_DEADLINE 12 s
Codes: SCHEDULE_CAP_REACHED (UZ-SCHED-009, 409) · SCHEDULE_NOT_FLEET_OWNED (UZ-SCHED-010, 403) · SCHEDULE_NOT_RUNNABLE (UZ-SCHED-011, 409) · MESSAGE_NO_CHANNEL (UZ-RUN-019, 409) · MESSAGE_LIMIT_REACHED (UZ-RUN-020, 409)
Every 409 body names current_state
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Stale fence | Reclaimed lease posts late | Refused, nothing stored or sent (Dimensions 1.2, 2.2) |
| Cap reached | Looping fleet or injection | `SCHEDULE_CAP_REACHED`; the tool returns the code (Dimension 1.3) |
| Human schedule targeted | Fleet names an `api` or `trigger` row | `SCHEDULE_NOT_FLEET_OWNED` (Dimension 1.4) |
| QStash unreachable | Scheduler down | Row stored, `sync_status` reports it, the sweeper reconciles later, as for tenant creates |
| No channel | Steer or webhook event | `MESSAGE_NO_CHANNEL`; the fleet says it in the report instead (Dimension 2.4) |
| Channel delivery fails | Slack fault | Retried as a reply is; the tool returns `delivered: false` |
| Slack stalls | Posts held open | Answered `delivered: false` at the 12 s deadline, before the runner's call times out (Dimension 2.9) |
| Fired run never leased | A schedule's plain-text message cast to `jsonb` at lease | Stored as `{"message": …}`; the lease records it (Dimension 1.14) |
| Run-now loops or wakes a stopped fleet | A scheduled run calls run-now; a paused schedule or fleet | 409 `SCHEDULE_NOT_RUNNABLE` or `UZ-AGT-012` with `current_state` (Dimensions 1.10–1.12) |
| Secret in a message | Fleet echoes a token | Masked before delivery (Dimension 2.5) |
| Steered fleet pages the channel | Thread content tells the model to write `<!channel>` | `&`, `<`, `>` posted as Slack entities; the line shows as typed (Dimension 2.10) |

## Invariants

1. The runner owns no timer: every schedule lives in `core.fleet_schedules` and fires through QStash's callback (Dimensions 1.1, 1.6).
2. A fleet touches only its own fleet's schedules, and only those it created (Dimension 1.4).
3. No channel credential enters the runner; delivery happens in the daemon (Dimension 2.1).
4. Nothing a fleet posts carries a secret value (Dimension 2.5).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `fleet_schedule_created` / `fleet_schedule_deleted` (daemon log, info) | ops | A fleet creates or deletes a schedule | `fleet_id`, `schedule_id`, `event_id` | No cron message text | `test_fleet_creates_its_own_schedule` |
| `fleet_schedule_refused` (daemon log, info) | ops | Cap, ownership or validation refusal | `fleet_id`, `error_code` | No body | `test_schedule_cap_refuses_with_code` |
| `fleet_message_posted` (daemon log, info) | ops | An interim message is delivered or fails | `fleet_id`, `event_id`, delivered, bytes | No text | `test_fleet_posts_a_message_mid_run` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_fleet_creates_its_own_schedule` | valid body → row source `fleet`, fake QStash 1 upsert, `cron_list` returns it |
| 1.2 | integration | `test_stale_fence_schedule_refused` | stale token → refused, 0 rows |
| 1.3 | integration | `test_schedule_cap_refuses_with_code` | 16 rows then create → `SCHEDULE_CAP_REACHED` |
| 1.4 | integration | `test_fleet_cannot_touch_human_schedules` | patch and delete on an `api` row → `SCHEDULE_NOT_FLEET_OWNED`, row unchanged |
| 1.5 | unit | `test_schedule_fields_validated` | `* * * * * *`, `Mars/Olympus`, 9 KiB message → each refused with reason |
| 1.6 | integration | `test_schedule_run_now_admits_one_event` | run → 1 event, actor `cron:<id>`, type `cron` |
| 1.7 | integration | `test_schedule_runs_lists_events` | 3 fires → 3 events newest first, `limit=2` pages |
| 1.8 | integration | `test_fleet_deletes_its_schedule` | delete → desired status deleted, fake QStash 1 delete |
| 1.9 | integration | `test_once_schedule_retires_after_fire` | run-now of a `once` row → fake QStash 1 delete, row gone |
| 1.10 | integration | `test_run_now_refuses_a_schedule_that_would_not_fire` | paused `api` row, deleting `fleet` row → 409 `UZ-SCHED-011`, `current_state` = that status, 0 events |
| 1.11 | integration | `test_run_now_refuses_a_fleet_that_takes_no_work` | fleet paused → 409 `UZ-AGT-012`, `current_state` = fleet status, 0 events |
| 1.12 | integration | `test_run_now_from_a_scheduled_run_is_refused` | lease on a `cron:` event → 409 `UZ-SCHED-011`, `current_state` `scheduled_run` |
| 1.13 | integration | `test_event_list_plans_use_the_index` | generic plan of the exact-actor page → index scan on slot 930, no sort |
| 1.14 | integration | `test_scheduled_run_is_leased_with_its_message` | run-now, settle, poll → leased; `request_json->>'message'` = the schedule's message |
| 2.1 | integration | `test_fleet_posts_a_message_mid_run` | post → Slack fake receives text in `thread_ts` before report |
| 2.2 | integration | `test_stale_fence_message_refused` | stale token → 0 posts |
| 2.3 | integration | `test_message_caps_refuse` | 9th post → refused; 5 KiB text → refused |
| 2.4 | integration | `test_message_without_channel_refused` | API-steer event → `MESSAGE_NO_CHANNEL` |
| 2.5 | integration | `test_message_is_scrubbed` | text with the token → Slack fake sees `«secret:github.token»` |
| 2.6 | unit | `test_interim_marker_is_not_the_answer` | a thread holding an interim line → `holds_answer` for the final marker answers false |
| 2.7 | unit | `test_reclaimed_lease_line_is_not_the_dead_lease_line` | dead lease's line 1 in the thread → the new lease's line 1 is not held |
| 2.8 | integration | `test_concurrent_messages_never_pass_the_cap` | posts racing for the last slots → exactly the cap land, the rest 409, count = cap |
| 2.9 | integration | `test_message_to_a_stalled_slack_answers_at_the_deadline` | Slack holds posts → 200 `delivered: false`, elapsed within 12–15 s |
| 2.10 | unit | `test_a_line_naming_the_channel_notifies_nobody` | `<!channel> … <@U…>` → `&lt;!channel&gt; … &lt;@U…&gt;`; the live poster case sees the same text on the wire |
| 3.1 | unit | `test_cron_tools_map_onto_the_verb` | each of six tools → the verb, method and path expected, fence carried |
| 3.2 | unit | `test_schedule_tool_is_once` | `schedule {at, message}` → create with `once: true` |
| 3.3 | unit | `test_message_tool_posts_or_names_gap` | 200 → `delivered: true`; `MESSAGE_NO_CHANNEL` → tool error with that code |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A fleet schedules and posts through the daemon (§1, §2) | `make test-integration-rustd && grep -cE "fn test_(fleet_creates_its_own_schedule\|fleet_posts_a_message_mid_run)\(" rustd/crates/agentsfleetd/tests/integration_runner_schedules.rs rustd/crates/agentsfleetd/tests/integration_runner_messages.rs` | 2 | P0 | ✅ `✓ [rustd] integration suite — 841 passed`, exclusive `— 2 passed`; grep count 2 |
| R2 | The eight handlers map onto the verbs (§3) | `cargo test --manifest-path rustd/Cargo.toml -p afr_tools verbs` | exit 0 | P0 | ✅ exit 0, `test result: ok. 22 passed; 0 failed` |
| R3 | The five codes are declared | `grep -rhcE "^pub const (SCHEDULE_CAP_REACHED\|SCHEDULE_NOT_FLEET_OWNED\|SCHEDULE_NOT_RUNNABLE\|MESSAGE_NO_CHANNEL\|MESSAGE_LIMIT_REACHED):" rustd/crates/afd_core/src/error_code/ \| paste -sd+ - \| bc` | 5 | P0 | ✅ 5 |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 0 paths missing (Files Changed glob check over `git diff --name-only origin/main...HEAD`) |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | → `orly gate pr`, PR Session Notes |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | → `orly gate pr`, PR Session Notes |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | → `orly gate pr`, PR Session Notes |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | → `orly gate pr`, PR Session Notes |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | → `orly gate pr`, PR Session Notes |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -E '\.(zig\|js\|jsx\|ts\|tsx\|py\|rs\|go\|sh\|sql)$' \| grep -vE '^(docs\|vendor\|third_party)/\|/fixtures?/' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Messages into the app's fleet thread for events with no channel — the thread today shows the reply at settle; an interim row in the thread is a chat change for a later spec.
- Schedules that wake a different fleet — a fleet schedules only itself.
- The changelog entry (owner direction, Oct 05, 2026). The published pages are in scope on their own branch in `~/Projects/docs`, never edited through this worktree.
- The Zig runner keeps refusing these tools until the cutover.
- A per-lease budget on schedule writes — deferred with no follow-up spec (Discovery).

---

## Product Clarity (authoring record)

1. **Successful user moment** — A fleet repairing an outage says in the thread "fix pushed as a draft; I'll re-check the error rate at 09:00 tomorrow", and at 09:00 a new turn appears with the reading, without anyone setting a schedule.
2. **Preserved user behaviour** — Schedules people create keep their source and cannot be changed by a fleet; the operator schedule routes and `agentsfleet schedule` are unchanged; QStash still owns every timer.
3. **Optimal-way check** — Two verbs on the memory push's fenced shape, over a store and a poster that exist; no new timer, no new channel credential path.
4. **Rebuild-vs-iterate** — Iterate on `afd_cron` and `afd_outbound`.
5. **What we build** — Two verbs, one source value, three codes, an interim poster, eight handlers.
6. **What we do NOT build** — Thread rows for channel-less events, cross-fleet schedules, the docs pages here (see Out of Scope).
7. **Fit with existing features** — The schedules list in the app shows `fleet`-sourced rows beside the others; the events list already carries `cron:<schedule_id>` actors.
8. **Surface order** — API first; the app's schedules page shows the new source without change.
9. **Dashboard restraint** — A schedule a fleet made says so in its source; nothing is attributed to a person.
10. **Confused-user next step** — The thread's `Scheduled …` cell names the schedule; the schedules page shows it with source `fleet` and its next fire.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** daemon half and runner half in one workstream because the handlers are thin and the verbs are what need review; the daemon half can start before the harness exists.
- **Alternatives considered:** a runner-local scheduler (rejected: the runner owns no timer, `docs/architecture/data_flow.md` §B. TRIGGER); letting the fleet use the tenant schedule routes with a minted token (rejected: a workspace-wide surface in a sandbox); posting messages straight to Slack from the runner (rejected: a channel credential in the runner).
- **Patch-vs-refactor verdict:** this is a **patch** because the store, the sync and the posters exist; the verbs are additions.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 02, 2026): "i need all the tools, since cron has a scheduler i think"; chose "Yes, runner verb onto daemon schedules" when asked whether to reverse "Scheduled wakes are not a child tool". The reversal is recorded in `docs/architecture/capabilities.md` §2 and `runner_execution.md` Decisions.
- **Agent defaults** — the cap of 16 schedules per fleet; 4 KiB and 8 messages per run; `source_key` as the creating event id; `schedule` as a once schedule.
- **S7 amended at close** (agent, Oct 06, 2026): the authored command counted `public/openapi.json` and `rustd/Cargo.lock`, which RULE FLL exempts; it now runs the rule's own self-audit (`dispatch/write_any.md` §LENGTH GATE). Three files already over the cap on `main` that this branch touched were split: `afd_cron/tests/integration_store.rs`, `scripts/check_architecture_doc.sh`, `scripts/check_architecture_doc_test.sh`.
- **Metrics review** — No analytics or funnel playbook update required: no user action; four operator log events added.
- **Skill-chain outcomes** — `/orly-write-unit-test` audit closed its gaps in `bc09ef632`; the boundary pass added the fire-body unit tests and the scheduled-run lease case (Dimension 1.14). `/orly-write-integration-test`: the live cases under `agentsfleetd/tests/integration_runner_*` and `afd_api/tests/integration_qstash_fire_once.rs` cross the real Postgres, Dragonfly and HTTP boundaries. `orly-babysit-prs`: pending, after the push. gstack `/review` (Oct 06, 2026) ran six specialists: security, API design, performance, testing, maintainability, adversarial.
- **Review: fixed** (agent, Oct 06, 2026):
  - Run-now skipped the callback's gates; it now refuses a fleet that takes no work, a paused or deleting schedule, and a run a schedule started (Dimensions 1.10–1.12, new `SCHEDULE_NOT_RUNNABLE`), keyed `run:<event_id>`.
  - A schedule's runs scanned the fleet's history; exact-actor statements read slot 930 (Dimension 1.13).
  - The message count proves the fence in one statement (Dimension 2.8); delivery is bounded by a 12 s deadline under the runner's 20 s call timeout (Dimension 2.9); interim lines carry their own stamp and a lease-scoped part (Dimension 2.7).
  - Every 409 names `current_state`; `MESSAGE_LIMIT_REACHED` moved from 429 to 409, since the cap is the run's state and a retry cannot clear it.
  - Schedule messages are masked on both sides: the runner masks minted tokens, the daemon masks declared static secrets.
  - A fleet-made schedule is keyed by its minted id, so two creates in one millisecond cannot collide; `schedule` rounds `at` up to the next minute and needs a minute of lead; `SLACK_API_URL` must be https or loopback http.
  - A dropped `once` fire for a fleet that takes no work retires the schedule; a held retirement claim answers `UZ-SCHED-006`, so the caller repeats it.
  - Interim lines post `&`, `<` and `>` as Slack entities, so a steered fleet cannot page the channel up to eight times a run (Dimension 2.10; agent call while Indy was away, Oct 06, 2026, revertible).
  - Found by the integration lane, already on `main`: a fire stored the schedule's plain-text message as the event body, and the lease's `$6::jsonb` cast refused it, so no scheduled run could be leased (`UZ-INTERNAL-002` on every poll that reached it). `afd_cron::fire` now stores `{"message": …}`, the shape every producer stores and `afr_agent::prompt` reads (Dimension 1.14). Folded in: run-now is this spec's, and without it no schedule wakes its fleet.
  - Splits and helpers for over-long files and functions; cross-fleet negative tests on run-now, PATCH, DELETE and runs.
- **Greptile on #731** (agent, Oct 06, 2026): fixed — a `once` schedule's fire is keyed by the schedule alone, so a run-now racing its scheduled fire replays it; a retired one-off's runs stay listed; `SLACK_API_URL` refuses a query or fragment; patch coverage back over 99%. Open for Indy's call, not deferred: a lease that expires mid-request can still write one schedule; a one-off whose QStash registration fails past its minute registers a year late (a fix needs its intended minute stored, a new slot).
- **Review: answer-path escaping** — Slack mention escaping on the answer path, exposed since M206, is deferred with no follow-up spec; Indy tests and fixes it himself (quote under Deferrals).
- **Deferrals** — ten review items, shipped as recorded:

> Indy (2026-10-06 07:58): "Ship the other 10 open review items as recorded yes" — context: the per-lease schedule-write budget, with no follow-up spec (quote below); a distinct actor for fleet-made fires; `installed()` over-fetch on the mask path; `once` retirement on the request path; a `lock_timeout` on slot 929 (no precedent); list envelope and DELETE idempotency parity with the tenant routes; PATCH reviving a `deleting` row (the tenant PATCH does the same); the dashboard's `cron:*` filter now matching every fire; four functions already over the length cap grew a few lines; the `afr_secrets` scrub-failure lift no test reaches without a test-only constructor.

> Indy (2026-10-06 08:00): "I donot need this spec for now, so nuke it?" — context: the M212_002 write-budget spec was removed before it left `pending/`; the budget stays an open finding with no spec.

> Indy (2026-10-06 08:19): "slack escaping skip follow up spec, let me test and fix it later." — context: escaping `&`, `<` and `>` on the answer path; interim lines already escape them (Dimension 2.10).

- **PLAN decisions** (agent, Oct 05, 2026, from source on `main`): the seven assumptions under the handshake. The ones that change the spec as authored: the fence rides the query on `GET` and `DELETE`; run-now is `POST …/runs`, a run created, not a `/run` verb in the path; a patch names `paused` as the tenant route does, so `deleting` cannot be set by a patch; the fire actor becomes `cron:<schedule_id>`; slots 928 and 929; the daemon reuses `afr_secrets::Scrub`; a fourth code for the per-run message cap; `source_key` gains a millisecond suffix, because one run may create two schedules and the key is unique per fleet.
- **Owner direction** (Indy, in-session, Oct 05, 2026): "If there is api docs to be updated do so, and update docs, but skip changelog" — the docs pages move into this stream on their own branch in `~/Projects/docs`; no changelog entry. "the rust patch diff must be 99%, all typescript must be 100%".
