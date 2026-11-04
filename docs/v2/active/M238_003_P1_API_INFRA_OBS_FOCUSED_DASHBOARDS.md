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
| `rustd/crates/afd_admin/` | EDIT | Bounded canonical catalogue snapshots |
| `rustd/crates/afd_api/` | EDIT | Matched-template request timing |
| `rustd/crates/afd_api_runner/` | EDIT | Distinguish first report settlement from its repeated successful acknowledgements |
| `rustd/crates/afd_api_ingress/` | EDIT | Verified schedule callback outcomes |
| `rustd/crates/afd_authoring/` | EDIT | Attribute private model calls to the actual provider |
| `rustd/crates/afr_supervisor/` | EDIT | Count inline memory and fallback choices at the actual branch |
| `rustd/crates/afr_providers/` | EDIT | Attribute actual runner model calls |
| `rustd/crates/afd_wire/` | EDIT | Internal optional token observations, without serialized API changes |
| `rustd/crates/afd_observability/` | EDIT | Typed instruments, observations, metadata and export proofs |
| `rustd/crates/afd_db/` | EDIT | Actual pool occupancy, capacity, acquisition wait and outcomes |
| `rustd/crates/afd_cron/` | EDIT | Accepted/replayed fires, dispatch lag and failed/stale targets |
| `rustd/crates/afd_inference/` | EDIT | First-token and terminal timing for individual provider attempts |
| `rustd/crates/afr_telemetry/` | EDIT if needed | Model attribution and missing runner measurements |
| `rustd/crates/agentsfleetd/` | EDIT | Connect boot-owned readings without blocking collection callbacks |
| `playbooks/operations/acceptance/baselines/M238_001-20261104/` | CONSOLIDATE | One compact receipt, native screenshot and rejection log; raw history retained |
| `docs/v2/active/M233_002_P1_API_INFRA_OBS_UI_CHAT_ROUND_TRIP_STAGE_TIMING.md` | EDIT | Point evidence at the compact receipt |
| `docs/v2/done/M225_001_P1_API_OBS_LEASE_PICKUP_WAKE_HINT.md` | EDIT | Preserve the ten original queue readings through the consolidated receipt |
| `scripts/collector_temporality_test.py` | EDIT | Exercise all three real metrics pipelines |
| `scripts/runner_collector_admission_test.py` | CREATE | Prove the nftables rule rejects a non-root sandbox identity |
| `scripts/fixtures/` | CREATE | Isolated collector admission test image |
| `make/quality.mk` | EDIT | Run the new infrastructure proof automatically |
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

- **Dimension 2.1** — Each provider stream request records first-token and terminal
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
  receiver. No logs pipeline is enabled; journald stays local until its separate allowlist is implemented.
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

The runner collector uses memory_limiter 200 MiB, MemoryMax 256 MiB, an in-memory queue of 1,000 batches and retry at most 300 seconds. It has no disk queue or off-host log retention; trace admission keeps the runner's 256-per-lease and 128-per-second limits. Host credential deployment waits on the explicit M233_002 D1 choice between the existing grafana-dev key and a dedicated write-only key.

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
| R1 | Pool and schedule observations are real and replay-safe | `make test-integration-rustd` | 0 failed | P0 | |
| R2 | Model timing and metadata preserve source semantics | `make test-unit-all` | 0 failed | P0 | |
| R3 | Runner signals arrive through the isolated collector | `make check-collector-temporality check-runner-collector-admission`, then live probe | 4 pipeline + 2 admission tests pass; live export verified | P0 | |
| R4 | Six dashboards apply, render and pass drift checks | `bash playbooks/operations/observability/01_assets_check.sh`; `OBS_ENV=dev ALLOW_VAULT_READS=1 bash playbooks/operations/observability/04_verify.sh`, native review | 6 dashboards match; 0 query errors; live samples verified | P0 | |
| R5 | Changed Rust files exceed 99%; TypeScript stays 100% | `make test-coverage-rustd`; `make test-coverage-all`; GitHub CI | changed Rust >99%, TypeScript 100% | P0 | |
| R6 | Repository gates and secrets checks | `orly gate pr`, gitleaks | 0 failures and 0 leaks | P0 | |

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

Design review: current daemon boot opens the API-role pool alone; only pools actually opened receive occupancy readings. An acquisition is measured once across the existing retry, and cancellation drops its pending guard. Provider terminal time/cause must be latched before optional usage recovery. Capability snapshots require a bounded vendor-model catalogue, expiry and removal semantics; raw custom model strings are not labels.

Nov 04 implementation checkpoint: added daemon pool, API, scheduler and model
observations, plus runner per-model calls and token observations. Focused
`afd_inference`, `afr_telemetry` and `afd_observability` tests passed before the
additional abandonment-usage refinement. The daemon and supervisor compile
with all features. Six dashboard assets now have exact per-page inventories;
the nine negative asset cases and three drift suites pass. Live apply and
native review are still being checked. These are inner-loop results, not a
repository gate or coverage claim. New instrumentation has not been deployed.

Indy review, Nov 04: remove duplicated queue depth/age and general lease-poll
cost from Scheduler. Keep runner wakeups only in Chat latency, where they
explain #799. Scheduler now owns scheduled triggers, durable fires, verified
callbacks, one-shot dispatch lag and sync outcomes. Blank model-call panels
remain pending the new producer deployment and isolated runner export.

Runner collector assets are drafted in `deploy/baremetal/otelcol-runner.{yml,nft,service}`
and `install-collector.sh`. The real contrib 0.164.0 binary validates the config.
Release archive hashes were read from upstream's per-architecture `.sha256`
assets. These assets are not installed on dev. Host execution and the D1 key
choice remain pending; local syntax validation does not prove sandbox rejection.

Live recheck after Indy's question, Nov 04, 2026, 07:40 UTC:
`flyctl` confirms `otelcol-dev` machine `812d66a974ed38` is started,
running contrib 0.164.0; daemon machine `48ee32df500958` is also started.
The daemon has both `OTEL_EXPORTER_OTLP_ENDPOINT` and
`OTEL_EXPORTER_OTLP_HEADERS` configured (values not printed).
The collector's 100 buffered log records from 07:39:06–07:40:01 UTC include
11 HTTP 400 export failures. At 07:39:11 it reports “invalid temporality and
type combination” for `agentsfleet_fleet_delivery_stage_seconds` and
`agentsfleet.billing.credit.consumed`, dropping 56 items in that export.
Thus Fly collector presence is verified, but healthy metric delivery is not.
The prior user-facing wording blurred this existing Fly collector with the
proposed runner-host collector. No direct live runner-host inspection was
performed in this recheck; the host's current export configuration remains
unverified, rather than inferred from missing Grafana samples.

Live drop diagnosis, Nov 04, 2026, 07:54 UTC: Grafana reports approximately
6.008 extrapolated `gen_ai.provider.name` / `unmapped_provider` attribute
omissions in the preceding hour. This counter omits an attribute and does
not count a whole span as lost. Daemon export-discard, runner export-discard
and runner span-suppression queries returned no series; absence is not zero
and does not disprove collector-side loss. Split Overview's combined panel
into Attribute omissions and Export loss with explicit ownership.
Direct runner-host recheck is blocked by the managed executor's current
`vpn_configured: false` and empty TCP grants; no live host claim is made.

The Overview split is applied to dev and `04_verify.sh` confirms all six
live dashboards match the current assets. Ten asset negative cases and all
three drift suites pass, including the new UID/table-mode checks. The vault's
dev runner target is `debian@zombie-dev-worker-ant`; the supported executor
network policy has VPN disabled and no TCP grants, so direct host inspection
still cannot proceed here. No Tailscale client was installed or network route
changed to work around this boundary. Running collector config remains unchanged.

Indy requested pushing the refinement before completing validation, then
starting the remaining checks. This commit is a draft checkpoint, not a
completion or coverage claim. Repository integration, final coverage and CI
remain pending; the runner-host diagnosis is owned by Indy's other session.
Live audit checked all 62 PromQL targets: 14 returned finite values, 48 were
empty and none returned a query error. The last-24-hour Tempo searches returned
daemon delivery spans but no runner.lease, chat or execute_tool spans. This
proves missing runner telemetry in the queried datasource, not collector absence.

Review follow-up: seed known SDK usage before any finish await, restart an
existing collector after installation, and clean temporary files on EXIT.
The code fixes are pushed in `783b4cef0`; the earlier checkpoint gate was
stopped for these changes and will rerun. Consolidated duplicate evidence
responses into one receipt while retaining all ten segment observations and
original queue-log values. No runner host or Fly collector deployment performed.
