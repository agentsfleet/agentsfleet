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

# M214_001: The Rust runner exports its spans and its own metric families over OpenTelemetry Protocol (OTLP) to a runner collector, holding no credential, so an operator sees provider latency, sandbox start and tool time that `agentsfleetd` cannot

**Prototype:** v2.0.0
**Milestone:** M214
**Workstream:** 001
**Date:** Oct 04, 2026
**Status:** DONE
**Priority:** P1 — the cutover retires the Zig runner, and after it nobody can see a slow provider, a sandbox that starts slowly or a memory push that fails, because `agentsfleetd` never observes them
**Categories:** DOCS, OBS
**Batch:** B1 — before M213_001, by Indy's call ("Own spec, before cutover")
**Branch:** `feat/m214-001-runner-telemetry`
**Baseline revision:** `c5f7680f2ee4a475a9f6f6c8701c98262c6d5c8d`
**Test Baseline:** unit=3776 integration=4442 (Rust: `test-unit-rustd` 3776 passed, 0 failed, 828 ignored across 191 binaries; `test-integration-rustd` 4442 passed, 0 failed, 0 ignored across 241 binaries, both tiers through the coverage shards. TypeScript, same revision: app 3642, design-system 647, website 142 passed; cli 1779 passed, 17 skipped, 0 failed)
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M214_001-c5f7680f2.md`
**Depends on:** M210_002 (the agent loop wired into `run`, and the `runner.lease`, `invoke_agent`, `chat` and `execute_tool` spans in `afr_agent/src/spans.rs` and `afr_supervisor/src/identity.rs`)
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 04, 2026) from Indy's Oct 03 and Oct 04 decisions and a source trace of `feat/m210-agent-loop-hosted-tools` at `fbb13e8fd`
**Canonical architecture:** `docs/architecture/observability.md` §"`agentsfleet-runner` — a collector of its own", §"Signal routing", §Traces; `docs/architecture/runner_fleet.md` §Observability

---

## Overview

**Goal (testable):** `test_runner_exports_spans_and_metrics_when_configured` — with `OTEL_EXPORTER_OTLP_ENDPOINT` set, one lease run delivers its `runner.lease`, `invoke_agent`, `chat` and `execute_tool` spans and the runner metric families to an OTLP receiver standing in for the runner collector, and the runner sent no header; with the knob unset, `run` builds no exporter and starts no export thread.
**Problem:** The runner creates four span kinds and exports none. `agentsfleetd` sees only what a verb carries, so provider turn latency and retries, sandbox start time, dropped activity frames and failed memory pushes are invisible; tool durations reach it as frames but feed no metric. The OTLP builder lives in the `agentsfleetd` binary (`agentsfleetd/src/telemetry.rs:195-271`), which the runner may not link (`agentsfleet_runner/tests/dependency_graph.rs:13`). The metric census is daemon-only, and `every_census_family_has_a_producer` would fail a runner row.
**Solution summary:** The exporter construction leaves the daemon binary for a small crate, `afd_otlp`, that both binaries call. The runner's `run` entry reads the same endpoint, protocol and timeout knobs, refuses any header and any credential in the endpoint, and exports traces and metrics; `sandbox` and `probe` export nothing. Logs stay on stderr for the runner collector to read from the host's log store. A sampler holds runner spans to a fixed budget. Six runner metric families with closed label sets are declared in their own census, read with `Registry::read` and graded by their own producer-coverage test. The runner collector itself, on the bare-metal host, is separate later work.

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(runner): export the runner's spans and metrics over OTLP`
- **Intent (one sentence):** An operator who points the runner at its collector sees every lease as a trace and the runner's own latencies as metrics, and a runner nobody configured behaves exactly as today.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/observability.md` — §"`agentsfleet-runner` — a collector of its own" and the bounds the runner side must keep.
2. `rustd/crates/agentsfleetd/src/telemetry.rs` — `install`, the builder that moves; `rustd/crates/agentsfleetd/src/preflight/otlp.rs` — `OtlpConfig` and the knobs.
3. `rustd/crates/afd_observability/src/export.rs` — `CountingExporter`, the drop count the runner reuses.
4. `rustd/crates/afd_observability/src/metrics/registry.rs` — `Registry::read`, which takes a census other than the daemon's.
5. `rustd/crates/agentsfleet_runner/src/main.rs` — why `sandbox` runs first and alone.
6. `rustd/crates/agentsfleet_runner/tests/dependency_graph.rs` — the crates a runner may link.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `rustd/Cargo.toml`, `rustd/Cargo.lock`, `rustd/crates/afd_otlp/` | CREATE | `OtlpConfig`, the knobs and the span, log and meter providers, moved from the daemon binary |
| `rustd/crates/agentsfleetd/src/telemetry.rs`, `src/telemetry/`, `src/preflight.rs`, `src/preflight/otlp.rs` and its tests, `src/error.rs`, `src/logs.rs`, `src/lib.rs`, `Cargo.toml` | EDIT | Call `afd_otlp`; daemon behaviour unchanged — a census refused inside the transport's install still reports as the census refusal |
| `rustd/crates/agentsfleetd/tests/` (`daemon_suite.rs`, `integration_rust_runner_telemetry.rs`, `integration_telemetry.rs`, `integration_rust_runner.rs`, `support/bundle_run.rs`) | EDIT | Dimension 5.1 end to end; the collector fixture and runner helpers shared with it |
| `rustd/crates/agentsfleet_runner/` (`src/main.rs`, `Cargo.toml`, `tests/dependency_graph.rs`, `tests/runner_suite.rs`) | EDIT | `run` installs the exporter; `afd_otlp` joins the allowed crates |
| `rustd/crates/afr_telemetry/` | CREATE | The credential-free endpoint config, the span sampler, the runner instruments, the runner census reader |
| `rustd/crates/afr_agent/src/` (`loop.rs`, `ledger.rs`, `spans.rs`), `afr_providers/src/` (`transport.rs`, `turn.rs`, `registry/tests.rs`), `afr_supervisor/src/` (`activity.rs`, `memory.rs`, `error.rs`, `identity.rs`, `lib.rs`, `lease_loop/workspace.rs`, `lease_telemetry_tests.rs`), their `Cargo.toml` and tests | EDIT | Record the six families where each fact is known; the lease span carries `agentsfleet.event.id`; the tool span names its tool by the catalog's closed set; frames a lease abandons at its drain wait are counted |
| `rustd/crates/afd_observability/src/metrics/label.rs`, `src/semconv.rs`, `src/producers/http.rs` | EDIT | `closed_set!` exported so the runner's label sets use the daemon's macro rather than a copy; `RUNNER_SPAN_KEYS` gains the event id; the shared counting wrappers' losses route to a process with no daemon producer set |
| `docs/metrics.runner.census.tsv` | CREATE | The runner families, graded both directions |
| `playbooks/operations/acceptance/baselines/M214_001-c5f7680f2.md` | CREATE | The test baseline and the branch's delta |
| `docs/architecture/observability.md`, `docs/architecture/runner_fleet.md`, `docs/architecture/runner_execution.md` | EDIT | The runner side moves from "decided" to "built"; the collector stays "built later" |
| `~/Projects/docs` (self-hosting runner page, changelog) | EDIT | The runner's three knobs, on a `chore/m214-runner-telemetry-changelog` branch |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (family names, label values and budgets are constants), LOG (`telemetry_export_started` / `telemetry_export_disabled`, never the endpoint value), ORP (the daemon's builder leaves no copy), NDC, TCF, TST-NAM.
- `dispatch/write_rust.md` — §ERR-RS (`afd_otlp` and `afr_telemetry` each declare one `ErrorKind` through `afd_core::error_shell!`), §FN-RS (the endpoint parses once into a type that cannot carry a credential), §UFS.
- `docs/LOGGING_STANDARD.md`; `afd_observability` for every instrument and drop counter (no second counting wrapper).
- Microsoft Pragmatic Rust Guidelines — `M-SMALLER-CRATES` (two small crates instead of a runner path inside the daemon's), `M-DI-HIERARCHY` (the sampler is a concrete type implementing the SDK's `ShouldSample`).
- Indy's standards list (Oct 03): traits and structs with behaviour, few clones, no `Mutex` (atomics for the budget), `error_shell!`, no duplicates, popular crates (`opentelemetry_sdk`, `opentelemetry-otlp`), `afd_core` reuse, smaller crates.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| RUST ERR / LOGGING / UFS | yes | `error_shell!`; scoped events with `error_code`; constants for every family, label and budget |
| File & Function Length (≤350/≤50/≤70) | yes | `afd_otlp` splits config from providers; `telemetry.rs` shrinks |
| Architecture consult | yes | Both pages say what the runner exports in the same commit as the code |
| SCHEMA GUARD / UI GATE | no | No schema or UI file |

## Prior-Art / Reference Implementations

- **Reference:** `rustd/crates/agentsfleetd/src/telemetry.rs` — moved, not rewritten: same queues, same counting wrappers, same off-when-unset rule.
- **Reference:** `rustd/crates/afd_observability/src/runner.rs` — bounded label sets by construction, overflow to one series; the runner families follow it.
- **Reference:** OpenTelemetry GenAI semantic conventions — `invoke_agent`, `chat`, `execute_tool` span names and `gen_ai.*` attributes, already used by `afr_agent/src/spans.rs`.

## Sections (implementation slices)

### §1 — One OTLP builder, two binaries

`afd_otlp` owns `OtlpConfig`, the knob reading and `install`, moved from `agentsfleetd`. The daemon calls it unchanged; the runner's dependency guard allows it. Unset `OTEL_EXPORTER_OTLP_ENDPOINT` means nothing is built.

- **Dimension 1.1** — The daemon builds the same providers from the same knobs after the move → Test `test_daemon_otlp_install_is_unchanged` — DONE (`rustd/crates/agentsfleetd/src/telemetry/tests.rs`)
- **Dimension 1.2** — The runner links `afd_otlp` and still no datastore or control-plane crate → Test `test_runner_links_no_datastore_crate` — DONE (`rustd/crates/agentsfleet_runner/tests/dependency_graph.rs`)

### §2 — `run` exports with no credential; `sandbox` and `probe` never do

`run` parses `OTEL_EXPORTER_OTLP_ENDPOINT` after its configuration into a type that refuses user information in the URL, refuses every header knob the exporter reads (`OTEL_EXPORTER_OTLP_HEADERS` and each signal's own, which the exporter reads from the environment itself and prefers) and every compression knob, installs the trace and meter providers, logs `telemetry_export_started` with the knob name, and on exit shuts the providers down within one export's timeout. Logs stay on stderr. `sandbox` and `probe` construct no provider, so hardening still sees one thread.

- **Dimension 2.1** — `run` with the endpoint set exports a lease's spans and families to an in-process receiver, with no header → Test `test_runner_exports_spans_and_metrics_when_configured` — DONE (`rustd/crates/afr_supervisor/src/lease_telemetry_tests.rs`: the real lease loop and agent loop over a scripted model, since `run` refuses every lease until M213_001)
- **Dimension 2.2** — A header knob, or user information in the endpoint, refuses `run` naming the knob → Test `test_runner_refuses_a_credential` — DONE (`rustd/crates/afr_telemetry/src/endpoint/tests.rs`; the binary: `run_refuses_a_credential_naming_the_knob` in `rustd/crates/agentsfleet_runner/tests/entries.rs`)
- **Dimension 2.3** — `run` without the endpoint builds nothing and logs `telemetry_export_disabled` once → Test `test_runner_exports_nothing_when_unconfigured` — DONE (`rustd/crates/agentsfleet_runner/tests/entries.rs`)
- **Dimension 2.4** — `sandbox` hardens with the endpoint set → Test `test_sandbox_hardens_with_telemetry_configured` — DONE (`rustd/crates/agentsfleet_runner/tests/entries.rs`)

### §3 — A fixed span budget

A `ShouldSample` implementation admits at most `MAX_LEASE_SPANS` spans per lease and `RUNNER_SPANS_PER_SECOND` per monotonic second, counted with atomics; a shed span increments `agentsfleet_runner_spans_suppressed_total`. Each lease is its own root trace carrying `agentsfleet.lease.id` and `agentsfleet.event.id`, joining the daemon's `fleet.delivery` span by attribute; no trace context crosses the runner protocol.

- **Dimension 3.1** — A lease past `MAX_LEASE_SPANS` exports exactly that many and counts the rest → Test `test_lease_spans_stop_at_the_budget` — DONE (`rustd/crates/afr_telemetry/src/budget/tests.rs`)
- **Dimension 3.2** — A burst past the per-second budget is shed and counted, and the next second admits again → Test `test_span_budget_refills_each_second` — DONE (`rustd/crates/afr_telemetry/src/budget/tests.rs`)

### §4 — Six runner metric families

`provider_turn_duration_seconds` (provider, outcome), `provider_retries_total` (provider, reason), `sandbox_start_duration_seconds` (outcome), `activity_frames_dropped_total` (reason), `memory_push_failures_total` (reason) and `tool_call_duration_seconds` (tool, outcome), each `agentsfleet_runner_`-prefixed. Every label value comes from a closed set: providers from the registry, tools from the catalog, outcomes and reasons from enums. They are declared in `docs/metrics.runner.census.tsv` and read with `Registry::read`; the daemon census is untouched.

- **Dimension 4.1** — Every runner census family has a producer, and every producer a row → Test `test_every_runner_census_family_has_a_producer` — DONE (`rustd/crates/afr_telemetry/src/families/tests.rs`)
- **Dimension 4.2** — A provider retry records `provider_retries_total` with the registry's provider name → Test `test_provider_retry_is_counted` — DONE (`rustd/crates/afr_providers/tests/providers/retries.rs`)
- **Dimension 4.3** — A failed memory push records `memory_push_failures_total` with its reason → Test `test_memory_push_failure_is_counted` — DONE (`rustd/crates/afr_supervisor/src/memory/tests.rs`)
- **Dimension 4.4** — Each family's declared ceiling admits its label product → Test `test_runner_ceilings_admit_their_label_product` — DONE (`rustd/crates/afr_telemetry/src/families/tests.rs`)

### §5 — The architecture says the runner side is built

`observability.md` and `runner_fleet.md` move the runner's export from "decided" to "built": its spans, families, budget and knobs, and what it never sends (logs, prompts, tool output, credentials). The runner collector stays "built later".

- **Dimension 5.1** — A runner pointed at a stock OpenTelemetry Collector container delivers one lease as one trace with four span kinds → Test `test_e2e_runner_lease_trace_reaches_a_collector` — DONE (`rustd/crates/agentsfleetd/tests/integration_rust_runner_telemetry.rs`: the daemon that ships, the runner's real loop on a thread of its own, and the OTLP/HTTP collector fixture `integration_telemetry` stands up; see Discovery for why not a collector container)

## Interfaces

```
afd_otlp::OtlpConfig::from_env(&impl Env) → Result<Option<OtlpConfig>>     (None ⇔ endpoint unset)
afd_otlp::Builder::new(&OtlpConfig, Service, Registry)…install() → Result<(Exports, Instruments)>
afr_telemetry::Endpoint::from_env(&impl Env) → Result<Option<Endpoint>>    refuses any header knob and URL user information
afr_telemetry::LeaseSampler: opentelemetry_sdk::trace::ShouldSample        (per-lease and per-second budget)
OTEL_EXPORTER_OTLP_ENDPOINT | _PROTOCOL | _TIMEOUT                           (the runner refuses every *_HEADERS and *_COMPRESSION knob)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Collector down or wrong | Not running, endpoint typo | Batches drop and count in `agentsfleet_runner_otlp_entries_discarded_total` (the daemon's loss family, under the runner's name: neither census declares the other's families); no lease slows (`a_refused_collector_counts_each_loss_on_its_own_signal`, `a_shared_wrappers_loss_reaches_the_runner_recorder`) |
| Credential on a runner | An operator sets any header knob — a signal's own included — or `user:pass@` in the endpoint | `run` refuses with the knob's name (Dimension 2.2) |
| Malformed knob | Bad URL (no scheme, no host, a query), protocol, timeout or any compression knob | `run` refuses with the knob's name, as the daemon does (Dimension 2.2's negative cases) |
| Span storm | A lease with thousands of tool calls | Budget sheds and counts (Dimensions 3.1, 3.2) |
| Label explosion | A provider or tool name outside its set | The instrument maps it to the overflow value; ceilings hold (Dimension 4.4) |
| Thread before hardening | Exporter built in `sandbox` | `sandbox` constructs no provider (Dimension 2.4) |

## Invariants

1. The runner holds no observability credential — enforced by `afr_telemetry::Endpoint`, which refuses every header knob the exporter reads and URL user information, and by the runner's dependency graph, in which `afr_telemetry` is the only crate that reaches `afd_otlp` (`only_the_credential_gate_reaches_the_transport`); Dimension 2.2.
2. `sandbox` starts no thread before hardening — enforced by `main.rs` dispatching `sandbox` before any telemetry call; Dimension 2.4.
3. A runner span never carries prompt, response or tool output text — enforced by the span constructors in `afr_agent/src/spans.rs`, the only place runner spans are built, which take no content argument.
4. Runner metric labels come from closed sets — enforced by typed label enums and `test_runner_ceilings_admit_their_label_product`.
5. Export never blocks a lease — enforced by the SDK's bounded batch queue, pinned in `afd_otlp` rather than read from `OTEL_BSP_*`; failed exports are counted, never retried. A queue a slow collector lets fill drops inside the SDK, where nothing counts it.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| The six `agentsfleet_runner_*` families | ops | Per §4 | provider, tool, outcome, reason | Closed sets; no tenant, fleet, lease or event id | `test_every_runner_census_family_has_a_producer` |
| `agentsfleet_runner_spans_suppressed_total` | ops | A span is shed | none | Count only | `test_lease_spans_stop_at_the_budget`, `every_span_the_budget_sheds_is_counted` |
| `agentsfleet_runner_otlp_entries_discarded_total` | ops | An export fails | signal, reason | Closed sets | `every_producer_writes_the_keys_its_census_row_declares` |
| `telemetry_export_started` / `telemetry_export_disabled` (runner log) | ops | `run` boots | knob name, protocol | Never the endpoint value | `test_runner_exports_nothing_when_unconfigured` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_daemon_otlp_install_is_unchanged` | same knobs before and after → same endpoints, protocol, timeout, header names |
| 1.2 | unit | `test_runner_links_no_datastore_crate` | resolved graph → `afd_otlp` present; `sqlx`, `redis`, control-plane crates absent |
| 2.1 | integration | `test_runner_exports_spans_and_metrics_when_configured` | scripted lease with one tool call → receiver holds 4 span kinds under one trace and the turn and tool families; no request carried an `Authorization` header |
| 2.2 | unit | `test_runner_refuses_a_credential` | each of the four header knobs `=a=b`, endpoint set or not → refused; `http://u:p@collector:4318` → refused; each names its knob |
| 2.3 | unit | `test_runner_exports_nothing_when_unconfigured` | endpoint unset → no provider, one disabled event |
| 2.4 | integration | `test_sandbox_hardens_with_telemetry_configured` | endpoint set, `sandbox` entry → refuses exactly as without it and logs no export line, so it never reached the export decision; that the sequence hardens once bound is `afr_sandbox/tests/confine.rs` (see Discovery) |
| 3.1 | unit | `test_lease_spans_stop_at_the_budget` | `MAX_LEASE_SPANS` + 10 spans in one lease → budget exported, 10 counted |
| 3.2 | unit | `test_span_budget_refills_each_second` | burst over the per-second budget → shed and counted; after one second → admitted |
| 4.1 | unit | `test_every_runner_census_family_has_a_producer` | runner census ↔ producers, both directions |
| 4.2 | unit | `test_provider_retry_is_counted` | one 429 then success → retries 1 with the registry's provider name |
| 4.3 | unit | `test_memory_push_failure_is_counted` | push answered 500 → `memory_push_failures_total{reason="upstream"}` 1 |
| 4.4 | unit | `test_runner_ceilings_admit_their_label_product` | every runner row's ceiling ≥ its label product |
| 5.1 | e2e | `test_e2e_runner_lease_trace_reaches_a_collector` | runner + collector container, one lease → one trace, four span kinds, lease id attribute |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | One builder for both binaries (§1) | `grep -rn "SpanExporter::builder" rustd/crates --include='*.rs' \| grep -v '^rustd/crates/afd_otlp/'` | no output | P0 | ✅ no output |
| R2 | The runner exports a lease when configured (§2–§4) | `cargo test --manifest-path rustd/Cargo.toml -p afr_supervisor --all-features test_runner_exports_spans_and_metrics_when_configured` | exit 0 | P0 | ✅ `test_runner_exports_spans_and_metrics_when_configured ... ok`, 1 passed |
| R3 | The architecture says the runner side is built (§5) | `grep -c "runner exporter.*absent" docs/architecture/observability.md` | `0` | P0 | ✅ `0` |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | ✅ exit 0; every commit's `orly gate work` green |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ `✓ All unit lanes passed` — runner 643, daemon 131, daemon libraries 3056; TypeScript coverage 100% |
| S3 | Lint green | `make lint-all` | exit 0 | P0 | ✅ `✓ All lint checks passed` |
| S4 | Integration green | `make test-integration-rustd` | exit 0 | P0 | ✅ `integration suite — 802 passed`, `(exclusive) — 2 passed` |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | ✅ `all versions match 0.56.0` |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Every required check passes before the Pull Request is ready; a P1 ❌ needs an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

| File to delete | Verify |
|----------------|--------|
| The provider construction in `rustd/crates/agentsfleetd/src/telemetry.rs` | `grep -c "with_batch_exporter" rustd/crates/agentsfleetd/src/telemetry.rs` → `0` |

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `agentsfleetd::preflight::otlp::OtlpConfig` | `grep -rn "preflight::otlp::OtlpConfig" rustd/crates --include='*.rs'` | 0 matches |

## Out of Scope

- The runner collector itself — Indy, Oct 04: "we will have a new collector for the runner later on in the baremetal host"; its deployment, its log allowlist and the credential it holds for its backends are that work's.
- The daemon's collectors, `otelcol-{dev,prod}` — they "are for the agentsfleetd daemon only" (Indy, Oct 04); nothing here points a runner at them.
- An OTLP log bridge in the runner — the runner collector reads stderr from the host's log store.
- W3C trace context on the runner protocol; Grafana dashboards for the new families.

---
## Product Clarity (authoring record)

1. **Successful user moment** — An operator opens a slow lease and sees it as one trace (each provider turn, each tool call), and a panel showing which provider is slow this hour.
2. **Preserved user behaviour** — A runner with no endpoint behaves exactly as today; `sandbox` and `probe` are unchanged; the daemon's export is unchanged.
3. **Optimal-way check** — The daemon's builder, counting wrappers and census machinery are reused; only the runner's facts are new.
4. **Rebuild-vs-iterate** — Iterate: move the builder, add a sampler and six families.
5. **What we build** — `afd_otlp`, `afr_telemetry`, the runner census, the doc flip for the runner side.
6. **What we do NOT build** — The runner collector, a log bridge, protocol trace context, dashboards.
7. **Fit with existing features** — Same knobs as the daemon, minus headers; the daemon's families and census are untouched.
8. **Surface order** — Builder move, `run` wiring, budget, families, docs.
9. **Dashboard restraint** — N/A — no dashboard in this workstream.
10. **Confused-user next step** — A header knob refuses `run` with "the runner carries no credential; give it to the runner collector"; the runner page lists the three knobs.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream in its own milestone, ahead of the cutover; the runner collector follows as its own work.
- **Alternatives considered:** relaying runner facts through new `/v1/runners` fields so only `agentsfleetd` exports (spans cannot ride a verb, and every new fact becomes a protocol change); pointing runners at `otelcol-{env}` (refused: those serve the daemon only); exporter inside `afd_observability` (rejected for `M-SMALLER-CRATES`).
- **Patch-vs-refactor verdict:** a **feature** plus a **move** of the daemon's builder.

## Discovery (consult log)

- **Consults** — > Indy (2026-10-05 23:10): "INDY IS GOING TO BED, ENSURE THE LIFECYCLE IS FOLLOWED AND YOU PUSH A PR WITH CI GATES GREEN, GREPTILE FEEDBACK RESOLVED. AND THE PR IS PRIME READY FOR MEREGE." — context: the standing instruction this workstream closed under; it acknowledges no deferral, and this spec claims none. Indy, Oct 03, 2026: runner telemetry gets "Own spec, before cutover". Indy, Oct 04, 2026: "since we will run an otel collector and collect the observability log, trace, metrics?"; "update the relevant observability md file for runners"; "we will have a new collector for the runner later on in the baremetal host"; the current collectors are "for the agentsfleetd daemon only". The architecture pages record these as decided in `feat/m210-agent-loop-hosted-tools`.
- **Agent defaults** — the runner refuses every header knob and URL user information, so the credential lives only with the runner collector; logs reach that collector from the host's log store rather than a runner log bridge; `MAX_LEASE_SPANS` and `RUNNER_SPANS_PER_SECOND` are set at EXECUTE from a measured lease.
- **Where 2.1 is proven** — `run` refuses every lease until the runner cutover switches the agent engine on, so the lease is driven through the real lease loop and agent loop over a scripted model in `afr_supervisor/src/lease_telemetry_tests.rs`, against an in-process OTLP receiver.
- **What proves 2.4** — the `sandbox` entry binds its executor socket at a fixed path before it hardens, and outside a sandbox that bind refuses first. The binary test therefore proves the entry never reaches the export decision (both runs refuse identically and neither logs an export line), and `afr_sandbox/tests/confine.rs` proves the sequence hardens once bound.
- **The collector 5.1 uses** — the daemon suite's OTLP/HTTP collector fixture (`integration_telemetry`) rather than a collector container: the integration lane runs its datastores in containers on the host and the runner in the test process, so a fixture in that process is the receiver it can reach on every lane.
- **Provider labels** — §4 says "providers from the registry". The first build labelled by OpenTelemetry's well-known names, which cover six of the registry's 39 providers; review corrected it to the registry's names (`afr_providers/assets/providers.json`, 40 values with `_other`). The `invoke_agent` span keeps the well-known `gen_ai.provider.name`.
- **The loss family's name** — the Failure Modes row named the daemon's `agentsfleet_otlp_entries_discarded_total`; the census header rule (neither census declares the other's families) puts the runner's under its own prefix, `agentsfleet_runner_otlp_entries_discarded_total`.
- **Review (gstack `/review`, three passes)** — specialists, a native adversarial pass and a red team, then two re-review cycles. The one CRITICAL was a credential bypass: the exporter reads each signal's own header knob from the environment itself, and the runner refused only the general one. Fixed, with the test proven red on the old code (`a credential for OTEL_EXPORTER_OTLP_TRACES_HEADERS must refuse run`) and green after. Every other finding was fixed or answered with evidence; the PR's Session notes carry the table.
- **Skill-chain outcomes** — `/orly-write-unit-test`: patch coverage 99.79% at the boundary audit before review, graded again by the integration workflow's coverage shards on the pull request; mutation testing over the budget, endpoint and labels after review, 40 of 43 viable mutants caught and the three survivors equivalent or diagnostic-only (the PR lists them). gstack `/review`: as above. `/orly-write-integration-test`: Dimensions 2.1 and 5.1 cross the runner–collector boundary with real input and output.
- **Awaiting Indy** — the published docs (`~/Projects/docs`: the runner page's knobs and a changelog entry) are drafted, not written: a write to that repository needs Indy's per-session approval. Recommended timing is with the runner cutover, since until then no deployed runner reads these knobs.
- **Deferrals** — none.
