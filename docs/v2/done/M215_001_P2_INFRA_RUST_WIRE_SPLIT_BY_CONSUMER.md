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

# M215_001: `afd_wire` holds only what the daemon and the runner both speak — the daemon's own API types live in `afd_api_wire`, the runner's own state in `afr_agent`, and an edit to a daemon-only type rebuilds no runner crate

**Prototype:** v2.0.0
**Milestone:** M215
**Workstream:** 001
**Date:** Oct 07, 2026
**Status:** DONE
**Priority:** P2 — build tooling: every edit to an admin, tenant or ingress wire type recompiles the runner's crates, which never read those types
**Categories:** API, INFRA
**Batch:** B1 — folded into M211_002's Pull Request at Indy's direction; runs before M211_004 and M211_005, which edit `afd_wire::lease` and `afd_wire::event`
**Branch:** feat/m211-nested-loops-and-chat-continuity
**Folded-into:** `M211_002`
**Baseline revision:** bb007001545cb97f4dc27c9325235a6a0ebb4fb9
**Test Baseline:** unit=4212 integration=4929 — Rust unit 4212 passed, 0 failed, 879 ignored (runner 982 · daemon 132 · daemon libraries 3098); integration through the coverage shards 4929 passed, 0 failed (substrate 4158 · runner 550 · daemon 221); TypeScript app 3642, design-system 647, website 142 passed, cli 1779 passed and 17 skipped, at `bb0070015` via PR #732's identical tree. The branch at `886733be6`: Rust unit 4312 passed, 0 failed, 895 ignored (+100); integration 868 + 2 exclusive passed, 0 failed; kernel lane 34 passed.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M211-bb0070015.md`
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 07, 2026) from a consumer audit of every `afd_wire` item at `cc318b856`, recorded in Discovery
**Canonical architecture:** `docs/architecture/runner_execution.md` §Crates

---

## Overview

**Goal (testable):** `touch rustd/crates/afd_api_wire/src/admin.rs && cargo build -p agentsfleet_runner` compiles no crate, and every type both sides exchange still comes from one `afd_wire`.
**Problem:** `afd_wire` carries 38 modules. Seven runner crates depend on it, and 24 of its modules (admin, tenant, ingress, approval and the rest) are read only by `agentsfleetd`. Editing any of them rebuilds `afr_agent`, `afr_supervisor`, `afr_tools` and the four others. Four runner-internal types sit in the wire crate though the daemon never names them, four public items are used nowhere, and the daemon spells all 20 runner route attributes as literals the runner spells again in `afd_wire::paths`.
**Solution summary:** Split by consumer. `afd_wire` keeps the 14 modules both sides read (activity, credentials, event, lease, memory, message_verb, paths, policy, report, runner, schedule_verb, tool_detail, tool_trace, and `redact`'s impls for them). A new `afd_api_wire` takes the 23 daemon-only modules plus `schedule`; it depends on `afd_wire`, never the reverse. The runner's own report state moves into `afr_agent`. `paths` becomes the one source of every runner route: the daemon's `#[utoipa::path]` attributes name its templates, which `const_format::concatcp!` composes from the same segments the runner's client joins.

## PR Intent & comprehension handshake

- **PR title (eventual):** folded into M211_002's Pull Request
- **Intent (one sentence):** A daemon-only API change stops recompiling the runner, and the runner's routes cannot drift from the daemon's.
- **Handshake** (PLAN, Oct 07, 2026) — restated: move what only the daemon reads out of the shared wire crate, move what only the runner holds into the runner, and make both sides name routes from one place. Matches the Intent. `ASSUMPTIONS I'M MAKING:` (1) The shared set is decided by non-test use: a module either side reads in `src/` stays shared; the audit's counts are in Discovery. (2) `event` stays shared whole: `LeasePayload.event` is an `EventEnvelope` (`afd_wire/src/lease.rs:7`), and splitting `event` would cut its steer and entry types from their envelope. (3) `schedule` moves: `schedule_verb.rs:8` names it only in a doc link. (4) `redact.rs`'s four `Debug` impls move with their types; three stay, `RunnerTokenRotatedResponse`'s goes with `admin`. (5) Crate names: `afd_wire` stays, because `runner_execution.md:91` names it as the runner's one wire; the new crate is `afd_api_wire`. **Quality ceiling:** one crate per consumer set is the leanest shape the dependency graph allows; a third runner-only wire crate would hold no protocol. **Surface checklist:** OpenAPI — the document must not change by a byte · CLI no · user docs no · release/version at close · schema no · spec vs rules: none.

## Implementing agent — read these first

1. `rustd/crates/afd_wire/src/lib.rs` — the module list and the crate's invariants (borrowed text, no `skip_serializing_if`, no `afd_core`), which `afd_api_wire` keeps.
2. `rustd/crates/afd_wire/Cargo.toml` — the `openapi` feature and the one aggregated test binary, mirrored by the new crate.
3. `rustd/crates/afr_supervisor/src/client.rs` (`lease_path`, line 234) — how the runner joins route segments today.
4. `rustd/crates/afd_api_runner/src/handler/runner/lease.rs` (line 51) — one of the 20 route literals the templates replace.
5. `docs/architecture/runner_execution.md` §Crates — which daemon crates a runner may depend on.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_api_wire/` (`Cargo.toml`, `src/lib.rs`, the 23 daemon-only modules with their submodules, `schedule.rs`, `src/redact.rs`, `tests/`) | CREATE | The daemon's own API types, moved whole with their tests and schema derives |
| `rustd/crates/afd_wire/{Cargo.toml,src/{lease,lib,paths,redact,report,runner,schedule_verb}.rs,tests/{redaction,routes,wire_suite}.rs}` | EDIT | Keeps the shared 14; route templates and their snapshot; drops the runner-internal report types and the four unused items; the moved modules and tests leave |
| `rustd/crates/{afr_agent/src/{engine.rs,lib.rs,loop/{end_tests,finish,provider_failure_tests,session_tests,tests}.rs,nested/run_tests.rs,result.rs,scripted/tests.rs,scripted.rs},afr_providers/tests/{providers/{ends,hosts,images,retries}.rs,providers.rs},afr_supervisor/src/{report/tests.rs,report.rs,test_support.rs}}` | CREATE / EDIT | `ExecutionResult`, `ResultOutcome`, `Failure` and `Completed` move into the runner as `afr_agent::result` |
| `rustd/crates/afd_api_runner/src/handler/runner/*.rs`, `rustd/crates/afd_auth/src/credential.rs`, `rustd/crates/afd_api/src/openapi.rs` | EDIT | Routes and the token prefix named from `afd_wire::paths` |
| `rustd/crates/{afd_api/{Cargo.toml,src/{lib.rs,router/probes.rs},tests/{integration_connector_status.rs,openapi_stream_frames.rs,support/fleet_stream_transport_fixture.rs,tenant_model_entry_input.rs,tenant_shape_parity.rs,tenant_shape_parity_pages.rs,tenant_shape_parity_team.rs}},afd_api_ingress/{Cargo.toml,src/handler/{events.rs,mention/{admit,notice}.rs,mention.rs,webhook/{app_route,approval_route,github,identity_route,mod,qstash_route}.rs}},afd_api_operator/{Cargo.toml,src/handler/{admin/{libraries.rs,libraries_request.rs,library_import.rs,models.rs,platform_keys/tests.rs,platform_keys.rs},operator/{events,leases,query,runner_patch,runners}.rs}},afd_api_runner/Cargo.toml,afd_api_tenant/{Cargo.toml,src/handler/{approval/{mod,resolve}.rs,auth/{session/dashboard.rs,session.rs},connector/{callback,catalogue,connect,landing,status}.rs,fleet/{detail,detail_request,install_request,memory_access,message_steer,mod}.rs,fleet_bundles.rs,grant.rs,library_entry/list.rs,preference.rs,schedule/{read,write}.rs,secret/support.rs,secret.rs,tenant/{api_key.rs,billing.rs,cli_credential.rs,identity.rs,invite.rs,invite_email.rs,invite_view.rs,member.rs,mod.rs,model_entry/{input,render,write}.rs,model_entry.rs,models/input.rs,models.rs,provider/write.rs,provider.rs,workspace/render.rs,workspace.rs},workspace_library/render.rs,workspace_library.rs}}}` | EDIT | The daemon's API crates that import a moved module: `afd_wire::<moved>` becomes `afd_api_wire::<moved>`, and the `openapi` features name both crates |
| `rustd/crates/{afd_approval/{Cargo.toml,src/{decision.rs,gate_status.rs,grant.rs,inbox/{announce.rs,resolve/{stood,tests}.rs,resolve.rs,sweep.rs},lib.rs,request/{install,tests}.rs,request.rs},tests/{integration_grant_forgery,integration_grant_install,integration_grant_request,integration_grants}.rs},afd_cron/{Cargo.toml,src/model.rs},afd_events/{Cargo.toml,src/{closed,counters,steer}.rs},afd_fleet/{Cargo.toml,src/lease/{bracket,event,finalize}.rs,tests/{integration_activity_publish,integration_runner_admin,integration_runner_maintenance,integration_runner_retire,integration_runner_views,integration_wall_counters}.rs},afd_fleet_lifecycle/{Cargo.toml,src/{live_set,read,sql}.rs,tests/{integration_install_grants,integration_wall_counters}.rs},afd_fleet_ops/{Cargo.toml,src/runner_leases.rs,tests/integration_runner_leases.rs},afd_gate/{Cargo.toml,src/{gate/{decision,grant_sql,park}.rs,lib.rs}},afd_http/{Cargo.toml,src/{handler/library_onboard.rs,services/{event,fleets,memory}.rs}},afd_ingress/{Cargo.toml,src/{app.rs,slack/message.rs}},afd_memory/{Cargo.toml,src/{access,memories}.rs,tests/integration_shared.rs},afd_runner/{Cargo.toml,src/{admin.rs,sql/mod.rs,sweep/{liveness/tests.rs,liveness.rs},view/{decode,events}.rs,view.rs}},afd_sse/{Cargo.toml,src/{frame/tests.rs,frame.rs}},afd_state/{Cargo.toml,src/sql.rs}}` | EDIT | The daemon's library crates that import a moved module: the same rename |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | The new member and workspace dependency; `const_format` for `afd_wire`; `cargo_metadata` leaves with the dependency-graph test |
| `cli/src/commands/whoami.ts`, `cli/test/fleet-schedule.unit.test.ts`, `ui/packages/app/lib/api/events-types.ts`, `ui/packages/app/tests/e2e/acceptance/fixtures/cli-runner.ts`, `cli/test/acceptance/fixtures/grant-ops.ts`, `cli/test/fleetbundle-pr-reviewer.unit.test.ts`, `ui/packages/app/lib/api/approvals-types.ts`, `ui/packages/app/tests/e2e/acceptance/fixtures/grants.ts`, `docs/architecture/runner_fleet.md` | EDIT | Comments citing a moved module's path or item now cite `afd_api_wire`; no code changes. `schema/810_fleet_approval_gates.sql:65,79` still cite `afd_wire::approval`: an applied migration, left for Indy |
| `rustd/crates/afd_http/src/route/{runner,runner_ops,path}.rs`, `rustd/crates/afd_auth/Cargo.toml` | EDIT | The route table names the `paths` templates; `runner_path!` is deleted; `afd_auth` re-exports the wire's token prefix |
| `docs/architecture/runner_execution.md` | EDIT | §Crates: `afd_api_wire` is daemon-only; `afd_wire` holds what both sides speak |
| `rustd/rust-toolchain.toml`, `rustd/Cargo.toml` (`rust-version`), `playbooks/operations/ci_rust_images/versions.env`, `README.md`, `scripts/check_builder_pin_test.sh` | EDIT | The toolchain pin moves to 1.99.0 in every place the image playbook's checklist names |
| `rustd/crates/{afd_admission/src/{budget/ceiling.rs,reconcile/progress/tests.rs},afd_api/tests/{app_ingress_route,integration_fleet_memories,integration_invite_email,integration_invite_email_send,integration_tenant,webhook_fleet_route,webhook_receive_route,webhook_svix_route}.rs,afd_api_tenant/src/handler/{stream/revocable/tests.rs,tenant/{identity.rs,model_entry/render.rs}},afd_connector/src/{grant/parse/tests.rs,jira.rs,state/tests.rs},afd_core/{src/clock.rs,tests/problem.rs},afd_credential/{src/{error/tests.rs,provider/{endpoint/url.rs,managed.rs},secrets/mod.rs},tests/integration_rotation/registry_walk.rs},afd_crypto/tests/envelope.rs,afd_db/tests/integration_migrate_batch.rs,afd_fleet/{src/{error/tests.rs,lease/{admit/tests.rs,coverage.rs,history_tests.rs,mint/tests.rs,report/tests.rs,tool_detail/tests.rs}},tests/integration_runner_views.rs},afd_fleet_lifecycle/tests/integration_install_grants/recovery.rs,afd_fleet_runtime/{src/frontmatter/skill.rs,tests/frontmatter_corpus.rs},afd_gate/src/policy/{build.rs,context.rs,egress/read.rs,shape.rs},afd_identity/src/metadata/tests.rs,afd_library/src/{error/tests.rs,github/tests/transport.rs},afd_mail/src/mailer/{tests/relay_read.rs,tests.rs},afd_observability/src/{metrics/registry.rs,runner.rs},afd_otlp/src/config/tests.rs,afd_runner/src/{error/tests.rs,policy.rs,sweep/census/tests.rs},afd_sse/src/{fanin,live}.rs,afd_tenant/{src/{apikey/tests.rs,cli_credential/tests.rs,models/mod.rs},tests/integration_team.rs},afd_vault/tests/integration_list_no_decrypt.rs,afd_wire/tests/{memory_shapes,validation_lease}.rs,afr_agent/src/{loop/{provider_failure_tests,session_tests}.rs,scripted/tests.rs},afr_executor/{src/edges/tests.rs,tests/processes.rs},afr_memory/src/tests.rs,afr_providers/tests/providers/images.rs,afr_sandbox/src/{probe/tests.rs,toolbox/{holds/tests.rs,stage/tests.rs},warm_slots/tests/fakes.rs},afr_secrets/src/scrub/tests.rs,afr_supervisor/src/{lease_loop/tests.rs,lib_tests.rs,records/tests.rs,report_spool/tests.rs,test_support.rs},afr_telemetry/src/{budget/table.rs,endpoint/tests.rs},afr_tools/src/{catalog/tests.rs,sandbox/{exec_session/refusal_tests.rs,git/tests.rs,repositories/tests.rs,sessions/tests.rs,shell/tests.rs},verbs/{once/tests.rs,schedules/tests.rs}},agentsfleetd/{src/preflight/otlp/tests.rs,tests/{integration_runner_messages,integration_runner_schedules_edit}.rs}}` | EDIT | Every file the 1.99 move touched: `fetch_update` is renamed `try_update`; clippy 1.99's `assert_is_empty` wants the value printed; `double_must_use` and an unfulfilled `float_cmp` expectation are removed |
| `rustd/crates/afd_api/tests/openapi_artifact.rs`, `rustd/crates/afd_core/tests/workspace.rs`, `cli/test/manifest.unit.test.ts` | DELETE | `900ee9e1d`: "Each of these read a build file, a manifest or a component's source and asserted on its text instead of on behaviour". `openapi_artifact.rs` held "the committed-document comparison", which R2 runs as a command. Reverses M203_001's kept list, pending Indy (Discovery) |
| `rustd/crates/afd_api/tests/{http_substrate_suite,integration_workspace_approvals}.rs`, `rustd/crates/afd_core/{src/{clock,lib}.rs,tests/core_suite.rs}`, `rustd/crates/{afd_crypto,afd_wire}/Cargo.toml`, `ui/packages/design-system/src/design-system/BrailleSpinner.test.tsx`, `docs/architecture/testing.md` | EDIT | `900ee9e1d`: the suites drop the deleted modules; "Comments and docs that cited them now state the rule without the citation"; `BrailleSpinner.test.tsx` drops "the one assertion that grepped the component's source for hooks and timers; the render checks stay" |
| `.orly/docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | The parity table drops `test_openapi_build_is_the_source`, which left with `openapi_artifact.rs`. `900ee9e1d` edited the old `docs/` copy, and merge `c3c215dd3` carried the edit onto this managed file, which `orly doctor` reports as edited after orly wrote it (Discovery) |
| `rustd/crates/agentsfleet_runner/tests/dependency_graph.rs`, `rustd/crates/agentsfleet_runner/{Cargo.toml,tests/runner_suite.rs}`, `docs/architecture/observability.md` | DELETE / EDIT | `cda82d6f4`: the test "walked cargo metadata to prove the runner binary links no datastore client and no control-plane crate. It enforced layering, not access: a linked crate holds no credential"; "the test, its cargo_metadata dev-dependency and the observability doc's citation of it go" |
| `rustd/crates/{afd_admission/src/reconcile/{progress/{duplicate_tests,tests}.rs,progress.rs},afd_api/tests/{app_ingress_route,app_ingress_route_dropped,ingress_plane_suite,integration_workspace_approvals,integration_workspace_approvals_fixture,integration_workspace_approvals_listing,tenant_plane_suite}.rs,afd_api_wire/src/lib.rs,afr_executor/src/server/launch/terminal.rs,afr_providers/src/{request/{image_tests,tests}.rs,request.rs},afr_tools/src/{catalog,schema,selection}.rs}` | CREATE / EDIT | `0f3abb04d`, "Review-only changes from the M211 audits, no behaviour change": four test files "split under the length cap"; "`afr_executor` terminal pumps take `impl Read`/`impl Write`, not boxes"; "`afd_api_wire` and `afr_tools` module docs say what the code does" |
| `docs/v2/done/M215_001_P2_INFRA_RUST_WIRE_SPLIT_BY_CONSUMER.md` | CREATE | This spec, folded into M211_002's Pull Request |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (the four unused items and the runner types' old home go), UFS (route segments named once; templates composed, never re-spelled), ORP (no orphan module, test or doc link), FLL, MSID, ARCH.
- `dispatch/write_rust.md` + `docs/RUST_ERROR_STANDARD.md` — the new crate declares no error type; it carries data and garde bounds only, as `afd_wire` does.
- `docs/REST_API_DESIGN_GUIDELINES.md` — the published document is unchanged; routes keep their paths.
- `dispatch/name_architecture.md` — a new crate and a moved boundary; `runner_execution.md` §Crates says so in the same commit.
- Indy's direction (Oct 07, 2026): split by what each side uses — `agentsfleetd`, `agentsfleet-runner`, shared.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UFS GATE | yes — Rust | Route segments stay single constants; templates are `concatcp!` of them |
| MILESTONE-ID GATE | yes | No milestone identifiers in code, tests or comments |
| Architecture consult | yes | `runner_execution.md` §Crates edited in the same commit |
| File & Function Length (≤350/≤50/≤70) | yes | Files move whole; `afd_api_wire/src/lib.rs` mirrors `afd_wire`'s |
| SCHEMA GUARD | no | No schema change |

## Prior-Art / Reference Implementations

- **Reference:** `afd_wire` itself — the new crate copies its manifest, lint block, `openapi` feature and aggregated test binary.
- **Reference:** `utoipa-gen` 5.5.0 `src/path.rs:47,137` — `path` takes a literal or an expression, so a route constant is accepted.
- **Crates, not hand-rolled code** (Indy, Oct 07, 2026): `const_format::concatcp!` composes the templates, `insta` pins them, `walkdir` walks the schema sources; all three already resolve in `Cargo.lock`.

## Sections (implementation slices)

### §1 — Measure before the cut — DONE

Three scenarios on this Mac at the branch head before the cut, (A) and (B) the median of three runs and (C) one: (A) `touch rustd/crates/afd_wire/src/admin.rs`, then `cargo build -p agentsfleet_runner`; (B) the same touch, then `cargo build -p agentsfleetd`; (C) a clean `cargo build --workspace --timings` in a fresh target directory, reading `afd_wire`'s unit time. Wall time and the count of `Compiling` lines go into Discovery. Indy has decided the split; the numbers are its record, not its gate.

- **Dimension 1.1** DONE — The three baseline scenarios are recorded with revision, machine and medians → Test `measure_baseline_rebuilds`

### §2 — The daemon's own types leave the shared crate — DONE

The 23 daemon-only modules and `schedule` move to `afd_api_wire`, with their tests, schema derives and `RunnerTokenRotatedResponse`'s `Debug` impl. `tests/names.rs` moves with them and reads both crates' `src/`, its directory walk through `walkdir` rather than a hand-written recursion. `afd_api_wire` depends on `afd_wire` for the five shared types its modules name (`runner::AssignedPolicy`, `CapabilityReport`, `RunnerLiveness`, `SelftestReport`, and `event::EventSummary` through `tail`); nothing in `afd_wire` names `afd_api_wire`. Each daemon crate's imports and `openapi` feature follow.

- **Dimension 2.1** DONE — No two schema types across both crates publish under one name → Test `no_two_schema_types_publish_under_one_name`
- **Dimension 2.2** DONE — The published document is byte-identical after the move → Test `regenerated_openapi_matches`
- **Dimension 2.3** DONE — No runner crate depends on `afd_api_wire` → Test `runner_tree_has_no_api_wire`

### §3 — The runner's own state lives in the runner — DONE

`ExecutionResult`, `ResultOutcome`, `Failure` and `Completed` (`afd_wire/src/report.rs:85-118`) move to `afr_agent`, whose loop builds them and whose supervisor reads them. `RunnerChildInput` (`lease.rs:91`), `FAIL_CLOSED_DEFAULT` (`runner.rs:70`) and `paths::FLEET_RUNNERS` are deleted; nothing reads them. `paths::RUNNERS` stays: it is the enrolment constant §4 names, since `/v1/runners` is still spelled as a literal at `afd_api_runner/src/handler/runner/enrolment.rs:48` and `afd_http/src/route/runner_ops.rs:75`. The result types carry no serde or schema derive in `afr_agent`: `RunOutput` (`engine.rs:127`) is never serialized and `public/openapi.json` names none of them, so `Completed` becomes a unit struct.

- **Dimension 3.1** DONE — A finished run still reports through the moved types → Test `test_rust_runner_lease_roundtrip`
- **Dimension 3.2** DONE — The three unused items are gone from the tree → Test `dead_items_absent`

### §4 — One source for every runner route — DONE

`paths` gains a template per daemon route, composed with `concatcp!` from the segments the runner joins (`RUNNER_LEASES`, `LEASE_ACTIVITY_SUFFIX`, …); `/v1/runners` enrolment keeps its own constant. The 20 `#[utoipa::path(path = …)]` attributes in `afd_api_runner` name the templates; `afd_auth`'s `RUNNER_TOKEN_PREFIX` and the OpenAPI bearer format read `paths::RUNNER_TOKEN_PREFIX`.

- **Dimension 4.1** DONE — Each template equals the route a runner joins → Test `test_route_templates_compose_from_their_segments`
- **Dimension 4.2** DONE — No runner route is spelled as a literal outside `paths` → Test `route_literals_only_in_paths`

### §5 — Measure after — DONE

§1's scenarios rerun at the branch head, with (A) touching `afd_api_wire/src/admin.rs`.

- **Dimension 5.1** DONE — Scenario A compiles no crate, and B and C are recorded beside the baseline → Test `measure_after_rebuilds`

### §6 — The workspace builds on Rust 1.99.0 — DONE

The pin moves from 1.98.1 to 1.99.0, the newest stable (`channel-rust-stable.toml` reads `1.99.0 (b940084d7 2026-09-28)`). The CI base image is rebuilt and pushed under the new tag, which the workflows derive from `versions.env`. 1.99 renames `fetch_update` to `try_update`. Its clippy adds `assert_is_empty`: string checks take `assert_ne!(x, "")` and every other check keeps `assert!` with the value in its message, which needs no `PartialEq` on the element type.

- **Dimension 6.1** DONE — The workspace lints clean on 1.99.0 → Test `lint_on_pinned_toolchain`
- **Dimension 6.2** DONE — The image playbook refuses a build when the three pins disagree, and agrees on 1.99.0 → Test `build_and_push_pin_check`
- **Dimension 6.3** DONE — `ci-rust-alpine:1.99.0-alpine3.24` is published for both architectures → Test `ci_image_manifest_has_both_arches`

## Interfaces

```
afd_wire::{activity, credentials, event, lease, memory, message_verb, paths, policy,
           report, runner, schedule_verb, tool_detail, tool_trace}     shared, unchanged paths
afd_api_wire::{admin, admin_catalogue, admin_library, approval, auth, connector, fleet, grant, health, identity, ingress,
               models, operator, preference, schedule, schema, secret, tail, team, tenant,
               tenant_model_entry, tenant_provider, workspace, workspace_library}   admin_catalogue and admin_library private
afd_api_wire --features openapi    enables afd_wire/openapi
afd_wire::paths::LEASE_ACTIVITY    "/v1/runners/me/leases/{lease_id}/activity", one per route
afr_agent::result::{ExecutionResult, ResultOutcome, Failure, Completed}
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Schema name collision across crates | Two types in different crates publish one name | `no_two_schema_types_publish_under_one_name` reads both crates and fails |
| Document drift | A moved derive loses a rename or an alias | Caught when VERIFY runs R2 (`regenerated_openapi_matches`, a command): any differing byte prints. No test guards it between runs; the committed-document comparison test was dropped on purpose (`900ee9e1d`) |
| Runner pulls the API crate | A runner crate adds `afd_api_wire` | Caught when VERIFY runs R3 (`runner_tree_has_no_api_wire`, a command); the dependency-graph test was dropped on purpose (`cda82d6f4`) |
| Route drift | A daemon route or runner segment changes alone | Both read one constant; the round trip `test_rust_runner_lease_roundtrip` fails on a mismatch |
| Broken doc link | A moved module's intra-doc link points across crates | `cargo doc` warnings fail `make lint-all` |

## Invariants

1. `afd_wire` never depends on `afd_api_wire` — Cargo refuses a cycle between them.
2. A route has one spelling — the daemon's attributes and the runner's client read `afd_wire::paths`; `test_route_templates_compose_from_their_segments` pins each template, and the `route_literals_only_in_paths` command (R4) is run at VERIFY.
3. The move changes no byte of the published document — `regenerated_openapi_matches` (R2) at `c7226727b`. Later Sections of this branch change the document on purpose (M211_004's wire fields) and regenerate it.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | manual | `measure_baseline_rebuilds` | scenarios A and B (median of 3) and C (once) at the head before the cut → wall time and `Compiling` count each, in Discovery |
| 2.1 | unit | `no_two_schema_types_publish_under_one_name` | every `ToSchema` derive in both crates' `src/`, walked with `walkdir` → no published name twice |
| 2.2 | command | `regenerated_openapi_matches` | `cargo run -p agentsfleetd --features openapi --bin agentsfleetd -- --no-banner openapi` → identical to `public/openapi.json` |
| 2.3 | command | `runner_tree_has_no_api_wire` | `cargo tree -p agentsfleet_runner -e normal` → no `afd_api_wire` line |
| 3.1 | integration | `test_rust_runner_lease_roundtrip` | a lease served by the real daemon → the runner's report lands, built from `afr_agent`'s result types |
| 3.2 | command | `dead_items_absent` | `git grep -wE 'RunnerChildInput\|FAIL_CLOSED_DEFAULT\|FLEET_RUNNERS' rustd` → no match |
| 4.1 | unit | `test_route_templates_compose_from_their_segments` | every template, one per line → an `insta` snapshot reading `/v1/runners/me/leases/{lease_id}/activity` and the rest |
| 4.2 | command | `route_literals_only_in_paths` | `git grep -n '"/v1/runners' -- 'rustd/crates/*/src/**' ':!rustd/crates/afd_wire/src/paths.rs' ':!rustd/crates/*/src/**/*tests.rs'` → no match |
| 5.1 | manual | `measure_after_rebuilds` | scenario A at the head → 0 `Compiling` lines; B and C beside the baseline in Discovery |
| 6.1 | command | `lint_on_pinned_toolchain` | `cd rustd && rustc --version` → `rustc 1.99.0`; `make lint-rustd` → `clippy -D warnings` and `rustfmt --check` both ✓ |
| 6.2 | command | `build_and_push_pin_check` | `bash playbooks/operations/ci_rust_images/build_and_push_test.sh` → every case passes, the mismatch case refusing |
| 6.3 | command | `ci_image_manifest_has_both_arches` | `docker manifest inspect ghcr.io/agentsfleet/ci-rust-alpine:1.99.0-alpine3.24` → `amd64` and `arm64` entries |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A daemon-only edit rebuilds no runner crate (§5) | `cd rustd && cargo build -q -p agentsfleet_runner && touch crates/afd_api_wire/src/admin.rs && cargo build -p agentsfleet_runner 2>&1 \| grep -c Compiling` | 0 | P0 | ✅ `0` at `c7226727b` (§5) |
| R2 | The document is unchanged (§2) | `cd rustd && cargo run -q -p agentsfleetd --features openapi --bin agentsfleetd -- --no-banner openapi \| diff - ../public/openapi.json` | no output | P0 | ✅ `cmp` against `public/openapi.json` silent, 684620 bytes each (§2) |
| R3 | The runner links no API crate (§2) | `cd rustd && cargo tree -p agentsfleet_runner -e normal \| grep -c afd_api_wire` | 0 | P0 | ✅ `cargo tree -e normal,build,dev --all-features` over `agentsfleet_runner` and all ten `afr_*` crates: 0 `afd_api_wire` lines (§2) |
| R4 | Routes have one spelling and dead items are gone (§3, §4) | `git grep -nE '"/v1/runners\|RunnerChildInput\|FAIL_CLOSED_DEFAULT\|FLEET_RUNNERS' -- 'rustd/crates/*/src/**' ':!rustd/crates/afd_wire/src/paths.rs' ':!rustd/crates/*/src/**/*tests.rs'` | no output | P0 | ✅ exit 1, no output (§3, §4) |
| R6 | The workspace and its CI image are on Rust 1.99.0 (§6) | `docker manifest inspect ghcr.io/agentsfleet/ci-rust-alpine:1.99.0-alpine3.24 \| grep -c '"architecture": "\(amd64\|arm64\)"'` | 2 | P0 | ✅ `amd64` and `arm64` at `sha256:8109325371efcc71b38381a60eb9ed96dbd2767e96259bdcd79fbc2fdb68fafd`; `build_and_push_test.sh` 19 passed, 0 failed (§6) |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Command source rule:** every declared `conform` and `verify.*` command from `.oracle/orly.json` appears above verbatim. **Grading protocol (VERIFY):** Graded = ✅/❌ + one decisive output line; repository rows point to the final `orly gate pr` results in Session Notes.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `rustd/crates/afd_wire/src/admin.rs` and the other 23 moved modules | `test ! -f rustd/crates/afd_wire/src/admin.rs` |
| `rustd/crates/afd_api/tests/openapi_artifact.rs`, `rustd/crates/afd_core/tests/workspace.rs`, `cli/test/manifest.unit.test.ts` (`900ee9e1d`), `rustd/crates/agentsfleet_runner/tests/dependency_graph.rs` (`cda82d6f4`) | `test ! -f rustd/crates/afd_api/tests/openapi_artifact.rs` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `afd_wire::admin` and every moved module path | `git grep -nwE 'afd_wire::(admin\|approval\|tenant\|ingress\|schedule)' rustd` (`-w`, not `\b`: this machine's `git grep -E` matches nothing on `\b`) | 0 matches |
| `RunnerChildInput`, `FAIL_CLOSED_DEFAULT`, `FLEET_RUNNERS` | `git grep -nwE 'RunnerChildInput\|FAIL_CLOSED_DEFAULT\|FLEET_RUNNERS' rustd` | 0 matches |

## Out of Scope

- A runner-only wire crate: the runner-only items are runner state, not protocol.
- Splitting `event` by type: its envelope rides the lease, and its steer and entry types belong with it.
- Renaming `afd_wire`.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A developer edits a tenant API type and the runner's crates do not recompile.
2. **Preserved user behaviour** — Every route, payload and published schema is byte-identical; nothing a user or operator sees changes.
3. **Optimal-way check** — The cut follows the measured consumer sets; a finer split (one crate per plane) would add crates without a consumer boundary to justify them.
4. **Rebuild-vs-iterate** — Iterate: files move whole; only `paths`, `report` and `redact` change shape.
5. **What we build** — One crate, a route template per route, the runner's result types in the runner, the measurements.
6. **What we do NOT build** — A third wire crate, an `event` split, a rename (Out of Scope).
7. **Fit with existing features** — M211_004 and M211_005 edit `lease` and `event`, which stay shared; this lands first.
8. **Surface order** — N/A — no user surface.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — N/A — no user surface; a developer reads `runner_execution.md` §Crates.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** measure, move the daemon's modules, move the runner's types, unify routes, measure — each Section compiles and passes on its own.
- **Alternatives considered:** three crates, one per consumer set (rejected: the runner-only set holds no wire type); a test that compares the daemon's route literals to `paths` (rejected: one constant removes the drift a test would only detect).
- **Patch-vs-refactor verdict:** this is a **refactor** because it moves a crate boundary; no behaviour changes.

## Discovery (consult log)

- **Consults** — Indy (in-session, Oct 07, 2026): "ensure that the afd_wire is split relevantly on what is used in which daemon(agentsfleetd, agentsfleet-runner, commong or shared)", then chose "Go as drawn" for two crates, the runner-only types into `afr_agent`, `paths` as the one route source, and the dead items deleted. Consumer audit at `cc318b856` (non-test uses): the runner names 12 modules and reaches `event` through `lease.rs:7`; `paths` has no daemon `src/` use (the daemon re-spells 20 routes in `afd_api_runner/src/handler/runner/*.rs` and the prefix at `afd_auth/src/credential.rs:78`); `ExecutionResult`, `ResultOutcome`, `Failure` and `Completed` have no daemon reference; `RunnerChildInput`, `FAIL_CLOSED_DEFAULT`, `RUNNERS` and `FLEET_RUNNERS` have none anywhere; `schema` is used only in `afd_fleet_lifecycle/src/sql.rs:298`, under `#[cfg(test)]`; `activity` and `tool_trace` reference each other, both shared; `redact.rs:39,67,80,89` implement `Debug` for three shared types and one daemon type; only the daemon's `afd_api*` crates enable `openapi`; `agentsfleet_runner` itself does not depend on `afd_wire`. Architecture consult: `ARCH: grounded in runner_execution.md:91 | proposal: afd_wire holds what both sides speak; afd_api_wire is daemon-only | status: extends | landing: a`.
- **Baseline, §1** (Oct 07, 2026: 11:00 AM) — revision `ce95ce655`, rustc 1.98.1, Apple M2, 8 CPUs, dev profile; median of runs 1–3 for (A) and (B). (A) `touch rustd/crates/afd_wire/src/admin.rs` then `cargo build -p agentsfleet_runner`: 12 `Compiling` lines, 7.33s / 4.42s / 4.41s, median **4.42s**. (B) the same touch then `cargo build -p agentsfleetd`: 29 `Compiling` lines, 13.59s / 11.46s / 12.53s, median **12.53s**. (C) `cargo build --workspace --timings` in a fresh target directory: 513 `Compiling` lines in 2m 27s; `afd_wire` lib unit 9.38s, starting at 31.38s. Produced by a scratch script that loops the commands above; the earlier run stopped silently because cargo prints `in 1m 02s` past a minute and the elapsed-time grep missed it.
- **After, §5** (Oct 07, 2026: 12:20 PM) — revision `c7226727b`, same machine, toolchain and script as §1; (A) and (B) now touch `rustd/crates/afd_api_wire/src/admin.rs`. (A) `cargo build -p agentsfleet_runner`: runs 2 and 3 compile **0** crates in 0.23s / 0.20s (was 12 crates, median 4.42s). Run 1 compiled 12 in 12.07s, because the warm-up builds the runner and the daemon together and a runner-only build resolves features differently; the §1 baseline's run 1 carried the same cost. (B) `cargo build -p agentsfleetd`: 19 crates, 8.95s / 9.00s after a first run of 33 crates in 34.97s, median **9.00s** (was 29 crates, 12.53s). (C) clean `cargo build --workspace --timings`: 514 `Compiling` lines in 2m 20s (was 513 in 2m 27s); `afd_wire` lib unit 5.50s (was 9.38s), `afd_api_wire` 3.86s.
- **§3 amendment** (Oct 07, 2026: 11:30 AM) — `paths::RUNNERS` is kept, not deleted: §3's deletion list contradicted §4, whose enrolment route keeps its own constant and whose Dimension 4.2 forbids the `"/v1/runners"` literal outside `paths`. The audit's "no use anywhere" held at `cc318b856`; §4 gives it its reader.
- **§4 amendment** (Oct 07, 2026: 12:05 PM) — Dimension 4.2 and R4 exclude `*tests.rs` under `src/`: the runner client's unit tests (`afr_supervisor/src/client/tests.rs`, `client/lease_verbs/tests.rs`) spell `/v1/runners/...` as the independent expected value the client must produce, and naming the constant there would compare it with itself. The route table in `afd_http/src/route/runner.rs` spelled every route a third time through `runner_path!`; it now names the templates and the macro is gone. The bearer description reads the enrolment path from `RunnerOpsRoute::Register`, since `afd_api` holds `afd_wire` only as a dev-dependency.
- **§6 scope** (Oct 07, 2026: 3:10 PM) — Indy (in-session, Oct 07, 2026): "I think move rust to 1.99.0 or any thing latest in this repo and the CI jobs?", then "for CI jobs new images needs to built and pushed", then "Well i want the rust-1-00 in this branch/worktree not a new tree". Folded here because this spec is the Rust infrastructure workstream of this Pull Request. 1.99.0 is the newest stable at that date. The first 1.99 build failed on `afd_core/src/clock.rs:223`: "use of deprecated method `fetch_update`: renamed to `try_update` for consistency". Clippy then reported `assert_is_empty`, `double_must_use` and an unfulfilled `float_cmp` expectation. `cargo clippy --fix` rewrote collections as `assert_eq!(v, [] as [T; 0])`, which fails to compile wherever the element type lacks `PartialEq`, so those sites keep `assert!` with the value in the message.
- **Commits no spec owned (Oct 08, 2026)** — `900ee9e1d`, `cda82d6f4` and `0f3abb04d` landed on this branch with no spec naming them; their files sit in this spec's Files Changed, since it is this Pull Request's Rust infrastructure workstream. `900ee9e1d` deletes `afd_api/tests/openapi_artifact.rs` and `afd_core/tests/workspace.rs`, which reverses the kept list at `docs/v2/done/M203_001_P1_API_CLI_CONTINUATION_LINEAGE_AND_LEDGER_KEY.md` Discovery: "Kept, with reasons: `openapi_artifact.rs` and `workspace.rs` (structured, not heuristics)". The reversal awaits Indy's confirmation; no recorded decision of his approves it. The same commit edited `docs/REST_API_DESIGN_GUIDELINES.md`, and merge `c3c215dd3` carried the edit onto the managed `.orly/docs/REST_API_DESIGN_GUIDELINES.md`. `orly doctor` reports "managed file was edited after orly wrote it" and names two resolutions, moving the change into the pack source or discarding it with `orly update --force`; the choice is Indy's.
- **Metrics review** — no analytics or funnel playbook update required: no product or operator signal changes.
- **Skill-chain outcomes** — `/orly-write-unit-test` at the boundary (Oct 08, 2026): the review round's ledger over `657ae5807..df3b1f52d` (44 production files) found 8 gaps, closed in `190012f01`, with two won't-test rows (a refused `memory.high` write needs a cgroup file system; an unencodable report cannot occur); a coverage pass closed the remaining testable arms in `b9e282233`. `make test-coverage-rustd` at `b9e282233`: patch coverage 99.3078% (2726 of 2745 changed lines) against the 99 floor, line coverage 98.8582%; the 19 unhit lines are pre-exec hooks and the sandbox entry, which run in a forked child or inside bubblewrap where no profile is written, plus arms no input reaches. gstack `/review`: ten readers over `e2242b5b4..657ae5807`, fixes in `585f8c29f`, `6905beb74` and `d0afd22a0`; a second pass over the fixes found four more, fixed in `e96879f57` and `a414fbc13`; the findings left open are listed for Indy in the Pull Request. `make test-runner-kernel` on afr-kernel at `a414fbc13`: 41 passed, 0 failed. Mutation testing over the review round runs after the Pull Request opens and its result goes in the Pull Request's Session notes; `orly-babysit-prs` follows the push.
- **Deferrals** — none.
