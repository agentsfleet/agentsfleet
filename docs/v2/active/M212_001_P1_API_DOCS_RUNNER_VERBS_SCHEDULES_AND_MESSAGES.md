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
**Status:** IN_PROGRESS
**Priority:** P1 — the tools the published page lists that the Zig runner refuses; without them a fleet cannot plan a follow-up or speak before it finishes
**Categories:** API, DOCS
**Batch:** B1 — the daemon half depends on nothing and can start beside M210_002; the runner half plugs into M210_002's catalog. One Pull Request
**Branch:** `feat/m212-001-runner-schedules-messages`
**Baseline revision:** `c5f7680f2ee4a475a9f6f6c8701c98262c6d5c8d`
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
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
| `schema/928_fleet_schedules_once.sql`, `schema/929_runner_leases_messages_posted.sql`, `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | `once` on a schedule; the per-run message count on a lease |
| `rustd/crates/afd_wire/src/schedule_verb.rs`, `rustd/crates/afd_wire/src/message_verb.rs`, `rustd/crates/afd_wire/src/schedule.rs`, `rustd/crates/afd_wire/src/paths.rs`, `rustd/crates/afd_wire/src/lib.rs` | CREATE / EDIT | Request and response types, bounds, the path segments; a schedule view names its source and `once` |
| `rustd/crates/afd_http/src/route/runner.rs`, `rustd/crates/afd_http/src/services/leasing.rs`, `rustd/crates/afd_http/src/services/schedule.rs` | EDIT | Variants, `ALL` entries and `meta()`; the lease seam's two verbs; a once schedule retires after it fires |
| `rustd/crates/afd_api_runner/src/handler/runner/schedule.rs`, `rustd/crates/afd_api_runner/src/handler/runner/schedule_fire.rs`, `rustd/crates/afd_api_runner/src/handler/runner/message.rs`, `rustd/crates/afd_api_runner/src/handler/runner/mod.rs`, `rustd/crates/afd_api_runner/src/lib.rs` | CREATE / EDIT | Fence, narrow, cap, store, reconcile; fire and runs; fence, scrub, post |
| `rustd/crates/afd_cron/src/model.rs`, `rustd/crates/afd_cron/src/store.rs`, `rustd/crates/afd_cron/src/store/decode.rs`, `rustd/crates/afd_cron/src/sql.rs`, `rustd/crates/afd_cron/src/fire.rs`, `rustd/crates/afd_cron/src/lib.rs` | EDIT | `Source::Fleet`, `once`, the fleet-authored cap, the `cron:<schedule_id>` actor |
| `rustd/crates/afd_fleet/src/lease/standing.rs`, `rustd/crates/afd_fleet/src/lease/message.rs`, `rustd/crates/afd_fleet/src/lease/sql/*.rs`, `rustd/crates/afd_fleet/src/lease/pull.rs`, `rustd/crates/afd_fleet/src/lease/mod.rs`, `rustd/crates/afd_fleet/Cargo.toml` | CREATE / EDIT | The proved lease a verb names; the interim message's fence, count, destination, scrub and post |
| `rustd/crates/afd_connector/src/slack/answered.rs` | EDIT | A marker part, so an interim line is never read as the answer |
| `rustd/crates/afd_outbound/src/interim.rs`, `rustd/crates/afd_outbound/src/slack.rs`, `rustd/crates/afd_outbound/src/slack/*.rs`, `rustd/crates/afd_outbound/src/lib.rs` | CREATE / EDIT | An interim post into the event's origin thread, through the existing poster and retry |
| `rustd/crates/afd_core/src/error_code/fleet.rs`, `rustd/crates/afd_core/src/error_code/request.rs`, `rustd/crates/afd_core/src/error_code.rs`, `rustd/crates/afd_core/src/problem/*.rs` | EDIT | `SCHEDULE_CAP_REACHED`, `SCHEDULE_NOT_FLEET_OWNED`, `MESSAGE_NO_CHANNEL`, `MESSAGE_LIMIT_REACHED` and their problems |
| `rustd/crates/afr_secrets/src/statics.rs` | EDIT | A view over a declared map, so the daemon masks with the one masker |
| `rustd/crates/agentsfleetd/src/plane/*.rs`, `rustd/crates/agentsfleetd/src/outbound.rs`, `rustd/crates/agentsfleetd/src/preflight*` | EDIT | One Slack poster for the worker and the interim post; its base address knob |
| `public/openapi.json` | EDIT | Regenerated; new fields declare `x-stability` |
| `rustd/crates/afr_tools/src/verbs.rs`, `rustd/crates/afr_tools/src/verbs/*.rs`, `rustd/crates/afr_tools/src/lease.rs`, `rustd/crates/afr_tools/src/runtime.rs`, `rustd/crates/afr_tools/src/catalog.rs`, `rustd/crates/afr_tools/src/lib.rs`, `rustd/crates/afr_tools/Cargo.toml` | CREATE / EDIT | The lease-verb seam and the eight handlers onto it |
| `rustd/crates/afr_supervisor/src/client.rs`, `rustd/crates/afr_supervisor/src/client/http.rs`, `rustd/crates/afr_supervisor/src/verbs.rs`, `rustd/crates/afr_supervisor/src/turns.rs`, `rustd/crates/afr_agent/src/*.rs` | CREATE / EDIT | The seam over the daemon's HTTP verbs, with `PATCH` and `DELETE`; handed to each run |
| `rustd/crates/agentsfleetd/tests/support/*.rs`, `rustd/crates/agentsfleetd/tests/integration_runner_schedules.rs`, `rustd/crates/agentsfleetd/tests/integration_runner_messages.rs`, `rustd/crates/*/tests/**`, `rustd/crates/**/tests.rs` | EDIT / CREATE | Runner-shaped posts, fake QStash and Slack, live-datastore proofs (`#[ignore]`d, run by `make test-integration-rustd`); unit proofs beside each change |
| `docs/architecture/capabilities.md`, `docs/architecture/runner_execution.md`, `docs/architecture/data_flow.md` | EDIT | The fleet as a third schedule author; the interim post; the fire actor |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS + TFX (the cap, the message bound and the per-run message count are constants the tests and the runner import), NTP (both bodies narrowed at the verb), STS (the `source` column holds a word; `fleet` is a named constant, never a `CHECK`), ERR (the three codes are declared and referenced), OBS, ORP, TST-NAM, TCF, ECL (a QStash reconcile failure stores the row and reports `sync_status`, as the tenant route does).
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md`; `dispatch/write_http.md` + `docs/REST_API_DESIGN_GUIDELINES.md` (§1, §5, §7, §9); `docs/LOGGING_STANDARD.md` — never log a message body or a schedule message.
- `docs/architecture/data_flow.md` §B. TRIGGER — QStash owns the clock; nothing here sleeps or ticks.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ERROR REGISTRY | yes | Three codes declared in `afd_core` and used by the verbs |
| SCHEMA GUARD | yes | `VERSION=0.56.0` (migrate): `migration:schema/928_fleet_schedules_once.sql`, `migration:schema/929_runner_leases_messages_posted.sql`. `source` stays `TEXT` with no `CHECK`; `fleet` is an application constant |
| UFS / LOGGING / MILESTONE-ID | yes | Constants once; scoped events with ids and counts only |
| Architecture consult | yes | `capabilities.md` §2 and `runner_execution.md` §"Tool catalog" already state these facts |
| File & Function Length (≤350/≤50/≤70) | yes | One verb per handler file; the interim poster in its own module |

## Prior-Art / Reference Implementations

- **Reference:** the memory push (`rustd/crates/afd_api_runner/src/handler/runner/memory.rs` → `rustd/crates/afd_fleet/src/lease/memory.rs`) — a fenced runner write whose fleet is derived from the lease, never from the body. Both verbs copy its shape.
- **Reference:** the tenant schedule routes (`rustd/crates/afd_api_tenant/src/handler/schedule/write.rs`) — create, patch, purge and sync over `afd_cron`; the runner verb is the same store with the fleet fixed and the source `fleet`.
- **Reference:** `rustd/crates/afd_outbound/src/poster.rs` — `dispatch` delivers a settled reply to the event's channel; the interim post reuses its posters and retry with a different body.

## Sections (implementation slices)

### §1 — A fleet owns schedules on the daemon's plane

`POST`, `GET`, `PATCH`, `DELETE` on `/v1/runners/me/leases/{lease_id}/schedules[/{schedule_id}]` and `POST …/schedules/{schedule_id}/runs` (run now, modelled as creating a run, since the REST guide bans a verb in a path), each carrying `fencing_token`. The fleet is the lease's; the body never names one. A create stores source `fleet` and `source_key` = the creating event id, validates `cron`, `timezone` and `message` through `afd_cron::validate`, refuses the `FLEET_SCHEDULES_MAX`+1th schedule with `SCHEDULE_CAP_REACHED`, and reconciles to QStash as the tenant create does. List returns every schedule of the fleet with its source; update and delete refuse a schedule whose source is not `fleet` with `SCHEDULE_NOT_FLEET_OWNED`. Run-now admits one `schedule_fire` event for that schedule through the same producer QStash's callback uses. `GET …/schedules/{schedule_id}/runs` lists the fleet's events with actor `cron:<schedule_id>`, newest first, paged.

- **Dimension 1.1** — A valid create stores a `fleet`-sourced row and reconciles once → Test `test_fleet_creates_its_own_schedule`
- **Dimension 1.2** — A stale fence is refused and stores nothing → Test `test_stale_fence_schedule_refused`
- **Dimension 1.3** — The cap refuses the next create with its code → Test `test_schedule_cap_refuses_with_code`
- **Dimension 1.4** — An `api`- or `trigger`-sourced schedule cannot be updated or deleted by the fleet → Test `test_fleet_cannot_touch_human_schedules`
- **Dimension 1.5** — An invalid cron, timezone or message is refused with the validator's reason → Test `test_schedule_fields_validated`
- **Dimension 1.6** — Run-now admits exactly one event with actor `cron:<schedule_id>` → Test `test_schedule_run_now_admits_one_event`
- **Dimension 1.7** — Runs lists that event, newest first → Test `test_schedule_runs_lists_events`
- **Dimension 1.8** — Delete sets the desired status and reconciles; the row leaves QStash → Test `test_fleet_deletes_its_schedule`
- **Dimension 1.9** — A `once` schedule retires after its first fire, from QStash or run-now → Test `test_once_schedule_retires_after_fire`

### §2 — A fleet posts to its thread mid-run

`POST /v1/runners/me/leases/{lease_id}/messages { fencing_token, text }` delivers `text` to the event's origin channel in the same thread through the outbound posters, with retry. `text` is capped at `MESSAGE_MAX_BYTES` and scrubbed of the lease's secret values; a run may post at most `MESSAGES_PER_RUN_MAX`. An event with no origin channel (an API steer, a webhook) is refused with `MESSAGE_NO_CHANNEL`, which the tool returns to the model.

- **Dimension 2.1** — A message reaches the Slack thread fake before the run ends → Test `test_fleet_posts_a_message_mid_run`
- **Dimension 2.2** — A stale fence posts nothing → Test `test_stale_fence_message_refused`
- **Dimension 2.3** — The per-run count and the byte cap refuse with their codes → Test `test_message_caps_refuse`
- **Dimension 2.4** — An event without a channel is refused with `MESSAGE_NO_CHANNEL` → Test `test_message_without_channel_refused`
- **Dimension 2.5** — A secret value in the text is masked before delivery → Test `test_message_is_scrubbed`
- **Dimension 2.6** — An interim line carries its own marker, so a repeat of the final answer still posts it → Test `test_interim_marker_is_not_the_answer`

### §3 — The harness carries the eight tools

`cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs` and `schedule` (NullClaw's one-shot: a cron that deletes itself after firing, expressed as a create with `once: true`) map onto §1; `message` maps onto §2. Each is a `Supervisor` handler in the catalog; a refusal's code reaches the model as the tool's error.

- **Dimension 3.1** — Each cron tool issues its verb with the lease's fence and returns the daemon's answer → Test `test_cron_tools_map_onto_the_verb`
- **Dimension 3.2** — `schedule` creates a once schedule that is deleted after its first fire → Test `test_schedule_tool_is_once`
- **Dimension 3.3** — `message` posts and returns the refusal code when there is no channel → Test `test_message_tool_posts_or_names_gap`

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
source_key for a fleet create = "<creating event id>-<millis>" until QStash's id replaces it
fleet.runner_leases.messages_posted (slot 929)
A schedule fire's actor = cron:<schedule_id> (QStash and run-now alike)
Constants: FLEET_SCHEDULES_MAX 16 · MESSAGE_MAX_BYTES 4096 · MESSAGES_PER_RUN_MAX 8
Codes: SCHEDULE_CAP_REACHED (UZ-SCHED-009, 409) · SCHEDULE_NOT_FLEET_OWNED (UZ-SCHED-010, 403) · MESSAGE_NO_CHANNEL (UZ-RUN-019, 409) · MESSAGE_LIMIT_REACHED (UZ-RUN-020, 429)
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
| Secret in a message | Fleet echoes a token | Masked before delivery (Dimension 2.5) |

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
| 1.9 | unit | `test_once_schedule_retires_after_fire` | a fire of a `once` row → the plane claims it `deleting` and reconciles; a recurring row is left alone |
| 2.1 | integration | `test_fleet_posts_a_message_mid_run` | post → Slack fake receives text in `thread_ts` before report |
| 2.2 | integration | `test_stale_fence_message_refused` | stale token → 0 posts |
| 2.3 | integration | `test_message_caps_refuse` | 9th post → refused; 5 KiB text → refused |
| 2.4 | integration | `test_message_without_channel_refused` | API-steer event → `MESSAGE_NO_CHANNEL` |
| 2.5 | integration | `test_message_is_scrubbed` | text with the token → Slack fake sees `«secret:github.token»` |
| 2.6 | unit | `test_interim_marker_is_not_the_answer` | a thread holding an interim line → `holds_answer` for the final marker answers false |
| 3.1 | unit | `test_cron_tools_map_onto_the_verb` | each of six tools → the verb, method and path expected, fence carried |
| 3.2 | unit | `test_schedule_tool_is_once` | `schedule {at, message}` → create with `once: true` |
| 3.3 | unit | `test_message_tool_posts_or_names_gap` | 200 → `delivered: true`; `MESSAGE_NO_CHANNEL` → tool error with that code |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A fleet schedules and posts through the daemon (§1, §2) | `make test-integration-rustd && grep -cE "fn test_(fleet_creates_its_own_schedule\|fleet_posts_a_message_mid_run)\(" rustd/crates/agentsfleetd/tests/integration_runner_schedules.rs rustd/crates/agentsfleetd/tests/integration_runner_messages.rs` | 2 | P0 | |
| R2 | The eight handlers map onto the verbs (§3) | `cargo test --manifest-path rustd/Cargo.toml -p afr_tools verbs` | exit 0 | P0 | |
| R3 | The four codes are declared | `grep -rhcE "^pub const (SCHEDULE_CAP_REACHED\|SCHEDULE_NOT_FLEET_OWNED\|MESSAGE_NO_CHANNEL\|MESSAGE_LIMIT_REACHED):" rustd/crates/afd_core/src/error_code/ \| paste -sd+ - \| bc` | 4 | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes. **Ship gate:** every required check passes before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may be MOVED only under the transfer rule in `docs/TEMPLATE.md` (successor carries the row, both specs record it, owner's verbatim quote in Discovery); a MOVED row is never ✅.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Messages into the app's fleet thread for events with no channel — the thread today shows the reply at settle; an interim row in the thread is a chat change for a later spec.
- Schedules that wake a different fleet — a fleet schedules only itself.
- The changelog entry (owner direction, Oct 05, 2026). The published pages are in scope on their own branch in `~/Projects/docs`, never edited through this worktree.
- The Zig runner keeps refusing these tools until the cutover.

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
- **Metrics review** — No analytics or funnel playbook update required: no user action; four operator log events added.
- **Skill-chain outcomes** — pending.
- **PLAN decisions** (agent, Oct 05, 2026, from source on `main`): the seven assumptions under the handshake. The ones that change the spec as authored: the fence rides the query on `GET` and `DELETE`; run-now is `POST …/runs`, a run created, not a `/run` verb in the path; a patch names `paused` as the tenant route does, so `deleting` cannot be set by a patch; the fire actor becomes `cron:<schedule_id>`; slots 928 and 929; the daemon reuses `afr_secrets::Scrub`; a fourth code for the per-run message cap; `source_key` gains a millisecond suffix, because one run may create two schedules and the key is unique per fleet.
- **Owner direction** (Indy, in-session, Oct 05, 2026): "If there is api docs to be updated do so, and update docs, but skip changelog" — the docs pages move into this stream on their own branch in `~/Projects/docs`; no changelog entry. "the rust patch diff must be 99%, all typescript must be 100%".
- **Deferrals** — none.
