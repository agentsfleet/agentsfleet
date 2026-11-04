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

**Design flow:** model calls a tool → `LeaseVerbs` → runner route proves the lease and fencing token → daemon checks workspace, target status, hop and per-run count → `afd_events::Steer::append` admits once → the target leases under its own pinned revision → the sender reads the answer with `fleet_activity` on a later turn.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(fleets): let a fleet steer another fleet and read its activity
- **Intent (one sentence):** A fleet given two tools can hand work to a named fleet and read what the workspace's fleets did, so a person no longer relays between them and a chief-of-staff fleet can exist.
- **Authoring handshake:** Indy asked for fleet-to-fleet trigger and communication, then narrowed it: "its just steer to a fleet id. and notify essentially is in the chat informing the user. thats it so its fleet_activity?". ASSUMPTIONS I'M MAKING: 1. Owner consent is the fleet's `tools` list in its reviewed revision; no allowlist, rule or subscription. 2. A fleet with `fleet_steer` may steer any active fleet in its workspace. 3. The sender learns the answer by reading, never by being woken. 4. Notify is the fleet's own answer in its own chat; `message` and `pushover` already exist.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/afr_tools/src/verbs/message/mod.rs` and `rustd/crates/afr_tools/src/verbs/mod.rs` — a tool over `LeaseVerbs` whose refusal codes the model reads; mirror both new tools on it.
2. `rustd/crates/afd_api_runner/src/handler/runner/message.rs` — a lease-proven runner route: fencing token, bounds in `afd_wire`, refusal codes with `current_state`.
3. `rustd/crates/afd_events/src/steer/mod.rs` — the steer admission: `<fleet_id>:<operation_id>` key, repeat answered not re-run, actor prefix.
4. `rustd/crates/afd_admission/src/admission.rs` and `rustd/crates/afd_admission/src/sql/select_reply_destination.sql` — `Reply::Inherit` and the same-fleet filter this spec widens to the workspace.
5. `docs/architecture/scenarios/fleet-to-fleet.md` — the shape, what it reuses and the bounds.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M239_001_P1_API_DOCS_INFRA_UI_FLEET_TO_FLEET_STEER_AND_ACTIVITY.md` | CREATE, then MOVE to `docs/v2/done/` | this spec |
| `docs/architecture/scenarios/fleet-to-fleet.md`; `docs/architecture/scenarios/README.md` | CREATE / EDIT | the scenario and its index row (authoring Pull Request) |
| `docs/architecture/data_flow.md`; `docs/architecture/README.md` | EDIT | the `fleet:` actor in §"B. TRIGGER" and a glossary row (authoring Pull Request) |
| `docs/architecture/runner_execution.md` | EDIT | the two tools in §"Tool catalog" |
| `rustd/crates/afd_wire/src/steer_verb/` | CREATE | `FleetSteerRequest`, `FleetSteered`, the text bound, `STEERS_PER_RUN_MAX`, `HOP_MAX` |
| `rustd/crates/afd_wire/src/fleets_verb/` | CREATE | lease-plane rows for the fleet list and a fleet's events, `EXCERPT_MAX_BYTES` |
| `rustd/crates/afr_tools/src/catalog/mod.rs`; `rustd/crates/afr_tools/src/verbs/mod.rs` | EDIT | two catalog entries; `LeaseVerbs` gains `fleet_steer` and `fleet_activity` |
| `rustd/crates/afr_tools/src/verbs/fleet_steer/`; `rustd/crates/afr_tools/src/verbs/fleet_activity/` | CREATE | the two tools and their tests |
| `rustd/crates/afr_supervisor/src/verbs/mod.rs` | EDIT | the control-plane `LeaseVerbs` implementation and the per-run steer sequence |
| `rustd/crates/afd_api_runner/src/handler/runner/fleet_steer.rs`; `.../fleet_activity.rs`; `.../mod.rs` | CREATE / EDIT | the three routes' handlers |
| `rustd/crates/afd_http/src/route/runner.rs` | EDIT | three route variants in the runner family |
| `rustd/crates/afd_api/tests/route_inventory.rs`; `rustd/crates/afd_api/tests/route_meta_total.rs` | EDIT | inventory rows and counts |
| `rustd/crates/afd_fleet/src/lease/steer/`; `rustd/crates/afd_fleet/src/lease/fleets/` | CREATE | daemon half: lease proof, target checks, hop, per-run count, admission; workspace reads for the lease plane |
| `rustd/crates/afd_events/src/steer/mod.rs` | EDIT | the `fleet:` actor constructor and the request body with sender and hop |
| `rustd/crates/afd_admission/src/sql/select_reply_destination.sql`; `rustd/crates/afd_admission/src/admit.rs` | EDIT | the inherited destination is read by workspace and event id |
| `rustd/crates/afd_core/src/error_code/fleet.rs`; the `REGISTRY` rows; `rustd/crates/afd_core/src/error_code/problem/` | EDIT / CREATE | three codes: self target, hop limit, steer cap |
| `public/openapi.json` | EDIT | regenerated with `--no-banner` |
| `ui/packages/app/lib/events/event-summary.ts`; `ui/packages/app/lib/events/sender-names.ts`; their tests | EDIT | the `fleet:` actor arm, "asked by <fleet name>" |
| `ui/packages/app/tests/e2e/` | EDIT | one dry-lane spec rendering a fleet-steered row |
| `tests/fixtures/fleets/fleet-to-fleet/cases.json` | CREATE | the behaviour-eval cases |
| `playbooks/operations/acceptance/baselines/M239_001-<sha>.md` | CREATE | the manual crew evidence ledger |
| `~/Projects/docs`: API reference for the three runner routes, the tools page, the changelog entry | EDIT | on its own branch after this repository's Pull Request merges |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE, RS-NEWTYPE, RS-BORROW, RS-RACE
- **`domains/http.md`** — HTTP-NOUN, HTTP-NEST, HTTP-PARAM, HTTP-ROUTE, HTTP-METHOD, HTTP-IDEMKEY, HTTP-GARDE, HTTP-VALID, HTTP-SENTENCE
- **`domains/http-responses.md`** — HTTP-LIST, HTTP-PAGE, HTTP-NULL, HTTP-TIME, HTTP-STATUS, HTTP-STATEEXT, HTTP-REGISTRY, HTTP-NEWCODE, HTTP-LEAK
- **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-ANNOTATE, HTTP-BREAK, HTTP-STABILITY, HTTP-SURFACE, HTTP-TESTS
- **`domains/auth.md`** — AUTH-GUARD, WAUTH, AWO, CTX, AUTH-NOLOG, AUTH-LOGID
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-CODE, LOG-SECRET, OBS
- **`domains/architecture.md`** — ARCH-GROUND, ARCH-LAND, ARCH-HOMES, ARCH-PUBLISHED
- **`code.md`** — PRI, NTP, EMS, CODE-ERRREG, FLL
- **`tests.md`** — TEST-OUTCOME, TEST-ERROR, TEST-NEG, TEST-INJECT, TEST-RESIDUAL, TEST-ACKLOSS, TEST-CONTEND, TEST-REAL, TEST-LANE
- **`languages/typescript.md`** — TS-SCHEMA, TS-ANY, TS-TESTFILE
- **`docs.md`** — DOC-02, DOC-03, DOC-05, DOC-06

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` (clippy, OpenAPI drift, route registration) | yes, three routes and a regenerated document | annotate every route; regenerate `public/openapi.json` in the same commit |
| `make test-unit-all` | yes, new wire types, tools and the UI arm | unit tests beside each; vitest for the label |
| `make test-integration-rustd` | yes, every daemon Dimension | `#[ignore]` tests in the integration lane only |
| `orly gate work` (conform at commit) | yes | whole-file commits |
| gitleaks | no secrets in the diff | fixtures use the fixture actors already in the repository |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, `afd_fleet/src/lease/steer/` | split checks, admission and tests into their own files, as `lease/message/` does |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afr_tools/src/verbs/message/mod.rs` → `rustd/crates/afd_api_runner/src/handler/runner/message.rs` → `rustd/crates/afd_fleet/src/lease/message/` — one verb end to end. Divergence: a steer admits work instead of posting a line, so its daemon half ends in `afd_events::Steer::append`, not in `afd_outbound`.
- **Reference:** `rustd/crates/afd_api_runner/src/handler/runner/memory.rs` — a lease-plane read that refuses a runner holding no live lease; `fleet_activity` proves the lease the same way.
- **Reference:** `rustd/crates/afd_approval/src/inbox/resolve/mod.rs` — `Reply::Inherit` on a continuation; the steer inherits the same way across fleets.

## Sections (implementation slices)

### §1 — `fleet_activity`: read the workspace's fleets over the lease plane

Two reads behind the lease: the workspace's fleets with status, last event instant and last outcome, and one fleet's recent events with a bounded excerpt of each answer. Both reuse the reads the tenant routes already run (`rustd/crates/afd_api_tenant/src/handler/fleet/mod.rs` list; `rustd/crates/afd_events/src/history/mod.rs`), scoped to the lease's workspace. The tool is offered only to a fleet whose revision declares it, through the catalog admission every tool already has. **Implementation default:** a `GET` proves the lease by lease id and runner identity, as the memory read does, because a read has no fencing token to echo and a stale reader does no harm. **Implementation default:** `EXCERPT_MAX_BYTES` is 1024, cut on a UTF-8 boundary, because an excerpt is for a digest and the full answer is one `fleet_activity` call on the event away.

- **Dimension 1.1** — the list names every fleet in the lease's workspace → Test `fleet_activity_lists_workspace_fleets_for_a_live_lease` (integration): three fleets, one stopped → three items with status, `last_event_at` and `last_outcome` from `fleet.events`, `total` 3, no fleet from another workspace.
- **Dimension 1.2** — a fleet outside the workspace is unknown → Test `fleet_activity_refuses_a_fleet_from_another_workspace` (integration): events of a fleet in workspace B through a lease in A → 404 `UZ-AGT-009`, `detail` names no fleet, no id.
- **Dimension 1.3** — no live lease, no read → Test `fleet_activity_refuses_a_lease_the_runner_does_not_hold` (integration): a settled lease id, and another runner's lease id → 404 `UZ-RUN-006` on both.
- **Dimension 1.4** — the excerpt is bounded → Test `fleet_activity_excerpt_is_bounded` (unit): a 10 KiB answer with a multibyte character at byte 1024 → `response_excerpt` of at most 1024 bytes ending on a character boundary.
- **Dimension 1.5** — undeclared, unoffered → Test `fleet_activity_is_offered_only_when_declared` (unit): a policy without the entry → the call reads the catalog refusal, no verb reached.

### §2 — `fleet_steer`: put work on a fleet by id through the steer admission

The runner sends the target `fleet_id`, the text and a per-run `sequence` the supervisor numbers, so a retried call repeats it. The daemon proves the lease and fencing token, loads the sender's event (fleet, workspace, hop, reply destination), and refuses in this order: a target that is the sender; a target not in the workspace (404, as a person gets); a target not taking work (409 `UZ-AGT-012`, the ingress check the tenant route runs); a hop that would reach 3; a ninth steer in the run. Then it calls `afd_events::Steer::append` with actor `fleet:<sender_fleet_id>`, `operation_id` `<sender_event_id>:<sequence>`, a request body carrying text, sender fleet, sender event and hop, and `Reply::Inherit` of the sender's event. The destination statement reads by workspace and event id so a fleet in the same workspace inherits it. **Implementation default:** `HOP_MAX` is 2 and `STEERS_PER_RUN_MAX` is 8, because A→B→C covers every crew in the scenario and 8 is the cap `message` already proves. **Implementation default:** the per-run count rides the mechanism `lease/message/` uses for `MESSAGES_PER_RUN_MAX`, because two counters on one lease row would need two fences.

- **Dimension 2.1** — once per sequence, under the fleet actor → Test `fleet_steer_admits_once_under_the_fleet_actor` (integration): 100 concurrent calls with one sequence → one `fleet.admissions` row, actor `fleet:<sender>`, `event_type` `chat`, request body holds text, sender ids and hop 1 and nothing else; every caller reads the same `event_id`, 99 with `replayed` true.
- **Dimension 2.2** — the answer lands where the sender was asked → Test `fleet_steer_copies_the_sender_reply_destination` (integration): sender event with a Slack destination → target admission carries the same `reply_provider` and `reply_address`; sender event with none → target carries none.
- **Dimension 2.3** — a chain stops at two hops → Test `fleet_steer_refuses_the_third_hop` (integration): A steers B (hop 1), B steers C (hop 2), C steers D → 409 with the hop-limit code and `current_state` `hop_limit`; D has no admission; the warn record carries the code and the hop.
- **Dimension 2.4** — the wrong target is refused with nothing written → Test `fleet_steer_refuses_self_other_workspace_and_stopped_targets` (integration): the sender's own id → 409 self code; a fleet in another workspace → 404 `UZ-AGT-009`; a stopped fleet → 409 `UZ-AGT-012` `current_state` `stopped`; a fleet deleted between the check and the insert → 404 through `afd_db::constraint::violates_foreign_key`; zero rows after each.
- **Dimension 2.5** — the ninth is refused → Test `fleet_steer_refuses_the_ninth_in_a_run` (integration): eight admitted, the ninth → 409 steer-cap code, `current_state` `at_capacity`, eight rows.
- **Dimension 2.6** — a fleet's steer is not a person's → Test `fleet_steer_is_not_a_person_steer_to_onboarding_and_filters` (integration): after a fleet steer, onboarding's "first steer" read stays false and `actor_prefix=steer:` returns no row for it.
- **Dimension 2.7** — a superseded holder writes nothing → Test `fleet_steer_refuses_a_stale_fencing_token` (integration): a token below the fleet's live sequence → 412 `UZ-RUN-005`, zero rows.
- **Dimension 2.8** — the text is bounded at both ends → Test `fleet_steer_refuses_text_over_the_bound` (unit): 4097 bytes, and a NUL → the `garde` refusal sentence; the same constant the tool's schema states.

### §3 — Codes, routes and the published surface

Three codes land as a family constant, a `REGISTRY` row and a `problem/` entry in one commit (`rustd/crates/afd_core/src/error_code/fleet.rs`), numbered from the next free `UZ-RUN-` values on freshly fetched main. Three route variants join the runner family with their inventory rows and counts. `public/openapi.json` is regenerated with `--no-banner`. The docs repository's API reference and tools page follow on their own branch after merge.

- **Dimension 3.1** — the routes are registered → Test `route_inventory_lists_the_three_runner_routes` (unit): the existing inventory and total tests updated; a route without an annotation fails `make lint-all`.
- **Dimension 3.2** — the codes round-trip → Test `registry_round_trips_the_three_new_codes` (unit): each code parses back to its constant, its title has two to five words, its problem type resolves under `https://docs.agentsfleet.net/errors/`.
- **Dimension 3.3** — the document matches the server → Test `openapi_matches_the_served_routes` (lint): `make lint-all` exit 0 after regeneration; a hand edit to `public/openapi.json` fails it.

### §4 — The chat names the sender, and the crew runs live

The event row for a `fleet:` actor reads "asked by <fleet name>", the name resolved from the workspace fleet list the thread page already holds and the id shown when the fleet is gone. Layout, fonts and the status pill do not change. The live proof is the chief-of-staff and responder-to-repairer crew on `api-dev`, run by Indy and recorded in the evidence ledger.

- **Dimension 4.1** — the label → Test `event_summary_labels_a_fleet_steer_with_the_sender_name` (unit): actor `fleet:<id>` with the id in the list → "asked by ci-dev-responder"; id not in the list → "asked by fleet <id>"; actor `steer:<user>` unchanged.
- **Dimension 4.2** — the row renders → Test `fleet_thread_renders_a_fleet_steer_row` (e2e): the dry-lane Playwright spec renders a thread with one fleet-steered event and asserts the label and an unchanged status pill.
- **Dimension 4.3** — the crew works end to end → Test `chief_of_staff_steers_the_repairer_and_reads_its_answer` (manual): Indy, on `api-dev`, steers the chief of staff to have `ci-dev-repairer` act on a Linkwarden run; the repairer's draft Pull Request (PR) link appears in the originating thread; `fleet_activity` on the next turn reads the repairer's answer; the chief's chat shows the summary. Evidence: event ids, the PR link and the chat screenshot in `playbooks/operations/acceptance/baselines/M239_001-<sha>.md`.

## Interfaces

Runner plane, authenticated as the runner, the lease named in the path.

```text
POST /v1/runners/me/leases/{lease_id}/steers
  { "fencing_token": 7, "fleet_id": "<uuid7>", "text": "retry run #991 against dev", "sequence": 1 }
  202 { "event_id": "<uuid7>", "replayed": false }
  404 UZ-AGT-009 (no such fleet in this workspace) · 404 UZ-RUN-006 (no such lease)
  409 UZ-AGT-012 current_state stopped|paused · 409 <self> · 409 <hop_limit> · 409 <at_capacity>
  412 UZ-RUN-005 (stale fencing token)

GET /v1/runners/me/leases/{lease_id}/fleets?limit=
  200 { "items": [ { "fleet_id", "name", "status", "last_event_at", "last_outcome", "queued" } ], "total", "next_cursor" }

GET /v1/runners/me/leases/{lease_id}/fleets/{fleet_id}/events?starting_after=&limit=
  200 { "items": [ { "event_id", "actor", "event_type", "status", "created_at", "response_excerpt", "failure_reason" } ], "total", "next_cursor" }
```

Tools, as the model sees them: `fleet_steer { fleet_id, text }` and `fleet_activity { fleet_id?, limit? }`. The supervisor adds `sequence`. Optional response fields are omitted when absent. Constants live in `afd_wire` so the tool schema and the route refuse the same requests.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Stale holder | security | a reclaimed lease's runner steers | 412 `UZ-RUN-005`; no row; the model reads a refusal naming the code | `fleet_steer_refuses_a_stale_fencing_token` |
| No lease | security | settled or foreign lease id on a read or a steer | 404 `UZ-RUN-006`; nothing read or written | `fleet_activity_refuses_a_lease_the_runner_does_not_hold` |
| Cross-workspace target | security | fleet id from another workspace | 404 `UZ-AGT-009`, `detail` names nothing | `fleet_steer_refuses_self_other_workspace_and_stopped_targets` |
| Self target | failure | sender steers itself | 409 self code; no row | `fleet_steer_refuses_self_other_workspace_and_stopped_targets` |
| Target not taking work | failure | stopped or paused fleet | 409 `UZ-AGT-012` with `current_state`; no row; the sender says so in its answer | `fleet_steer_refuses_self_other_workspace_and_stopped_targets` |
| Target deleted mid-flight | race | fleet killed between check and insert | FK violation read as 404; no row | `fleet_steer_refuses_self_other_workspace_and_stopped_targets` |
| Loop | capacity | A→B→C→D | 409 hop-limit code, `current_state` `hop_limit`; D has no admission | `fleet_steer_refuses_the_third_hop` |
| Flood | capacity | ninth steer in one run | 409 steer-cap code, `current_state` `at_capacity` | `fleet_steer_refuses_the_ninth_in_a_run` |
| Lost acknowledgement | replay | runner retries after a timeout | same `sequence` → same `event_id`, `replayed` true, one run | `fleet_steer_admits_once_under_the_fleet_actor` |
| Malformed body | failure | over-long or NUL text, unknown field | 400 with the `garde` sentence; the tool schema refused the same | `fleet_steer_refuses_text_over_the_bound` |
| Oversized answer read | capacity | a fleet's 1 MiB answer | excerpt cut at `EXCERPT_MAX_BYTES` | `fleet_activity_excerpt_is_bounded` |
| Target out of budget | failure | target's wallet or ceiling spent | the steer admits; the target's lease path refuses as it does a person's steer today, and the sender reads that outcome with `fleet_activity` | existing `afd_fleet` budget lane tests |

Backpressure answers the registry's `UZ-API-001` as every route does. The admission commits before the 202, so a crash after commit is a replay on retry, never a second run.

## Invariants

1. A steer never widens authority — the target's lease reads policy from the target's pinned revision, and `FleetSteerRequest` is `deny_unknown_fields` with no policy field; Dimension 2.1 asserts the admitted body.
2. One admission per `(producer, producer_key)` — `uq_admissions_producer_producer_key` and the repeat path in `afd_events::Steer`; Dimension 2.1 with 100 callers.
3. A chain never passes hop 2 — the daemon reads the sender's hop from its own event row and refuses before the insert; Dimension 2.3.
4. The runner holds no tenant credential — `LeaseVerbs` carries the lease id and fencing token only, and the routes authenticate `RunnerIdentity`; Dimensions 1.3 and 2.7.
5. `fleet:` is never `steer:` — one constant prefix, and `is_steer_actor` is false for it; Dimension 2.6.
6. An excerpt never exceeds `EXCERPT_MAX_BYTES` — enforced where the row is built, on a character boundary; Dimension 1.4.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `runner_fleet_steer_refused` (warn) | operator | a steer is refused | `error_code`, `lease_id`, `sender_fleet_id`, `target_fleet_id`, `hop` | no text, no workspace id | `fleet_steer_refuses_the_third_hop` |
| `runner_fleet_steer_admitted` (info) | operator | a steer is admitted or replayed | `event_id`, `sender_fleet_id`, `target_fleet_id`, `hop`, `replayed` | no text | `fleet_steer_admits_once_under_the_fleet_actor` |

No product analytics event changes. Admitted steers are already visible as admissions and events.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A fleet lists its workspace's fleets and one fleet's events, and nothing beyond the workspace (§1) | `make test-integration-rustd` | exit 0; the five §1 tests pass | P0 | |
| R2 | A fleet steer admits once under `fleet:<id>` and answers in the sender's thread (§2) | `make test-integration-rustd` | exit 0; Dimensions 2.1 and 2.2 pass | P0 | |
| R3 | Self, cross-workspace, stopped, third-hop, ninth and stale-token steers write nothing (§2) | `make test-integration-rustd` | exit 0; Dimensions 2.3 to 2.7 pass | P0 | |
| R4 | Three routes registered, three codes in the registry, OpenAPI regenerated (§3) | `make lint-all` | exit 0 | P0 | |
| R5 | The chat labels a fleet-steered row with the sender's name (§4) | `make dry-app` | exit 0 | P1 | |
| R6 | The chief of staff steers the repairer and reads its answer on `api-dev` (§4) | Manual: Dimension 4.3's procedure; evidence in `playbooks/operations/acceptance/baselines/M239_001-<sha>.md` | draft PR link in the thread; answer read back; summary in the chief's chat | P1 | |
| S1 | Declared lint | `make lint-all` | exit 0 | P0 | |
| S2 | Declared unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Declared version | `make check-version` | exit 0 | P0 | |
| S5 | Required Pull Request gate | `orly gate pr` | exit 0 at the pushed head | P0 | |
| S6 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

### Behaviour evals

- **Grounding rule:** a digest or status answer states only fleets, statuses and excerpts present in the `fleet_activity` output of that run.
- **Golden set:** 6 cases in `tests/fixtures/fleets/fleet-to-fleet/cases.json`: quiet workspace, one stopped fleet, one failed run, a fleet asked by name that does not exist, a steer refused as stopped, a steer that replayed. The worst known case is the missing fleet.
- **Ship threshold:** grounding holds in every case and at least 5 of 6 pass, graded in the R6 ledger.
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
2. **Preserved user behaviour** — Human steers, Slack mentions, schedules and webhooks are untouched; onboarding and the history filter still mean a person by `steer:`.
3. **Optimal-way check** — Two tools over the lease plane the fleet already speaks; the gap is only the code.
4. **Rebuild-vs-iterate** — Iterate: no new producer, trigger type, queue or service.
5. **What we build** — Two tools, three routes, one actor prefix, a hop count, a workspace-wide destination copy, one chat label.
6. **What we do NOT build** — Outcome subscriptions (routing nobody asked for); wake-on-answer (M236 owns waits); channel posting (no destination source exists); allowlists (the `tools` list is the consent).
7. **Fit with existing features** — Compounds with schedules (a digest), shared memory and the Slack crew; must not destabilise the human steer, which shares `Steer::append`.
8. **Surface order** — Runner tools first; the user surfaces are the setup chat's `tools` list and the fleet chat. No CLI work.
9. **Dashboard restraint** — Only the sender label on the event row. No crew view until two fleets talk in production.
10. **Confused-user next step** — The refusal the tool reads ("ci-dev-repairer is stopped, UZ-AGT-012") and the tools page in the docs.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** §1 read first (no write risk, ships the digest alone), §2 steer, §3 surface, §4 label and live proof.
- **Alternatives considered:** a named-outcome subscription with a `type: fleet` trigger — rejected: directed asks need no routing, and it adds a trigger type and a vocabulary; a fleet resolving another fleet's approval gate — rejected: covers review only; waiting for M236 — rejected: no wait is needed; `http_request` against the tenant API with an API key in secrets — rejected: Indy ruled out API keys, and a credential would sit in model reach.
- **Patch-vs-refactor verdict:** a **patch** because every piece extends a verb, route or statement that exists.

## Discovery (consult log)

- **Consults** — `origin/main` at `1920ff345` (Nov 04, 2026): highest milestone M238, no schema slot needed. Report outcome is binary (`afd_wire/src/report/mod.rs`), so a fleet must name its ask in text, not in an outcome field. `select_reply_destination.sql` filters by fleet, so inheriting across fleets is one statement change. `steer:%` is read by onboarding and the history filter, so a fleet's actor is `fleet:`. The `message` verb caps 8 per run; the steer reuses the cap.
- **Model reviews** — EGM implementation-readiness review: pending; recorded here with model, finding counts and dispositions when it runs.
- **Open blockers** — none.
- **Deferrals and transfers** — none.

> Owner (Indy, Nov 04, 2026, time not recorded): "I want to brainstorm the usecases for fleet to fleet trigger and communication."
> Owner (Indy, Nov 04, 2026, time not recorded): "we need to find the use case first"
> Owner (Indy, Nov 04, 2026, time not recorded): "Yes additionaly, lets just say 1. I have a chief of staff that manages all the fleets? and announces me updates, gets command from me. 2. An incident-responder responds to slack, and calls a fleet saying yes now fix this? incident-repairer, and communicates to a incident-communicator 3. An personaly orly agent that wakes and summarizes what happened so far with the agents? who is doing what 5. A chief of staff says and does ask someone saying yo are you awake." — context: the four scenarios; the "Yes" accepts the responder-to-repairer handoff without a person relaying.
> Owner (Indy, Nov 04, 2026, time not recorded): "Well be adversarial since in my mind its just steer to a fleet id. and notify essentially is in the chat informing the user. thats it so its fleet_activity?" — context: the shape; subscriptions, outcomes and a communicator fleet dropped.
> Owner (Indy, Nov 04, 2026, time not recorded): "my view is fleet to fleet" — context: the name and the go.
> Owner (Indy, Nov 04, 2026, time not recorded): "Yes write it and push the PR so i can take a pull and ask Codex to review this. Once you write it review with egm for completeness in implementation" — context: authorises the docs-only Pull Request and the EGM review.
