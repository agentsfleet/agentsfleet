<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M239_001: A fleet steers another fleet by id and reads its activity

**Prototype:** v2.0.0

**Milestone:** M239

**Workstream:** 001

**Date:** Nov 04, 2026

**Status:** PENDING

**Priority:** P1, a person relays work between fleets today, and a chief-of-staff or digest fleet cannot exist without this

**Categories:** API, DOCS, INFRA, UI

**Batch:** B1, one Workstream with no parallel group

**Depends on:** none. M236 adds durable waits; this spec adds no wait and does not touch M236's files.

**Provenance:** agent-drafted (Claude Fable 5.1, Nov 04, 2026) from Indy's brainstorm; his words are quoted in Discovery

**Canonical architecture:** `docs/architecture/scenarios/fleet-to-fleet.md`; `docs/architecture/data_flow.md` §"B. TRIGGER"

---

## Overview

**Goal (testable):** `a_fleet_steers_another_fleet_once_by_id_and_reads_its_activity_without_an_api_key`

**Problem:** A person is the wire between fleets: in `#ci-dev` the responder posts a diagnosis and someone must type "open the fix" before the repairer runs. A fleet cannot learn another fleet's status or last answer without an API key in its secrets. No fleet can be the owner's single point of contact for the workspace.

**Solution summary:** Two runner tools over three lease-proven runner routes. `fleet_steer` puts work on a fleet in the same workspace through the admission a person's steer already uses, with actor `fleet:<sender_fleet_id>`, a hop count and the sender's reply destination copied across. `fleet_activity` lists the workspace's fleets and one fleet's recent events over the lease plane. The fleet chat labels a fleet-sent event with the sender's name.

**Design flow:** model calls a tool → `LeaseVerbs` → runner route proves the lease and fencing token → handler checks self, replay, target status, hop and the per-lease slot → `Steer::append_from_fleet` admits once with the sender's reply destination → the target leases under its own pinned revision → the sender reads the answer with `fleet_activity` on a later turn.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(fleets): let a fleet steer another fleet and read its activity
- **Intent (one sentence):** A fleet given two tools can hand work to a named fleet and read what the workspace's fleets did, so a person no longer relays between them and a chief-of-staff fleet can exist.
- **Authoring handshake:** Indy asked for fleet-to-fleet trigger and communication, then narrowed it: "its just steer to a fleet id. and notify essentially is in the chat informing the user. thats it so its fleet_activity?". ASSUMPTIONS I'M MAKING: 1. Owner consent is the fleet's `tools` list in its reviewed revision; no allowlist, rule or subscription. 2. A fleet with `fleet_steer` may steer any active fleet in its workspace. 3. The sender learns the answer by reading, never by being woken. 4. Notify is the fleet's own answer in its own chat; `message` and `pushover` already exist.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs` — the person's steer route: ownership, replay before the status check, ingress status, `Steer::append`, `announce`. The fleet steer handler composes the same `Services` calls.
2. `rustd/crates/afr_tools/src/verbs/message/mod.rs`, `rustd/crates/afr_supervisor/src/client/lease_verbs/mod.rs` and `rustd/crates/afd_api_runner/src/handler/runner/message.rs` — one verb end to end: tool, `LeaseVerbs`, supervisor client, runner route, refusal codes the model reads.
3. `rustd/crates/afd_events/src/steer/mod.rs` and `rustd/crates/afd_admission/src/sql/insert_admission.sql` — the steer key, the repeat path, and the `LEFT JOIN` that copies an inherited reply destination only within one fleet.
4. `rustd/crates/afd_fleet/src/lease/sql/count_message.sql` and `schema/1810_runner_leases.sql` — the guarded per-run counter the steer cap mirrors.
5. `docs/architecture/scenarios/fleet-to-fleet.md` — the shape, what it reuses and the bounds.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M239_001_P1_API_DOCS_INFRA_UI_FLEET_TO_FLEET_STEER_AND_ACTIVITY.md` | CREATE, then MOVE to `docs/v2/done/` | this spec |
| `docs/architecture/scenarios/fleet-to-fleet.md`; `docs/architecture/scenarios/README.md` | CREATE / EDIT | the scenario and its index row (authoring Pull Request) |
| `docs/architecture/data_flow.md`; `docs/architecture/README.md` | EDIT | the `fleet:` actor in §"B. TRIGGER" and a glossary row (authoring Pull Request) |
| `docs/architecture/runner_execution.md` | EDIT | the two tools in §"Tool catalog" |
| `schema/2200_runner_leases_steer_count.sql`; `rustd/crates/afd_db/src/migration.rs` | CREATE / EDIT | `runner.leases.posted_steer_count`; slot rechecked against fresh main (M236 proposes 2190) |
| `rustd/crates/afd_wire/src/steer_verb/`; `rustd/crates/afd_wire/src/fleets_verb/`; `rustd/crates/afd_wire/src/lib.rs`; `rustd/crates/afd_wire/src/paths.rs` | CREATE / EDIT | request and row types (the reply reuses `afd_wire::event::SteerAccepted`); `STEERS_PER_RUN_MAX`, `HOP_MAX`, `EXCERPT_MAX_BYTES`; three path constants |
| `rustd/crates/afd_wire/src/event/steer.rs` | EDIT | `FleetSteerBody`, the stored body, beside `SteerRequest` |
| `rustd/crates/afr_tools/src/catalog/mod.rs`; `rustd/crates/afr_tools/src/catalog/tests.rs`; `rustd/crates/afr_tools/src/verbs/mod.rs`; `rustd/crates/afr_tools/src/runtime/mod.rs`; `rustd/crates/afr_tools/src/testing.rs` | EDIT | two catalog entries (`PUBLISHED` 39 → 41, both hosted); two `LeaseVerbs` methods; `tool_code` arms and `ToolErrorCode` variants for the three codes; the test fake |
| `rustd/crates/afr_tools/src/verbs/fleet_steer/`; `rustd/crates/afr_tools/src/verbs/fleets/` | CREATE | the two tools and their tests |
| `rustd/crates/afr_supervisor/src/client/lease_verbs/mod.rs`; `rustd/crates/afr_supervisor/src/client/verb.rs`; `rustd/crates/afr_supervisor/src/verbs/mod.rs` | EDIT | the control-plane calls, the `Verb` variants, the fenced implementation |
| `rustd/crates/afd_api_runner/src/handler/runner/fleet_steer.rs`; `.../fleets.rs`; `.../mod.rs`; `rustd/crates/afd_api_runner/src/lib.rs`; `rustd/crates/afd_api_runner/src/openapi.rs` | CREATE / EDIT | the three handlers, router arms and OpenAPI registration |
| `rustd/crates/afd_http/src/route/runner.rs` | EDIT | three `RunnerRoute` variants and their `meta` arms |
| `rustd/crates/afd_http/src/services/leasing/mod.rs`; `.../leasing/plane.rs`; `rustd/crates/afd_http/src/services/event.rs`; `rustd/crates/afd_http/src/services/fleets.rs` | EDIT | `Leasing::count_steer`, `Leasing::sender_hop`; `FleetSteering::append_from_fleet`; `Fleets::activity` and `Fleets::activity_events` |
| `rustd/crates/afd_api/tests/harness/stubs_runner.rs`; `rustd/crates/afd_api/tests/route_meta_total.rs`; `rustd/crates/afd_wire/tests/routes.rs` and its snapshot | EDIT | the stub; a `FLEET_TO_FLEET_ADDITIONS` term of 3 in `RUST_ROUTE_COUNT`; three path rows (runner routes are not in `route_inventory.rs`) |
| `rustd/crates/afd_fleet/src/lease/sql/count_steer.sql`; `.../select_sender_hop.sql`; `rustd/crates/afd_fleet/src/lease/steer.rs`; `rustd/crates/afd_fleet/src/error/refuse.rs` | CREATE / EDIT | the guarded counter, the sender's hop read, their `Plane` methods, two refusal constructors |
| `rustd/crates/afd_fleet/src/lease/bracket.rs`; `rustd/crates/afd_fleet/src/lease/history.rs` | EDIT | a `fleet:` event's received frame and prior turns read `message` from `FleetSteerBody` |
| `rustd/crates/afd_events/src/steer/mod.rs`; `rustd/crates/afd_events/src/lib.rs` | EDIT | `FLEET_ACTOR_PREFIX`, `fleet_actor`, `is_fleet_actor`; `Steer::append_from_fleet`, which takes the reply event |
| `rustd/crates/afd_events/src/activity/`; its SQL | CREATE | the fleet list with each fleet's last event, and a fleet's events with `response_text` cut to the excerpt |
| `rustd/crates/afd_admission/src/sql/insert_admission.sql`; `rustd/crates/afd_admission/src/tests/reply.rs`; `rustd/crates/afd_fleet/tests/integration_reply_destination.rs` | EDIT | inherit by `workspace_id` instead of `fleet_id`; both reruns prove the gate continuation still inherits |
| `rustd/crates/afd_core/src/error_code/fleet.rs`; `rustd/crates/afd_core/src/error_code/mod.rs`; `rustd/crates/afd_core/src/problem/fleet.rs` | EDIT | `UZ-AGT-018`, `UZ-AGT-019`, `UZ-RUN-021`: constant, `REGISTRY` row, problem entry |
| `public/openapi.json` | EDIT | regenerated from `agentsfleetd openapi` |
| `ui/packages/app/lib/events/event-summary.ts`; `ui/packages/app/lib/events/event-summary.test.ts` | EDIT | the `fleet:` arm: a user-side turn labelled "asked by <sender_fleet_name>" |
| `ui/packages/app/tests/e2e/fleet-steer-row.spec.ts` | CREATE | the dry-lane row (`playwright.config.ts`, `testDir` `./tests/e2e`) |
| `tests/fixtures/fleets/chief-of-staff/definition.json`; `tests/fixtures/fleets/ci-responder/definition.json` | CREATE / EDIT | the chief of staff; the responder gains `fleet_steer` |
| `tests/fixtures/fleets/fleet-to-fleet/cases.json`; `playbooks/operations/acceptance/baselines/M239_001-<sha>.md` | CREATE | the hand-graded eval cases and the manual evidence ledger |
| `~/Projects/docs`: API reference for the three runner routes, the tools page, the changelog entry | EDIT | on its own branch after this repository's Pull Request merges |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE, RS-NEWTYPE, RS-BORROW, RS-RACE
- **`domains/http.md`** — HTTP-NOUN, HTTP-NEST, HTTP-PARAM, HTTP-ROUTE, HTTP-METHOD, HTTP-GARDE, HTTP-VALID, HTTP-SENTENCE
- **`domains/http-responses.md`** — HTTP-LIST, HTTP-PAGE, HTTP-NULL, HTTP-TIME, HTTP-STATUS, HTTP-STATEEXT, HTTP-REGISTRY, HTTP-NEWCODE, HTTP-TITLE, HTTP-LEAK
- **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-ANNOTATE, HTTP-BREAK, HTTP-STABILITY, HTTP-SURFACE, HTTP-TESTS
- **`domains/auth.md`** — AUTH-GUARD, WAUTH, AWO, CTX, AUTH-NOLOG, AUTH-LOGID
- **`domains/sql.md`** — the slot and statement rules for the new column and statements
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-CODE, LOG-SECRET, OBS
- **`domains/architecture.md`** — ARCH-GROUND, ARCH-LAND, ARCH-HOMES, ARCH-PUBLISHED
- **`code.md`** — PRI, NTP, EMS, CODE-ERRREG, CODE-SEAM, FLL
- **`tests.md`** — TEST-OUTCOME, TEST-ERROR, TEST-NEG, TEST-INJECT, TEST-RESIDUAL, TEST-CONTEND, TEST-REAL, TEST-LANE
- **`languages/typescript.md`** — TS-SCHEMA, TS-ANY, TS-TESTFILE
- **`docs.md`** — DOC-02, DOC-03, DOC-05, DOC-06

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` (clippy, OpenAPI drift, route registration) | yes | annotate every route; regenerate `public/openapi.json` in the same commit |
| `make test-unit-all` | yes | unit tests beside each new type, tool and the UI arm |
| `make test-integration-rustd` | yes | daemon Dimensions are `#[ignore]`d integration tests in that lane |
| `orly gate work` | yes | whole-file commits |
| gitleaks | no secrets in the diff | fixtures reuse the repository's fixture actors |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, the steer handler | checks, admission and refusal mapping in separate functions, as `message.rs` splits them |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs` — the steer handler's order and `Services` composition. Divergence: the caller is a lease, not a session, and the reply destination is inherited.
- **Reference:** `rustd/crates/afd_fleet/src/lease/sql/count_message.sql` — a guarded per-lease counter. The steer counter is a sibling column with its own statement.
- **Reference:** `rustd/crates/afd_approval/src/inbox/resolve/mod.rs` — `Reply::Inherit` on a continuation. The steer inherits the same way, across fleets in one workspace.
- **Reference:** `rustd/crates/afd_api_runner/src/handler/runner/schedule.rs` — a runner `GET` that proves the lease with `?fencing_token=`; both reads mirror it.

## Sections (implementation slices)

Each Section registers its own routes, codes and path constants, so its Dimensions run through the real route. §3 publishes the surface once all routes exist.

### §1 — `fleet_activity`: read the workspace's fleets over the lease plane

Two reads behind the lease: the workspace's fleets, and one fleet's recent events with a bounded excerpt of each answer. Both prove the lease through `Leasing::standing` with the `?fencing_token=` query, as the schedules `GET` does. Two new statements in `afd_events::activity` serve them. The first lists the workspace's fleets with each one's latest `fleet.events` row (`created_at` as `last_event_at`, `status` as `last_status`). The second pages one fleet's events selecting `response_text`, cut to the excerpt. Paging mirrors the history read: `starting_after`, `limit` default 50, maximum 200. The tool is `fleet_activity`; its module is `verbs/fleets/`, because "activity" already names the runner's live-tail route (`RunnerRoute::Activity`). **Implementation default:** `EXCERPT_MAX_BYTES` is 1024, cut with `str::floor_char_boundary` as `lease/history.rs` does. A digest needs a line, and the full answer is in the dashboard.

- **Dimension 1.1** — the list names every fleet in the lease's workspace → Test `fleets_read_lists_workspace_fleets_for_a_live_lease` (integration): three fleets in workspace A, one stopped and one never run, one fleet in workspace B → three items; the stopped one reads `status` `stopped`; the never-run one omits `last_event_at` and `last_status`; `total` 3; no item from B.
- **Dimension 1.2** — a fleet outside the workspace is unknown → Test `fleets_read_refuses_a_fleet_from_another_workspace` (integration): events of a fleet in workspace B through a lease in A → 404 `UZ-AGT-009`, `detail` names no fleet; a path segment that is not a UUID → 400.
- **Dimension 1.3** — no live lease, no read → Test `fleets_read_refuses_a_lease_the_runner_does_not_hold` (integration): a settled lease id → 404 `UZ-RUN-006`; another runner's lease id → 404 `UZ-RUN-006`; a stale `fencing_token` → 409 `UZ-RUN-005` `current_state` `superseded`.
- **Dimension 1.4** — the excerpt is bounded → Test `fleets_read_excerpt_is_bounded` (unit): a 10 KiB answer with a three-byte character spanning byte 1024 → `response_excerpt` of 1022 bytes ending before that character.
- **Dimension 1.5** — undeclared, unoffered → Test `fleet_tools_are_offered_only_when_declared` (unit): a policy without the entries → `Selection::tool("fleet_activity")` and `Selection::tool("fleet_steer")` are `None`; `PUBLISHED` holds 41 entries and `every_published_handler_entry_is_hosted` passes.

### §2 — `fleet_steer`: put work on a fleet by id through the steer admission

**The handler composes `Services` as the tenant steer route does** — no `Plane` field and no new crate edge. In order:

1. `Leasing::standing` proves the lease and fencing token and yields the sender's fleet, workspace and event.
2. The key is computed: `operation_id` `<sender_event_id>:<hex of SHA-256 of the text, first 16 bytes>`, scoped by `Steer` to `<target_fleet_id>:<operation_id>`.
3. A target equal to the sender is refused `UZ-AGT-018`.
4. `FleetSteering::replayed` answers a repeat with the first event, whatever the target's status now, as the person's route does.
5. `Fleets::ingress_status` refuses a fleet outside the workspace or killed (404 `UZ-AGT-009`) and one not taking work (409 `UZ-AGT-012`, `current_state` `installing`, `stopped` or `paused`).
6. `Leasing::sender_hop` reads the sender admission's `request_json` by fleet and id: a `fleet:` actor's body carries `hop`, and every other actor counts as hop 0. A new hop above `HOP_MAX` is refused `UZ-AGT-019`, `current_state` `hop_limit`.
7. `Leasing::count_steer` takes one slot: `UPDATE runner.leases SET posted_steer_count = posted_steer_count + 1 WHERE … AND posted_steer_count < STEERS_PER_RUN_MAX`. No row → `UZ-RUN-021`, `current_state` `at_capacity`.
8. `Leasing::masked` masks the sender fleet's stored secret values in the text; the runner already masked minted tokens (`egress::masked`), as the `message` verb does at both ends.
9. `FleetSteering::append_from_fleet` admits with actor `fleet:<sender_fleet_id>`, `event_type` `chat`, and the `FleetSteerBody` as `request_json`. That body holds `message`, `sender_fleet_id`, `sender_fleet_name`, `sender_event_id` and `hop`. The reply is `Reply::Inherit` of the sender's event.
10. On a fresh admission the handler calls `Steer::announce`, so the target's dashboard shows the row before a runner takes it.

`insert_admission.sql` joins the inherited row on `workspace_id = $3` instead of `fleet_id = $2`, so a fleet in the same workspace inherits a destination. A gate continuation stays within one fleet, which is inside one workspace, so it still inherits. **Implementation default:** the key hashes the text, because the control plane never retries a verb and only the model does. A model that resends the same words to the same fleet in one event gets the first run back. A reclaimed lease resending them replays too. Different words are new work, inside the cap. **Implementation default:** `HOP_MAX` is 2 and `STEERS_PER_RUN_MAX` is 8, in their own column, because A→B→C covers every crew in the scenario and a shared counter would let `message` spend the steer budget. **Implementation default:** the text bound is `STEER_MESSAGE_MAX_BYTES` (8192), the bound a person's steer has. **Implementation default:** concurrent identical calls may each spend a slot before one admits; the cap is a flood bound, not a count of admissions.

`bracket.rs` and `history.rs` read `message` for a `fleet:` actor through `FleetSteerBody`, so the target's received frame and its chat prior turns carry the words. `is_steer_actor` stays false for `fleet:`. The history filter's `actor_prefix` is a `LIKE` prefix, so `fleet` (the assistant's actor) also matches `fleet:<id>`; a caller wanting fleet steers alone filters on `fleet:`.

- **Dimension 2.1** — once per text, under the fleet actor → Test `fleet_steer_admits_once_under_the_fleet_actor` (integration): 100 concurrent calls with one text to one target → one `fleet.admissions` row; actor `fleet:<sender>`; `event_type` `chat`; `request_json` equals the expected `FleetSteerBody` with hop 1; every caller reads the same `event_id`.
- **Dimension 2.2** — the answer lands where the sender was asked → Test `fleet_steer_copies_the_sender_reply_destination` (integration): sender event with a Slack destination → target admission carries the same `reply_provider` and `reply_address`; sender with none → target carries none; the gate-continuation reruns in `afd_admission/src/tests/reply.rs` and `integration_reply_destination.rs` still pass.
- **Dimension 2.3** — a chain stops at two hops → Test `fleet_steer_refuses_the_third_hop` (integration): a person steers A (hop 0); A steers B (hop 1); B steers C (hop 2); C steers D → 409 `UZ-AGT-019` `current_state` `hop_limit`; D has no admission; one `runner_fleet_steer_refused` warn with `error_code` and `hop`.
- **Dimension 2.4** — the wrong target is refused with nothing written → Test `fleet_steer_refuses_self_other_workspace_and_stopped_targets` (integration): the sender's own id → 409 `UZ-AGT-018`; a fleet in another workspace → 404 `UZ-AGT-009`; a killed fleet → 404 `UZ-AGT-009`; a stopped fleet → 409 `UZ-AGT-012` `current_state` `stopped`; zero rows and zero slots spent after each, one warn each.
- **Dimension 2.5** — the ninth is refused, and `message` does not spend it → Test `fleet_steer_refuses_the_ninth_in_a_run` (integration): three `message` posts, then eight steers with distinct texts admitted, then a ninth → 409 `UZ-RUN-021` `current_state` `at_capacity`; eight rows.
- **Dimension 2.6** — a fleet's steer is not a person's → Test `fleet_steer_is_not_a_person_steer_to_onboarding_and_filters` (integration): after a fleet steer, onboarding's first-steer read stays false and `actor_prefix=steer:` returns no row for it.
- **Dimension 2.7** — a superseded holder writes nothing → Test `fleet_steer_refuses_a_stale_fencing_token` (integration): a token below the fleet's live sequence → 409 `UZ-RUN-005` `current_state` `superseded`, zero rows, zero slots.
- **Dimension 2.8** — the body is bounded → Test `fleet_steer_request_refuses_bad_bodies` (unit): 8193 bytes, a NUL, and an unknown field `policy` → the `garde` sentence for the first two and the `deny_unknown_fields` refusal for the third.
- **Dimension 2.9** — a repeat to a target that stopped since is answered → Test `fleet_steer_replays_to_a_stopped_target` (integration): admit, stop the target, resend → 202 with the first `event_id`, `replayed` true.
- **Dimension 2.10** — the target reads the words → Test `fleet_steer_body_reaches_the_target_turn` (integration): lease the target's event → the received frame and the chat lease's prior turn carry `message`.
- **Dimension 2.11** — the stored text is masked → Test `fleet_steer_masks_secret_values` (integration): text carrying a value of the sender's stored secret → the stored `message` holds the mask, not the value.

### §3 — The published surface

Once all three routes exist, the implementer regenerates `public/openapi.json` from `agentsfleetd openapi` (built with `--features openapi`). The three codes carry two-to-five-word titles and resolve under `https://docs.agentsfleet.net/errors/` (`.orly/LOCAL.md`). The docs repository's API reference and tools page follow on their own branch after merge.

- **Dimension 3.1** — the routes are registered → Test `runner_routes_snapshot_lists_the_three_fleet_routes` (unit): `afd_wire/tests/routes.rs` snapshots the three new paths, and `route_meta_total` counts three more through `RunnerRoute::ALL`.
- **Dimension 3.2** — the codes round-trip → Test `registry_round_trips_the_three_new_codes` (unit): `UZ-AGT-018`, `UZ-AGT-019` and `UZ-RUN-021` parse back to their constants and each has a `problem/fleet.rs` entry.

### §4 — The chat names the sender, and the crew runs live

`event-summary.ts` checks the `fleet:` prefix before its `colon > 0` fallthrough. A `fleet:` event is a user-side turn: its body is the `message` and its label is "asked by <sender_fleet_name>" from the stored body. Layout, fonts and the status pill do not change. The live proof is the chief-of-staff crew on `api-dev`, run by Indy.

- **Dimension 4.1** — the label → Test `event_summary_labels_a_fleet_steer_with_the_sender_name` (unit): actor `fleet:<id>` with `sender_fleet_name` `ci-dev-responder` → role user, label "asked by ci-dev-responder", body the message; a body without the name → "asked by a fleet"; actor `steer:<user>` and `fleet` unchanged.
- **Dimension 4.2** — the row renders → Test `fleet_thread_renders_a_fleet_steer_row` (e2e): `tests/e2e/fleet-steer-row.spec.ts` in the dry lane renders a thread with one fleet-steered event and asserts the label, the body and an unchanged status pill.
- **Dimension 4.3** — the crew works end to end → Test `chief_of_staff_steers_the_repairer_and_reads_its_answer` (manual): Indy installs `chief-of-staff` from its fixture on `api-dev` and asks it to have `ci-dev-repairer` act on a Linkwarden run. The repairer's draft PR link appears; `fleet_activity` on the next turn reads the repairer's answer; the chief's chat shows the summary. Evidence: event ids, the PR link and a screenshot in the ledger.

## Interfaces

Runner plane, authenticated as the runner, the lease named in the path.

```text
POST /v1/runners/me/leases/{lease_id}/steers
  { "fencing_token": 7, "fleet_id": "<uuid7>", "text": "retry run #991 against dev" }
  202 { "event_id": "<uuid7>", "replayed": false }
  400 body bounds · 404 UZ-AGT-009 (no such fleet here) · 404 UZ-RUN-006 (no such lease)
  409 UZ-AGT-012 current_state installing|stopped|paused · 409 UZ-AGT-018 (self)
  409 UZ-AGT-019 current_state hop_limit · 409 UZ-RUN-021 current_state at_capacity
  409 UZ-RUN-005 current_state superseded (stale fencing token)

GET /v1/runners/me/leases/{lease_id}/fleets?fencing_token=&starting_after=&limit=
  200 { "items": [ { "fleet_id", "name", "status", "last_event_at", "last_status" } ], "total", "next_cursor" }

GET /v1/runners/me/leases/{lease_id}/fleets/{fleet_id}/events?fencing_token=&starting_after=&limit=
  200 { "items": [ { "event_id", "actor", "event_type", "status", "created_at", "response_excerpt", "failure_label" } ], "total", "next_cursor" }

Stored request_json of a fleet steer (FleetSteerBody):
  { "message": "...", "sender_fleet_id": "<uuid7>", "sender_fleet_name": "ci-dev-responder",
    "sender_event_id": "<uuid7>", "hop": 1 }
```

Tools, as the model sees them: `fleet_steer { fleet_id, text }` and `fleet_activity { fleet_id?, limit? }`. Optional response fields are omitted when absent. The tool maps the three new codes to their own hints in `Unanswered::tool_code`, naming the `fleet_id` it sent; the daemon's `detail` names no fleet (HTTP-LEAK).

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Stale holder | security | a reclaimed lease's runner steers or reads | 409 `UZ-RUN-005` `current_state` `superseded`; nothing read, no row, no slot | `fleet_steer_refuses_a_stale_fencing_token` |
| No lease | security | settled or foreign lease id | 404 `UZ-RUN-006` | `fleets_read_refuses_a_lease_the_runner_does_not_hold` |
| Cross-workspace or killed target | security | fleet id from another workspace, or killed | 404 `UZ-AGT-009`, `detail` names nothing | `fleet_steer_refuses_self_other_workspace_and_stopped_targets` |
| Self target | failure | sender steers itself | 409 `UZ-AGT-018`; no row | `fleet_steer_refuses_self_other_workspace_and_stopped_targets` |
| Target not taking work | failure | installing, stopped or paused | 409 `UZ-AGT-012` with `current_state`; no row | `fleet_steer_refuses_self_other_workspace_and_stopped_targets` |
| Target stops after admission | race | stop lands between admit and lease | the admission stands; the target's lease path handles it as it handles a person's steer | `fleet_steer_replays_to_a_stopped_target` |
| Loop | capacity | A→B→C→D | 409 `UZ-AGT-019`; D has no admission | `fleet_steer_refuses_the_third_hop` |
| Flood | capacity | ninth steer in one lease | 409 `UZ-RUN-021` | `fleet_steer_refuses_the_ninth_in_a_run` |
| Lost acknowledgement | replay | the model resends after a timeout | same text → same key → first `event_id`, `replayed` true | `fleet_steer_admits_once_under_the_fleet_actor` |
| Same key, other words | replay | not reachable: the key hashes the words | — | `fleet_steer_admits_once_under_the_fleet_actor` |
| Malformed body | failure | over-long or NUL text, unknown field | 400; the tool schema refused the same | `fleet_steer_request_refuses_bad_bodies` |
| Oversized answer read | capacity | a fleet's 1 MiB answer | excerpt cut at `EXCERPT_MAX_BYTES` | `fleets_read_excerpt_is_bounded` |
| Target out of budget | failure | target's wallet or ceiling spent | the steer admits; the target's lease path refuses as for a person's steer; the sender reads it with `fleet_activity` | the existing `afd_fleet` budget lane tests |

Backpressure answers `UZ-API-001` as every route does. The admission commits before the 202, so a crash after commit is a replay on retry, never a second run.

## Invariants

1. A steer never widens authority — the target's lease reads policy from the target's pinned revision, and `FleetSteerRequest` is `deny_unknown_fields` with no policy field; Dimension 2.8 refuses `policy`, Dimension 2.1 asserts the stored body.
2. One admission per `(producer, producer_key)` — `uq_admissions_producer_producer_key` and the repeat path in `afd_events::Steer`; Dimension 2.1 with 100 callers.
3. A chain never passes hop 2 — `Leasing::sender_hop` reads the hop from the sender's own admission, not from the request; Dimension 2.3.
4. A lease spends at most 8 steer slots — the guarded `UPDATE … WHERE posted_steer_count < 8`; Dimension 2.5.
5. The runner holds no tenant credential — `LeaseVerbs` carries the lease id and fencing token only, and the routes authenticate `RunnerIdentity`; Dimensions 1.3 and 2.7.
6. `fleet:` is never `steer:` — one constant prefix each, and `is_steer_actor` is false for `fleet:`; Dimension 2.6.
7. An excerpt never exceeds `EXCERPT_MAX_BYTES` — enforced where the row is built; Dimension 1.4.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `runner_fleet_steer_refused` (warn) | operator | a steer is refused | `error_code`, `lease_id`, `sender_fleet_id`, `target_fleet_id`, `hop` | no text; no workspace id (AUTH-LOGID) | `fleet_steer_refuses_the_third_hop`, `fleet_steer_refuses_self_other_workspace_and_stopped_targets` |
| `runner_fleet_steer_admitted` (info) | operator | a steer is admitted or replayed | `event_id`, `sender_fleet_id`, `target_fleet_id`, `hop`, `replayed` | no text | `fleet_steer_admits_once_under_the_fleet_actor` |

No product analytics event changes.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A fleet lists its workspace's fleets and one fleet's events, and nothing beyond the workspace (§1) | `make test-integration-rustd` | exit 0; Dimensions 1.1 to 1.3 pass | P0 | |
| R2 | A fleet steer admits once under `fleet:<id>`, answers in the sender's thread and reaches the target's turn (§2) | `make test-integration-rustd` | exit 0; Dimensions 2.1, 2.2 and 2.9 to 2.11 pass | P0 | |
| R3 | Self, cross-workspace, killed, stopped, third-hop, ninth and stale-token steers write nothing (§2) | `make test-integration-rustd` | exit 0; Dimensions 2.3 to 2.7 pass | P0 | |
| R4 | Three routes, three codes, OpenAPI regenerated (§3) | `make lint-all` | exit 0 | P0 | |
| R5 | The chat labels a fleet-steered row with the sender's name (§4) | `make dry-app` | exit 0 | P1 | |
| R6 | The chief of staff steers the repairer and reads its answer on `api-dev` (§4) | Manual: Dimension 4.3; evidence in `playbooks/operations/acceptance/baselines/M239_001-<sha>.md` | draft PR link in the thread; answer read back; summary in the chief's chat | P1 | |
| S1 | Declared lint | `make lint-all` | exit 0 | P0 | |
| S2 | Declared unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Declared version | `make check-version` | exit 0 | P0 | |
| S5 | Required Pull Request gate | `orly gate pr` | exit 0 at the pushed head | P0 | |
| S6 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

### Behavior evals

- **Grounding rule:** a digest or status answer states only fleets, statuses and excerpts present in that run's `fleet_activity` output.
- **Golden set:** 6 cases in `tests/fixtures/fleets/fleet-to-fleet/cases.json`: quiet workspace, one stopped fleet, one failed run, a fleet asked by name that does not exist, a steer refused as stopped, a steer that replayed. The worst known case is the missing fleet.
- **Ship threshold:** grounding holds in every case and at least 5 of 6 pass, graded by hand in the R6 ledger, as the `ci-investigation` cases are.
- **Fallback:** the fleet names what it could not read ("docs-bot: no such fleet") instead of guessing. Fabricated status fails R6.

## Dead Code Sweep

N/A: no files deleted.

## Out of Scope

- Waking the sender when the target answers: the sender reads with `fleet_activity`; a durable wait is M236's.
- Named outcomes, subscriptions or a new trigger type: not planned; a steer reaches every fleet.
- A fleet posting to a channel other than the thread it was asked from: not planned here.
- Steering a fleet in another workspace: not planned.
- A CLI change: none; the CLI already steers and reads events.

---

## Product Clarity (authoring record)

1. **Successful user moment** — In `#ci-dev` the repairer's draft PR link appears in the thread with no person typing "open the fix"; in the chief of staff's chat, "docs-bot is out of budget" appears with no model call on docs-bot.
2. **Preserved user behavior** — Human steers, Slack mentions, schedules and webhooks are untouched; onboarding and the history filter still mean a person by `steer:`.
3. **Optimal-way check** — Two tools over the lease plane the fleet already speaks; the gap is only the code.
4. **Rebuild-vs-iterate** — Iterate: no new producer, trigger type, queue or service.
5. **What we build** — Two tools, three routes, one actor prefix, a hop count, one counter column, a workspace-wide destination copy, one chat label.
6. **What we do NOT build** — Outcome subscriptions (routing nobody asked for); wake-on-answer (M236 owns waits); channel posting (no destination source exists); allowlists (the `tools` list is the consent).
7. **Fit with existing features** — Compounds with schedules (a digest), shared memory and the Slack crew; must not destabilize the human steer, which shares `Steer::append`.
8. **Surface order** — Runner tools first; the user surfaces are the setup chat's `tools` list and the fleet chat. No CLI work.
9. **Dashboard restraint** — Only the sender label on the event row. No crew view until two fleets talk in production.
10. **Confused-user next step** — The refusal the tool reads ("fleet <id> is not taking work: UZ-AGT-012") and the tools page in the docs.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 read first (no write risk, ships the digest alone), §2 steer, §3 surface, §4 label and live proof.
- **Alternatives considered:** a named-outcome subscription with a `type: fleet` trigger — rejected: directed asks need no routing, and it adds a trigger type and a vocabulary; a fleet resolving another fleet's approval gate — rejected: covers review only; waiting for M236 — rejected: no wait is needed; `http_request` against the tenant API with an API key in secrets — rejected: Indy ruled out API keys, and a credential would sit in model reach.
- **Patch-vs-refactor verdict:** a **patch** because every piece extends a verb, route or statement that exists.

## Discovery (consult log)

- **Consults** — `origin/main` at `1920ff345` (Nov 04, 2026): highest milestone M238, highest schema slot 2180; M236 (pending) proposes 2190, so this spec takes 2200. Report outcome is binary (`afd_wire/src/report/mod.rs`), so a fleet must name its ask in text, not in an outcome field. `insert_admission.sql` joins an inherited destination on the target fleet, so inheriting across fleets is one join change; `Steer::append` hardcodes `Reply::None`, so the fleet steer needs its own entry point. `steer:%` is read by onboarding and the history filter, so a fleet's actor is `fleet:`. The `message` verb caps 8 per run; the steer reuses the cap.
- **Model reviews** — Elephant-Goldfish Model (EGM) implementation-readiness Goldfish (Claude Opus 5.5, Nov 04, 2026): 7 blockers, 16 majors, 8 nits. Fixed: every blocker, 15 majors and 7 nits, by the Section rewrite above. Declined: M11's error-URL form, because `.orly/LOCAL.md` names `https://docs.agentsfleet.net/errors/`. Declined: N4, because the OpenAPI drift check is inside `make lint-all`. EGM codebase-details Goldfish (same model and date): 4 blockers, 13 majors, 9 nits; the blockers duplicated readiness B1, B1, B5 and B2, and all majors and nits were fixed (409 not 412 for `UZ-RUN-005`, route-count files, catalog selection, masking, OpenAPI command, `SteerAccepted` reuse, the `LIKE` overlap). EGM style Goldfish (same): 1 blocker, 46 majors, 23 nits; the scenario page's findings were fixed, the spec's British spellings and EGM expansion were fixed, and its long Dimension sentences were kept because `docs.language` exempts spec records. Agent choices the reviews forced, recorded for Indy: the text-hash key, a separate counter column, the handler composing `Services`, and `sender_fleet_name` frozen in the stored body.
- **Open blockers** — none.
- **Deferrals and transfers** — none.

> Owner (Indy, Nov 04, 2026, time not recorded): "I want to brainstorm the usecases for fleet to fleet trigger and communication."
> Owner (Indy, Nov 04, 2026, time not recorded): "we need to find the use case first"
> Owner (Indy, Nov 04, 2026, time not recorded): "Yes additionaly, lets just say 1. I have a chief of staff that manages all the fleets? and announces me updates, gets command from me. 2. An incident-responder responds to slack, and calls a fleet saying yes now fix this? incident-repairer, and communicates to a incident-communicator 3. An personaly orly agent that wakes and summarizes what happened so far with the agents? who is doing what 5. A chief of staff says and does ask someone saying yo are you awake." — context: the four scenarios; the "Yes" accepts the responder-to-repairer handoff without a person relaying.
> Owner (Indy, Nov 04, 2026, time not recorded): "Well be adversarial since in my mind its just steer to a fleet id. and notify essentially is in the chat informing the user. thats it so its fleet_activity?" — context: the shape; subscriptions, outcomes and a communicator fleet dropped.
> Owner (Indy, Nov 04, 2026, time not recorded): "my view is fleet to fleet" — context: the name and the go.
> Owner (Indy, Nov 04, 2026, time not recorded): "Yes write it and push the PR so i can take a pull and ask Codex to review this. Once you write it review with egm for completeness in implementation" — context: authorises the docs-only Pull Request and the EGM review.
