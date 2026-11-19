<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

# M252_001: A person reaches a fleet by name, and a fleet steers a fleet and hears back

**Prototype:** v2.0.0

**Milestone:** M252

**Workstream:** 001

**Date:** Nov 19, 2026

**Status:** IN_PROGRESS

**Priority:** P1, a person relays work between fleets and copies UUIDs to talk to them, and a chief-of-staff fleet cannot exist

**Categories:** API, CLI, DOCS, INFRA, UI

**Batch:** B1, one Workstream with no parallel group

**Depends on:** none

**Provenance:** agent-drafted from Indy's brainstorm (Nov 04, 2026) and written against `main` at `055aef441` after his review (Claude Fable 5.2, Nov 19, 2026). His words are quoted in Discovery.

**Canonical architecture:** `docs/architecture/scenarios/chief-of-staff.md`; `docs/architecture/data_flow.md` §"B. TRIGGER"

---

## Overview

**Goal (testable):** `a_person_reaches_a_fleet_by_name_and_a_fleet_steers_a_fleet_hears_back_and_reads_activity`

**Problem:** A person is the wire between fleets, and must copy a fleet's UUID to message it from the command line. A fleet cannot hand work to another fleet, learn its status or read its last answer. Slack matches a fleet by a different name than the sidebar shows.

**Solution summary:** Two things, on one name. A person reaches a fleet by its saved name: `@name` in the dashboard chat and the command line, and the same name after the bot's mention in Slack. A fleet reaches a fleet with two runner tools: `fleet_steer` puts work on any other fleet in its workspace through the admission a person's message already uses, and `fleet_activity` lists the workspace's fleets and one fleet's recent events. The sender does not wait; the receiver is told who asked and reports back with the same tool. The fleet's `tools` list is the only consent, and nothing limits the targets, the chain or the count.

**Design flow, a person:** `@agentbob retry` → the surface's parser splits name and words → `GET /v1/workspaces/{workspace_id}/fleets?name=agentbob` returns zero or one fleet → the existing message route. **A fleet:** model calls `fleet_steer` → runner route → the handler proves the lease, refuses a steer to itself, reads the sender's name and masks the text → one transaction proves the lease's write guard and runs the admission statement, which decides target exists in the workspace, repeat, target status and the deployment ceiling → commit, wake, announce → the target runs under its own revision and reads who asked.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(fleets): reach a fleet by name, and let a fleet steer a fleet and hear back
- **Intent (one sentence):** Nobody copies a UUID or relays between fleets: a person names a fleet on any surface, and a fleet given two tools hands work to another fleet, is told when it is done and reads what the workspace's fleets did.
- **Authoring handshake:** Indy asked for fleet-to-fleet steering ("its just steer to a fleet id", "send and move on", "boby informs when ready", "no limits") and for `@agentbob` in the chat and the command line, with Slack keeping `@agentsfleetdev bob: do this`. ASSUMPTIONS I'M MAKING: 1. Owner consent is the fleet's `tools` list in its reviewed revision: a fleet that holds `fleet_steer` may steer any other fleet in its workspace. 2. The scope is the sender's workspace; account or tenant is out of scope. 3. No list of targets, no chain limit, no per-run cap and no configuration key. Each fleet's budget and the deployment's admission ceiling are the bounds. 4. The sender is woken only by a steer back; there is no wait primitive. 5. A fleet's steer runs as its own run on the target and never folds into a live one. 6. `@name` from a person is a direct address: the message goes to the named fleet as that person's own, and the fleet whose chat was open never sees it. 7. The name is the fleet's saved name (`fleet.fleets.name`), on every surface; Slack switches to it from the definition's name. 8. The live test uses the `runner-link-watch` teaser fleet, after merge.
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `rustd/crates/daemon/afd_api_runner/src/handler/runner/schedule.rs` and `rustd/crates/daemon/afd_cron/src/store/guarded.rs` — a lease-proven write: the handler proves `standing`, masks the text, builds `write_fence`, and the service proves that guard on the write's own transaction (`afd_db::Precondition`). The steer copies this shape. `handler/runner/memory.rs` renders a stale token as 409 `superseded`.
2. `rustd/crates/daemon/afd_admission/src/sql/admit_one.sql`, `src/admit.rs`, `src/stage/mod.rs` and `src/stage/verdict/` — the one admission statement, `admit_runnable`, the refusal order, and how the estimate read, the insert and the ceiling count are ordered around a transaction.
3. `rustd/crates/daemon/afd_events/src/steer/mod.rs`, `steer/admission.rs` and `rustd/crates/daemon/afd_api_tenant/src/handler/fleet/message_steer/mod.rs` — the person's steer: the scoped key, the repeat answer, `announce`, and the refusal mapping.
4. `rustd/crates/runner/afr_tools/src/verbs/message/mod.rs`, `rustd/crates/runner/afr_supervisor/src/client/lease_verbs/mod.rs` and `rustd/crates/daemon/afd_api_runner/src/handler/runner/message.rs` — one verb end to end: tool, `LeaseVerbs`, supervisor client, runner route.
5. `rustd/crates/daemon/afd_api_tenant/src/handler/fleet/mod.rs` and `rustd/crates/daemon/afd_fleet_lifecycle/src/read.rs` — the fleet list route and its store.
6. `cli/src/program/tree/fleet.command.ts`, `cli/src/program/tree/flags.ts` and `cli/src/commands/fleet/steer.ts` — the steer command and its id argument.
7. `ui/packages/app/components/domain/conversation/delivery/useFleetMessageDelivery.ts` (the hook is `useMessageDelivery`), `delivery/useFleetPendingSends.ts` and `conversation/thread/FleetThread.tsx` — the one place the dashboard sends a message.
8. `rustd/crates/daemon/afd_ingress/src/slack/mod.rs` (`subscribed`) and `rustd/crates/daemon/afd_ingress/src/sql/select_mention_candidates.sql` — where a Slack subscriber gets the name the router matches.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/active/M252_001_P1_API_CLI_DOCS_INFRA_UI_REACH_FLEETS_BY_NAME_AND_STEER.md` | EDIT, then MOVE to `docs/v2/done/` | this spec |
| `docs/architecture/scenarios/chief-of-staff.md`; `docs/architecture/scenarios/README.md`; `docs/architecture/data_flow.md`; `docs/architecture/README.md` | EDIT (spec commit) | the one scenario page and its pointers; "proposed" removed at close |
| `docs/architecture/runner_execution.md`; `docs/architecture/scenarios/slack-incident-responder.md`; `docs/architecture/api_resources.md` | EDIT (with the code) | the two tools in §"Tool catalog"; the Slack routing name; the list route's `name` filter |
| `tests/fixtures/fleet-address/cases.json`; `tests/fixtures/fleets/chief-of-staff/definition.json`; `tests/fixtures/fleets/runner-link-watch/definition.json`; `tests/fixtures/fleets/fleet-to-fleet/cases.json` | CREATE (spec commit) | the address case table; the two fleets of the live test; the hand-graded eval cases. No test reads them by enumeration |
| `rustd/crates/protocol/af_wire/src/steer_verb/`; `src/fleets_verb/`; `src/lib.rs`; `src/paths.rs`; `src/event/steer.rs`; `src/event/mod.rs` | CREATE / EDIT | request, reply and row types; `EXCERPT_MAX_BYTES`; three path constants; `FleetSteerBody` and the one function that composes its sender line |
| `rustd/crates/runner/afr_tools/src/catalog/mod.rs`; `catalog/tests.rs`; `verbs/mod.rs`; `testing.rs`; `src/verbs/fleet_steer/`; `src/verbs/fleets/` | EDIT / CREATE | two catalog entries (`PUBLISHED` 39 → 41, `Runtime::Supervisor`); two `LeaseVerbs` methods with default bodies, as `task_inputs` has, so no other implementor changes; the recording fake; the two tools and their tests |
| `rustd/crates/runner/afr_supervisor/src/client/lease_verbs/mod.rs`; `client/lease_verbs/tests.rs`; `client/verb.rs`; `src/verbs/mod.rs` | EDIT | the control-plane calls, three `Verb` variants, the fenced `LeaseVerbs` implementation |
| `rustd/crates/daemon/afd_api_runner/src/handler/runner/fleet_steer.rs`; `handler/runner/fleets.rs`; `handler/runner/mod.rs`; `src/lib.rs`; `src/openapi.rs`; `rustd/crates/daemon/afd_http/src/route/runner.rs` | CREATE / EDIT | three handlers, router arms, OpenAPI registration, three `RunnerRoute` variants |
| `rustd/crates/daemon/afd_http/src/services/leasing/mod.rs`; `services/leasing/lease_service.rs`; `services/event.rs`; `services/fleets.rs` | EDIT | `Leasing::sender`, answering a `SteerSender` (name and masked text); `FleetSteering::append_from_fleet`; `WorkspaceFleets::named` |
| `rustd/crates/daemon/afd_fleet/src/lease/message/mod.rs`; `src/lease/bracket.rs`; `src/task/admit/source.rs` | EDIT | `LeaseService::sender`, sharing `masked`'s one `installed` read; a `fleet:` event's received frame carries its message; one unit test beside `check_human` |
| `rustd/crates/daemon/afd_admission/src/admit.rs`; `src/lib.rs`; `src/admission.rs`; `src/sql/admit_one.sql`; `tests/receipts/guarded.rs`; `tests/receipts/mod.rs`; `tests/receipts/lookups.rs` | EDIT / CREATE | `Admissions::admit_runnable_guarded` and the exported `Unheld`; the inherited reply joins on `workspace_id`, with the comments that say "same fleet" corrected; the guard's tests |
| `rustd/crates/daemon/afd_events/src/steer/mod.rs`; `steer/admission.rs`; `src/lib.rs` | EDIT | `FLEET_ACTOR_PREFIX`, `fleet_actor`, `is_fleet_actor`; the fleet steer's admission and key builder; `Steer::append_from_fleet` with its `FleetSteer` input and `FleetSteered` answer. No error kind is added |
| `rustd/crates/foundation/af_core/src/error_code/fleet.rs`; `error_code/mod.rs`; `src/problem/fleet.rs` | EDIT | `UZ-AGT-018`: constant, `REGISTRY` row, problem entry; the comment at `fleet.rs:254-255` that also reserves `UZ-AGT-019` is corrected |
| `rustd/crates/daemon/afd_api_tenant/src/handler/fleet/mod.rs`; `rustd/crates/daemon/afd_fleet_lifecycle/src/read.rs`; `src/sql/select_fleet_by_name.sql`; `src/sql/mod.rs`; `tests/integration_fleet_page.rs`; `tests/support/seed.rs`; `rustd/crates/daemon/afd_api/tests/fleets/input.rs` | EDIT / CREATE | the `name` query and its bound; the store's `named` and its one keyed statement; a seed that takes a name and a status; the refused queries. `page` and its callers are untouched |
| `rustd/crates/daemon/afd_ingress/src/slack/mod.rs`; `slack/tests.rs`; `src/sql/select_mention_candidates.sql`; `rustd/crates/daemon/afd_api/tests/integration_slack_mention/fixture.rs`; `integration_slack_mention/addressed.rs` | EDIT | `Subscriber.name` read from the fleet row, which changes `subscribed` and its test calls; a fixture fleet saved under a name that differs from its definition's. `slack/route.rs` is untouched |
| `rustd/crates/daemon/main/tests/integration_runner_fleets.rs`; `integration_runner_steers.rs`; `integration_runner_steers_fence.rs`; `integration_fleet_list_name.rs`; `tests/daemon_suite.rs`; `tests/support/e2e_seed.rs` | CREATE / EDIT | the integration Dimensions; each new file is a `#[path] mod` in `daemon_suite.rs` (the crate sets `autotests = false`); a helper that seeds a named, active fleet in the scenario's workspace, tagged with `scenario_tag(&run.fleet)` so the scenario's runner is offered its work, as `integration_runner_stream/fairness.rs` does |
| `rustd/crates/daemon/afd_api/tests/harness/stubs_runner.rs`; `rustd/crates/daemon/afd_api/tests/route_meta_total.rs`; `rustd/crates/protocol/af_wire/tests/routes.rs`; `public/openapi.json` | EDIT | the `Leasing` stub; a `FLEET_TO_FLEET_ADDITIONS` term of 3 in `RUST_ROUTE_COUNT`; three path rows; the regenerated OpenAPI document |
| `cli/src/lib/fleet-address.ts`; `cli/src/program/tree/flags.ts`; `cli/src/program/tree/fleet.command.ts`; `cli/src/commands/fleet/steer.ts`; `cli/test/fleet-address.unit.test.ts`; `cli/test/fleet-steer-address.integration.test.ts`; `cli/test/fleet-steer-address.subprocess.test.ts` | CREATE / EDIT | the parser; an argument that accepts a fleet id or `@name`; the lookup before the stream opens; the case table, the in-process run and the built binary against the mock API, as `cli/test/runners.subprocess.test.ts` runs it |
| `ui/packages/app/lib/chat-address.ts`; `lib/chat-address.test.ts`; `lib/api/fleet-by-name.ts`; `lib/api/fleet-by-name.test.ts`; `lib/api/setup-proxy/path.ts`; `ui/packages/app/tests/setup-proxy.test.ts` | CREATE / EDIT | the parser; the lookup client; `name` allowed on the proxied list route only |
| `ui/packages/app/components/domain/conversation/delivery/useFleetMessageDelivery.ts`; `delivery/useFleetMessageDelivery.test.tsx`; `conversation/thread/FleetThread.tsx`; `conversation/composer/useFleetUploadPreviews.ts` | EDIT | an addressed send resolves, posts to the named fleet and opens its chat; the context gains the open fleet's name and a way to open a fleet |
| `ui/packages/app/lib/events/event-summary.ts`; `lib/events/event-summary.test.ts`; `lib/events/sender-names.ts`; `lib/events/sender-names.test.ts` | EDIT | a `fleet:` row is a user-side turn labelled "Another fleet" |
| `ui/packages/app/tests/e2e/fleet-address.spec.ts`; `tests/e2e/fleet-steer-row.spec.ts`; `tests/e2e/fixtures/chat-address-probe.tsx`; `tests/e2e/fixtures/chat-browser-probe.tsx` | CREATE / EDIT | the two dry-lane scenarios and the probe that mounts the real delivery hook |
| `playbooks/operations/acceptance/baselines/M252_001-<sha>.md` | CREATE (after merge) | the live test's evidence ledger |
| `~/Projects/docs`: the runner routes, the tools page, the `fleet steer` command page, the fleets list reference, the Slack page, the changelog entry | EDIT | on its own branch after this repository's Pull Request merges |

## Applicable Rules

- **`languages/rust.md`** — ERR-RS, RS-FROM, RS-MAPERR, RS-SOURCE, RS-NEWTYPE, RS-BORROW, RS-RACE
- **`domains/http.md`** — HTTP-NOUN, HTTP-NEST, HTTP-PARAM, HTTP-ROUTE, HTTP-METHOD, HTTP-GARDE, HTTP-VALID, HTTP-SENTENCE
- **`domains/http-responses.md`** — HTTP-LIST, HTTP-PAGE, HTTP-NULL, HTTP-TIME, HTTP-STATUS, HTTP-STATEEXT, HTTP-REGISTRY, HTTP-NEWCODE, HTTP-TITLE, HTTP-LEAK
- **`domains/http-surface.md`** — HTTP-OPENAPI, HTTP-ANNOTATE, HTTP-BREAK, HTTP-STABILITY, HTTP-SURFACE, HTTP-TESTS
- **`domains/auth.md`** — AUTH-GUARD, WAUTH, AWO, CTX, AUTH-NOLOG, AUTH-LOGID
- **`domains/sql.md`** — SQL-FILE, SQL-ROW, SQL-PREPARE, SQL-REPO, CNX, TXN, NSQ
- **`domains/logging.md`** — LOG-EVENT, LOG-FIELDS, LOG-CODE, LOG-SECRET, OBS
- **`domains/architecture.md`** — ARCH-GROUND, ARCH-LAND, ARCH-HOMES, ARCH-PUBLISHED
- **`code.md`** — PRI, NTP, EMS, CODE-ERRREG, CODE-SEAM, FLL
- **`tests.md`** — TEST-OUTCOME, TEST-ERROR, TEST-NEG, TEST-INJECT, TEST-RESIDUAL, TEST-CONTEND, TEST-REAL, TEST-LANE
- **`languages/typescript.md`** — TS-SCHEMA, TS-ANY, TS-TESTFILE
- **`docs.md`** — DOC-02, DOC-03, DOC-05, DOC-06

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `make lint-all` | yes | clippy and the TypeScript lints over the new code |
| `make test-unit-all` | yes | the Rust workspace and the command-line and app coverage floors; it runs the OpenAPI checks in `rustd/crates/daemon/afd_api/tests/openapi_contract.rs`, so `public/openapi.json` is regenerated in the commit that adds a route |
| `make test-integration-rustd` | yes | daemon Dimensions are `#[ignore]`d tests under `tests/`, the only kind that lane runs |
| `make dry-app` | yes, not in the Pull Request gate | run locally; a green Pull Request does not prove Playwright (`.orly/LOCAL.md`) |
| `orly gate work` | yes | whole-file commits |
| gitleaks | no secrets in the diff | fixtures name credentials, never hold them |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes, the steer handler | checks, body, service call and refusal mapping in separate functions, as `schedule.rs` splits them |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/daemon/afd_api_runner/src/handler/runner/schedule.rs` (`create`) with `rustd/crates/daemon/afd_cron/src/store/guarded.rs` — standing, masking, `write_fence`, and a service that proves the guard on its own transaction. Divergence: the guarded write is an admission.
- **Reference:** `rustd/crates/daemon/afd_ingress/src/slack/message.rs` and `rustd/crates/daemon/afd_api_ingress/src/handler/mention/admit.rs` — a producer composes the words the model reads, and stores the person's own words beside them.
- **Reference:** `tests/fixtures/retry-policy/cases.json`, read by `cli/test/transport-replay.unit.test.ts` and `ui/packages/app/lib/api/retry.fixture.test.ts` — one case table two TypeScript packages read.
- **Reference:** `cli/src/commands/secret/secret.ts` (`findSecretByName`) and `cli/src/commands/fleet/read.ts` (`decodeFleetPage`) — a name looked up before a command acts. The app's lookup reuses `FleetListSchema` (`ui/packages/app/lib/api/fleets.ts`).
- **Command-line practices (SPEC-PRIOR):** command, handler and error layers stay apart; the handler neither prints nor exits; `Output` is the one rendering layer; the unknown-name refusal is a `ValidationError` with a suggestion; tests cover the handler, the in-process run and the built binary; output stays JSON when stdout is not a terminal.

## Sections (implementation slices)

Each Section registers its own routes, codes and path constants, so its Dimensions run through the real route. Build order: §1, §2, §3, then §4 and §5 in either order, then §6 and §7.

### §1 — `fleet_activity`: read the workspace's fleets over the lease plane

Two reads behind the lease, with no new statement. Both prove the lease through `Leasing::standing` with the `?fencing_token=` query, and render a stale token as 409 `UZ-RUN-005` with `current_state` `superseded`, the way `memory.rs` does. The first lists the workspace's fleets through the existing `WorkspaceFleets::page`, newest first, with the tenant list's paging (default 20, maximum 100) and its own narrower row: id, name, status, description and events processed. Archived and installing fleets are listed with their status. The second reads one fleet's events through the existing `WorkspaceEvents::thread_for_fleet`, which returns bodies and the messages still waiting for a runner. Before it reads, the handler asks `WorkspaceFleets::ingress_status` for the lease's workspace, and `None` is 404 `UZ-AGT-009`. Each row's `message` (through `af_wire::event::message_of`) and `response_text` are cut to an excerpt in the handler; limit default 20, maximum 25, the thread read's own cap. The tool's module is `verbs/fleets/`, because "activity" already names the receipts module and the live-tail verb; it returns the route's JSON body unchanged. **Implementation default:** `EXCERPT_MAX_BYTES` is 1024, cut with `str::floor_char_boundary` as `lease/history.rs` does. **Implementation default:** the list carries no last-event fields; a fleet's latest run is one events read with `limit=1`.

- **Dimension 1.1** — the list names every fleet in the lease's workspace → Test `fleets_read_lists_workspace_fleets_for_a_live_lease` (integration): three fleets in workspace A, one archived and one installing, one fleet in workspace B → three items; the archived one reads `status` `archived`; no item from B; `total` is null. A fleet with one finished event and one waiting message reads two event items, the waiting one `queued` with no `response_excerpt`.
- **Dimension 1.2** — a fleet outside the workspace is unknown → Test `fleets_read_refuses_a_fleet_from_another_workspace` (integration): events of a fleet in workspace B through a lease in A → 404 `UZ-AGT-009`, `detail` names no fleet; a path segment that is not a UUID → 400.
- **Dimension 1.3** — no live lease, no read → Test `fleets_read_refuses_a_lease_the_runner_does_not_hold` (integration): a settled lease id → 404 `UZ-RUN-006`; another runner's lease id → 404 `UZ-RUN-006`; a stale `fencing_token` → 409 `UZ-RUN-005` whose body carries `current_state` `superseded`.
- **Dimension 1.4** — the excerpt is bounded → Test `fleets_read_excerpt_is_bounded` (unit): a 10 KiB answer with a three-byte character spanning byte 1024 → an excerpt of 1022 bytes ending before that character.
- **Dimension 1.5** — undeclared, unoffered → Test `fleet_tools_are_offered_only_when_declared` (unit): `Catalog::select` over a `tools` list without the two names offers neither, and over a list with them offers both; `PUBLISHED` holds 41 entries and `every_published_handler_entry_is_hosted` passes.

### §2 — `fleet_steer`: put work on a fleet by id through the steer admission

The handler composes `Services` as the schedule create handler does, with no new crate edge and no schema change. The admission is a person's in every field but four: actor `fleet:<sender_fleet_id>`, a `FleetSteerBody`, `Reply::Inherit` of the sender's event, and a key the daemon derives. It keeps `Producer::Steer`, so every reader that treats a steer specially already treats it correctly, and the readers that mean a person also test the `steer:` actor, which `fleet:` fails. In order:

1. The handler reads and validates the body, then calls `Leasing::standing` through `Refusal::conflict_or_at`, as `memory.rs` does, so a stale token names `superseded`. The `standing` helper in `schedule.rs` is not reused, because it names no `current_state`.
2. A target equal to the sender is refused 409 `UZ-AGT-018`, `current_state` `self`, before any further read.
3. `Leasing::sender` makes the one `installed` read `masked` already makes and answers the sender's name (`Installed.name`) and the text with the sender fleet's stored secret values masked. A sender that is no longer installed is 404 `UZ-RUN-006`, as `message` answers it.
4. The handler composes the `FleetSteerBody`: `text` (the masked words), `sender_fleet_id`, `sender_fleet_name`, `sender_event_id`, and `message`: the sender line, a newline, then `text`. One function in `af_wire` composes `message`, so the model, the chat and the history read the same words. The sender line carries the sender's id, so the receiver can answer with the same tool.
5. The handler serializes the body, as the person's handler does (`stored_payload`), builds the lease's `write_fence`, as `schedule.rs` does, and calls `FleetSteering::append_from_fleet` with a `FleetSteer` (the target, the workspace, the sender's fleet and event, the masked text and the serialized body) and the guard.
6. `Steer::append_from_fleet` builds the admission. The operation id is `<sender_event_id>:<the first 32 hex characters of af_core::etag::sha256_hex over the masked text>`, scoped as a person's is to `<target_fleet_id>:<operation_id>`. It then calls `Admissions::admit_runnable_guarded`.
7. `admit_runnable_guarded` works in this order. It mints the row id and reads the deployment estimate before any lock, as `prepare` does. It begins one transaction and asks the guard; a guard that does not hold rolls back with nothing written and answers `Err(Unheld)` inside an `Ok`, the nested shape `afd_cron`'s guarded writes use. Otherwise it runs `admit_one.sql` with the runnable gate on the same connection, and that statement decides, in its own order, fleet absent in this workspace, repeat, not runnable, setup needing review and the deployment ceiling. An `Ok` answer always commits, a repeat included, because the statement's conflict arm writes. After the commit it counts the ceiling for a fresh row and rings the wake, as `admit_runnable` does. `Unheld` is not an error kind, so no refusal is counted or logged for it.
8. `append_from_fleet` answers a repeat as `append` does: the first event with `replayed` true when the stored digest equals this admission's, the operation conflict when it differs, and the key looked up when the ceiling was spent before the statement ran. It returns a `FleetSteered`: the event id, `replayed` and, for a fresh admission, its instant. It computes no disposition and passes `Unheld` through.
9. The handler maps the outcome. `Unheld` is 409 `UZ-RUN-005` (`RUN_STALE_FENCING_TOKEN`), `current_state` `superseded`. Through the accessors `afd_events::Error` already has: fleet absent is 404 `UZ-AGT-009`; not runnable is 409 `UZ-AGT-012` with the target's status; a conflict is 409 `UZ-AGT-016`, `current_state` `admitted`, with the handler's own sentence, "This steer was already sent with different words". Anything else renders by its code, which makes a setup needing review 409 `UZ-AUTHOR-009`. It logs one refusal warn or one admitted info through two small functions, as `schedule.rs` has `log_refused`, and on a fresh admission calls `FleetSteering::announce` with an `EventAdmitted` frame.

`admit_one.sql` joins the inherited row on `earlier.workspace_id = $3` instead of the target fleet, so a fleet in the same workspace inherits a reply destination; a gate continuation stays inside one fleet and still inherits. `bracket.rs` reads `message` for a `fleet:` actor through `FleetSteerBody` and applies no length filter to it, because the daemon composed and stored it. The prompt and the prior turns already read `message` (`af_wire::event::message_of`), so they need no change.

**Implementation default:** the key hashes the masked text, because only the model retries a verb, and a hash taken after masking holds nothing derived from a secret. The same words to the same fleet in one event return the first run; different words are new work. **Implementation default:** the text bound is `STEER_MESSAGE_MAX_BYTES` (8192), a person's bound. **Implementation default:** the guard takes a shared lock on the lease row, as `WriteFence` does for a schedule write, and the statement then locks the target's fleet row; nothing in the transaction updates the lease row. **Implementation default:** a conflict means a person sent that operation id to the target first, or the sender was renamed between the send and its resend, since the stored body holds the sender's name.

- **Dimension 2.1** — once per text, under the fleet actor, to any fleet in the workspace → Test `fleet_steer_admits_once_under_the_fleet_actor` (integration): two active fleets with no relation to each other, the second seeded by the new helper; 20 concurrent calls from one to the other with one text → one `fleet.admissions` row; producer `steer`; actor `fleet:<sender>`; `event_type` `chat`; `request_json` equals the expected `FleetSteerBody`; every caller reads that `event_id`, exactly one of them with `replayed` false; a later resend reads that `event_id` with `replayed` true.
- **Dimension 2.2** — the answer lands where the sender was asked → Test `fleet_steer_copies_the_sender_reply_destination` (integration): sender event with a Slack destination → target admission carries the same `reply_provider` and `reply_address`; sender with none → target carries none; `an_inherited_destination_is_copied_from_the_named_row` (`afd_admission/tests/receipts/lookups.rs`) and `continuation_inherits_the_resumed_destination` (`afd_fleet/tests/integration_reply_destination.rs`) still pass.
- **Dimension 2.3** — the wrong target is refused with nothing written → Test `fleet_steer_refuses_self_other_workspace_and_inactive_targets` (integration): the sender's own id → 409 `UZ-AGT-018` `current_state` `self`; a fleet in another workspace → 404 `UZ-AGT-009`; an archived fleet → 409 `UZ-AGT-012` `current_state` `archived`; an installing fleet → 409 `UZ-AGT-012` `current_state` `installing`; a fleet whose setup needs review → 409 `UZ-AUTHOR-009`; zero admission rows after each.
- **Dimension 2.4** — a fleet's steer is not a person's → Test `fleet_steer_is_not_a_person_steer_to_onboarding_and_filters` (integration): after a fleet steer, onboarding's first-steer read stays false and `actor_prefix=steer:` returns no row for it.
- **Dimension 2.5** — a runner that lost its lease writes nothing → Test `fleet_steer_refuses_a_stale_fencing_token` (integration): a token below the fleet's live sequence → 409 `UZ-RUN-005` `current_state` `superseded`, zero rows. Test `guarded_admission_writes_nothing_when_the_guard_fails` (integration, in `afd_admission/tests/receipts/guarded.rs`): with the lane's fleet set active, as `locking.rs` sets it installing, `admit_runnable_guarded` with a guard that answers false → `Unheld` and zero rows; with one that answers true → the row `admit_runnable` writes. The guard is injected as `afd_api/tests/runner_plane/schedule_guard.rs` injects one.
- **Dimension 2.6** — the body is bounded → Test `fleet_steer_request_refuses_bad_bodies` (unit): 8193 bytes, a NUL, and an unknown field `policy` → the `garde` sentence for the first two and the `deny_unknown_fields` refusal for the third; a `fleet_id` that is not a UUID → 400.
- **Dimension 2.7** — a repeat is answered before the status refusal → Test `fleet_steer_replays_before_status` (integration): admit, resend unchanged → 202 with the first `event_id`, `replayed` true, still one row; archive the target, resend → the same answer.
- **Dimension 2.8** — the target reads the words and who asked → Test `fleet_steer_body_reaches_the_target_turn` (integration): with a short text and with an 8192-byte text, lease the target's event → the received frame, the lease payload's `message_of` and the chat lease's prior turn all read the sender line with the sender's name and id, then the whole text.
- **Dimension 2.9** — the stored text is masked → Test `fleet_steer_masks_secret_values` (integration): text carrying a value of the sender's stored secret → the stored `text` and `message` hold the mask, not the value.
- **Dimension 2.10** — a fleet's steer cannot start a durable task → Test `check_human_refuses_a_fleet_actor` (unit): a source with producer `steer` and actor `fleet:<id>` → `TaskRefusal::InvalidTarget`.
- **Dimension 2.11** — a fleet's steer never joins a live run → Test `fleet_steer_runs_as_its_own_run_on_a_busy_target` (integration): the scenario's runner has capacity 2; the target holds a live chat lease on an event with no reply destination; a fleet steer from a sender event with none is admitted, so both are in one conversation; a person's message sent the same way is claimed by the run's fold, and the fleet's is not; the steer is leased as its own event after the run reports.
- **Dimension 2.12** — a person's colliding operation id is a conflict, never a wrong answer → Test `fleet_steer_refuses_a_key_a_person_took` (integration): a person's message to the target through `Steer::append` with the operation id the fleet's steer will derive, then that steer → 409 `UZ-AGT-016` `current_state` `admitted`; one row, the person's.
- **Dimension 2.13** — the receiver answers, and nothing limits the chain → Test `fleet_steer_chain_has_no_limit_and_the_receiver_answers` (integration): four fleets A, B, C and D in one workspace, run one lease at a time. A's seeded event is the start. A steers B; B steers C and steers A, as the reply; C steers D; D steers A. Every steer is admitted, and each target's event reads the sender line naming the fleet that sent it.
- **Dimension 2.14** — nothing limits the count → Test `fleet_steer_has_no_per_run_limit` (integration): one lease sends 20 steers with distinct texts to one target → 20 admission rows, each a 202 with `replayed` false.
- **Dimension 2.15** — a sender that is no longer installed steers nothing → Test `fleet_steer_refuses_a_sender_that_is_no_longer_installed` (integration): the sender fleet is archived while its lease is live, then it steers → 404 `UZ-RUN-006`, zero rows.
- **Dimension 2.16** — each outcome logs once, with no text → Test `fleet_steer_logs_its_refusal_and_its_admission` (unit, beside the handler): the two log functions, called under `af_core`'s per-thread trace `Capture` and filtered on their event names, emit one `runner_fleet_steer_refused` and one `runner_fleet_steer_admitted`, each with exactly the properties Metrics allows.

### §3 — A fleet's name finds it: the list filter, the address rule and the command line

`GET /v1/workspaces/{workspace_id}/fleets` accepts `name`. Present, the handler ignores paging, asks `WorkspaceFleets::named` and answers the list's usual `{items, total, next_cursor}` body with that one fleet or no item, and both other keys null. `named` is a new method beside `page`, backed by one statement keyed on `(workspace_id, name)`, which `uq_fleets_workspace_id_name` serves. The decoded value is checked with `FleetName::parse` (lower-case ASCII letters, digits and `-`, 1 to 64 bytes) and anything else is 400.

The address rule, for the dashboard and the command line: after leading whitespace the text starts with `@`; the name is the run of ASCII letters, digits and `-` that follows, 1 to 64 bytes, lower-cased; the next character is the end, whitespace, `:` or `,`; that closing character and the whitespace after it are dropped; the rest is the message. Any other text is not an address. `tests/fixtures/fleet-address/cases.json` holds each case as `input`, `name` and `rest`, or as `input` and `not_an_address`. Each surface has one pure function, `cli/src/lib/fleet-address.ts` and `ui/packages/app/lib/chat-address.ts`, and each suite asserts every case against it, so the two cannot drift.

`fleet steer` takes a fleet id or `@name` as its first argument, which keeps the name `fleet_id` in help and errors. A value that does not start with `@` is checked as an id exactly as today. A value that starts with `@` must be an address with an empty rest, or it is refused at parse time with `INVALID_ARGUMENT`. The handler resolves the name through the filter, using `wsFleetsPath` and `decodeFleetPage`, before it opens the event stream; from there it runs exactly as it does for an id. An unknown name is a `ValidationError`: "No fleet named `<name>` in this workspace", suggesting `agentsfleet fleet list`. The message argument is never parsed for an address, and no other command changes.

- **Dimension 3.1** — one request resolves a name → Test `fleet_list_finds_one_fleet_by_exact_name` (integration, through the live router): workspace A holds `agentbob` and `agentbob-2`, workspace B holds `agentbob` → `?name=agentbob` in A returns one item with A's fleet id; `?name=agent` returns none; no `name` returns both.
- **Dimension 3.2** — a malformed name is refused → Test `fleet_list_refuses_a_name_outside_its_bound` (unit, in `afd_api/tests/fleets/input.rs`): `?name=Agent%20Bob`, an empty `name` and a 65-byte name → 400 each, the sentence names `name`, and the store is not reached.
- **Dimension 3.3** — an archived fleet is still found, and the store is workspace-scoped → Test `fleets_named_finds_an_archived_fleet_in_its_own_workspace` (integration, in `integration_fleet_page.rs`): `named` returns an archived fleet with `status` `archived`, and nothing for the same name asked from another workspace.
- **Dimension 3.4** — the parsers agree → Tests `fleet_address_reads_every_shared_case` (unit, command line) and `chat_address_reads_every_shared_case` (unit, app): every case in the file, which holds at least — `@agentbob retry` → `agentbob`, `retry`; `  @AgentBob: retry` → `agentbob`, `retry`; `@agentbob,retry` → `agentbob`, `retry`; `@agentbob` → `agentbob`, empty rest; `@ agentbob retry`, `@agent_bob hi`, `hello @agentbob`, `me@agentbob.dev hi`, an `@` with a 65-byte name → not an address.
- **Dimension 3.5** — a name reaches the fleet → Test `fleet_steer_sends_to_the_fleet_named_with_an_at_sign` (integration): against the mock API, `fleet steer @AgentBob "retry"` → one `GET …/fleets?name=agentbob`, then the stream and the `POST …/fleets/<that id>/messages` with `{"message":"retry"}`; the rendered output equals the run with the id.
- **Dimension 3.6** — an unknown name sends nothing → Test `fleet_steer_refuses_an_unknown_name` (integration): the lookup returns no item → no stream request, no `POST`, a validation error naming `agentbob`, and the exit code a validation error has today.
- **Dimension 3.7** — the built binary does the same → Test `fleet_steer_binary_reaches_a_fleet_by_name` (e2e): the built binary against the mock API with `fleet steer @agentbob "retry"` → the lookup, then the `POST` to that fleet's messages route, exit 0; with `@agent_bob`, `@` alone and an `@` with a 65-byte name → `INVALID_ARGUMENT` and no request.

### §4 — The dashboard chat: an addressed message goes to the named fleet

`useMessageDelivery` checks each delivery against §3's rule. Its context gains two things, both wired in `FleetThread.tsx`: the open fleet's name, and `openFleet(fleetId)`, which pushes the route `/w/<workspaceId>/fleets/<fleetId>`. A message that is not an address is sent as today. A message that addresses the open fleet by its own name is sent to it with the address removed. A message that addresses another fleet goes through the same pending-send ledger, optimistic row and per-fleet queue, then:

1. The ledger entry, the optimistic row and a restored draft keep the text as typed, address included. Every delivery, a Resend included, parses and resolves again, so a resend can never post the words to the open fleet.
2. The name resolves through §3's filter by way of the app's proxy. `lib/api/setup-proxy/path.ts` allows `name` on the fleets list alone, with its own query list; the shared paged list is not changed.
3. The words are posted to the named fleet's messages route with the send's operation id. A 202 settles the ledger entry, discards the optimistic row and calls `openFleet`.
4. An unknown name, or an address with no words after it, ends the send as `PENDING_SEND_STATE.REFUSED` with nothing posted; the second makes no lookup. The composer shows its existing refused notice, and the typed text stays in the row. No new sentence or component is added.
5. A lookup that fails maps as a failed send of that kind does through `outcomeOf`: a 401 is the session state, and a 5xx, an abort or the send's existing 30-second deadline is the unknown state. Nothing is posted.
6. An addressed message with an attachment is refused in `useFleetUploadPreviews` before any upload, because an upload belongs to the open fleet.

- **Dimension 4.1** — an addressed send posts to the named fleet and opens it → Test `delivery_sends_an_addressed_message_to_the_named_fleet` (unit): in fleet A's chat, `@agentbob retry` → one lookup, one `POST` to fleet B's messages URL with `retry` and the send's operation id, the entry settled, the optimistic row discarded, `openFleet` called with B's id; nothing posted to A.
- **Dimension 4.2** — the wrong address sends nothing → Test `delivery_refuses_an_unknown_or_empty_address` (unit): `@nobody retry` with an empty lookup → refused, no `POST`, `openFleet` not called, the ledger entry holds `@nobody retry`; `@agentbob` alone → refused, no lookup; `@agentbob look` with an attachment → refused, no upload.
- **Dimension 4.3** — the open fleet's own name is not a detour → Test `delivery_sends_a_self_address_here` (unit): in `agentbob`'s chat, `@agentbob retry` → no lookup, one `POST` to this fleet with `retry`, `openFleet` not called.
- **Dimension 4.4** — the proxy passes the filter on one route → Test `setup_proxy_allows_name_on_the_fleet_list` (unit, in `tests/setup-proxy.test.ts`): `GET fleets?name=agentbob` matches with `name` kept; `name` on the models list and on a fleet's own route is dropped.
- **Dimension 4.5** — a failed lookup is a failed send → Test `delivery_maps_a_failed_lookup_like_a_failed_send` (unit): a 401 lookup → the session state; a 503 lookup and one that outlasts the deadline → the unknown state; no `POST` in any case.
- **Dimension 4.6** — a resend goes where the address says → Test `delivery_resends_an_addressed_message_to_the_named_fleet` (unit): `@agentbob retry` is refused while the lookup is empty; the lookup then returns fleet B and the entry is resent → one `POST` to B with `retry`, none to A.
- **Dimension 4.7** — the scenario through a browser → Test `chat_sends_an_addressed_message_and_opens_the_named_fleet` (e2e): `tests/e2e/fleet-address.spec.ts` in the dry lane mounts the real `useMessageDelivery` through `chat-address-probe.tsx`, whose `openFleet` records its argument on the page; with the list and the messages route stubbed by `page.route`, typing `@agentbob retry` yields one `POST` to fleet B with `retry` and the recorded id of B.

### §5 — Slack: the same name, in the form Slack already has

Slack's form does not change: `@agentsfleet-dev agentbob: retry run 991`. The bot is the only Slack mention, and the first word, with a trailing `:` or `,` dropped, names the fleet. The one change is the name it is matched against: `Subscriber.name` is read from `fleet.fleets.name`, which `select_mention_candidates.sql` now selects, instead of the definition's name. The router, its verdicts, its notices, the reply destination and the admission are unchanged.

- **Dimension 5.1** — the saved name is the address → Test `mention_routes_by_the_saved_fleet_name` (integration, in `integration_slack_mention/addressed.rs`): one write-bound fleet saved as `ci-dev-repairer` from a definition named `ci-repairer`, attached to the channel → `ci-dev-repairer: open the fix` admits one event on it with message `open the fix`; `ci-repairer open the fix` admits nothing and earns the `AddressIt` notice.
- **Dimension 5.2** — the subscriber carries the saved name → Test `subscribed_names_a_fleet_by_its_saved_name` (unit, in `slack/tests.rs`): a fleet row named `ci-dev-repairer` whose document is named `ci-repairer` → the `Subscriber`'s name is `ci-dev-repairer`.
- **Dimension 5.3** — an unattached fleet stays unreachable → Test `mention_cannot_address_a_fleet_outside_the_channel` (integration): `agentbob` exists in the workspace and does not subscribe to the channel, whose one subscriber is the write-bound `agentmay` → `agentbob: retry` admits nothing on either fleet.

### §6 — The published surface, and the chat shows who asked

Once all routes exist, the implementer regenerates `public/openapi.json` from `agentsfleetd openapi` (built with `--features openapi`). The one new code carries a two-to-five-word title and resolves under `https://docs.agentsfleet.net/errors/`. `Unanswered::detail` already renders any registry code with its problem hint, so the tools add no error variant. In `event-summary.ts`, `roleFor` returns the user role for the `fleet:` prefix, and `senderLabelFor` returns "Another fleet" for it before its `colon > 0` fallthrough. `sender-names.ts` shows that label for a `fleet:` actor, where today it shows one only for a named teammate. The body is the stored `message`, whose first line names the sender, so a row that arrived live and one loaded from history read the same.

- **Dimension 6.1** — the routes are registered → Test `runner_routes_snapshot_lists_the_three_fleet_routes` (unit): `af_wire/tests/routes.rs` lists the three new paths, and `route_meta_total` counts three more through `RunnerRoute::ALL`.
- **Dimension 6.2** — the code round-trips → Test `registry_round_trips_the_self_steer_code` (unit): `UZ-AGT-018` parses back to its constant and has a `problem/fleet.rs` entry with status 409, a title and a hint.
- **Dimension 6.3** — the label → Test `event_summary_labels_a_fleet_steer_as_another_fleet` (unit): actor `fleet:<id>` → role user, label "Another fleet", shown; actor `steer:<user>` and `fleet` unchanged.
- **Dimension 6.4** — the row renders → Test `fleet_thread_renders_a_fleet_steer_row` (e2e): `tests/e2e/fleet-steer-row.spec.ts` in the dry lane renders a thread with one fleet-steered event and asserts the label, the sender line, the text and an unchanged status pill.

### §7 — The teaser runs live

The live test is two fleets on `api-dev`, both from fixtures. `runner-link-watch` is Indy's read-only teaser: it reads Grafana and writes one report, and its fixture adds `fleet_steer` and one rule, to send the report's first lines back to the fleet named in the sender line. `chief-of-staff` holds the two tools and steers only when the owner asks. Indy runs the test after the Pull Request merges and the dev database is rebuilt with M247, because the dev API runs `origin/main`.

- **Dimension 7.1** — the teaser works end to end → Test `chief_of_staff_and_runner_link_watch_talk_on_api_dev` (manual), four steps. One: `agentsfleet fleet steer @runner-link-watch "Hourly runner link check"` prints the report. Two: in the chief's chat, `@runner-link-watch Hourly runner link check` opens the watch's chat with the message waiting. Three: in the chief's chat, "have runner-link-watch run its check and tell me the result" → the chief answers at once; the watch's chat shows a row labelled "Another fleet"; the watch's report wakes the chief, whose second answer carries the decision line. Four: "give me a digest" → the chief lists every fleet with its status and the watch's last result. Evidence: event ids and one screenshot per step in the ledger.

## Interfaces

```text
POST /v1/runners/me/leases/{lease_id}/steers
  { "fencing_token": 7, "fleet_id": "<uuid7>", "text": "run the hourly check" }
  202 { "event_id": "<uuid7>", "replayed": false }
  400 body bounds · 404 UZ-AGT-009 (no such fleet in this workspace) · 404 UZ-RUN-006 (no such lease)
  409 UZ-AGT-012 current_state installing|archived · 409 UZ-AGT-018 current_state self
  409 UZ-RUN-005 current_state superseded · 409 UZ-AGT-016 current_state admitted
  409 UZ-AUTHOR-009 (the target's setup needs review) · 503 the admission ceiling is spent

GET /v1/runners/me/leases/{lease_id}/fleets?fencing_token=&starting_after=&limit=
  200 { "items": [ { "fleet_id", "name", "status", "description", "events_processed" } ],
        "total": null, "next_cursor" }

GET /v1/runners/me/leases/{lease_id}/fleets/{fleet_id}/events?fencing_token=&starting_after=&limit=
  200 { "items": [ { "event_id", "actor", "event_type", "status", "created_at",
                     "request_excerpt", "response_excerpt", "failure_label" } ],
        "total": null, "next_cursor" }

Stored request_json of a fleet steer (FleetSteerBody):
  { "message": "[from fleet chief-of-staff, fleet_id <uuid7>; answer with fleet_steer to that id]\nrun the hourly check",
    "text": "run the hourly check", "sender_fleet_id": "<uuid7>",
    "sender_fleet_name": "chief-of-staff", "sender_event_id": "<uuid7>" }

GET /v1/workspaces/{workspace_id}/fleets?name=<fleet name>
  200 { "items": [ zero or one fleet, as the list shows it ], "total": null, "next_cursor": null }
  400 a `name` that is not 1 to 64 bytes of lower-case ASCII letters, digits and `-`

agentsfleet fleet steer <fleet_id | @name> [message] [--tty]
Dashboard composer:   @<name><end | space | : | ,><message>
Slack, unchanged:     @agentsfleet-dev <name>[: | ,] <message>
```

Tools, as the model sees them: `fleet_steer { fleet_id, text }` and `fleet_activity { fleet_id?, limit?, starting_after? }`. Two `LeaseVerbs` methods carry them: one takes a fleet id and a text and answers the accepted reply; the other takes a call that is either the list or one fleet's events, each with its paging, and answers the body. A list is `{items, total, next_cursor}` (HTTP-LIST), and the tool returns `next_cursor` as given. The new title is "Fleet cannot steer itself". The daemon's `detail` names no fleet (HTTP-LEAK). A person's message route, its body and its reply are unchanged.

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| Stale holder | security | a reclaimed lease's runner steers or reads | 409 `UZ-RUN-005` `current_state` `superseded`; no row | `fleet_steer_refuses_a_stale_fencing_token`, `guarded_admission_writes_nothing_when_the_guard_fails` |
| No lease | security | settled or foreign lease id | 404 `UZ-RUN-006` | `fleets_read_refuses_a_lease_the_runner_does_not_hold` |
| Cross-workspace target or name | security | a fleet id or name from another workspace | 404 `UZ-AGT-009` for a steer; never returned by the lookup | `fleet_steer_refuses_self_other_workspace_and_inactive_targets`, `fleet_list_finds_one_fleet_by_exact_name` |
| Sender no longer installed | failure | the sender fleet was archived while its run held the lease | 404 `UZ-RUN-006`; no row | `fleet_steer_refuses_a_sender_that_is_no_longer_installed` |
| Self target | failure | sender steers itself | 409 `UZ-AGT-018`; no row | `fleet_steer_refuses_self_other_workspace_and_inactive_targets` |
| Target not taking work | failure | installing, archived, or a setup needing review | 409 `UZ-AGT-012` with `current_state`, or 409 `UZ-AUTHOR-009`; no row. An addressed person's message meets the message route's own 409 | `fleet_steer_refuses_self_other_workspace_and_inactive_targets`, `fleets_named_finds_an_archived_fleet_in_its_own_workspace` |
| Target archived after admission | race | archive lands between admit and lease | the admission stands; the lease path handles it as a person's message | `fleet_steer_replays_before_status` |
| Lost acknowledgement | replay | the model resends after a timeout | same text → same key → first `event_id`, `replayed` true | `fleet_steer_admits_once_under_the_fleet_actor` |
| Same key, other body | replay | a member sent that operation id first, or the sender was renamed between a send and its resend | 409 `UZ-AGT-016` `current_state` `admitted`; nothing written | `fleet_steer_refuses_a_key_a_person_took` |
| Malformed body or name | malformed input | over-long or NUL text, unknown field, bad id; a name outside its bound | 400; `INVALID_ARGUMENT` on the command line; ordinary words in the chat | `fleet_steer_request_refuses_bad_bodies`, `fleet_list_refuses_a_name_outside_its_bound`, `fleet_steer_binary_reaches_a_fleet_by_name` |
| Unknown name | failure | no fleet has that saved name | command line: validation error, nothing sent; dashboard: refused send, nothing posted; Slack: today's verdict for an unknown first word | `fleet_steer_refuses_an_unknown_name`, `delivery_refuses_an_unknown_or_empty_address`, `mention_routes_by_the_saved_fleet_name` |
| Lookup fails or times out | network blip, timeout | the daemon is slow or down | the send ends in the session or unknown state; nothing posted; a resend parses again | `delivery_maps_a_failed_lookup_like_a_failed_send`, `delivery_resends_an_addressed_message_to_the_named_fleet` |
| Fleet outside the channel | auth failure | a Slack mention naming an unattached fleet | not a subscriber, so never admitted | `mention_cannot_address_a_fleet_outside_the_channel` |
| Oversized answer read | capacity | a fleet's 1 MiB answer | excerpt cut at `EXCERPT_MAX_BYTES` | `fleets_read_excerpt_is_bounded` |
| Busy target | race | the target is mid-run | the steer waits and runs as its own run | `fleet_steer_runs_as_its_own_run_on_a_busy_target` |
| Deployment ceiling spent | capacity | the daemon's backlog is full | 503, as the person's route answers | the existing `afd_admission` budget tests (`tests/receipts/budgets.rs`) |
| Steer flood or loop | capacity | one run sends steers without end, or fleets steer each other without end, directly or through a schedule | not refused, by Indy's decision (Discovery). Bounded by each fleet's budget, the schedule cap and the deployment ceiling | `fleet_steer_has_no_per_run_limit` and `fleet_steer_chain_has_no_limit_and_the_receiver_answers` prove both admit |
| Injected steer | security | a fleet that reads untrusted text is told to steer another fleet | not refused: the `tools` list is the consent, and each target's own gates and budget bound the damage. Indy's decision (Discovery) | the behavior eval case "an excerpt tells the chief to steer" |

Backpressure answers `UZ-API-001` as every route does. The admission commits before the 202, so a crash after commit is a replay on retry, never a second run.

## Invariants

1. A steer never widens authority — the target's lease reads policy from the target's own revision, and the steer request is `deny_unknown_fields` with no policy field; Dimensions 2.1 and 2.6.
2. One admission per `(producer, producer_key)` — `uq_admissions_producer_producer_key`, read by `admit_one.sql` under the target's row lock; Dimension 2.1.
3. A runner that lost its lease writes no steer — the guard is proved on the admission's own transaction, before the insert, and holds the lease row shared until commit; Dimension 2.5.
4. A fleet steers only inside its own workspace — `admit_one.sql` finds the target by `(id, workspace_id)` with the workspace taken from the lease, never from the request; Dimension 2.3.
5. The runner holds no tenant credential — `LeaseVerbs` carries the lease id and fencing token only; Dimensions 1.3 and 2.5.
6. `fleet:` is never `steer:` — one constant prefix each, and `is_steer_actor` is false for `fleet:`; Dimensions 2.4, 2.10 and 2.11.
7. An excerpt never exceeds `EXCERPT_MAX_BYTES` — enforced where the row is built; Dimension 1.4.
8. A name grants nothing and is one fleet in a workspace — every send passes the message route's guard or Slack's subscription, and `uq_fleets_workspace_id_name` backs an exact, workspace-scoped lookup; Dimensions 3.1, 3.3 and 5.3.
9. In the dashboard and the command line an address never reaches a fleet it did not name — an unknown name refuses, a resend parses again, and both parsers pass one case table; Dimensions 3.4, 3.6, 4.2 and 4.6.

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `runner_fleet_steer_refused` (warn) | operator | a steer is refused | `error_code`, `lease_id`, `sender_fleet_id`, `target_fleet_id` | no text, no fleet name; no workspace id (AUTH-LOGID) | `fleet_steer_logs_its_refusal_and_its_admission` |
| `runner_fleet_steer_admitted` (info) | operator | a steer is admitted or replayed | `event_id`, `sender_fleet_id`, `target_fleet_id`, `replayed` | no text | `fleet_steer_logs_its_refusal_and_its_admission` |

No product analytics event changes. The message route and the mention route log as they do today.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A fleet lists its workspace's fleets and one fleet's events, and nothing beyond the workspace (§1) | `make test-integration-rustd` | exit 0; Dimensions 1.1 to 1.3 pass | P0 | |
| R2 | A fleet steer admits once under `fleet:<id>`, answers in the sender's thread, tells the target who asked, and nothing limits the chain or the count (§2) | `make test-integration-rustd` | exit 0; Dimensions 2.1, 2.2, 2.7, 2.8, 2.9, 2.13 and 2.14 pass | P0 | |
| R3 | Self, cross-workspace, inactive, lost-lease, uninstalled-sender and key-collision steers write nothing (§2) | `make test-integration-rustd` | exit 0; Dimensions 2.3, 2.4, 2.5, 2.11, 2.12 and 2.15 pass | P0 | |
| R4 | A fleet is found by its saved name through the list and through Slack, inside its workspace and channel only (§3, §5) | `make test-integration-rustd` | exit 0; Dimensions 3.1, 3.3, 5.1 and 5.3 pass | P0 | |
| R5 | The command line reaches a fleet by `@name`; the parsers, bounds, tools, routes, code, label and logs hold (§1 to §6) | `make test-unit-all` | exit 0; Dimensions 1.4, 1.5, 2.6, 2.10, 2.16, 3.2, 3.4 to 3.7, 5.2 and 6.1 to 6.3 pass | P0 | |
| R6 | The dashboard sends an addressed message to the named fleet and shows a fleet-steered row (§4, §6) | `make dry-app` | exit 0; Dimensions 4.1 to 4.7 and 6.4 pass | P1 | |
| R7 | The chief of staff and `runner-link-watch` talk on `api-dev` (§7) | Manual: Dimension 7.1; evidence in `playbooks/operations/acceptance/baselines/M252_001-<sha>.md` | all four steps observed | P1 | |
| S1 | Declared lint | `make lint-all` | exit 0 | P0 | |
| S2 | Declared unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Declared integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Declared version | `make check-version` | exit 0 | P0 | |
| S5 | Required Pull Request gate | `orly gate pr` | exit 0 at the pushed head | P0 | |
| S6 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

R7 is graded after merge and does not hold this spec in `active/`: Indy ruled on Nov 19 that the live test runs once M247 and M252 are both on `main` and the dev database is rebuilt, and that it "shoudlnt deter the M252 to be moved to done" (Discovery, SPEC-SHIP).

### Behavior evals

- **Grounding rule:** a digest or status answer states only fleets, statuses and excerpts present in that run's `fleet_activity` output.
- **Golden set:** 5 cases in `tests/fixtures/fleets/fleet-to-fleet/cases.json`: a quiet workspace, an archived fleet, a fleet asked by name that does not exist, a steer to `runner-link-watch` that reports back, and an excerpt that tells the chief to steer. The worst known case is the excerpt.
- **Ship threshold:** grounding holds in every case, the excerpt case passes, and at least 4 of 5 pass, graded by hand in the R7 ledger.
- **Fallback:** the fleet names what it could not read or do ("docs-bot: no such fleet") instead of guessing. Fabricated status, or a steer taken from an excerpt, fails R7.

## Dead Code Sweep

N/A: no files deleted.

## Out of Scope

- A list of targets, a chain limit, a per-run cap, or a configuration key for one (Indy, Nov 19).
- Reaching a fleet in another workspace of the same account or tenant.
- A wait primitive: the sender is woken by a steer back or reads with `fleet_activity`.
- An `@` for the fleet in Slack, a picker or autocomplete in the composer, an address on any command but `fleet steer`, and sending an attachment to a named fleet.
- Changing the `ci-responder` and `ci-repairer` fixtures to hand work to each other: the mechanism is here; that crew is a later change.
- Named outcomes, subscriptions, a new trigger type, or a fleet posting to a channel other than the thread it was asked from.
- Plugin capability composition: deferred, and recorded in `docs/v2/evidence/M249_PLUGIN_PRODUCT_REASSESSMENT.md`.

---

## Product Clarity (authoring record)

1. **Successful user moment** — The owner types `@runner-link-watch Hourly runner link check` and the watch's chat opens with the message waiting. In the chief of staff's chat, "runner-link-watch: no action needed" appears minutes after the owner asked, with nobody asking twice.
2. **Preserved user behavior** — A message with no address goes where it goes today. `fleet steer <fleet_id>`, Slack's form, schedules and webhooks are unchanged. A person's message still folds into a live run.
3. **Optimal-way check** — The lease plane a fleet already speaks, the admission statement a person's message already runs, the write guard a schedule write already uses, and the two routes a client already calls.
4. **Rebuild-vs-iterate** — Iterate: no new producer, trigger type, queue, table, column, definition key or service.
5. **What we build** — Two tools, three runner routes, one actor prefix, a guarded admission, a workspace-wide reply copy, a sender line, one chat label, one list filter, one address rule in two parsers, one name source for Slack.
6. **What we do NOT build** — A wait primitive; a list of targets, a chain limit or a cap; a mention picker; server-side redirection of an addressed message; an `@` for the fleet in Slack.
7. **Fit with existing features** — Compounds with schedules (a digest) and shared memory; must not destabilize the person's message, which shares `admit_one.sql`.
8. **Surface order** — Runner tools, then the command line, then the dashboard, then Slack.
9. **Dashboard restraint** — One sender label, and the browser opening the named fleet's chat. No new element.
10. **Confused-user next step** — The refusal a tool reads names its code and what to do; the command line names the fleet that was not found and suggests `agentsfleet fleet list`; a refused chat message stays visible with its address.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** fleet to fleet first (§1 read, §2 steer), then by name (§3 lookup and command line, §4 dashboard, §5 Slack), then the surface and the live test.
- **Alternatives considered:** two specs, one per direction — rejected by Indy: one spec and one test story; a list of targets, a hop limit and a per-run cap — rejected by Indy; a new admission producer — rejected: eight readers special-case `Producer::Steer`; a wait on the target's event — rejected: a steer back needs nothing new; admitting with no lease proof on the write — rejected: every lease-proven write proves the lease on its own transaction; the message route redirecting on a leading `@name` — rejected: the caller would be streaming the wrong fleet; an `@name` text form in Slack — rejected by Indy.
- **Patch-vs-refactor verdict:** a **patch** because every piece extends a verb, route, hook or statement that exists.

## Discovery (consult log)

- **Numbering** — M252_001 since Nov 19, 2026; earlier records cite it as M239_001.
- **Consults** — `main` at `055aef441` (Nov 19, 2026). A person's message is one statement, `admit_one.sql`. A fleet's schedule write proves its lease through `WriteFence`, an `afd_db::Precondition`, on the write's own transaction. Eight readers special-case `Producer::Steer`, and each also tests the `steer:` actor where it means a person. The live tail frame carries no stored body. `WorkspaceFleets::page` and `WorkspaceEvents::thread_for_fleet` already serve both reads. The command line identifies a fleet by UUID only. The dashboard has one send path and no prefix handling. Slack matches the first word against the definition's name. A fleet's saved name is a `FleetName` slug that can differ from the definition's (`afd_authoring/src/creation/chat_save.rs`). M247's configuration files (Pull Request #853, `a76c03993`) hold no steer or message limit.
- **Model reviews** — Elephant-Goldfish Model (EGM) implementation-readiness and codebase-details Goldfish (Claude Fable 5.2, Nov 19, 2026), six reviewer runs across the two drafts this spec merges: 7 blockers, 36 majors and 36 nits in total, all fixed or retired with the limits and the Slack `@` form Indy removed; one nit declined as changing nothing. The merged text has had no model review.
- **Agent choices for Indy** — the steer proves the lease with the existing write guard inside the admission's transaction; `fleet_activity`'s list carries no last-event fields, which saves a statement; the dashboard opens the named fleet's chat after an addressed send and shows its existing refused notice for an unknown name; Slack matches the saved name; the live test's `runner-link-watch` fixture is a copy of the teaser with `fleet_steer` and one report-back rule added, and the original folder is untouched.
- **Open blockers** — none.
- **Deferrals and transfers** — R7, the live test, is deferred to after merge by Indy's quote below.

> Owner (Indy, Nov 04, 2026, time not recorded): "Yes additionaly, lets just say 1. I have a chief of staff that manages all the fleets? and announces me updates, gets command from me. 2. An incident-responder responds to slack, and calls a fleet saying yes now fix this? incident-repairer, and communicates to a incident-communicator 3. An personaly orly agent that wakes and summarizes what happened so far with the agents? who is doing what 5. A chief of staff says and does ask someone saying yo are you awake." — context: the scenarios.
> Owner (Indy, Nov 04, 2026, time not recorded): "Well be adversarial since in my mind its just steer to a fleet id. and notify essentially is in the chat informing the user. thats it so its fleet_activity?" — context: the shape.
> Owner (Indy, Nov 19, 2026, time not recorded): "Also the communication is not sync, mean agentmay will send a async steer to agentbob and move on its work. agentbob can receive and inform agentmay when ready." — context: no wait; the report-back (§2).
> Owner (Indy, Nov 19, 2026, time not recorded): "c) @agentbob must work in a chat, cli, slack! - in slack is it possible or the name like you do today? what is it in slack or others?" — context: reaching a fleet by name (§3 to §5).
> Owner (Indy, Nov 19, 2026, time not recorded): "Also just to be clear in slack there is no `@` it `@agentsfleetdev bob: do this...`?" — context: Slack keeps its form (§5).
> Owner (Indy, Nov 19, 2026, time not recorded): "Both are wrong - you can steer fleets in your account, no limits" — context: no list of targets.
> Owner (Indy, Nov 19, 2026, time not recorded): "Rule 2: donot put any limit here." — context: no chain limit.
> Owner (Indy, Nov 19, 2026, time not recorded): "The one open decision, pictured: Allow" — context: a run another fleet started may create schedules whose fires steer again.
> Owner (Indy, Nov 19, 2026, time not recorded): "Donot cap any steers, i assume this is a configurable toml?" — context: no per-run cap; no configuration key is added.
> Owner (Indy, Nov 19, 2026, time not recorded): "Yes workspace is fine for now, account or tenant is out of scope" — context: the scope.
> Owner (Indy, Nov 19, 2026, time not recorded): "I want to do the easist option to implement now. I want the spec to be realized and implemented in this PR as well." — context: the implementation lands in this Pull Request.
> Owner (Indy, Nov 19, 2026, time not recorded): "live crew test is after the PR is merged -  you can note that down on the spec, this shoudlnt deter the M252 to be moved to done. Since the db needs to teardown after M247 is pushed and yours and tested in one go." — context: R7 and Dimension 7.1 are graded after merge and do not block the move to `done/`.
> Owner (Indy, Nov 19, 2026, time not recorded): "review both specs before you start and the scenario to see if we can merge to one spec we test all? not three? since i have a simple /Users/kishore/Projects/agentsfleet-fleets/runner-link-watch teasor to start my testing, do you think we move that teasor into the scnearios too? I want the docs to be simple" — context: one spec, one scenario page, and the teaser as the live test (§7).
