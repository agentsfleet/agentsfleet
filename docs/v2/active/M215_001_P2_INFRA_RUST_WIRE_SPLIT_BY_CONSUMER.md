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
**Status:** IN_PROGRESS
**Priority:** P2 — build tooling: every edit to an admin, tenant or ingress wire type recompiles the runner's crates, which never read those types
**Categories:** API, INFRA
**Batch:** B1 — folded into M211_002's Pull Request at Indy's direction; runs before M211_004 and M211_005, which edit `afd_wire::lease` and `afd_wire::event`
**Branch:** feat/m211-nested-loops-and-chat-continuity
**Folded-into:** `M211_002`
**Baseline revision:** bb007001545cb97f4dc27c9325235a6a0ebb4fb9
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 07, 2026) from a consumer audit of every `afd_wire` item at `cc318b856`, recorded in Discovery
**Canonical architecture:** `docs/architecture/runner_execution.md` §Crates

---

## Overview

**Goal (testable):** `touch rustd/crates/afd_api_wire/src/admin.rs && cargo build -p agentsfleet_runner` compiles no crate, and every type both sides exchange still comes from one `afd_wire`.
**Problem:** `afd_wire` carries 38 modules. Seven runner crates depend on it, and 24 of its modules (admin, tenant, ingress, approval and the rest) are read only by `agentsfleetd`. Editing any of them rebuilds `afr_agent`, `afr_supervisor`, `afr_tools` and the four others. Four runner-internal types sit in the wire crate though the daemon never names them, four public items are used nowhere, and the daemon spells all 22 runner routes as literals the runner spells again in `afd_wire::paths`.
**Solution summary:** Split by consumer. `afd_wire` keeps the 14 modules both sides read (activity, credentials, event, lease, memory, message_verb, paths, policy, report, runner, schedule_verb, tool_detail, tool_trace, and `redact`'s impls for them). A new `afd_api_wire` takes the 22 daemon-only modules plus `schedule`; it depends on `afd_wire`, never the reverse. The runner's own report state moves into `afr_agent`. `paths` becomes the one source of every runner route: the daemon's `#[utoipa::path]` attributes name its templates, which `const_format::concatcp!` composes from the same segments the runner's client joins.

## PR Intent & comprehension handshake

- **PR title (eventual):** folded into M211_002's Pull Request
- **Intent (one sentence):** A daemon-only API change stops recompiling the runner, and the runner's routes cannot drift from the daemon's.
- **Handshake** (PLAN, Oct 07, 2026) — restated: move what only the daemon reads out of the shared wire crate, move what only the runner holds into the runner, and make both sides name routes from one place. Matches the Intent. `ASSUMPTIONS I'M MAKING:` (1) The shared set is decided by non-test use: a module either side reads in `src/` stays shared; the audit's counts are in Discovery. (2) `event` stays shared whole: `LeasePayload.event` is an `EventEnvelope` (`afd_wire/src/lease.rs:7`), and splitting `event` would cut its steer and entry types from their envelope. (3) `schedule` moves: `schedule_verb.rs:8` names it only in a doc link. (4) `redact.rs`'s four `Debug` impls move with their types; three stay, `RunnerTokenRotatedResponse`'s goes with `admin`. (5) Crate names: `afd_wire` stays, because `runner_execution.md:91` names it as the runner's one wire; the new crate is `afd_api_wire`. **Quality ceiling:** one crate per consumer set is the leanest shape the dependency graph allows; a third runner-only wire crate would hold no protocol. **Surface checklist:** OpenAPI — the document must not change by a byte · CLI no · user docs no · release/version at close · schema no · spec vs rules: none.

## Implementing agent — read these first

1. `rustd/crates/afd_wire/src/lib.rs` — the module list and the crate's invariants (borrowed text, no `skip_serializing_if`, no `afd_core`), which `afd_api_wire` keeps.
2. `rustd/crates/afd_wire/Cargo.toml` — the `openapi` feature and the one aggregated test binary, mirrored by the new crate.
3. `rustd/crates/afr_supervisor/src/client.rs` (`lease_path`, line 234) — how the runner joins route segments today.
4. `rustd/crates/afd_api_runner/src/handler/runner/lease.rs` (line 51) — one of the 22 route literals the templates replace.
5. `docs/architecture/runner_execution.md` §Crates — which daemon crates a runner may depend on.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/crates/afd_api_wire/` (`Cargo.toml`, `src/lib.rs`, the 22 daemon-only modules with their submodules, `schedule.rs`, `src/redact.rs`, `tests/`) | CREATE | The daemon's own API types, moved whole with their tests and schema derives |
| `rustd/crates/afd_wire/src/` (`lib.rs`, `redact.rs`, `paths.rs`, `report.rs`, `lease.rs`, `runner.rs`), `rustd/crates/afd_wire/Cargo.toml`, `rustd/crates/afd_wire/tests/` | EDIT / DELETE | Keeps the shared 14; route templates; drops the runner-internal report types and the four unused items; moved modules and tests leave |
| `rustd/crates/afr_agent/src/` (a `result.rs` module and its users), `rustd/crates/afr_supervisor/src/` (`report.rs`, `report/tests.rs`, `test_support.rs`), `rustd/crates/afr_providers/tests/` | CREATE / EDIT | `ExecutionResult`, `ResultOutcome`, `Failure` and `Completed` move into the runner |
| `rustd/crates/afd_api_runner/src/handler/runner/*.rs`, `rustd/crates/afd_auth/src/credential.rs`, `rustd/crates/afd_api/src/openapi.rs` | EDIT | Routes and the token prefix named from `afd_wire::paths` |
| Every daemon crate importing a moved module (`afd_admission`, `afd_api`, `afd_api_ingress`, `afd_api_operator`, `afd_api_runner`, `afd_api_tenant`, `afd_approval`, `afd_bench`, `afd_credential`, `afd_cron`, `afd_events`, `afd_fleet`, `afd_fleet_lifecycle`, `afd_fleet_ops`, `afd_gate`, `afd_http`, `afd_ingress`, `afd_memory`, `afd_observability`, `afd_runner`, `afd_sse`, `afd_state`, `agentsfleetd`): `Cargo.toml`, `src/`, `tests/` | EDIT | `afd_wire::<moved>` becomes `afd_api_wire::<moved>`; the `openapi` features name both crates |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | The new member and workspace dependency; `const_format` for `afd_wire` |
| `cli/src/commands/whoami.ts`, `cli/test/fleet-schedule.unit.test.ts`, `ui/packages/app/lib/api/events-types.ts`, `ui/packages/app/tests/e2e/acceptance/fixtures/cli-runner.ts`, `cli/test/acceptance/fixtures/grant-ops.ts`, `cli/test/fleetbundle-pr-reviewer.unit.test.ts`, `ui/packages/app/lib/api/approvals-types.ts`, `ui/packages/app/tests/e2e/acceptance/fixtures/grants.ts`, `docs/architecture/runner_fleet.md` | EDIT | Comments citing a moved module's path or item now cite `afd_api_wire`; no code changes. `schema/810_fleet_approval_gates.sql:65,79` still cite `afd_wire::approval`: an applied migration, left for Indy |
| `rustd/crates/afd_http/src/route/` (`runner.rs`, `runner_ops.rs`, `path.rs`), `rustd/crates/afd_auth/Cargo.toml` | EDIT | The route table names the `paths` templates; `runner_path!` is deleted; `afd_auth` re-exports the wire's token prefix |
| `docs/architecture/runner_execution.md` | EDIT | §Crates: `afd_api_wire` is daemon-only; `afd_wire` holds what both sides speak |

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

The 22 daemon-only modules and `schedule` move to `afd_api_wire`, with their tests, schema derives and `RunnerTokenRotatedResponse`'s `Debug` impl. `tests/names.rs` moves with them and reads both crates' `src/`, its directory walk through `walkdir` rather than a hand-written recursion. `afd_api_wire` depends on `afd_wire` for the five shared types its modules name (`runner::AssignedPolicy`, `CapabilityReport`, `RunnerLiveness`, `SelftestReport`, and `event::EventSummary` through `tail`); nothing in `afd_wire` names `afd_api_wire`. Each daemon crate's imports and `openapi` feature follow.

- **Dimension 2.1** DONE — No two schema types across both crates publish under one name → Test `no_two_schema_types_publish_under_one_name`
- **Dimension 2.2** DONE — The published document is byte-identical after the move → Test `regenerated_openapi_matches`
- **Dimension 2.3** DONE — No runner crate depends on `afd_api_wire` → Test `runner_tree_has_no_api_wire`

### §3 — The runner's own state lives in the runner

`ExecutionResult`, `ResultOutcome`, `Failure` and `Completed` (`afd_wire/src/report.rs:85-118`) move to `afr_agent`, whose loop builds them and whose supervisor reads them. `RunnerChildInput` (`lease.rs:91`), `FAIL_CLOSED_DEFAULT` (`runner.rs:70`) and `paths::FLEET_RUNNERS` are deleted; nothing reads them. `paths::RUNNERS` stays: it is the enrolment constant §4 names, since `/v1/runners` is still spelled as a literal at `afd_api_runner/src/handler/runner/enrolment.rs:48` and `afd_http/src/route/runner_ops.rs:75`. The result types carry no serde or schema derive in `afr_agent`: `RunOutput` (`engine.rs:127`) is never serialized and `public/openapi.json` names none of them, so `Completed` becomes a unit struct.

- **Dimension 3.1** — A finished run still reports through the moved types → Test `test_rust_runner_lease_roundtrip`
- **Dimension 3.2** DONE — The three unused items are gone from the tree → Test `dead_items_absent`

### §4 — One source for every runner route — DONE

`paths` gains a template per daemon route, composed with `concatcp!` from the segments the runner joins (`RUNNER_LEASES`, `LEASE_ACTIVITY_SUFFIX`, …); `/v1/runners` enrolment keeps its own constant. The 22 `#[utoipa::path(path = …)]` attributes in `afd_api_runner` name the templates; `afd_auth`'s `RUNNER_TOKEN_PREFIX` and the OpenAPI bearer format read `paths::RUNNER_TOKEN_PREFIX`.

- **Dimension 4.1** DONE — Each template equals the route a runner joins → Test `test_route_templates_compose_from_their_segments`
- **Dimension 4.2** DONE — No runner route is spelled as a literal outside `paths` → Test `route_literals_only_in_paths`

### §5 — Measure after — DONE

§1's scenarios rerun at the branch head, with (A) touching `afd_api_wire/src/admin.rs`.

- **Dimension 5.1** DONE — Scenario A compiles no crate, and B and C are recorded beside the baseline → Test `measure_after_rebuilds`

## Interfaces

```
afd_wire::{activity, credentials, event, lease, memory, message_verb, paths, policy,
           report, runner, schedule_verb, tool_detail, tool_trace}     shared, unchanged paths
afd_api_wire::{admin, approval, auth, connector, fleet, grant, health, identity, ingress,
               models, operator, preference, schedule, schema, secret, tail, team, tenant,
               tenant_model_entry, tenant_provider, workspace, workspace_library}
afd_api_wire --features openapi    enables afd_wire/openapi
afd_wire::paths::LEASE_ACTIVITY    "/v1/runners/me/leases/{lease_id}/activity", one per route
afr_agent::result::{ExecutionResult, ResultOutcome, Failure, Completed}
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Schema name collision across crates | Two types in different crates publish one name | `no_two_schema_types_publish_under_one_name` reads both crates and fails |
| Document drift | A moved derive loses a rename or an alias | `regenerated_openapi_matches` diffs the regenerated document; any byte fails |
| Runner pulls the API crate | A runner crate adds `afd_api_wire` | `runner_tree_has_no_api_wire` fails |
| Route drift | A daemon route or runner segment changes alone | Both read one constant; the round trip `test_rust_runner_lease_roundtrip` fails on a mismatch |
| Broken doc link | A moved module's intra-doc link points across crates | `cargo doc` warnings fail `make lint-all` |

## Invariants

1. `afd_wire` never depends on `afd_api_wire` — Cargo refuses a cycle between them.
2. A route has one spelling — the daemon's attributes and the runner's client read `afd_wire::paths`; `route_literals_only_in_paths`.
3. The published document is unchanged — `regenerated_openapi_matches`.

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

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A daemon-only edit rebuilds no runner crate (§5) | `cd rustd && cargo build -q -p agentsfleet_runner && touch crates/afd_api_wire/src/admin.rs && cargo build -p agentsfleet_runner 2>&1 \| grep -c Compiling` | 0 | P0 | ✅ `0` at `c7226727b` (§5) |
| R2 | The document is unchanged (§2) | `cd rustd && cargo run -q -p agentsfleetd --features openapi --bin agentsfleetd -- --no-banner openapi \| diff - ../public/openapi.json` | no output | P0 | ✅ `cmp` against `public/openapi.json` silent, 684620 bytes each (§2) |
| R3 | The runner links no API crate (§2) | `cd rustd && cargo tree -p agentsfleet_runner -e normal \| grep -c afd_api_wire` | 0 | P0 | ✅ `cargo tree -e normal,build,dev --all-features` over `agentsfleet_runner` and all ten `afr_*` crates: 0 `afd_api_wire` lines (§2) |
| R4 | Routes have one spelling and dead items are gone (§3, §4) | `git grep -nE '"/v1/runners\|RunnerChildInput\|FAIL_CLOSED_DEFAULT\|FLEET_RUNNERS' -- 'rustd/crates/*/src/**' ':!rustd/crates/afd_wire/src/paths.rs' ':!rustd/crates/*/src/**/*tests.rs'` | no output | P0 | ✅ exit 1, no output (§3, §4) |
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
| `rustd/crates/afd_wire/src/admin.rs` and the other 22 moved modules | `test ! -f rustd/crates/afd_wire/src/admin.rs` |

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

- **Consults** — Indy (in-session, Oct 07, 2026): "ensure that the afd_wire is split relevantly on what is used in which daemon(agentsfleetd, agentsfleet-runner, commong or shared)", then chose "Go as drawn" for two crates, the runner-only types into `afr_agent`, `paths` as the one route source, and the dead items deleted. Consumer audit at `cc318b856` (non-test uses): the runner names 12 modules and reaches `event` through `lease.rs:7`; `paths` has no daemon `src/` use (the daemon re-spells 22 routes in `afd_api_runner/src/handler/runner/*.rs` and the prefix at `afd_auth/src/credential.rs:78`); `ExecutionResult`, `ResultOutcome`, `Failure` and `Completed` have no daemon reference; `RunnerChildInput`, `FAIL_CLOSED_DEFAULT`, `RUNNERS` and `FLEET_RUNNERS` have none anywhere; `schema` is used only in `afd_fleet_lifecycle/src/sql.rs:298`, under `#[cfg(test)]`; `activity` and `tool_trace` reference each other, both shared; `redact.rs:39,67,80,89` implement `Debug` for three shared types and one daemon type; only the daemon's `afd_api*` crates enable `openapi`; `agentsfleet_runner` itself does not depend on `afd_wire`. Architecture consult: `ARCH: grounded in runner_execution.md:91 | proposal: afd_wire holds what both sides speak; afd_api_wire is daemon-only | status: extends | landing: a`.
- **Baseline, §1** (Oct 07, 2026: 11:00 AM) — revision `ce95ce655`, rustc 1.98.1, Apple M2, 8 CPUs, dev profile; median of runs 1–3 for (A) and (B). (A) `touch rustd/crates/afd_wire/src/admin.rs` then `cargo build -p agentsfleet_runner`: 12 `Compiling` lines, 7.33s / 4.42s / 4.41s, median **4.42s**. (B) the same touch then `cargo build -p agentsfleetd`: 29 `Compiling` lines, 13.59s / 11.46s / 12.53s, median **12.53s**. (C) `cargo build --workspace --timings` in a fresh target directory: 513 `Compiling` lines in 2m 27s; `afd_wire` lib unit 9.38s, starting at 31.38s. Produced by a scratch script that loops the commands above; the earlier run stopped silently because cargo prints `in 1m 02s` past a minute and the elapsed-time grep missed it.
- **After, §5** (Oct 07, 2026: 12:20 PM) — revision `c7226727b`, same machine, toolchain and script as §1; (A) and (B) now touch `rustd/crates/afd_api_wire/src/admin.rs`. (A) `cargo build -p agentsfleet_runner`: runs 2 and 3 compile **0** crates in 0.23s / 0.20s (was 12 crates, median 4.42s). Run 1 compiled 12 in 12.07s, because the warm-up builds the runner and the daemon together and a runner-only build resolves features differently; the §1 baseline's run 1 carried the same cost. (B) `cargo build -p agentsfleetd`: 19 crates, 8.95s / 9.00s after a first run of 33 crates in 34.97s, median **9.00s** (was 29 crates, 12.53s). (C) clean `cargo build --workspace --timings`: 514 `Compiling` lines in 2m 20s (was 513 in 2m 27s); `afd_wire` lib unit 5.50s (was 9.38s), `afd_api_wire` 3.86s.
- **§3 amendment** (Oct 07, 2026: 11:30 AM) — `paths::RUNNERS` is kept, not deleted: §3's deletion list contradicted §4, whose enrolment route keeps its own constant and whose Dimension 4.2 forbids the `"/v1/runners"` literal outside `paths`. The audit's "no use anywhere" held at `cc318b856`; §4 gives it its reader.
- **§4 amendment** (Oct 07, 2026: 12:05 PM) — Dimension 4.2 and R4 exclude `*tests.rs` under `src/`: the runner client's unit tests (`afr_supervisor/src/client/tests.rs`, `client/lease_verbs/tests.rs`) spell `/v1/runners/...` as the independent expected value the client must produce, and naming the constant there would compare it with itself. The route table in `afd_http/src/route/runner.rs` spelled every route a third time through `runner_path!`; it now names the templates and the macro is gone. The bearer description reads the enrolment path from `RunnerOpsRoute::Register`, since `afd_api` holds `afd_wire` only as a dev-dependency.
- **Metrics review** — no analytics or funnel playbook update required: no product or operator signal changes.
- **Skill-chain outcomes** — pending.
- **Deferrals** — none.
