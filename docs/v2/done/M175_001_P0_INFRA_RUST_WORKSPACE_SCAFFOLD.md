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

# M175_001: Rust workspace scaffold — wire-fixture round-trip and repository lanes

**Prototype:** v2.0.0
**Milestone:** M175
**Workstream:** 001
**Date:** Aug 23, 2026
**Status:** DONE
**Priority:** P0 — every later port milestone (M176–M181) consumes these crates and lanes
**Categories:** INFRA
**Batch:** B1 — family opener, serial; nothing in the port family runs before it
**Branch:** feat/m175-rust-workspace-scaffold
**Test Baseline:** unit=4237 integration=719 (Zig `src/**`, `make _lint_zig_test_depth` at CHORE(open); VERIFY Test Delta compares against this. The Rust cargo suite starts at zero — this milestone creates it — and is tracked by its own `cargo test --workspace` count.)
**Depends on:** none — opener of the M175–M181 Zig-to-Rust daemon port family
**Provenance:** LLM-drafted (Claude Fable 5, Aug 23, 2026)
**Canonical architecture:** `docs/architecture/runner_fleet.md` §The control protocol

---

## Overview

**Goal (testable):** `cargo test` in `rustd/` round-trips every `/v1/runners` wire fixture emitted by the Zig source of truth byte-identically, and the repository lanes (`make lint-all`, `make test-unit-all`, `make check-version`, both git hooks) catch seeded Rust defects.
**Problem:** Rust code can land in this repository today completely ungated — the hooks carry no `*.rs` case (a Rust-only commit prints "no lint-relevant files staged"), no `Cargo.toml` exists anywhere, and `dispatch/write_rust.md`'s assertion that the repository declares formatting/Clippy/build/test commands is aspiration, not fact (verified Aug 23, 2026: `grep -nE '\brs\b|rust|cargo' .githooks/pre-commit .githooks/pre-push` → no matches; `find . -name Cargo.toml -not -path '*/node_modules/*'` → no matches).
**Solution summary:** Create the `rustd/` Cargo workspace with its first two crates — `afd_core` (domain primitives) and `afd_wire` (the daemon↔runner wire types) — wire Rust into the existing make lanes, hooks, Continuous Integration (CI) jobs and coverage flags, and prove the wire layer against fixtures generated from the Zig implementation before any server code exists.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): scaffold Rust workspace with wire parity and lanes
- **Intent (one sentence):** Rust becomes first-class gated work in this repository, with the daemon↔runner wire layer proven byte-identical to the Zig implementation before any daemon code is written.
- **Handshake** (filled at PLAN, Aug 23, 2026) — **Restatement:** Rust stops being ungated in this repository. I create a two-crate Cargo workspace under `rustd/`, join it to the existing make lanes, both git hooks, Continuous Integration (CI) and coverage, and prove `afd_wire` speaks the current `/v1/runners` wire byte-for-byte against fixtures the Zig `src/lib/contract` module generates — before any daemon code exists to disagree with it. Zig stays the wire source of truth; Rust conforms to it, never the reverse.
- **ASSUMPTIONS I'M MAKING:**
  1. **Fixtures flow one way.** Only `src/lib/contract/fixture_export.zig` writes `samples/fixtures/wire-v2/`; Rust never does. A disagreement is fixed by changing Rust or regenerating — never by hand-editing a fixture.
  2. **The emitter needs no `build.zig` edit.** Every `src/lib/contract/*.zig` import is sibling-relative, so the emitter compiles standalone under `zig run`. If that proves false I stop and flag rather than touching the Zig build graph (Product Clarity 7).
  3. **Rust learns the current lease shape only** (Addendum A1). `protocol_lease_v1` is an explicit, test-asserted exclusion; no version-one serde type exists in `afd_wire`.
  4. **"Every exported type gets a fixture" means one canonical JSON document per type** — structs emit a fully-populated object (every optional present, so no field escapes the round-trip); enums emit the array of all their tag names, so every wire spelling is proven rather than only the one a sample happened to use.
  5. **The CI Rust job stays out of the `test` aggregate's `needs:` list.** That aggregate is the required context, so joining it would make the Rust job required by proxy and break Invariant 5.
  6. **The coverage floor is measured, never guessed.** `cargo llvm-cov` runs once and the `rust-afd` target is set from that number; a floor of 0 is not shipped.
  7. **Neither crate performs I/O, spawns a thread, or pulls an async runtime** (Invariant 2). `afd_core`'s config types are value types; nothing reads a file or an environment variable.
  8. **Edition 2024, everything inherited from the workspace root**, toolchain pinned to the repository's mise-managed Rust.
  9. **The Zig daemon is untouched.** One Zig file is created; none is edited.

## Implementing agent — read these first

1. `dispatch/write_rust.md` — the Rust authoring rules; carries the Microsoft Pragmatic Rust Guidelines pointer (mandatory sectioned read in every Rust REVIEW).
2. `src/lib/contract/protocol.zig` and the sibling files in `src/lib/contract/` — the wire source of truth this milestone mirrors (path constants, `LEASE_WIRE_VERSION_CURRENT = 2`, lease/report/activity/memory/credentials types).
3. `make/quality.mk` and `make/test-unit.mk` — the lane shapes the cargo steps join; extend targets, never add near-duplicate wrappers (AGENTS.orly.md operational default).
4. `.githooks/pre-commit` — the staged-file dispatch the `*.rs` case joins; mirror the existing per-language case structure.
5. `docs/architecture/runner_fleet.md` §The control protocol — the five verbs and wire versioning the fixtures must cover.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/Cargo.toml` | CREATE | workspace root: members, `[workspace.dependencies]`, `[workspace.lints]`, release profile |
| `rustd/rust-toolchain.toml` | CREATE | pinned toolchain with rustfmt + clippy components |
| `rustd/rustfmt.toml` | CREATE | formatting choices, chosen once, CI-enforced |
| `rustd/Cargo.lock` | CREATE | committed: the workspace builds a daemon binary, so its dependency graph is pinned |
| `.gitignore` | EDIT | ignore `rustd/target/` — build output, never committed |
| `rustd/crates/afd_core/**` | CREATE | domain primitives: error-code registry, UUID v7 identity, config types |
| `rustd/crates/afd_wire/**` | CREATE | serde port of the `/v1/runners` wire types + wire versioning |
| `src/lib/contract/fixture_export.zig` | CREATE | Zig-side canonical fixture emitter (test-build tool, imports the wire modules directly) |
| `samples/fixtures/wire-v2/**` | CREATE | committed canonical wire fixtures the Rust tests byte-compare against |
| `make/quality.mk` | EDIT | `lint-all` gains `cargo fmt --check` + `cargo clippy -- -D warnings` over `rustd/` |
| `make/test-unit.mk` | EDIT | `test-unit-all` gains `cargo test --workspace`; also houses the `wire-fixtures` regen recipe the emitter runs |
| `make/build.mk` | EDIT | `check-version` also compares the `rustd/` workspace version to `VERSION` |
| `.githooks/pre-commit` | EDIT | staged `*.rs` triggers the Rust fast lane |
| `.githooks/pre-push` | EDIT | pushed `*.rs` triggers the Rust fast lane |
| `.github/workflows/test.yml` | EDIT | Rust job beside the Zig jobs (non-required context — see Invariant 5) |
| `codecov.yml` | EDIT | `rust-afd` flag beside the `zig-*` flags, with an enforced floor |
| `src/lib/contract/fixture_sample.zig` | CREATE | the emitter's value-synthesis half, split out on the length rule |
| `Makefile` | EDIT | §6: `make help` no longer advertises retired targets; advertises the Rust lanes |
| `make/quality.mk` | EDIT | §6: `lint-zig` + its ten helpers and `lint-governance` removed; `lint-rustd` and `lint-scripts` added |
| `make/test-unit.mk` | EDIT | §6: the three Zig unit lanes and `test-coverage-zig` removed; `test-unit-rustd` and `wire-fixtures` added |
| `make/test.mk` | EDIT | §6: `test-coverage-grade` and the dead Zig coverage variables removed; `test-unit-all` rewired |
| `make/bench.mk` | EDIT | §6: the `memleak` lane and its two helpers removed |
| `make/test-integration.mk`, `make/check-test-reachability.mk` | DELETE | §6: whole fragments, nothing left in them |
| `.github/workflows/test.yml` | EDIT | Rust job added (non-required); §6 removes the three Zig jobs |
| `.github/workflows/lint.yml` | EDIT | §6: the `lint-zig` job removed |
| `.github/workflows/memleak.yml`, `.github/workflows/test-integration.yml`, `.github/workflows/cross-compile.yml` | DELETE | §6: whole workflows for retired lanes |
| `.github/workflows/deploy-dev.yml` | EDIT | §6: manual dispatch only — `agentsfleetd` and `agentsfleet-runner` no longer auto-deploy |
| `scripts/check_zig_*.py`, `scripts/verification_evidence*.py`, `scripts/check_lane_concurrency_test.py`, `scripts/check_ci_lane_config_test.py`, `scripts/check_allocating_writer_test.py`, `scripts/reachability_test_support.py`, `scripts/run-zig-memleak-lane.sh`, `scripts/check-kcov-components.sh` | DELETE | §6 orphan sweep (RULE ORP): each existed only to serve a retired lane |
| `scripts/check_readme_badges_test.py` | EDIT | §6: reads one workflow now; the Zig-flag assertions went with the flags |
| `scripts/check_route_registration_doc_test.py` | EDIT | §6: fixtures repointed to underscore targets that still exist |
| `docs/REST_API_DESIGN_GUIDELINES.md` | EDIT | §6: cited three retired targets; orly-MANAGED, see Discovery |
| `docs/architecture/testing.md` | REWRITE | §6: the page described the retired Zig lanes end to end — replaced with the current architecture rather than patched |
| `docs/architecture/README.md`, `docs/architecture/billing_and_provider_keys.md`, `docs/development.md` | EDIT | §6 orphan sweep: cited deleted targets and scripts; `check-architecture-doc` was red until fixed |
| `AGENTS.md` | EDIT | §6: the operating instructions named `make test-integration`, `make memleak` and `make lint-governance` as repository claims |
| `docs/v2/active/M175_001_*.md` | EDIT | this spec — CHORE(open/close), A1/A2, §6, rubric grades |
| `docs/v2/pending/M180_001_*.md` | EDIT | layout repoint: `rustd/src/` → `rustd/crates/` |
| `docs/v2/pending/M176_001_*.md` | EDIT | §6 consequence: the substrate no longer inherits Zig lanes |
| `AGENTS.md` | EDIT | worktree recipe corrected: the root `bun install` does not hydrate `ui/packages/*`, and `make test-coverage-all` fails on `next/headers` until it does |
| `README.md` | EDIT | `rust-afd` coverage badge + Rust toolchain badge — required by `scripts/check_readme_badges.py`, which fails any uploaded flag the README never shows |
| `docs/v2/pending/M177_001_*.md` | EDIT | addendum A1 propagation: the Rust lease handler carries no version negotiation; the dual-run differ drives current-shape requests only |
| `docs/v2/pending/M178_001_*.md`, `docs/v2/pending/M179_001_*.md` | EDIT | addendum A2 propagation: "pure port" bounds redesign, not the single-implementation parity rule |
| `docs/v2/pending/M181_001_*.md` | EDIT | addendum A2: single-implementation parity replaces parity-first; declared-divergence register created with the lease-wire entry |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (no dead code), UFS (literals → named consts; env knob and path constants), TST-NAM (test identifiers milestone-free — fixture directory is `wire-v2`, not `m175-*`), PSR (serde over hand-rolled parsing), MSID (no milestone ids in `rustd/` source or fixtures), FLL (file/function length).
- `dispatch/write_rust.md` — ownership visibility, preserved error variants, feature-combination tests; REVIEW cites Microsoft guideline mnemonics applied or diverged from.
- `dispatch/write_zig.md` — fires for `src/lib/contract/fixture_export.zig` (init/deinit pairing, errdefer, length caps).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE (`fixture_export.zig`) | yes — one new Zig file | allocator hygiene + errdefer; emitter imports `src/lib/contract` modules, re-declares nothing |
| File & Function Length (≤350/≤50/≤70) | yes — Rust crates + emitter | one module per wire type family; split before approaching caps |
| MILESTONE-ID | yes — new source + fixtures | no `M{N}` tokens anywhere in `rustd/` or `samples/fixtures/wire-v2/` |
| UFS | yes | lane names, fixture paths, wire version as named constants |
| SPEC TEMPLATE GATE | yes — this file | required sections filled, zero tpl residue |
| LOGGING | no | scaffold crates perform no runtime logging |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/exonum` — workspace layering: core crates carry no async runtime (its core `Cargo.toml` pulls `futures` but no tokio); mirrored here as Invariant 2. Its per-crate version duplication is superseded by `[workspace.dependencies]` — copy the intent, not the mechanism.
- **Reference:** `~/Projects/oss/core_api-develop` — `#![deny(unused_crate_dependencies)]` in every crate (verified in all 59 of its lib.rs files) and a single CI-enforced dependency-version source; adopt both via workspace-level lints and `[workspace.dependencies]`.
- **Reference:** `~/Projects/oss/bun` — the workspace mechanics: a VIRTUAL root `Cargo.toml` (no `[package]`) with an explicit member list, `[workspace.dependencies]`, and a `[workspace.lints]` block whose `warnings = deny` + `unreachable_pub = deny` rationale is adopted verbatim. Its crate DIRECTORY is not copied: bun nests members under `src/<name>/`, which Microsoft's `M-CRATES-FLAT-FOLDER` rejects, and Indy chose the guideline (Aug 23, 2026). Members live under `rustd/crates/<name>/`, dir == crate name, `afd_` prefix throughout — matching `~/Projects/oss/exonum`'s `components/` + `services/` shape.

## Sections (implementation slices)

### §1 — Workspace and hygiene

The cargo workspace every later crate joins; hygiene decided once, here, because lint policy is nearly impossible to retrofit. Layout follows Microsoft's `M-CRATES-FLAT-FOLDER`: a virtual root with an explicit member list and every crate a sibling in one direct subdirectory — `rustd/crates/<crate>/`, dir == crate name, `afd_` prefix (binary crate: `agentsfleetd`). **Implementation default:** edition 2024; `[workspace.lints]` denying `clippy::unwrap_used`/`expect_used`/`panic` in library crates plus `unused_crate_dependencies`; release profile `opt-level = 3` with `overflow-checks = true` (billing math crosses these crates later) — because the reference repos show both the cost of retrofitting (≈2,000 unwraps in core_api) and the wrong default (`opt-level = 'z'` on a server binary).

- **Dimension 1.1 — DONE** — workspace builds and tests under the pinned toolchain only → Test `test_workspace_builds_pinned`
- **Dimension 1.2 — DONE** — clippy runs with `-D warnings`, the workspace denies `unwrap_used`/`expect_used`/`panic`, and every member actually inherits that policy → Test `test_workspace_lint_policy`. (Amended at PLAN: `cargo metadata` does NOT surface lints — verified Aug 23, 2026, `cargo metadata --format-version 1` has no `lints` key on any package under cargo 1.98 — so the assertion reads the workspace manifest, which is the policy's single source. It also asserts each member carries `[lints]\nworkspace = true`, catching the real failure: a crate added without inheriting the policy, which metadata could not have caught either.)

### §2 — afd_core primitives

Domain primitives with zero I/O: the `ERR_*`-style error-code registry (single-source rule ported from `src/agentsfleetd/errors/error_registry.zig`), UUID v7 identity (mirroring `src/agentsfleetd/types/id_format.zig` semantics), config value types.

- **Dimension 2.1 — DONE** — UUID v7 validation parity: uppercase rejected, never normalized → Test `test_uuid_v7_rejects_uppercase`
- **Dimension 2.2 — DONE** — error-code registry declares each code once; uniqueness and format asserted → Test `test_error_registry_unique`
- **Dimension 2.3 — DONE** — afd_core and afd_wire depend on no async runtime, database, or HTTP crate → Test `test_core_dependency_freeze`

### §3 — afd_wire and golden fixtures

The serde port of the daemon↔runner wire types. The Zig emitter writes one canonical JSON fixture per wire type into `samples/fixtures/wire-v2/`; Rust tests deserialize and re-serialize each, comparing bytes. The emitter DEFINES the canonical byte form — field order as declared, no insignificant whitespace, explicit null/optional-emission policy, integer spelling — and emits a machine-readable `manifest.json` beside the fixtures listing every exported type and its per-type unknown-field policy; the Rust serde attributes mirror the manifest, so "byte-identical" is a defined claim, not a hope. **Implementation default:** the emitter runs as a make recipe regenerating the committed fixtures — because committed fixtures make drift a red diff, not a silent skew.

**Current-shape only — the version-one lease is excluded (Indy, Aug 23, 2026).** `contract.zig:16` re-exports `protocol_lease_v1`, so an unqualified "every exported type gets a fixture" rule would emit a version-one fixture and `afd_wire` would grow a version-one serde type to round-trip it. The emitter therefore carries a **declared exclusion list**, `protocol_lease_v1` its only entry, and `manifest.json` describes the current shape alone. The evidence: commit `312e09ced` (Aug 13, 2026) introduced `LEASE_WIRE_VERSION_V1` and `LEASE_WIRE_VERSION_CURRENT` in one commit, so "version one" names the pre-M157 shape rather than a designed protocol; `src/runner/daemon/control_plane_client.zig:96` posts `LEASE_REQUEST_CURRENT_JSON` unconditionally and 17 integration call sites do the same, so no in-tree code path emits version one. The exclusion is asserted, not assumed — an accidental re-admission fails `test_fixture_set_complete` rather than silently passing. `samples/fixtures/wire-v2/` is a literal directory name for the current shape, not one half of a versioned pair; no `wire-v1/` sibling exists or is planned. Nothing is deleted: the Zig daemon keeps its version-one path, which retires with the daemon.

- **Dimension 3.1 — DONE** — fixture set covers every CURRENT-shape wire type the Zig module exports, and the declared exclusion list is asserted too (both enumerations checked against the emitted manifest, never hand-counted; an accidental re-admission of the version-one lease fails the test) → Test `test_fixture_set_complete`
- **Dimension 3.2 — DONE** — deserialize→serialize byte-compare for every fixture → Test `test_wire_roundtrip_all_fixtures`
- **Dimension 3.3 — DONE** — the Rust wire-version constant equals the fixture-carried Zig value (2) → Test `test_wire_version_matches_fixture`
- **Dimension 3.4 — DONE** — unknown-field and optional-field handling mirrors the Zig parser's configured behaviour (read the parse options in `src/lib/contract/`, mirror via serde attributes) → Test `test_wire_unknown_field_policy`

### §4 — Repository lanes

The gating foundation. `make lint-all` gains `cargo fmt --check` + `cargo clippy -- -D warnings`; `make test-unit-all` gains `cargo test --workspace`; `make check-version` compares the workspace version to `VERSION`; both hooks gain a `*.rs` case; `test.yml` gains a Rust job; codecov gains a `rust-afd` flag. **Implementation default:** coverage via cargo-llvm-cov, floor set from the first measured baseline (never 0) — because it is the maintained llvm-native tool and a floor of 0 is a gate that grades nothing. **Landed at 100% for the whole project**, not measured-minus-margin: these crates carry no input/output, no runtime and no external dependency, so every line is reachable from a test and the suite reports 100.00% lines and 100.00% functions; a floor below what the suite achieves is slack, and the TypeScript packages already hold exactly this bar. Indy extended it at VERIFY: the codecov PROJECT target moves from `auto` to 100% and the measured set is app, website, the agentsfleet CLI and the Rust workspace ONLY — the Zig tree is added to `ignore`, because its lanes retired in §6 and nothing uploads a report covering it. The three `zig-*` patch targets are deleted for the same reason: a target on a flag that can never report is a gate that grades nothing.

- **Dimension 4.1 — DONE** — `make lint-all` fails on a formatting or clippy violation in `rustd/` → Test `test_lint_lane_rust`
- **Dimension 4.2 — DONE** — `make test-unit-all` runs the cargo suite and propagates failure → Test `test_unit_lane_rust`
- **Dimension 4.3 — DONE** — `make check-version` fails when the workspace version diverges from `VERSION` → Test `test_version_lane_rust`
- **Dimension 4.4 — DONE** — staged/pushed `*.rs` triggers the hook lanes → Test `test_hook_rs_dispatch`
- **Dimension 4.5 — DONE** — CI Rust job + `rust-afd` coverage flag report, as non-required contexts → Test `test_ci_rust_job_reports`. Confirmed on Pull Request #629: `test-unit-rustd` SUCCESS and `codecov/patch/rust-afd` SUCCESS, neither in `main`'s required-context set.

### §6 — Zig lane retirement (Indy override, Aug 23, 2026)

Scope added mid-milestone by Indy, overriding M181's "retirement is its own milestone" line. The deciding fact: **there are no production users.** The Zig daemon's last built revision keeps serving `api-dev`; nothing further is spent linting, testing, leak-checking, coverage-grading or deploying a codebase being replaced. Gating moves to the Rust workspace as it grows.

Deleted, not disabled — a lane commented out is a lane someone re-enables by accident: `lint-zig` and everything only it reached, the three `test-unit-agentsfleet*` lanes, `test-coverage-zig` + `test-coverage-grade`, `memleak`, the `test-integration` family, and their Continuous Integration (CI) jobs. `agentsfleetd` and `agentsfleet-runner` stop being deployed. The README coverage badge moves from `zig-agentsfleetd` to `rust-afd`.

**Consequence, recorded rather than hidden:** M177's dual-run row differ and M181's p95 baseline both named the LIVE Zig integration suite as their oracle. Deleting it deletes those oracles. The Rust daemon is proven on its own terms from here — those two specs are amended in the same change, so no later milestone consumes a lane that no longer exists.

- **Dimension 6.1 — DONE** — no Zig lint, unit, coverage, leak or integration make target survives, and no workflow references one → Test `test_zig_lanes_absent`
- **Dimension 6.2 — DONE** — `make lint-all`, `make test-unit-all` and `make harness-verify` still resolve and pass with the Zig lanes gone → Test `test_declared_commands_survive_retirement`
- **Dimension 6.3 — DONE** — the deploy path no longer ships `agentsfleetd` or `agentsfleet-runner` → Test `test_daemon_deploy_retired`

### §5 — Governance record

`.oracle/orly.json` stays untouched: its `commands` block already routes through the make targets this milestone extends, which satisfies `dispatch/write_rust.md`'s "repository owns the commands" assertion without a config edit. `orly doctor` must stay green.

- **Dimension 5.1 — DONE** — `orly doctor` green with the Rust lanes present and `.oracle/orly.json` unchanged → Test `test_orly_doctor_green`

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 (serial, one agent) | §1–§5 | Claude Code · Opus 5 · xhigh | gate and lane design is judgment-heavy, the diff is small, and every section depends on §1 — parallelism buys nothing here |

## Interfaces

```
afd_wire public surface  = /v1/runners wire types + path constants +
                           LEASE_WIRE_VERSION_CURRENT (= 2, fixture-asserted);
                           frozen inside the workspace until a runner-port family
Fixture layout           = samples/fixtures/wire-v2/<type>.json — generated only
                           by the Zig emitter, never edited by hand; current
                           shape only (protocol_lease_v1 declared-excluded)
Lane names               = make lint-all / test-unit-all / check-version
                           (unchanged names; the only repository claims)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Fixture drift | Zig wire types change after fixtures committed | regen recipe re-runs the emitter; Rust round-trip goes red in `make test-unit-all`; fix = regenerate + Rust type update in the same commit |
| Toolchain absent | CI or a fresh clone lacks the pinned toolchain | lane fails loud naming `rust-toolchain.toml` and the mise/brew install step; never silently skips |
| Clippy noise | a pedantic lint blocks sound code | per-site allow with a justification comment (exonum pattern); never a blanket allow, never weakening `[workspace.lints]` |
| Coverage context blocks merges | `rust-afd` flag made required prematurely | flag ships non-required (Invariant 5); the flip is an M181 cutover decision |
| Emitter divergence | fixture emitter re-declares types instead of importing them | emitter imports `src/lib/contract` modules directly; REVIEW checks the import list |
| Corrupt fixture | hand edit or partial write in `samples/fixtures/wire-v2/` | Rust tests fail with the offending filename; regen recipe restores canonical bytes |

## Invariants

1. The Zig `src/lib/contract` module remains the wire source of truth; Rust conforms to fixtures, never the reverse — enforced by the compare direction in `test_wire_roundtrip_all_fixtures` (fixtures are generated only from Zig).
2. afd_core and afd_wire carry no async runtime, database, or HTTP dependency — enforced by `test_core_dependency_freeze` over `cargo metadata`.
3. The `rustd/` workspace version equals `VERSION` — enforced by `make check-version`.
4. Library crates deny `unwrap`/`expect`/`panic` — enforced by `[workspace.lints]` + `cargo clippy -- -D warnings` inside `make lint-all`.
5. No new **required** branch-protection context lands in this milestone — enforced mechanically by rubric R4 (the diff must stay inside Files Changed, and no branch-protection surface is a repository file), with the PR #627 lesson (a required context that never reports blocks the PR) recorded as the reason.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes (build infrastructure only) | not applicable | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_workspace_builds_pinned` | the compiler resolved inside `rustd/` matches `rust-toolchain.toml`'s channel, and that channel satisfies the workspace `rust-version` floor; a drifted pin fails naming both values |
| 1.2 | unit | `test_workspace_lint_policy` | workspace manifest denies unwrap_used/expect_used/panic, AND every member manifest inherits via `[lints] workspace = true`; a member missing the inherit fails naming it |
| 2.1 | unit | `test_uuid_v7_rejects_uppercase` | an otherwise-valid uppercase UUID v7 string → validation error, not normalization |
| 2.2 | unit | `test_error_registry_unique` | duplicate or malformed code in the registry table → test failure naming the code |
| 2.3 | unit | `test_core_dependency_freeze` | dependency graph of afd_core/afd_wire contains no tokio/sqlx/axum/reqwest/redis |
| 3.1 | unit | `test_fixture_set_complete` | every current-shape wire type has exactly one fixture file; extras and gaps both fail; the manifest's exclusion list is exactly `protocol_lease_v1`, so re-admitting it fails |
| 3.2 | unit | `test_wire_roundtrip_all_fixtures` | for each fixture: parse → serialize → bytes identical; first differing byte reported |
| 3.3 | unit | `test_wire_version_matches_fixture` | version constant in fixture == afd_wire constant == 2 |
| 3.4 | unit (negative) | `test_wire_unknown_field_policy` | payload with an unknown field behaves exactly as the Zig parser is configured to behave |
| 3.4 | unit (negative) | `test_wire_rejects_malformed` | truncated/invalid JSON per type → typed error, no panic |
| 4.1 | integration (negative) | `test_lint_lane_rust` | a seeded fmt/clippy violation → `make lint-all` exit non-zero naming the file |
| 4.2 | integration (negative) | `test_unit_lane_rust` | a seeded failing test → `make test-unit-all` exit non-zero |
| 4.3 | integration (negative) | `test_version_lane_rust` | workspace version ≠ `VERSION` → `make check-version` exit non-zero |
| 4.4 | integration | `test_hook_rs_dispatch` | staged `.rs` file → pre-commit runs the Rust lane (observed in hook output) |
| 4.5 | integration | `test_ci_rust_job_reports` | CI run shows the Rust job + `rust-afd` flag reporting, both non-required |
| 5.1 | integration | `test_orly_doctor_green` | `orly doctor` exit 0 with `.oracle/orly.json` unchanged in the diff |
| 4.1 (FM) | integration (negative) | `test_toolchain_gate` | pinned toolchain removed from PATH → lane fails naming `rust-toolchain.toml` + the install step |
| 3.1 (FM) | unit | `test_emitter_imports_contract` | the emitter's type identities are the `src/lib/contract` module types (compile-time identity assert), never re-declarations |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Wire round-trip green (§3) | `cd rustd && cargo test -p afd_wire` | exit 0 | P0 | ✅ `test result: ok. 114 passed; 0 failed` (round-trip) + 6 strictness + 6 redaction = 126 in afd_wire, exit 0 |
| R2 | Fixture set complete + version-one excluded (§3) | `cd rustd && cargo test test_fixture_set_complete` | exit 0 | P0 | ✅ `test result: ok. 1 passed; 0 failed; ... 113 filtered out` |
| R3 | Lanes catch seeded defects (§4) | seeded-violation runs recorded in PR Session Notes | each lane exit non-zero, then green | P0 | ✅ six seeded defects, each red then green — see PR Session Notes for the verbatim runs |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | ✅ 138 changed paths against 46 table patterns, 0 uncovered (checked mechanically, not eyeballed — the first pass claimed clean and was wrong by 31) |
| R5 | Governance intact (§5) | `orly doctor` | exit 0 | P0 | ✅ `🟢 this repository's installed ruleset matches .oracle/orly.json` |
| R6 | Zig lanes retired (§6) | `git grep -nE '\b(lint-zig\|test-unit-agentsfleetd\|test-unit-agentsfleet-runner\|test-unit-agentsfleet-lib\|test-coverage-zig\|test-coverage-grade\|memleak\|test-integration)\b' -- Makefile make/ .github/workflows/` | no output | P0 | ✅ `hits: 0` — no retired lane referenced in Makefile, make/ or .github/workflows/ |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ `● ALL GATES GREEN ── ready for VERIFY` |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `✓ All unit lanes passed` — cargo workspace (166) + the four TypeScript coverage gates. First run of this lane in the worktree failed resolving `next/headers`; cause was worktree provisioning, not the diff (`main` passed the same lane), and `AGENTS.md`'s recipe is corrected in this branch |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | ✅ exit 0 — includes the new `lint-rustd` (rustfmt + Clippy, warnings are errors) and `lint-scripts` (111 self-tests) |
| S4 | Version sync | `make check-version` | exit 0 | P0 | ✅ `✓ all versions match 0.26.2` |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `INF no leaks found` |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -vE '\.(md\|json)$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | ✅ no output (scope amended — see the note below the table) |

**S6 scope note (amended at VERIFY):** the sweep excludes `.json` alongside `.md`. `dispatch/write_any.md:60` lists the LENGTH GATE's triggers — `.zig`, `.js`, `.ts`, `.tsx`, `.jsx`, `.py`, `.rs`, `.go`, `.sh`, `.sql`, `.yaml`/`.toml` — and `.json` is not among them, so the original command was BROADER than the rule it stands in for. The one file it caught is `samples/fixtures/wire-v2/manifest.json` (408 lines), generated output rather than source, kept indented on purpose: a minified manifest would make a regenerated wire type a single opaque changed line instead of a readable diff naming the type. Source files are unaffected — the sweep still fails on any oversize `.rs`, `.zig`, `.sh` or `.py` in the diff.

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Any server/runtime code — pools, Redis, axum, crypto (M176_001).
- `.oracle/orly.json` edits and branch-protection changes (recorded decision in §5).
- OpenAPI tooling changes — the existing Redocly pipeline and route-coverage script stay authoritative.
- The `agentsfleet-runner` port — the runner stays Zig behind the frozen wire seam for the whole family. §6 stops DEPLOYING it; it is not ported here.
- Deleting Zig SOURCE. §6 removes lanes, jobs and deploy steps only — `src/**`, `build.zig` and `build_runner.zig` are untouched, so the last built revision still compiles if anyone needs it.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a contributor commits a broken Rust change and watches `make test-unit-all` catch it, then sees the wire suite prove `afd_wire` speaks lease-wire v2 byte-for-byte: the port has a foundation whose green means something.
2. **Preserved user behaviour** — the TypeScript lanes behave byte-identically and the Zig daemon's SOURCE and build graph are untouched, so the revision serving `api-dev` still builds. Its lanes are not preserved: §6 deletes them by Indy's override, on the deciding fact that there are no production users. Nothing a user reaches changes, because no user reaches it.
3. **Optimal-way check** — direct: gates before code. The unconstrained-optimal adds nothing; deferring lanes until M176 is how unverified Rust lands.
4. **Rebuild-vs-iterate** — greenfield addition; nothing existing is refactored.
5. **What we build** — one workspace, two crates, one fixture emitter + committed fixtures, lane/hook/CI/coverage wiring.
6. **What we do NOT build** — server crates (M176), any handler (M177+), OpenAPI codegen (existing pipeline stays), a second version surface (single `VERSION`).
7. **Fit with existing features** — compounds with `orly gate` and the make lanes; must not destabilize the Zig build graph (`build.zig` untouched).
8. **Surface order** — N/A — no user surface (internal build infrastructure).
9. **Dashboard restraint** — N/A — no UI.
10. **Confused-user next step** — the failing lane names the cargo step, the file, and the toolchain requirement in its output; no new docs surface needed.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five slices — hygiene → primitives → wire parity → lanes → governance — because hygiene must precede code, and wire parity is the one provable claim available before any server exists.
- **Alternatives considered:** folding the scaffold into M176 (rejected: runtime code would land before the gates that grade it); generating Rust types from OpenAPI (rejected: the runner wire seam is not in the public OpenAPI and the Zig module is canonical).
- **Patch-vs-refactor verdict:** this is a **patch** (greenfield addition) because no existing code changes shape; the larger refactor — the daemon port itself — is the rest of the family, spec'd separately.

## Discovery (consult log)

- **Consults**
  - **Lease wire version one — does the port owe it compatibility?** Indy, Aug 23, 2026: no. The port implements the current shape only; the Zig daemon keeps its version-one path and retires with it. Evidence chain verified in-tree before the decision was applied: (a) `git log -S 'LEASE_WIRE_VERSION_V1' -- src/lib/contract/` and the same for `_CURRENT` both return exactly `312e09ced` (`git log -1 312e09ced` → `2026-08-13 feat(m157): close repairs on production evidence`), so both constants were born in one commit and "version one" names the pre-M157 shape rather than a designed protocol; (b) `src/runner/daemon/control_plane_client.zig:96` posts `protocol.LEASE_REQUEST_CURRENT_JSON` unconditionally — no branch, no configuration knob; (c) 17 integration call sites use the same constant, and the only version-one producers in the tree are `leaseWireVersion()`'s parse defaults (`src/agentsfleetd/http/handlers/runner/lease.zig:29,31,36`), which read a request rather than emit one. No in-tree code path emits version one.
  - **Stop condition — is a version-one runner binary deployed on an operator host?** No, and more strongly than the addendum assumed: `gh release list --limit 10` returns no rows at exit 0 and `git tag --list | wc -l` returns 0, so no runner artifact of any vintage has ever been released; `deploy/baremetal/deploy.sh:160` downloads the runner from a GitHub release tag and `deploy.sh:293` takes that tag as a positional argument, so `deploy/` pins no version; no fleet inventory file exists in the repository. The decision's factual premise holds.
  - **Two orly-MANAGED files were edited and will be reverted by the next `orly update`.** `docs/REST_API_DESIGN_GUIDELINES.md` cited `make lint-zig`, `make test-unit-agentsfleetd` and `make test-integration`, and `scripts/check_route_registration_doc.py` fails on a cited target that does not resolve — so the citations had to move or the gate stays red. `docs/VERIFY_TIERS.md` still names `make _lint_zig_test_depth` as the Test Baseline counter, which no longer exists; left as-is because no gate reads it. Both are pack-sourced: the durable fix is an orly pack change in that repository, which is outside this worktree and needs Indy's per-session cross-repo approval. Flagged rather than done.
  - **Zig lane retirement, pulled forward into this milestone.** Raised as a conflict: M181's Out of Scope reserved it ("Deleting or de-listing the Zig daemon, its lanes, or `make memleak` — retirement is its own milestone after a stable production window Indy defines"), M181 Invariant 2 kept the Zig binary deployable, and M177 §7 + M181 Dimension 3.2 named the live Zig integration suite as the parity oracle and latency baseline. Indy overrode all of it on a fact the objection had assumed away:
    > Indy (2026-08-23 19:4x): "Well there are no production users, since waiting for a complete removal will take time, i wanna build the milestones faster" — context: §6, deleting the Zig lint/unit/coverage/leak/integration lanes and the daemon deploy inside M175 rather than after cutover.
    > Indy (2026-08-23 19:4x): "the last deployed version will run in api-dev and we start moving towards linting, .. and so on to the rust one we are building" — context: the Zig daemon is frozen at its last built revision; gating moves to Rust.
    Accepted and executed as §6. The consequence — M177's dual-run differ and M181's p95 baseline lose their oracle — is written into both specs in the same change rather than left to be discovered at cutover.
  - **Layout — Microsoft `M-CRATES-FLAT-FOLDER` versus the bun canon.** The two references this spec names disagree: bun nests members under `src/<name>/`, while the Pragmatic Rust Guidelines (`~/Projects/oss/rust-guidelines/all.txt:1454-1498`) call crates under a `src/` folder "never acceptable" and `~/Projects/oss/exonum` uses `components/` + `services/`. Surfaced to Indy at PLAN with the structural difference drawn out (the clash is the middle directory's NAME — `rustd/`'s root is virtual, so nothing is genuinely nested). **Indy, Aug 23, 2026: take the Microsoft way.** Members moved to `rustd/crates/<name>/` before the first commit carrying them, and the family specs M176–M181 were repointed in the same edit so no sibling milestone inherits the old path. Bun stays the reference for the workspace MECHANICS (virtual root, explicit members, the `[workspace.lints]` rationale), not for the directory name.
- **Metrics review** — no product or operator signal changes: this milestone ships build infrastructure and a value layer, and the Metrics table records that as not applicable. No analytics or funnel change.
- **Docs-repo branch** — none. `~/Projects/docs` needs a branch for a public endpoint, command, flag or behaviour change; this milestone ships none. The Zig daemon's lanes and auto-deploy retire (§6), which is internal, and the revision serving `api-dev` is unchanged. Recorded here as the explicit why-not.
- **Skill-chain outcomes**
  - `/orly-write-unit-test` — invoked during implementation, before the first test was written. Mode Change-set plus Invariant and Regression; the Zig zero-leak and 100-connection concurrency proofs are not applicable, since both crates are value layers with no allocation-error path, no thread and no socket. 166 tests, 100.00% lines and 100.00% functions.
  - `/review` — run against the full diff before CHORE(close). One P0: three wire types carried a secret and derived `Debug`, so `{:?}` on a lease would log the provider key, the tenant secrets map, the minted credential and the runner token. Fixed with hand-written `Debug` in one module plus canary tests asserting the secret is absent from the rendered form AND present in the serialized form. Also caught 34 make variables, seven `make help` entries and one support module orphaned by §6, and an R4 claim that was wrong by 31 paths until checked mechanically.
  - `orly-babysit-prs` — runs after the push that opens the Pull Request.
- **Deferrals** — one, and it is a cross-repository blocker rather than a choice: `docs/REST_API_DESIGN_GUIDELINES.md` and `docs/VERIFY_TIERS.md` are orly-MANAGED, so the edits this milestone made to the first (and the stale `make _lint_zig_test_depth` reference left in the second) will be reverted by the next `orly update`. The durable fix is an orly pack change in that repository, which is outside this worktree and needs Indy's per-session cross-repo approval. Nothing else is deferred: every Dimension is DONE and every rubric row is graded.
