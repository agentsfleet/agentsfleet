# M238_003: Focused live dashboards

**Status:** IN_PROGRESS
**Priority:** P1
**Branch:** chore/m238-dev-round-trip
**Pull request:** #801
**Baseline:** ffde2699b39cca1463542c53782ebec52044c838

## Overview

Replace the crowded runtime view with Overview, Chat latency, API & Database,
Scheduler, Models and Sandbox dashboards. Short titles describe measurements;
tooltips carry denominators, sample bounds and missing-data meaning. Every
panel reads a real producer or catalogue field. Add missing instrumentation,
including individual model-call latency, pool state and schedule outcomes.

## PR Intent & comprehension handshake

Indy approved the six-dashboard proposal: "Yes change those and refine -- start".
He selected "Add the missing instrumentation in #801 too" and asked to track
actual sandbox runs, model calls and their latency. All work stays in #801;
merging remains Indy's.

Assumptions: preserve #799's request count and memory/report ordering; reuse
runner instruments before adding overlapping families; keep fleet/run identities
on traces, with bounded attributes on metrics; retain genuine zero values and
leave unmeasured values absent. Existing retained records supply historical
views; new measurements begin when their producers are deployed.

## Files Changed (blast radius)

| Surface | Action | Why |
|---|---|---|
| `playbooks/operations/observability/assets/` | EDIT/CREATE | Six native Grafana definitions and existing alert definitions |
| `playbooks/operations/observability/` | EDIT | Apply and verify every dashboard; clear labels, live-query and evidence guidance |
| `docs/metrics.census.tsv` | EDIT | Declare new daemon families and bounded labels |
| `docs/metrics.runner.census.tsv` | EDIT if needed | Declare only runner measurements not already recorded |
| `rustd/crates/afd_observability/` | EDIT | Typed instruments, observations, metadata and export proofs |
| `rustd/crates/afd_db/` | EDIT | Actual pool occupancy, capacity, acquisition wait and outcomes |
| `rustd/crates/afd_cron/` | EDIT | Accepted/replayed fires, dispatch lag and failed/stale targets |
| `rustd/crates/afd_inference/` | EDIT | First-token and terminal timing for individual provider attempts |
| `rustd/crates/afr_telemetry/` | EDIT if needed | Model attribution and missing runner measurements |
| `rustd/crates/agentsfleetd/` | EDIT | Connect boot-owned readings without blocking collection callbacks |
| `deploy/baremetal/` | CREATE/EDIT | Dedicated runner collector, bounded export and host admission rules |
| `docs/architecture/observability.md` | EDIT | Current producer ownership, collector wiring and metric contracts |

## Applicable Rules

Repository instructions; Rust, errors, tests, shell, documentation and PR rules.
Read `docs/RUST_ERROR_STANDARD.md` before changing fallible Rust interfaces.
Metric privacy and cardinality follow `docs/architecture/observability.md`.
The runner collector follows M233_002 section 2 and the architecture decision;
Fly collectors remain daemon-only.

## Applicable Gates

`orly doctor`, `orly gate work`, focused producer/export and asset proofs,
`orly gate pr`, and GitHub CI. Changed Rust files must exceed 99% coverage;
TypeScript coverage stays 100%. Record exact head and coverage in PR notes.

## Prior-Art / Reference Implementations

The existing daemon producer registry and `GaugeSources` keep callbacks free
of database I/O. `afr_telemetry` already records sandbox lifecycle and tool-call
metrics. `afd_inference::stream::trace` measures first text and terminal time.
The current apply/verify scripts share their dashboard renderer. Extend these
owners rather than adding parallel exporters or duplicate stores.

## Sections

### Section 1 — Pool and scheduler health

- **Dimension 1.1** — Pool readings reflect the actual role's total, idle, used,
  capacity and pending acquisitions; closing a pool cannot fabricate capacity.
- **Dimension 1.2** — Acquire success, timeout, dependency failure and cancellation
  leave accurate duration/outcome readings, with no leaked pending count.
- **Dimension 1.3** — Schedule outcomes distinguish one accepted fire from its
  retries, stale targets and dependency failures. Dispatch lag uses a measured
  scheduled instant and remains absent when that instant is unknown.

### Section 2 — Model usage, latency and capabilities

- **Dimension 2.1** — Each physical model attempt records first-token and terminal
  latency, including errors, timeouts and cancellation; run time remains separate.
- **Dimension 2.2** — Model identity is normalized before metrics; input, output,
  cache tokens and spend retain existing accounting and replay semantics.
- **Dimension 2.3** — Capability and limit tables use the same catalogue facts as
  the product. Missing metadata stays absent, and removed entries do not leave
  an indefinitely live capability claim.

### Section 3 — Actual sandbox runs and runner export

- **Dimension 3.1** — Existing start, wait, release, reuse, tool and limit signals
  reach the dedicated runner collector, with bounded resources and retry state.
- **Dimension 3.2** — Non-root sandbox processes cannot submit to the host OTLP
  receiver. Runner logs are forwarded only after the reviewed allowlist.
- **Dimension 3.3** — Real sandbox and model calls produce joinable traces and
  truthful metrics; preparation, waiting and execution remain separate.

### Section 4 — Six concise dashboards

- **Dimension 4.1** — Overview links to five focused dashboards; titles are short
  and units, filters and percentile labels are consistent.
- **Dimension 4.2** — Apply and drift verification cover every declared dashboard,
  reject missing/foreign sources and detect query, reducer and variable drift.
- **Dimension 4.3** — Every query names a family in a producer census or uses
  declared delivery-span attributes; unavailable data is never replaced by zero.
- **Dimension 4.4** — Retain a compact acceptance receipt and screenshots;
  dashboard definitions remain separate from diagnostic data.

## Interfaces

Prefer existing metrics, spans and catalogue resolvers. New labels are closed
roles, stages and outcomes, or normalized vendor model/provider identities.
No tenant, fleet, lease, event, raw tool result, prompt or credential becomes a
metric label. Do not add report fields or extra calls merely to export facts
already owned by the runner's telemetry pipeline.

## Failure Modes

Missing metadata, disabled export and failed snapshots leave gaps. Cancelled
acquisitions unwind pending state. Schedule retries do not count as new work.
Streaming-zero loss in the managed Tempo datasource remains an external visual
blocker until an editable non-streaming connection is available.

## Invariants

Collector failure never blocks a run. Database callbacks load snapshots only.
The daemon exports verb facts; the runner exports facts no verb carries.
#799's fewer waits and one terminal report remain intact.

## Metrics & Observability

Update the executable censuses alongside producers. Validate names, units,
labels, bounds and actual exported data; a well-formed dashboard alone does not
prove telemetry exists. Add a real environment attribute before claiming
separate dev/prod filters on the shared Grafana stack.

## Test Specification

Producer tests collect structured OpenTelemetry output. Pool integration tests
hold and release real connections and cancel waiting acquisitions. Schedule
proofs cover replay and stale/dependency paths. Model proofs cover first text,
completion, failure and drop. Runner collector proofs cover unauthenticated
sandbox submissions and allowlist rejection. Asset checks exercise the real
apply/verify scripts, including negative mutations.

## Acceptance Rubric

| ID | Criterion | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | Pool and schedule observations are real and replay-safe | Focused Rust tests, then repository integration | pass | P0 | |
| R2 | Model timing and metadata preserve source semantics | Producer/export and stream tests | pass | P0 | |
| R3 | Runner signals arrive through the isolated collector | Collector tests and dev probe | pass | P0 | |
| R4 | Six dashboards apply, render and pass drift checks | Observability scripts and native renders | pass | P0 | |
| R5 | Changed Rust files exceed 99%; TypeScript stays 100% | Coverage and GitHub CI | pass | P0 | |
| R6 | Repository gates and secrets checks | `orly gate pr`, gitleaks | pass | P0 | |

## Dead Code Sweep

Remove misleading panel titles and duplicate dashboard rendering logic. Fold
redundant empty query receipts into the compact evidence without losing the
recorded ten-chat measurements.

## Out of Scope

Merging, production rollout, model-quality benchmarks and inventing historical
measurements. Host-wide node-exporter metrics remain separate from sandbox
resource measurements.

## Discovery

Opening: the runner already declares sandbox start/wait/release/build/hold,
tool duration/OOM and provider-turn/retry metrics in `docs/metrics.runner.census.tsv`.
Their export path is unset on dev. The dedicated host collector is decided but
unbuilt in M233_002 section 2; routing through Fly would violate the current
architecture. Pool occupancy and schedule health are absent from the daemon
census. Individual provider-attempt timing exists in content-free stream logs,
while existing daemon model-duration metrics describe an entire invocation.
