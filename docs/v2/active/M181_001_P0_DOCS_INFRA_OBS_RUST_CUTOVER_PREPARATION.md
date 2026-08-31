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
| `make/quality.mk` | EDIT | §1: the pin gate rides `check-gh-actions-valid`; §4: `lint-all` gains the parity harness self-test |
| `playbooks/deploy/{dev,prod}/001_playbook.md` | EDIT | §2: there is no shell in the API container |
| `Dockerfile` | EDIT | §2: a distroless base carrying the Rust daemon and nothing else |
| `make/build.mk` | EDIT | §2: the local image build produces the Rust daemon for both architectures |
| `rustd/crates/afd_observability/**` | EDIT | §3: registry, typed domain handles, admission, `Observed<T>` cells, counting exporter, first error type |
| `docs/metrics.census.tsv` | CREATE | §3: the executable metric-family contract the Rust registry test grades against |
| `rustd/Cargo.toml` | EDIT | §2: the shipped profile strips debug info; §3: `opentelemetry_sdk` gains the `metrics` feature |
| `make/test-parity.mk` | CREATE | §4: the black-box HTTP parity lane, parameterised by base URL (distinct caller: the cutover checklist) |
| `scripts/parity_lane.sh` | CREATE | §4: the harness the lane invokes — roster, probe, normalisation, diff. Shell, not a crate: see §4 |
| `scripts/parity_lane_test.sh` | CREATE | §4: the harness's own tests — the differ has to be proven to differ |
| `make/test.mk` | EDIT | §4: includes the new lane; the file is the test graph's include list |
| `make/bench.mk` | EDIT | §4: `bench-cutover` adds a comparison mode with budget constants that refuse to be unset |
| `scripts/bench_cutover.sh` | CREATE | §4: the benchmark harness — budget refusal, percentile, resident set, verdict |
| `scripts/bench_cutover_test.sh` | CREATE | §4: its own tests — a lane that grades nothing must not look like one that passed |
| `make/dev.mk` | EDIT | §4: `make up` built the Zig binary while §2's Dockerfile reads the Rust one; also the shared local-daemon wait |
| `docker-compose.yml` | EDIT | §4: the healthcheck shells out to wget, which the distroless image does not carry; and the two local-boot repairs 4.3 surfaced — TLS certificate modes, and the Redis URL path segment |
| `.githooks/post-checkout` | EDIT | §4: links the daemon env compose has always declared and nothing ever filled |
| `playbooks/operations/ci_rust_images/**` | CREATE | §4: the musl toolchain baked as image layers instead of installed per build, on the ci_zig_images pattern |
| `make/build.mk` | EDIT | §4: the build consumes that image and gives each arch its own target dir, so a rebuild is incremental |
| `make/dry.mk` | EDIT | §4: dry lane variant booting the Rust daemon |
| `make/test-integration-rustd.mk` | EDIT | §4: the run-verdict guard moves inline as its script is swept |
| `scripts/rustd_lane_benchmark.py` | DELETE | §4 sweep: no caller in `make/` or `.github/` |
| `scripts/rustd_lane_benchmark_test.py` | DELETE | §4 sweep: self-test of a deleted script |
| `scripts/rustd_lane_contract_test.py` | DELETE | §4 sweep: static contracts over a deleted orchestration |
| `scripts/rustd_lane_result.py` | DELETE | §4 sweep: behaviour preserved inline in the lane |
| `scripts/rustd_lane_result_test.py` | DELETE | §4 sweep: self-test of a deleted script |
| `playbooks/operations/cutover/001_playbook.md` | CREATE | §5: the runbook skeleton, drain order, abort criteria, divergence register |
| `playbooks/operations/cutover/probes.sh` | CREATE | §5: the probe runner and its row-coverage assert |
| `playbooks/operations/cutover/probes_test.sh` | CREATE | §5: its self-tests — an assert that cannot fail is not an assert |
| `playbooks/operations/cutover/coverage.tsv` | CREATE | §5: the probe→row map as data; a rubric row id is a milestone id, which RULE TST-NAM bars from source |
| `playbooks/README.md` | EDIT | §5: the playbook index; `check-playbooks` grades README ↔ tree parity |
| `scripts/parity_lane.sh` | EDIT | §5: the lane reads the divergence register, so a declared difference is not a failure |
| `docs/architecture/observability.md` | EDIT | §5: the export path decision — standard knobs, collector-owned fan-out |
| `docs/architecture/runner_fleet.md` | EDIT | §5: the stale Prometheus-scrape claim reconciled against the deployed configuration |
| `ui/packages/app/tests/fleets-install-entry-gate.test.ts` | EDIT | VERIFY: a pre-existing flake failing the unit lane — a synchronous query asserting on an async render (Indy-approved, outside this spec's scope) |

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

### §3 — The metrics pipeline, in the crate shaped to receive it — DONE

`afd_observability` carries the span pipeline and the export wrapper. It carries no metric instrument, no aggregation, and no family registry — so a transport plugged in later would carry an empty payload. This section builds the pipeline half, entirely inside the crate, where it needs nothing from the boot path and collides with no other stream.

**The implementation is SDK configuration, not a port.** The Zig daemon hand-rolls instruments, delta windows, label-dimension products, cardinality caps and payload encoding across roughly 1,450 lines because Zig has no OpenTelemetry SDK. Rust has one, already a workspace dependency — the cluster is a feature flag plus configuration, deleting 1,450 lines that would otherwise be ours to keep correct.

**Design settled by adversarial consult (two external CTO reviews, both recorded in Discovery), Indy sign-off on the three open calls.** The agreed shape:

1. **Two meter providers, because temporality is per-family parity data.** The SDK selects temporality at the exporter, never per family — so the registry's `temporality` field routes each family to a cumulative or a delta provider, and the collector stays free of agentsfleet-specific processors. Rejected alternatives, recorded: a collector `cumulativetodelta` processor (stateful, and it would put family names in collector config, breaking the nothing-daemon-specific invariant) and one-provider-cumulative (silently rewrites the cost families' temporality).
2. **The executable contract is `docs/metrics.census.tsv`, and it is the ONLY copy.** The TSV carries the full parity surface — name, kind, number type, unit, temporality, label keys, histogram bounds, series policy, plus the operator guidance (`category`, `watch_for`) — and the Rust registry test `include_str!`s it. Per Indy's single-source-of-truth call (superseding the earlier keep-both design), the markdown census table and the cost-family table in `docs/architecture/observability.md` were replaced with pointers: the doc keeps the category legend and the naming rationale, and repeats no contract data. The Zig census test loses its table; nothing runs it (its lanes died in M175 §6) and it retires with the daemon.
3. **`_other` is domain admission, never overflow translation.** The per-runner/per-model families keep their bounded admission in front of the instrument: a runner resolves ONCE at admission into a `RunnerMetricSlot` (attributes built once, hot handles bound once), and a rejected runner records under `runner_id="_other"`. The SDK's own cardinality cap — whose marker is the spec-fixed `otel.metric.overflow=true` — stays as a backstop that must never fire: a negative test asserts zero overflow points under legitimate load, because an SDK overflow point is a bug indicator and disguising it as `_other` would hide the bug.
4. **Three recording tiers, because "atomics-only" is not what attributed recording is.** Hot Counter/Histogram families record through BOUND instruments (`experimental_metrics_bound_instruments` — an API-stability risk on a pinned SDK, taken deliberately and documented at the feature declaration; bound gauges do not exist in 0.32 and are not designed against). Hot state gauges (`in_flight`-class) are owned atomics published through observable callbacks. Cold families use plain attributed calls with static attribute slices.
5. **Observable callbacks load atomics and do nothing else.** The SDK invokes callbacks under its pipeline lock, with no catch_unwind and no timeout — a callback that touches Redis, `/proc`, or any lock can stall or poison the entire metrics pipeline. So `live_read` sources publish into `Observed<T>` snapshot cells (validity flag + atomic value) on their own cadence, and the callback only loads; an invalid snapshot declines to observe, preserving the Zig rule that a failed read is ABSENT, never a fake zero.
6. **There is no metric export queue, and the failure counter says what it counts.** PeriodicReader collects on its own thread, does not overlap cycles, and hands each batch to the exporter directly. The counting wrapper (`PushMetricExporter` twin of the span wrapper) counts FAILED EXPORT BATCHES; no HTTP retry is enabled, reproducing the recorded no-double-count decision by omission. Failure warns to stderr only, as the Zig exporter does.
7. **The typed layer is the label contract; Views do only what is stable.** Raw instruments are private to the crate: daemon code calls domain methods (`metrics.runner.execution_completed(&slot, elapsed)`) that take `Duration` and typed ids, so an illegal label or a wrong unit cannot be written from outside. Views carry histogram boundaries and cardinality limits only — the attribute-key allow-list API is behind an unstable SDK feature and is not enabled; the typed layer does that job.
8. **One label is unbounded, and only that one gets a budget (amended, Indy, this stream).** `SeriesPolicy { Fixed { max_series }, Runner { slots }, SharedCost }` replaces the boolean flags. `Runner { slots: 4096 }` is const-asserted and raises the SDK's per-stream cap explicitly so 4,096 admitted slots plus `_other` fit — `runner_id` is supplied by the customer spinning up runners, so it is the one census label nothing bounds by construction. `SharedCost` declares NO ceiling.

    **Why the 256 cost budget was dropped rather than ported.** The spec drafted a `const fn` summing "the declared cost-family budget" against 256, called the direct equivalent of the Zig comptime arithmetic. There is nothing to sum: `otel_metrics_families.zig:246` reads `if (!meta.cost and !meta.streamed) meta.max_series = …`, so cost families are EXCLUDED from that arithmetic and keep the struct default of `max_series = 1`; both comptime sums skip them too. `COST_SERIES_BUDGET = 256` is a flat pooled number sizing a static array (`var series_buf: [MAX_SERIES + 1]payload.Series`) in an aggregator we hand-rolled because Zig has no OpenTelemetry SDK. Rust's SDK allocates per stream, so the constraint that produced the number does not exist here, and there is no SDK primitive for a pool shared across five instruments anyway.

    Porting it would also cost the thing the families exist for: every cost label except the model is a closed enum, and the model list is vendor-published and small, so a cap can only ever fire by folding a real model's spend into an overflow bucket — losing the answer to "what did this model cost us." Dimension 3.2 already requires that cap never to fire. **Prior art, checked rather than asserted:** habitat/builder ships production Rust metrics with zero occurrences of `cardinality`, `max_series` or `series_limit`; bun has none either; exonum carries no metrics layer. No reference implementation budgets series, because none of them keys a metric on a customer-supplied id — which is precisely why `runner_id` keeps its slot table and the cost families do not.
9. **"Byte-stable" binds at the OTLP wire, and that is sufficient.** Grafana normalizes OTLP names on ingest — but it applies the same normalization to the Zig daemon's push today, so an identical wire yields identical dashboard names on both sides of the swap. Continuity is asserted at the collector's wire (testable); per-backend name mapping is documented in the runbook, not tested per backend.

**The crate gains its first error type here.** Constructing an instrument set from a registry is fallible, and `docs/RUST_ERROR_STANDARD.md` lists this crate as having no fallible function today. It takes the standard's shape on the commit that ends that — not later, and not exempt for predating the rule.

- **Dimension 3.1** — every family the TSV census declares is emitted under that exact name, kind, number type, unit, temporality and label keys; a family on one side only is named and fails → Test `test_metric_family_registry_parity` — **DONE**
- **Dimension 3.2** — past a runner-family admission bound, memory stays constant, overflow records under `runner_id="_other"`, and ZERO data points carry `otel.metric.overflow` → Tests `test_runner_admission_other_spelling`, `test_the_overflow_label_is_not_the_sdk_marker` — **DONE**
- **Dimension 3.3** — a failing exporter increments the failed-batch counter and never blocks or delays a recording call; no retry occurs → Tests `test_metric_export_fails_counted_never_blocks`, `test_metric_export_latency_does_not_reach_the_caller` — **DONE**
- **Dimension 3.4** — the crate's error type composes its sources by `#[from]`, and no variant's `source()` returns its own kind → Test `test_observability_error_chain_shape` — **DONE**
- **Dimension 3.5** — an observable callback with an invalid snapshot emits no data point (absent, never zero), and collection completes when a publisher never wrote → Test `test_observed_absent_never_zero` — **DONE**
- **Dimension 3.6** — every census label that is not bounded by construction is admitted through a bounded slot table, and no family declares a ceiling it cannot justify: the runner slot count is const-asserted against the SDK stream cap it must fit inside, and a `SharedCost` family declaring a ceiling fails → Tests `test_unbounded_labels_are_slot_admitted`, `test_a_cost_family_cannot_declare_a_ceiling` — **DONE**

### §4 — The lanes, and the scripts they no longer need

The lanes the cutover grades against, built while there is nothing yet to grade: a black-box HTTP parity harness parameterised by base URL so one suite can run against either daemon, a benchmark comparison mode, and a dry-lane variant that boots the Rust daemon.

The parity harness is deliberately NEW code rather than a repointed Zig suite. The Zig integration corpus imports Zig modules and calls them directly — of 145 such files, three use an HTTP client — so pointing it at a Rust-served environment still exercises Zig handler code. A green run would report a pass rate for the implementation being retired, which is worse than no number because it reads like evidence.

**The harness is shell, and that is a decision.** A Rust crate would join
`rustd/crates/` under the 100%-line coverage flag and pay that rent for the life
of the repository, for code whose whole job is pointing curl at two daemons. So
the harness is `scripts/parity_lane.sh` — shellcheck-linted with the rest of
`scripts/*.sh`, with `scripts/parity_lane_test.sh` beside it proving the differ
actually differs. A lane that compared nothing would pass every route.

**The roster is reflection, not a list.** Routes come from
`public/openapi.json`, so a new route joins the lane the moment it joins the
contract — the same principle the wire fixtures already hold to. A hand-kept
list is the one somebody forgets to update, and the forgotten route is the drift
the lane exists to catch.

**Every probe goes without credentials and without a body**, which is what lets
one command grade a bare container with no datastore behind it. What that grades
is the contract at the EDGE: which routes exist, what an unauthenticated caller
is told, and in what envelope. With one base URL the claim is that every
declared route answers and none answers 404 — an unauthenticated probe to a
mounted route is refused before a handler resolves an identifier, so a 404 means
the path is not routed at all. With two, the same roster is diffed per route ×
method after per-request volatile fields (`date`, `x-request-id`, the body's
`request_id`) are normalised away; without that normalisation the lane would be
red on every run and grade nothing.

**The dry lane's Rust variant is one variable, not a second suite.**
`playwright.config.ts` declares its backend explicitly rather than falling back
— production code throws on an unset `NEXT_PUBLIC_API_URL` — and threads
whatever it is given into the Next server it spawns. So `dry-app-rustd` points
that one variable at the locally-booted daemon and runs the SAME suite
`dry-app` runs. A parallel copy would drift; an identical suite against a
different backend is the property worth having.

**Budgets refuse to be unset.** The latency budget per route class and the
resident-set ceiling are named constants embedded in the benchmark lane, and the
lane exits non-zero when they are unset, so the gate M181_002 leans on is a real
command with real numbers rather than a judgment.

They are declared in `make/bench.mk` and default to NOTHING. Neither has been
measured — the Rust daemon has not yet run under load beside the Zig one — and a
number written before its measurement is exactly the judgment the row exists to
replace. The Discovery log already routes the absolute ceilings to staging Fly
machines under the swap milestone, because a workstation figure says nothing
about a shared-cpu-4x/4GB machine. So this half ships the lane and proves it
grades; the swap supplies the numbers. `make bench-cutover` fails today, by
design, naming the constant it is missing.

**`make up` was broken, and §4 is where it bit.** §2 moved the image to a
distroless base carrying `dist/agentsfleetd-rs-linux-${TARGETARCH}`, while
`make/dev.mk` still cross-compiled the Zig daemon to
`dist/agentsfleetd-linux-${ARCH}` — two names for one slot, so the local build
produced one file and the image build looked for another. It survived only while
a stale artifact from an earlier `make _dist-daemons` sat in `dist/`. The compose
healthcheck was the same class of miss: it shells out to `wget`, which the
distroless image deliberately does not carry, so the container could never
report healthy. Both are repaired here because both lanes below want to boot a
daemon, and `LOCAL=1` on either is what makes that one command.

**The sweep.** The five `scripts/rustd_lane_*.py` files go. Four have no caller. The fifth is the run-verdict guard both Rust lanes invoke — the check that a suite which silently ran nothing fails instead of passing — so its BEHAVIOUR moves inline into the lane that calls it. The guard is preserved; the script is not.

- **Dimension 4.1** — the parity lane runs the same suite against two base URLs and diffs status, body and the contract headers per route × method; a seeded difference fails naming the route → Test `test_parity_lane_detects_difference` — **DONE**
- **Dimension 4.2** — the benchmark lane refuses to run with either budget constant unset, and passes with both set → Test `test_bench_cutover_refuses_unset_budget` — **DONE**
- **Dimension 4.3** — the dry lane boots the Rust daemon and its page renders pass → Test `test_dry_lane_rust_variant` — **DONE**
- **Dimension 4.4** — a Rust lane whose suite ran zero tests fails, and one whose child exits non-zero fails, with no Python script on the path → Test `test_lane_guard_inline_rejects_silent_noop` — **DONE**

### §5 — The runbook skeleton, the probe framework, and two documents that disagree — DONE

The cutover runbook and its executable probe runner, built to the point where M181_002 fills in the rows the swap needs: drain order, abort criteria, the one-move rollback, and the declared-divergence register that lets a parity differ tell a declared difference from a regression. Its first entry is recorded, inherited from M175.

The probe runner's completeness assert is over ROWS, not probes: every rubric row of the merged milestones is either tagged by at least one probe or named in an exclusion manifest the script prints on every run. This half wires the assert and covers the merged milestones; M181_002 adds the rows its own dependencies contribute.

**The export path is recorded as an architecture decision here.** The daemon is a pure OTLP pusher to one configured endpoint, addressed by the OpenTelemetry specification's own environment names. Vendor fan-out belongs to a collector, not to the daemon, so moving from one backend to another is collector configuration and not a daemon redeploy. `docs/architecture/observability.md` currently describes the direct-to-vendor path the Zig daemon runs; it gains the decision and its reasoning.

**And one document contradicts the deployment.** `docs/architecture/runner_fleet.md` states that a platform Prometheus scrapes a metrics block in the production Fly configuration. No such block exists in either environment's configuration, and `docs/architecture/observability.md` states the daemon has no pull endpoint at all. A milestone that grades metric continuity cannot cite a document that describes a scrape path the deployment does not have.

- **Dimension 5.1** — the probe runner executes end to end and its row-coverage assert fails on an uncovered row, an untagged probe, or an undeclared skip → Test `test_probe_runner_row_coverage` — **DONE**
- **Dimension 5.2** — the runbook's rollback path invokes no migration command, and the probe runner asserts that absence rather than trusting the prose → Test `test_rollback_carries_no_migrate` — **DONE**
- **Dimension 5.3** — the architecture documents agree with the deployed configuration on whether a pull endpoint exists → Test `test_architecture_matches_deployed_metrics_path` — **DONE**

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
playbooks/operations/cutover/    rust_daemon.md (runbook + divergence register)
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
| 3.1 | unit | `test_metric_family_registry_parity` | every TSV-census family emits under the same name, kind, number, unit, temporality, label keys; both directions |
| 3.2 | unit (negative) | `test_runner_admission_other_spelling` + `test_no_sdk_overflow_under_legit_load` | past admission bound: constant memory, `runner_id="_other"` records, zero `otel.metric.overflow` points |
| 3.3 | unit (negative) | `test_metric_export_fails_counted_never_blocks` | failing exporter → failed-batch counter climbs, recording latency unchanged, no retry issued |
| 3.4 | unit | `test_observability_error_chain_shape` | every variant carrying a cause exposes it through `source()`, and no `source()` repeats its own kind |
| 3.5 | unit | `test_observed_absent_never_zero` | invalid snapshot → no data point; collection completes with a never-written publisher |
| 3.6 | unit | `test_unbounded_labels_are_slot_admitted` | `runner_id` is the only census label bounded by admission rather than by construction; its slot count const-asserts against the stream cap; a `SharedCost` family declaring a ceiling fails |
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
| R2 | Rust binary cross-compiles static for both linux targets (§2) | `make _dist-daemons` | exit 0 | P0 | |
| R3 | The daemon serves from the shipped image, proven black-box (§2+§4) | `docker run -d -p 3000:3000 <image>` then `make test-parity BASE_URL=http://127.0.0.1:3000` | exit 0 | P0 | |
| R4 | Metric family registry parity and overflow spelling (§3) | `cd rustd && cargo test --package afd_observability metric_` | exit 0 | P0 | |
| R5 | Lanes exist, refuse unset budgets, and preserve the run guard (§4) | `make test-parity-self-test && make bench-cutover-self-test` | exit 0 each | P0 | |
| R6 | Probe runner row coverage holds (§5) | `make check-cutover-probes` | exit 0 | P0 | |
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
- **Binding instruments to call sites.** §3 ships the metrics pipeline's receiving half — registry, error type, snapshot cells, counting exporter, admission spelling — and nothing that produces a measurement: `afd_observability::metrics` has no caller outside its own crate. That is deliberate and now explicitly owned by M181_002 §2, which ships producers and transport together: a producer with no transport emits into a process nobody can read, so landing it here would be unverifiable work. Recorded because the gap sat between the two specs and neither claimed it.
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

- > Indy (2026-08-30): "I prefer we remove all the entries from the observability.md (no duplicates) but point to the docs/metrics.census.tsv" / "i just want a single source of truth" — the census table and the cost-family kind/unit table were replaced with pointers; `category` and `watch_for` migrated into the TSV so no operator knowledge was lost; the name-parity test is retired with the duplication it existed to police.

- **Consult — adversarial §3 review, round 1 (Tarzy/ChatGPT CTO via Indy, 2026-08-30):** high-level wiring review. Adopted: no `/metrics` endpoint ever (push-only confirmed); PeriodicReader is `opentelemetry_sdk` built-in, own thread; supervisor owns telemetry LIFECYCLE, never an export loop (→ M181_002 task rename); raw instruments private behind typed handles; production collector config gains memory_limiter + sending queue (→ M181_002). Rejected: "remove temporality from the registry" — temporality is per-family parity data that routes provider selection (superseded by round 2's two-provider design).
- **Consult — adversarial §3 review, round 2 (Tarzy/ChatGPT CTO via Indy, 2026-08-30, against the full design brief):** all ten questions answered with verdicts; adopted per the settled-design list in §3 (two providers; TSV contract; domain `_other` admission with SDK overflow as tested-never-fires backstop; three recording tiers with scoped `experimental_metrics_bound_instruments`; atomic-snapshot-only callbacks; failed-batch counter semantics, no retry by omission; typed layer as label contract, stable-Views only; `SeriesPolicy` + const-assert budgets; wire-level byte-stability with normalization inherited equally on both sides). Modified: TSV is Rust-only, tied to the markdown census by name-parity test — no Zig edits in a retiring codebase. Build blocker confirmed: attribute-key allow-list Views are feature-gated unstable and are NOT enabled.
- > Indy (2026-08-30): sign-off on the three open calls — bound instruments "Yes, scoped", census contract "TSV + name-parity", temporality "Two providers".

- > Indy (2026-08-30): "Okay Indy appreciates your fix" — the gitleaks cache-key hit resolved by restructuring to a block scalar, no suppression added.
- > Indy (2026-08-30): "I think in the VERIFY step you will need to check the container in local with `make test-integration-rustd` along with it." Superseded same day: "image-check (not needed) — since you can verify like test-parity does." So the container proof is the parity lane pointed at a daemon served FROM the image (single-target mode, `BASE_URL`), not a bespoke target; rubric R3 says so. the prebuilt-binaries target stays — `build`/`push` need it. Its brief rename to a public `dist-daemons` is reverted: the behaviour predates today as `_dist-daemons`, and a new public make target needs a caller the private one did not already have.
- > Indy (2026-08-30): "remove any arcade decisions we took in zig for containers" / "I donot want zig or legacy belching crap for agentsfleetd (zig)" — the image carries the Rust daemon alone; `build-linux-alpine` (a Zig-daemon build target with its own stale Zig download) removed with its Makefile help row.
- **Lane run-locations (decided, 2026-08-30):** `test-parity` diff mode runs LOCALLY on the compose stack — both daemons against identically reset datastores, which staging cannot provide since one daemon serves it at a time; its single-target mode reruns against staging in M181_002's soak. `bench-cutover` comparison mode runs locally on one machine (relative tolerance is what survives a hardware change); absolute RSS/latency ceilings are graded on staging Fly machines in M181_002 via the exported families, because a workstation number says nothing about a shared-cpu-4x/4GB machine.
- **OTLP-pure invariant (Indy, 2026-08-30): every backend is an OTLP gateway.** The daemon exports OTLP only; the collector's exporters are `otlphttp` ONLY — no vendor-native exporters (no loki/elasticsearch/prometheusremotewrite). A backend without a native OTLP intake is not a supported backend. The one permitted vendor-awareness is a per-backend temporality/transform processor in collector configuration, never a daemon change. Collector deployment shape for M181_002: a per-environment Fly app mirroring `cloudflared-{env}` (own small vm, config baked by Dockerfile, no public service, inbound over 6PN at `otel-{env}.internal`, outbound egress to vendors).

- **`make up` was broken by §2, repaired in §4 (2026-08-30):** `make/dev.mk:52` cross-compiled the Zig daemon to `dist/agentsfleetd-linux-$(LOCAL_DOCKER_ARCH)`; `Dockerfile:39`, since §2, reads `dist/agentsfleetd-rs-linux-${TARGETARCH}`. Only the prebuilt-binaries target writes the second name, so `make up` on a clean checkout fails at COPY — it worked locally solely because a stale artifact was present. Alongside it, `docker-compose.yml` declared a `wget` healthcheck against an image whose own header records that it carries no shell and no HTTP client. Repaired together: the local binary is now a real file target delegating to the prebuilt-binaries target for one arch, and the healthcheck is removed with readiness left where it is acted on (Fly's `[checks.readiness]`). Cost recorded: `make up` now pays a musl release cross-compile instead of a `zig build`, which is the price of the image carrying the Rust daemon. Proven: the image built from the repaired path, the container started, and the Rust daemon ran its own preflight — where `docker build` previously died at COPY.

- **The daemon half of `make up` is a PROVISIONING gap, not a §2 regression (2026-08-30):** with the COPY repaired, the container boots and preflight refuses on five knobs the compose inline block never carried — `AUTH_SESSION_CODE_PEPPER`, `OIDC_ISSUER`, `OIDC_AUDIENCE`, `CLERK_API_BASE`, `CLERK_SECRET_KEY`. This predates the port: `src/agentsfleetd/config/runtime_validate.zig:38` refuses boot on the same pepper, so the Zig daemon would have failed identically. `docs/AUTH.md` §614 names the local-dev source as `~/Projects/agentsfleet/.env`, gitignored and symlinked into worktrees, with the value in `op://ops/ZMB_CD_LOCAL_DEV/...`; that file is absent on this machine and `.githooks/post-checkout` links no daemon env at all. NOT fixed by writing placeholder values into `docker-compose.yml`: `docs/AUTH.md` §665 classes the pepper "catastrophic if disclosed" and bars it from disk, so a committed literal under that name is the wrong shape whatever its value. `LOCAL=1` stays wired and correct; `_ensure-local-daemon` now names the provisioning step when the daemon does not answer.

- **The daemon env was declared and never filled (2026-08-30):** `docker-compose.yml` has always carried `.env.agentsfleetd.local` as an optional `env_file`, and `.githooks/post-checkout` linked `ui.env.local` and `runner.env.local` but never a daemon entry — so the slot existed and nothing filled it. The hook now links it, which took the boot from five environment faults to two (`CLERK_API_BASE`, `CLERK_SECRET_KEY`), both operator-provisioned. Dimension 4.3 is BUILT but UNGRADED for exactly that reason: `make dry-app-rustd` needs a daemon that answers, and `test_dry_lane_rust_variant` is graded by running it, not by reading the target.

- **Two lanes had been red on this branch for hours, and the boundary is where that surfaced (2026-08-30):** `make lint-all` since §3 (`223799a0a`) — `docs/architecture/observability.md` carried `§Metrics stay semantic explains each divergence`, where the anchor parser runs from `§` to the next `;` and swallowed the descriptor, so `check-architecture-doc` failed on an anchor naming no heading. `make test-unit-all` since §2 (`7386aa90a`) — the cargo cache that commit added to `deploy-dev-build.yml` carried a prefix fallback, and M156's release-gate guard bans that string across the whole concatenated `deploy-dev*` family because a softened miss on the PLAYWRIGHT cache serves a browser binary nobody asked for. Neither failure came from §4 or §5 work; both were invisible because the Test Baseline override defers the declared `verify.*` set to this boundary. Resolved by fixing the violations, not the gates: the descriptor moved ahead of the anchor, and the cargo fallback was dropped so a lockfile bump pays a cold dependency build rather than teaching a browser rule about compilers.

- **A third failure was a pre-existing flake, not a regression (2026-08-30):** `fleets-install-entry-gate.test.ts` asserted `getByRole("alert")` synchronously one line after an awaited click, so it read the DOM a tick before the failed page settled and re-rendered. It passed 22/22 in isolation across three runs and failed under the full 235-file suite — the shape of a race, not a defect in the component. Lines 289 and 312 of that same file already await `findByRole`, so the outlier was an inconsistency rather than a decision. This branch carries no other `ui/` change; fixed on Indy's approval because the repository's unit claim should not rest on winning a race.

- **4.3 is GRADED, and getting there cost four faults that each hid the next (2026-08-31):** `make dry-app-rustd` now reports `✓ [app] Dry lane passed against the Rust daemon` — Vitest 235 files / 2406 tests, Playwright 33 passed and 1 skipped, against a container the Rust daemon serves from. Every fault was the same species: something the Zig daemon tolerated and the Rust daemon does not. (1) `CLERK_API_BASE` unset — `clerk_backend_config.zig:10` carries `API_BASE` as a compiled-in default and `resolveApiBase(null)` returns it, while `preflight/read.rs:74` marks the knob `required`; set locally, but see the swap-day note below. (2) `CLERK_SECRET_KEY` blank — satisfied with a placeholder, which is the shape Zig's own tests use: `clerk_backend_test.zig:112` passes `null` because the secret is a typed argument rather than an env read, and `tests/support/mod.rs:47` spells the Rust equivalent `fixture-provider-secret-not-a-credential`. No lane dials it. (3) `/tls/ca.crt` unreadable — openssl writes the whole fixture directory `0600 root` and the daemon image runs as uid `65532`, so the daemon could not read the trust anchor it was handed. Repaired by making the three CERTIFICATES `0644` and leaving every `.key` at `0600`, outside the generation branch so an already-populated volume is fixed too. (4) `REDIS_URL_API` carried a `/api` path segment that has selected nothing since M92: `redis_config.zig:109` takes `hostpath[0..slash_pos]` and never reads past it, so the Zig daemon has always used db 0, while redis-rs parses the same segment as a database INDEX and refuses `api` with `Invalid database number`. Dropped rather than respelled `/0`. None of the four is a §4 regression; all four are first-boot findings, because no Rust daemon had ever booted from this compose file before.

- **Two swap-day environment preconditions have no home in the divergence register (2026-08-31):** faults (1) and (4) above are local symptoms of a deployed-environment risk. `CLERK_API_BASE` is set by neither `deploy/fly/agentsfleetd-dev/fly.toml` nor `.github/workflows/deploy-dev-build.yml`'s sibling `deploy-dev-fly.yml`, which stages 18 secrets and not that one — so a Rust daemon swapped in under the current Fly configuration refuses boot on a knob the Zig daemon defaulted. `REDIS_URL_API` comes from `op://<dev vault>/upstash-dev/api-url`; if that URL carries any path segment, the same `Invalid database number` refusal follows, and Zig's silent truncation is exactly why nobody would have noticed. Neither belongs in the Declared-divergence register — `scripts/parity_lane.sh:118` reads that table as `METHOD /path` rows, so an environment knob has no row shape there. Recorded here pending the user's call on whether the `required`-versus-defaulted half is repaired in this milestone (a boot-behaviour change, which Out of Scope currently bars) or handed to M181_002 with the swap.

- **The builder image was baked, never published, and CI never used it (2026-08-31):** §4 added `playbooks/operations/ci_rust_images/` and pointed `make/build.mk`'s `BUILDER_IMAGE` at `ghcr.io/agentsfleet/ci-rust-alpine`, but `build_and_push.sh` had never been run against the registry — the organisation's published containers were `ci-zig-alpine`, `ci-zig-debian-trixie`, `ci-zig-ubuntu` and `agentsfleetd`, and nothing else. So `_builder-image`'s `inspect → pull → bake` chain fell through to baking on every machine, and the recorded 11m15s → 5m57s cold-build improvement was a property of one laptop rather than of the repository. CI meanwhile hand-rolled the target it was meant to call: `deploy-dev-build.yml` and `release.yml` each ran stock `rust:1.98-alpine` and `apk add --no-cache build-base perl cmake go linux-headers` inline, paying per run exactly the cost the image exists to remove, against a floating `1.98-alpine` tag while `versions.env` pinned `1.98.0` and `alpine3.24`. That is both the hand-rolled-make-target rule and versions.env's own "a second compiler nobody chose". Fixed by publishing the multi-architecture image and having both workflows DERIVE the tag from `versions.env` with the same `sed` `make/build.mk:51` uses, so one bump moves CI and local together. `check-gh-actions-valid` gained the guard that keeps it that way, and the guard asserts the reachable property rather than the vacuous one: not "does the literal match" but "is there a literal at all", since a stale pasted tag would still RUN and would simply compile on the wrong toolchain. Mutation-proven — reintroducing a literal fails the gate naming the file and line.

- **`LOCAL=1` on both lanes (2026-08-30):** the run-locations decision above wants the compose stack, so `make test-parity LOCAL=1` and `make bench-cutover LOCAL=1` boot it and point at it through one shared `_ensure-local-daemon` prerequisite. That target polls `/healthz` rather than trusting `docker compose up -d`, which returns when a container is STARTED and — with the healthcheck correctly gone — has nothing to wait on. NOT yet built: the two-daemon local diff the run-locations note describes needs a second compose service carrying the Zig daemon, which no target produces; single-target mode is what this half ships, and rubric R3 is the claim it makes.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
