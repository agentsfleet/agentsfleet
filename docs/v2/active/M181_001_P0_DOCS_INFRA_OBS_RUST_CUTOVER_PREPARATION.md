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

# M181_001: Cutover preparation — the binary ships, the metrics flow, the lanes exist

**Prototype:** v2.0.0
**Milestone:** M181
**Workstream:** 001
**Date:** Aug 30, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — everything the swap needs that does not need the swap's route surface
**Categories:** DOCS | INFRA | OBS
**Batch:** B6 — family closer, first half; runs concurrent with M180_001
**Branch:** feat/m181-cutover-prep
**Test Baseline:** deferred to CHORE(close) per Indy override (Aug 29 2026, recorded on M180_001): no `make test-unit-all` / `make test-integration-rustd` runs mid-milestone — `cargo fmt` + `cargo clippy` per section only; the full declared `verify.*` set runs once at the boundary, where the Test Delta is graded against `origin/main`'s counts
**Depends on:** M177_001 (runner plane); M176_001 (substrate, `afd_observability`)
**Provenance:** split from the single M181_001 cutover spec (LLM-drafted, Claude Fable 5, Aug 23, 2026) on the axis "needs the full route surface or does not"; this half does not
**Canonical architecture:** `docs/architecture/observability.md` §The three signal paths + `docs/architecture/runner_fleet.md` §Multi-replica

---

## Overview

**Goal (testable):** a Rust `agentsfleetd` binary links statically for both linux targets, ships in the release artifact set as the only daemon, runs on the distroless base the deployment uses, exports a metric family registry that matches the Zig daemon's names and label keys, and the parity/benchmark lanes exist and refuse to run with their budget constants unset — all provable while M180_001 is still in flight.

**Problem:** the cutover milestone was one spec whose four slices had two different readiness dates. Half of it — the route parity gate, the OTLP boot wiring, the soak, the swap — cannot start until every route serves from Rust, because it grades the whole route surface or edits the exact boot files M180_001 is rewriting. The other half is blocked on nothing, and it carries the milestone's only two genuine unknowns: whether `aws-lc-sys` cross-compiles static against musl, and whether OpenTelemetry SDK Views can express every Zig metric-family spelling. Discovering either at swap time is discovering it in the worst place.

**Solution summary:** take the half that does not need the route surface and land it first. Bring the Continuous Integration (CI) actions onto a supported Node runtime, prove the musl cross-compile before designing anything on top of it, ship the Rust daemon as the only binary in an image that carries nothing else, build the metrics pipeline inside `afd_observability` where no other stream is writing, create the parity and benchmark lanes with their constants declared, and write the runbook skeleton and probe framework that M181_002 fills in. Every unknown that could reshape the cutover gets answered while M180_001 finishes.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(rustd): cutover preparation — supported CI runtime, distroless Rust release, metrics pipeline, parity lanes
- **Intent (one sentence):** everything the production swap depends on that does not depend on the production route surface, landed and proven while the ingress port finishes.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/observability.md` §The three signal paths — the daemon pushes all three signals over OTLP with no pull endpoint; §3's family registry is graded against what that document declares, and §5 reconciles the one place a sibling document contradicts it.
2. `rustd/Cargo.toml` around the `aws-lc-rs` pin — the workspace records that the musl cross-compile is unproven and names this milestone as where it gets proven. Read the reasoning before §2.
3. `.github/workflows/release.yml` + `Dockerfile` + `make/build.mk` — the build/ship path the Rust binary joins, including `make push` (CI/CD edits — explicit user approval per repository rule; this spec is the record, and REVIEW re-confirms before merge).
4. `docs/RUST_ERROR_STANDARD.md` — `afd_observability` is listed there as having no fallible function. §3 ends that, so the crate takes the standard's shape on the commit that does.
5. `docs/LOGGING_STANDARD.md` §8A — the Rust binding, and `[JUDGMENT → EVENT-COMPAT]`: a port preserves the event bytes dashboards match on. The same principle governs metric family names in §3.
6. `make/test-integration-rustd.mk` — the declared `verify.integration` lane, and the file §4's parity lane sits beside rather than duplicating.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `.github/workflows/*.yml` | EDIT | §1: every action pin moves to a version whose runtime survives the hosted-runner Node 20 removal |
| `.github/actions/*/action.yml` | EDIT | §1: the two nested composite actions carry pins of their own; a workflow-only sweep leaves them stale |
| `.github/workflows/release.yml` | EDIT | §2: the Rust binary joins the target matrix and the artifact set |
| `.github/workflows/deploy-dev-build.yml` | EDIT | §2: the dev image gets the Rust daemon; the Zig daemon build goes |
| `audits/gh-actions-runtime.sh` | CREATE | §1: the pin gate — retired runtimes and mutable refs |
| `make/quality.mk` | EDIT | §1: the pin gate rides `check-gh-actions-valid` |
| `playbooks/deploy/{dev,prod}/001_playbook.md` | EDIT | §2: there is no shell in the API container |
| `Dockerfile` | EDIT | §2: a distroless base carrying the Rust daemon and nothing else |
| `make/build.mk` | EDIT | §2: the local image build produces the Rust daemon for both architectures |
| `rustd/crates/afd_observability/**` | EDIT | §3: metric instruments, the family registry, SDK Views pinned to the Zig spellings, and the crate's first error type |
| `rustd/Cargo.toml` | EDIT | §2: the shipped profile strips debug info; §3: `opentelemetry_sdk` gains the `metrics` feature |
| `make/test-parity.mk` | CREATE | §4: the black-box HTTP parity lane, parameterised by base URL (distinct caller: the cutover checklist) |
| `make/bench.mk` | EDIT | §4: `bench-cutover` adds a comparison mode with budget constants that refuse to be unset |
| `make/dry.mk` | EDIT | §4: dry lane variant booting the Rust daemon |
| `make/test-integration-rustd.mk` | EDIT | §4: the run-verdict guard moves inline as its script is swept |
| `scripts/rustd_lane_benchmark.py` | DELETE | §4 sweep: no caller in `make/` or `.github/` |
| `scripts/rustd_lane_benchmark_test.py` | DELETE | §4 sweep: self-test of a deleted script |
| `scripts/rustd_lane_contract_test.py` | DELETE | §4 sweep: static contracts over a deleted orchestration |
| `scripts/rustd_lane_result.py` | DELETE | §4 sweep: behaviour preserved inline in the lane |
| `scripts/rustd_lane_result_test.py` | DELETE | §4 sweep: self-test of a deleted script |
| `playbooks/cutover/rust_daemon.md` | CREATE | §5: the runbook skeleton, drain order, abort criteria, divergence register |
| `playbooks/cutover/probes.sh` | CREATE | §5: the probe runner and its row-coverage assert |
| `docs/architecture/observability.md` | EDIT | §5: the export path decision — standard knobs, collector-owned fan-out |
| `docs/architecture/runner_fleet.md` | EDIT | §5: the stale Prometheus-scrape claim reconciled against the deployed configuration |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — TIM (budget numbers are named constants, never vibes), UFS (drain timeouts, family names, target triples as named constants), NDC (no dead code at write time), NLR (touch-it-fix-it on the lane the sweep edits), ORP (orphan sweep — the five swept scripts leave no reference behind), TST-NAM, MSID, FLL.
- **`docs/RUST_ERROR_STANDARD.md`** — read before adding the first fallible signature to `afd_observability`: one error type, `pub type Result<T, E = Error>` beside it, `#[from]` composition, `map_err` only to add caller-only context, `source()` never returning your own kind.
- **`docs/LOGGING_STANDARD.md`** §8A + §4 — the Rust `tracing` binding, the `event` field on every emit, hoisted field expressions, the boundary `_started`/`_completed`|`_failed` pair, per-iteration paths at `debug`, and `source=env:NAME` never the value.
- `dispatch/write_rust.md` — ownership, justified `unsafe`, preserved error variants, deterministic concurrency tests; REVIEW cites the Microsoft guideline mnemonics for the instrument and registry code.
- `dispatch/write_shell.md` — `probes.sh`: quoted expansions, array arguments, no untrusted `eval`.
- `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — the runbook and both architecture-document edits are published prose.
- `dispatch/verify.md` — done-claims here are exactly the rubric rows; no package-scoped substitutes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | yes | the registry is a data table, not a function; Views are configured in a loop over it |
| LOGGING | yes | §3's instrument construction logs its boundary pair; endpoint values log as `source=env:NAME` |
| MILESTONE-ID | yes | none in source; runbook and architecture edits are docs (exempt) |
| UFS | yes | budget constants, target triples, family names, drain timeouts all named |
| SPEC TEMPLATE GATE | yes — this file | required sections filled |
| SCHEMA GUARD | no | no schema change — that is the rollback story this half preserves |
| ERROR REGISTRY | yes | `afd_observability`'s new error type maps its user-visible variants to registry codes |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/afd_observability/src/export.rs` — the bounded-buffer + drop-counter export wrapper M176 shipped for spans. §3's metrics exporter takes the same shape rather than inventing a second one; the property it states ("export that cannot slow a request down, and says so when it loses spans") is the property the metrics half must also hold.
- **Reference:** `docs/RUST_ERROR_STANDARD.md` §Conformance — `afd_core`, `afd_crypto`, `afd_db`, `afd_redis` already carry the `struct Error` + private `ErrorKind` shape. `afd_observability` copies it rather than inventing a third.
- **Reference:** `.github/workflows/release.yml` `binaries-linux-x86` + `verify-runtime-compat` — the existing static-ELF assertion and multi-distribution runtime check. §2 mirrors that shape for the Rust binary rather than trusting a successful compile.
- **Reference:** `make/test-integration-rustd.mk` — the lane shape (tally file, labelled progress wrapper, explicit verdict) §4's parity lane mirrors.

## Sections (implementation slices)

### §1 — The CI actions run on a supported runtime — DONE

Every GitHub Actions pin moves to a version whose runtime survives the hosted-runner Node 20 removal, including the two pins nested inside this repository's own composite actions, and the one third-party action floating on a mutable `master` ref gets a pinned commit.

Two pins are load-bearing rather than hygienic: the secret scanner runs on every pull request, so its removal date breaks every pull request; the release-notes publisher fires at tag push after the binaries have already built, so its removal date breaks a release halfway through one.

- **Dimension 1.1** — no workflow or composite action references an action whose runtime is Node 20, and none floats on a mutable ref → Test `test_action_runtimes_supported` — **DONE** (`audits/gh-actions-runtime.sh`, wired into `check-gh-actions-valid`)
- **Dimension 1.2** — every workflow still parses and every `make` target a workflow names still exists → Test `test_workflows_lint_clean` — **DONE**

### §2 — The Rust binary cross-compiles and ships, in an image that carries nothing else — DONE

The musl cross-compile is proven FIRST, because the workspace records it as unproven and the whole section rests on it: `aws-lc-sys` compiles a C library, which is the one dependency that can refuse to link static against musl. Only once it links does the rest follow.

**Decided at PLAN, and it collapsed the section — one binary, not two (Indy, this stream).** The spec was drafted for a dual-binary image with a selection knob: both daemons at distinct paths, one knob choosing which serves. Indy's call is that no Zig daemon ships at all. That is strictly simpler and it resolves, rather than answers, three of the questions this section was written around:

- **The naming collision disappears.** It only existed because two binaries had to share the artifact name and the image path `/usr/local/bin/agentsfleetd`. With one daemon in the image, the artifact carries `-rs` to say what built it and the in-container path is unchanged, so nothing downstream — `fly.toml`, the process command, the deploy — has to learn a new name.
- **The selection knob disappears with it.** There is nothing to select between.
- **Rollback becomes the container's own mechanism.** The previous image digest is the rollback, which the registry retains and the platform deploys by digest already. This also settles the contradiction the parent spec carried between "the Zig binary stays warm in the artifact set" and "rollback is a hand-dispatched frozen revision": neither, and no binary artifact is load-bearing for it.

The image is distroless as a consequence rather than a preference: a static binary that spawns no child process needs a certificate bundle and a clock, which is what `static-debian12` is.

- **Dimension 2.1** — the daemon links statically for both linux targets with zero dynamic dependencies and no interpreter, asserted on every release build → Test `test_rust_binary_static` — **DONE**
- **Dimension 2.2** — a release produces the daemon for both linux architectures under `-rs` artifact names, reporting the version in `VERSION` → Test `test_release_artifact_set` — **DONE**
- **Dimension 2.3** — the daemon serves from the distroless image, proven by the parity lane's single-target mode against a container-hosted daemon; the release job's runtime check covers the CI side → Test `test_runtime_on_production_base` (graded with §4's lane)

### §3 — The metrics pipeline, in the crate shaped to receive it

`afd_observability` carries the span pipeline and the export wrapper. It carries no metric instrument, no aggregation, and no family registry — so a transport plugged in later would carry an empty payload. This section builds the pipeline half, entirely inside the crate, where it needs nothing from the boot path and collides with no other stream.

**The implementation is SDK configuration, not a port.** The Zig daemon hand-rolls instruments, delta windows, label-dimension products, cardinality caps and payload encoding across roughly 1,450 lines because Zig has no OpenTelemetry SDK. Rust has one, and it is already a workspace dependency — the whole cluster is a feature flag plus configuration. Taking that trade is the single largest crate-versus-scaffolding decision in the family, and it deletes 1,450 lines that would otherwise be ours to keep correct.

**What the trade costs, stated rather than discovered.** The SDK implements the specification's cardinality limit, whose overflow marker is the attribute `otel.metric.overflow=true` — not the Zig registry's `_other` label value. Same protection, different wire shape, and a dashboard panel built against the Zig daemon reads the difference as a renamed series. So the pipeline is configured with SDK Views that pin family names, label keys and the overflow spelling to the Zig registry's. Where a View cannot express a Zig spelling, the divergence is registered in §5 rather than silently accepted because the SDK preferred it.

**The crate gains its first error type here.** Constructing an instrument set from a registry is fallible, and `docs/RUST_ERROR_STANDARD.md` lists this crate as having no fallible function today. It takes the standard's shape on the commit that ends that — not later, and not exempt for predating the rule.

**The oracle already exists, and it is not the Zig source (found while planning §3).** The section was written assuming the Rust registry would be graded against `otel_metrics_families.zig` — which would mean parsing Zig from a Rust test, or freezing a copied fixture that drifts. Neither is needed: `docs/architecture/observability.md` carries a **Metric family census** that lists every exported family exactly once, and the Zig suite already pins its own registry against it (`test_census_matches_exported_families`, which fails both on a census entry that is not a declared family and on a declared family missing from the census). The census names 71 families; the Zig `MetricId` enum declares 71.

So the census is the contract, and both implementations are graded against the same document rather than against each other. That survives the Zig daemon's retirement, which a fixture extracted from its source would not, and it means a family added to one side without the other fails on both.

- **Dimension 3.1** — every family the architecture census names is emitted under that exact name, with the census's label keys and the declared value type; a family on one side only is named and fails → Test `test_metric_family_registry_parity`
- **Dimension 3.2** — past the cardinality cap, memory stays constant and the overflow series carries the pinned Zig spelling rather than the SDK default → Test `test_metric_cardinality_overflow_spelling`
- **Dimension 3.3** — the metrics exporter drops rather than blocks when its queue fills, and the drop counter climbs, exactly as the span exporter does → Test `test_metric_export_drops_never_blocks`
- **Dimension 3.4** — the crate's error type composes its sources by `#[from]`, and no variant's `source()` returns its own kind → Test `test_observability_error_chain_shape`

### §4 — The lanes, and the scripts they no longer need

The lanes the cutover grades against, built while there is nothing yet to grade: a black-box HTTP parity harness parameterised by base URL so one suite can run against either daemon, a benchmark comparison mode, and a dry-lane variant that boots the Rust daemon.

The parity harness is deliberately NEW code rather than a repointed Zig suite. The Zig integration corpus imports Zig modules and calls them directly — of 145 such files, three use an HTTP client — so pointing it at a Rust-served environment still exercises Zig handler code. A green run would report a pass rate for the implementation being retired, which is worse than no number because it reads like evidence.

**Budgets refuse to be unset.** The latency budget per route class and the resident-set ceiling are named constants embedded in the benchmark lane, and the lane exits non-zero when they are unset, so the gate M181_002 leans on is a real command with real numbers rather than a judgment.

**The sweep.** The five `scripts/rustd_lane_*.py` files go. Four have no caller. The fifth is the run-verdict guard both Rust lanes invoke — the check that a suite which silently ran nothing fails instead of passing — so its BEHAVIOUR moves inline into the lane that calls it. The guard is preserved; the script is not.

- **Dimension 4.1** — the parity lane runs the same suite against two base URLs and diffs status, body and the contract headers per route × method; a seeded difference fails naming the route → Test `test_parity_lane_detects_difference`
- **Dimension 4.2** — the benchmark lane refuses to run with either budget constant unset, and passes with both set → Test `test_bench_cutover_refuses_unset_budget`
- **Dimension 4.3** — the dry lane boots the Rust daemon and its page renders pass → Test `test_dry_lane_rust_variant`
- **Dimension 4.4** — a Rust lane whose suite ran zero tests fails, and one whose child exits non-zero fails, with no Python script on the path → Test `test_lane_guard_inline_rejects_silent_noop` — **DONE**

### §5 — The runbook skeleton, the probe framework, and two documents that disagree

The cutover runbook and its executable probe runner, built to the point where M181_002 fills in the rows the swap needs: drain order, abort criteria, the one-move rollback, and the declared-divergence register that lets a parity differ tell a declared difference from a regression. Its first entry is recorded, inherited from M175.

The probe runner's completeness assert is over ROWS, not probes: every rubric row of the merged milestones is either tagged by at least one probe or named in an exclusion manifest the script prints on every run. This half wires the assert and covers the merged milestones; M181_002 adds the rows its own dependencies contribute.

**The export path is recorded as an architecture decision here.** The daemon is a pure OTLP pusher to one configured endpoint, addressed by the OpenTelemetry specification's own environment names. Vendor fan-out belongs to a collector, not to the daemon, so moving from one backend to another is collector configuration and not a daemon redeploy. `docs/architecture/observability.md` currently describes the direct-to-vendor path the Zig daemon runs; it gains the decision and its reasoning.

**And one document contradicts the deployment.** `docs/architecture/runner_fleet.md` states that a platform Prometheus scrapes a metrics block in the production Fly configuration. No such block exists in either environment's configuration, and `docs/architecture/observability.md` states the daemon has no pull endpoint at all. A milestone that grades metric continuity cannot cite a document that describes a scrape path the deployment does not have.

- **Dimension 5.1** — the probe runner executes end to end and its row-coverage assert fails on an uncovered row, an untagged probe, or an undeclared skip → Test `test_probe_runner_row_coverage`
- **Dimension 5.2** — the runbook's rollback path invokes no migration command, and the probe runner asserts that absence rather than trusting the prose → Test `test_rollback_carries_no_migrate`
- **Dimension 5.3** — the architecture documents agree with the deployed configuration on whether a pull endpoint exists → Test `test_architecture_matches_deployed_metrics_path`

## Parallelization & execution map

(Internal batch labels here sequence THIS milestone's work only; the frontmatter **Batch:** line is the family-level ordering — two different axes, deliberately.)

| Batch | Scope | Runtime · model · reasoning tier | Why |
|---|---|---|---|
| B1 (first, alone) | §2 musl cross-compile proof | Claude Code · Opus 5 · xhigh | the milestone's largest unknown; a refusal to link reshapes §2 entirely, so it runs before anything is built on it |
| B1 | §1 CI actions | Claude Code · Opus 5 · high | mechanical version bump with an exact oracle, and a deadline of its own |
| B2 | §3 metrics pipeline | Claude Code · Opus 5 · xhigh | SDK configuration where a wrong View is a renamed dashboard series rather than a failure |
| B2 | §4 lanes and sweep | Claude Code · Opus 5 · high | new harness code plus a guard-preserving deletion |
| B3 | §5 runbook, probes, architecture reconciliation | Claude Code · Opus 5 · high | published prose and an executable assert over the merged rubrics |

## Interfaces

```
Release artifacts     both daemons, distinct names, versions from VERSION
Image                 distroless; the Rust daemon at /usr/local/bin/agentsfleetd,
                      no shell, no package manager. Rollback is the previous
                      image digest, which the registry retains.
make test-parity      BASE_URL=<url> — black-box HTTP suite, either daemon
make bench-cutover    comparison mode; refuses to run with budgets unset
make dry-app          Rust daemon variant
playbooks/cutover/    rust_daemon.md (runbook + divergence register)
                      probes.sh (probe runner + row-coverage assert)
Metric families       names, label keys and overflow spelling pinned by SDK Views
                      to the Zig registry's
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| musl cross-compile refuses | the C crypto backend will not link static against musl | §2 stops and the decision is surfaced with its evidence — a different backend, a glibc image, or a dynamic link are all user decisions, never the agent's; nothing downstream is designed on an assumption that failed |
| Binary name collision | both daemons claim the same artifact and image path | release lane fails naming both paths; the image build refuses rather than overwriting one binary with the other |
| Metric family drift | an SDK View cannot express a Zig spelling | the family is named, registered as a declared divergence in §5's register, and fails the parity test until it is registered — never silently accepted |
| Cardinality overflow spelling differs | SDK default marker instead of the pinned one | `test_metric_cardinality_overflow_spelling` fails; the dashboard panel that would have broken is the one the test stands in for |
| Metrics export blocks a request | an exporter that waits rather than drops | `test_metric_export_drops_never_blocks` fails; the property is the reason the wrapper exists |
| Lane guard lost in the sweep | the deleted script's behaviour not preserved inline | `test_lane_guard_inline_rejects_silent_noop` fails on the seeded silent no-op, which is exactly the run the guard exists to catch |
| Budget constant unset | a benchmark lane that grades nothing | the lane exits non-zero and names the unset constant; a lane that runs with no budget is the failure, not the passing run it would report |
| Stale action pin missed | a nested composite action not swept | `test_action_runtimes_supported` fails naming the file, because it reads composite actions as well as workflows |

## Invariants

1. No schema or data migration is introduced by this milestone — the rollback story of the whole family rests on it, and it is enforced by the absence of `schema/` from Files Changed plus the SCHEMA GUARD.
2. Budgets are named constants compared mechanically, never prose judgments, and a lane refuses to run with one unset — `test_bench_cutover_refuses_unset_budget`.
3. Telemetry cannot slow the request path: instruments record through atomics, export runs on a background reader with a bounded queue and a bounded timeout, and loss is counted rather than absorbed — `test_metric_export_drops_never_blocks`.
4. Every metric family exported by the Rust daemon is either byte-identical to the Zig registry's declaration or listed in §5's declared-divergence register — `test_metric_family_registry_parity` fails on any third case.
5. The run-verdict guard survives its script: a suite that ran nothing fails — `test_lane_guard_inline_rejects_silent_noop`.
6. Every probe in the probe runner carries a rubric row tag, and every row is tagged or manifest-declared — `test_probe_runner_row_coverage`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| the Zig-declared metric families, emitted from Rust | ops | unchanged from the Zig daemon | names and label keys pinned by View | no tenant identity in labels; cardinality capped | `test_metric_family_registry_parity` |
| metric export drop counter | ops | an export batch fails | count only | none needed | `test_metric_export_drops_never_blocks` |
| `deploy.serving_binary` (one label on existing deploy telemetry) | ops | deploy or swap | binary name, environment | none needed | `test_deploy_binary_selection` |

No product-analytics changes — this milestone adds operator signal only, and the families it adds are the ones the Zig daemon already declares.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_action_runtimes_supported` | every `uses:` in workflows and composite actions resolves to a supported runtime; a mutable ref fails naming the file |
| 1.2 | unit | `test_workflows_lint_clean` | the workflow linter exits 0 and every `make` target named by a workflow exists |
| 2.1 | integration | `test_rust_binary_static_and_portable` | both linux targets link with zero dynamic dependencies and no interpreter; the binary answers on all three runtime distributions |
| 2.2 | integration | `test_release_artifact_set` | the artifact set contains both daemons under distinct names, each reporting `VERSION` |
| 2.3 | e2e | `test_deploy_binary_selection` | the knob flips the served binary on a staging machine with a clean drain, and flips back |
| 3.1 | unit | `test_metric_family_registry_parity` | every Zig-declared family emits under the same name, label keys and value type; a one-sided family is named and fails |
| 3.2 | unit | `test_metric_cardinality_overflow_spelling` | past the cap memory is constant and the overflow series carries the pinned spelling, not the SDK default |
| 3.3 | unit (negative) | `test_metric_export_drops_never_blocks` | a full queue drops and increments the counter; the recording call returns without waiting |
| 3.4 | unit | `test_observability_error_chain_shape` | every variant carrying a cause exposes it through `source()`, and no `source()` repeats its own kind |
| 4.1 | integration (negative) | `test_parity_lane_detects_difference` | identical daemons diff empty; a seeded status or header difference fails naming route and method |
| 4.2 | unit (negative) | `test_bench_cutover_refuses_unset_budget` | unset budget → non-zero exit naming the constant; both set → runs |
| 4.3 | e2e | `test_dry_lane_rust_variant` | the dry lane boots the Rust daemon and its page renders pass |
| 4.4 | integration (negative) | `test_lane_guard_inline_rejects_silent_noop` | a suite reporting zero tests fails; a non-zero child fails; no Python interpreter is invoked |
| 5.1 | integration (negative) | `test_probe_runner_row_coverage` | an uncovered row, an untagged probe, and an undeclared skip each fail; a complete set passes |
| 5.2 | unit (negative) | `test_rollback_carries_no_migrate` | the runbook's rollback section invokes no migration command, asserted by the probe runner rather than by reading |
| 5.3 | unit | `test_architecture_matches_deployed_metrics_path` | no architecture document claims a scrape configuration absent from the deployed configuration |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | CI actions on a supported runtime (§1) | `actionlint && bash audits/gh-actions-runtime.sh` | exit 0 | P0 | |
| R2 | Rust binary cross-compiles static for both linux targets (§2) | `make dist-daemons` | exit 0 | P0 | |
| R3 | The daemon serves from the shipped image, proven black-box (§2+§4) | `docker run -d -p 3000:3000 <image>` then `make test-parity BASE_URL=http://127.0.0.1:3000` | exit 0 | P0 | |
| R4 | Metric family registry parity and overflow spelling (§3) | `cd rustd && cargo test --package afd_observability metric_` | exit 0 | P0 | |
| R5 | Lanes exist, refuse unset budgets, and preserve the run guard (§4) | `make test-parity BASE_URL=http://127.0.0.1:8080 && make bench-cutover` | exit 0 each | P0 | |
| R6 | Probe runner row coverage holds (§5) | `bash playbooks/cutover/probes.sh --self-test` | exit 0 | P0 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | |
| S4 | Version sync | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S6 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Command source rule:** S1–S4 are copied verbatim from `.oracle/orly.json` (`conform`, `verify.lint`, `verify.unit`, `verify.version`) — the set `orly gate` runs; S5–S6 are the template's repository hygiene gates (secret scan, oversize sweep), deliberately outside the declared set; R-rows name oracles this spec's own Files Changed create, so every command is copy-paste by merge time.

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + one decisive line. **Ship gate:** every P0 ✅ → CHORE(close)-eligible; any ❌ → EXECUTE.

## Dead Code Sweep

Five files under `scripts/`, all named `rustd_lane_*.py`, are deleted in §4.

Four have no caller anywhere in `make/` or `.github/`. The fifth is invoked twice by the Rust integration lane as the run-verdict guard; its behaviour — a suite that ran nothing fails, a non-zero child fails — moves inline into that lane in the same commit, and `test_lane_guard_inline_rejects_silent_noop` proves the guard survived the file. Deleting the guard's behaviour along with its script would reopen the silent-no-op hole it was written to close, so the test is a precondition of the deletion rather than a follow-up to it.

Per RULE ORP, the sweep leaves no reference behind: the lane's invocations go with the scripts, and the discovery pattern that runs every `scripts/*_test.py` self-test simply finds three fewer files.

## Out of Scope

- The route parity gate, the OTLP transport at boot, the staging soak, and the production swap — all of M181_002, which needs the full route surface this milestone deliberately does not wait for.
- Deleting Zig source. Its lanes are already gone; the binary and its source remain, because the binary IS the rollback this milestone ships.
- Any behaviour change on a live surface. This milestone adds a build target, a metrics pipeline, lanes and documents; it changes no endpoint, command, flag, or response.
- Deploying collectors. §5 records the export-path decision and the standard knobs that make a collector a configuration choice; standing one up is deployment work, sequenced before the swap in M181_002 so that infrastructure change and binary change stay separately attributable.
- Public docs (`~/Projects/docs`): no endpoint, command, flag, or behaviour change ships, so no docs-repository branch — recorded here as the why-not.

---

## Product Clarity (authoring record)

1. **Successful user moment** — N/A — no user surface. The operator-facing moment: a release produces two daemons, and flipping one knob on staging changes which one answers, with dashboards unbroken either way.
2. **Preserved user behaviour** — everything. No endpoint, command, flag, or response shape changes.
3. **Optimal-way check** — proving the cross-compile before designing the pipeline around it beats discovering a linker refusal after the release workflow is rewritten; building the metrics pipeline from the SDK beats porting 1,450 lines of hand-rolled aggregation that Rust's ecosystem already solves.
4. **Rebuild-vs-iterate** — iterate on the pipeline shapes that exist (release workflow, lane structure, export wrapper); rebuild nothing.
5. **What we build** — a supported CI runtime, a proven static musl cross-compile, a distroless release image carrying only the Rust daemon, the metrics pipeline, three lanes, a runbook skeleton with an executable probe runner.
6. **What we do NOT build** — the route parity gate, the OTLP transport at boot, the soak, the swap, Zig retirement, collector infrastructure, new dashboards.
7. **Fit with existing features** — rides the existing release and deploy workflow shapes; must not destabilize the path that ships the Zig binary, which remains the rollback.
8. **Surface order** — N/A — no user surface.
9. **Dashboard restraint** — nothing new to show. The metric families this adds are the ones the Zig daemon already declares; continuity is the deliverable, and a new panel would be the defect.
10. **Confused-user next step** — N/A for users. An operator reading a failed lane gets the constant name or the route that differed, never a bare non-zero exit.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** five slices ordered so the milestone's largest unknown resolves first — cross-compile, then the pipeline and lanes that assume a shipping binary, then the documents that describe them.
- **Alternatives considered:** keeping the cutover as one milestone (rejected: half of it cannot start until the ingress port merges, and that half carries no unknowns while this half carries both — sequencing them serially would idle the risky work behind the mechanical work); porting the Zig aggregation cluster to Rust (rejected: the SDK is already a workspace dependency, and 1,450 lines of hand-rolled aggregation exist because Zig had no SDK, not because the design wanted them); deleting the run-verdict guard with its script (rejected: it closes a hole a green run cannot detect, so its behaviour moves inline).
- **Patch-vs-refactor verdict:** this is a **patch** to the operational layer — pipelines, lanes, one crate's interior — with one deliberate deletion whose behaviour is preserved by test.

## Discovery (consult log)

- > Indy (2026-08-30): "Okay Indy appreciates your fix" — the gitleaks cache-key hit resolved by restructuring to a block scalar, no suppression added.
- > Indy (2026-08-30): "I think in the VERIFY step you will need to check the container in local with `make test-integration-rustd` along with it." Superseded same day: "image-check (not needed) — since you can verify like test-parity does." So the container proof is the parity lane pointed at a daemon served FROM the image (single-target mode, `BASE_URL`), not a bespoke target; rubric R3 says so. `dist-daemons` stays — `build`/`push` need it.
- > Indy (2026-08-30): "remove any arcade decisions we took in zig for containers" / "I donot want zig or legacy belching crap for agentsfleetd (zig)" — the image carries the Rust daemon alone; `build-linux-alpine` (a Zig-daemon build target with its own stale Zig download) removed with its Makefile help row.
- **Lane run-locations (decided, 2026-08-30):** `test-parity` diff mode runs LOCALLY on the compose stack — both daemons against identically reset datastores, which staging cannot provide since one daemon serves it at a time; its single-target mode reruns against staging in M181_002's soak. `bench-cutover` comparison mode runs locally on one machine (relative tolerance is what survives a hardware change); absolute RSS/latency ceilings are graded on staging Fly machines in M181_002 via the exported families, because a workstation number says nothing about a shared-cpu-4x/4GB machine.
- **OTLP-pure invariant (Indy, 2026-08-30): every backend is an OTLP gateway.** The daemon exports OTLP only; the collector's exporters are `otlphttp` ONLY — no vendor-native exporters (no loki/elasticsearch/prometheusremotewrite). A backend without a native OTLP intake is not a supported backend. The one permitted vendor-awareness is a per-backend temporality/transform processor in collector configuration, never a daemon change. Collector deployment shape for M181_002: a per-environment Fly app mirroring `cloudflared-{env}` (own small vm, config baked by Dockerfile, no public service, inbound over 6PN at `otel-{env}.internal`, outbound egress to vendors).

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
