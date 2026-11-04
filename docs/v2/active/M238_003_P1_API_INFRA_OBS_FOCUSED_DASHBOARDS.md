# M238_003: Focused live dashboards

**Status:** IN_PROGRESS
**Priority:** P1
**Branch:** chore/m238-dev-round-trip
**Pull request:** #801
**Baseline:** ffde2699b39cca1463542c53782ebec52044c838
**Closeout:** Indy requested closure. His visual review and live acceptance remain pending in the closeout record below.

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
| `rustd/crates/afr_telemetry/` | EDIT | Shared provider attribution and missing runner measurements |
| `rustd/crates/agentsfleetd/` | EDIT | Connect boot-owned readings without blocking collection callbacks |
| `rustd/crates/afd_fleet/tests/support/fleet_report_reads.rs` | EDIT | Isolate proposed catalogue keys after the full integration run exposed a duplicate primary key |
| `playbooks/operations/acceptance/baselines/M238_001-20261104/` | CONSOLIDATE | One compact receipt, native screenshot and rejection log; raw history retained |
| `docs/v2/active/M233_002_P1_API_INFRA_OBS_UI_CHAT_ROUND_TRIP_STAGE_TIMING.md` | EDIT | Point evidence at the compact receipt |
| `docs/v2/done/M225_001_P1_API_OBS_LEASE_PICKUP_WAKE_HINT.md` | EDIT | Preserve the ten original queue readings through the consolidated receipt |
| `scripts/collector_temporality_test.py` | EDIT | Exercise all three real metrics pipelines |
| `scripts/runner_collector_admission_test.py` | CREATE | Prove the nftables rule rejects a non-root sandbox identity |
| `scripts/fixtures/` | CREATE | Isolated collector admission test image |
| `.github/workflows/test-unit-rustd.yml` | EDIT | Indy requested `cargo clean` after a hosted-runner disk exhaustion |
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

DONE records implementation closure at Indy’s request. The acceptance rubric and closeout record retain checks that still need evidence.

### Section 1 — Pool and scheduler health

- **Dimension 1.1** — **DONE** — Pool readings reflect the actual role's total, idle, used,
  capacity and pending acquisitions; closing a pool cannot fabricate capacity.
- **Dimension 1.2** — **DONE** — Acquire success, timeout, dependency failure and cancellation
  leave accurate duration/outcome readings, with no leaked pending count.
- **Dimension 1.3** — **DONE** — Schedule outcomes distinguish one accepted fire from its
  retries, stale targets and dependency failures. Dispatch lag uses a measured
  scheduled instant and remains absent when that instant is unknown.

### Section 2 — Model usage, latency and capabilities

- **Dimension 2.1** — **DONE** — Each provider stream request records first-token and terminal
  latency, including errors, timeouts and cancellation; run time remains separate.
- **Dimension 2.2** — **DONE** — Model identity is normalized before metrics; input, output,
  cache tokens and spend retain existing accounting and replay semantics.
- **Dimension 2.3** — **DONE** — Capability and limit tables use the same catalogue facts as
  the product. Missing metadata stays absent, and removed entries do not leave
  an indefinitely live capability claim.

### Section 3 — Actual sandbox runs and runner export

- **Dimension 3.1** — **DONE** — Existing start, wait, release, reuse, tool and limit signals
  reach the dedicated runner collector, with bounded resources and retry state.
- **Dimension 3.2** — **DONE** — Non-root sandbox processes cannot submit to the host OTLP
  receiver. No logs pipeline is enabled; journald stays local until its separate allowlist is implemented.
- **Dimension 3.3** — **DONE** — Real sandbox and model calls produce joinable traces and
  truthful metrics; preparation, waiting and execution remain separate.

### Section 4 — Six concise dashboards

- **Dimension 4.1** — **DONE** — Overview links to five focused dashboards; titles are short
  and units, filters and percentile labels are consistent.
- **Dimension 4.2** — **DONE** — Apply and drift verification cover every declared dashboard,
  reject missing/foreign sources and detect query, reducer and variable drift.
- **Dimension 4.3** — **DONE** — Every query names a family in a producer census or uses
  declared delivery-span attributes; unavailable data is never replaced by zero.
- **Dimension 4.4** — **DONE** — Retain a compact acceptance receipt and screenshots;
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

The runner collector uses memory_limiter 200 MiB, MemoryMax 256 MiB, an in-memory queue of 1,000 batches and retry at most 300 seconds. It has no disk queue or off-host log retention; trace admission keeps the runner's 256-per-lease and 128-per-second limits. Dev D1 is resolved to the existing grafana-dev key by the installation reported by Indy; production deployment remains outside this work.

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

Named proofs for the new mechanisms:

- Pool callback ownership: `pool_callback_reads_actual_capacity_and_withdraws_a_closed_pool`.
- Pool occupancy/cancellation: `test_pool_snapshot_tracks_use_and_cancelled_waiters`.
- Scheduler states and failed dependencies: `health_tracks_real_states_and_refuses_a_closed_database`.
- Schedule callback outcomes: `should_export_schedule_decisions_and_matched_http_requests`.
- Provider timeout/cancellation: `silent_provider_respects_the_deadline_and_caller_cancellation`
  and `a_silent_runtime_call_records_deadline_and_cancellation_once`.
- Metadata privacy/expiry: `catalogue_metadata_expires_without_inventing_zero`.
- Real sampler reads and withdrawal: `snapshots_read_the_catalogue_and_clear_on_database_failure`.
- Collector admission: `test_root_passes_nonroot_is_dropped_and_reload_keeps_the_boundary`.

## Acceptance Rubric

These grades preserve the recorded evidence. Closing the spec does not turn a pending check into a pass.

| ID | Criterion | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | Pool and schedule observations are real and replay-safe | `make test-integration-rustd` | 0 failed | P0 | PASS on `1bb7e7fcf`; local integration passed in 16 minutes, 23 seconds |
| R2 | Model timing and metadata preserve source semantics | `make test-unit-all` | 0 failed | P0 | PENDING — full unit gate |
| R3 | Runner signals arrive through the isolated collector | `make check-collector-temporality check-runner-collector-admission`, then live probe | 4 pipeline + 2 admission tests pass; live export verified | P0 | PARTIAL — collector proofs and real runner export pass; actual sandbox witness pending with Indy’s acceptance |
| R4 | Six dashboards apply, render and pass drift checks | `bash playbooks/operations/observability/01_assets_check.sh`; `OBS_ENV=dev ALLOW_VAULT_READS=1 bash playbooks/operations/observability/04_verify.sh`, native review | 6 dashboards match; 0 query errors; live samples verified | P0 | PARTIAL — six dashboards apply/render/match; new producer samples await deployment |
| R5 | Changed Rust files exceed 99%; TypeScript stays 100% | `make test-coverage-rustd`; `make test-coverage-all`; GitHub CI | changed Rust >99%, TypeScript 100% | P0 | PENDING — coverage for the final revision |
| R6 | Repository gates and secrets checks | `orly gate pr`, gitleaks | 0 failures and 0 leaks | P0 | PENDING — final repository gate |

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

CI follow-up: the daemon-library job on `ee62c737d` failed because the hosted
runner exhausted disk (`System.IO.IOException: No space left on device`),
not because of a reported test assertion. Indy: "just do cargo clean".
Add a clean-target step after cache restore in that job, preserving downloaded
registry sources and keeping test selections and coverage thresholds intact.

Runner-host update from Indy: the other session installed #801's collector
assets from `9b12fe0d8` using `ZMB_CD_DEV/grafana-dev` and restarted the runner
without active leases. Real data now reaches Tempo and Prometheus. Independent
Grafana checks confirm one provider-turn count, 0.687962662 seconds in its sum,
and a runner.lease result. No sandbox or tool ran, so their missing series are
expected inactivity. The actual-sandbox admission witness stays pending as
Indy accepted. This resolves dev D1 and the earlier unknown runner-export
status; it does not claim the new #801 runner binary is deployed.

Coverage follow-up targets the eight files named by CI: verified callback
failures, caller/deadline stream outcomes, schedule health and reconciliation
errors, snapshot Debug privacy, daemon pool callbacks and sampler snapshots.
The latest global line floor passed (99.6040%), but patch/per-file floors did
not. No completion or >99% changed-file claim is made yet.

Operator-link review: two API alerts retained links to their former Overview
panels after the dashboard split, and runner-silent referenced a removed panel.
Restore Heartbeat age in Overview, bind API alerts to API & Database, and grade
all alert dashboard/panel references in both asset and live drift checks.
This changes navigation only; alert rules and their four-rule count stay intact.

The coverage run on `ced6ac502` measured 99.6966% overall and 99.6487% of
changed lines. One touched file remains below the per-file floor: daemon
export startup, failure flush and the supervised sampler. Add a real-boot
collector witness in its own test binary so it owns the process subscriber.
The local integration run also exposed a shared model seed's fixed primary
key colliding during concurrent inserts; propose a fresh key per attempt while
retaining the existing provider/model upsert. No coverage floor is changed.

Indy approved the live inventory's proposed names: "yes update them".
Update all 57 panel titles and purpose descriptions across the six dashboards.
Keep sampling limits, overlapping chat segments, metadata expiry and missing-data
meaning in the tooltips. Queries, reducers, units and alert conditions retain
their measured meaning. Asset checks and live drift verification cover the
presentation as well as the linked dashboard and panel for each alert.

The approved naming update is live on dev: 57 panels across six dashboards,
with title and description drift verification passing and six native renders
returning HTTP 200. Fourteen asset cases and three drift suites pass. Review
corrected two tooltips: delivery stages include completion, and runner cleanup
includes tool-record posts, fallback memory capture and pending usage flush.

The exporter lifecycle witness passes against real datastores and a collector:
live memory, catalogue and schedule snapshots arrive, shutdown joins, and a
queue failure after the database opens flushes its buffered pool record before
boot returns. The isolated model-seed regression reproduces the old primary-key
collision and passes with fresh proposed keys. These are focused checks;
repository integration and exact-revision coverage remain pending.

Latest review: replace the raw provider-omission legend with Provider name
omitted using a display override. Its underlying metric labels remain useful
for investigation. Update acceptance grades explicitly rather than treating
finished dashboard presentation as completed live instrumentation acceptance.
The final gate on `93bb96acf` passed integration, then was stopped during unit
checks when #802 introduced merge conflicts. Resolve the conflicts, retain
retry-budget behavior and outcome measurements, and rerun on the merged head.

Provider-attribution correction: Indy asked, "Okay will there a fix to support
our providers?" The pinned OpenTelemetry attribute reference explicitly says
custom provider values MAY be used when no well-known value applies:
https://github.com/open-telemetry/semantic-conventions-genai/blob/2e994c6d59a93bb4fc1752c5378eedb9b8e14d6b/docs/registry/attributes/gen-ai.md#gen-ai-provider-name
Our normalizer incorrectly treated its well-known list as closed. A live runner
trace records `accounts/fireworks/models/minimax-m3p1-flash`, while provider-turn
metrics aggregate under `_other` and the daemon counts the omitted provider key.
Use one bounded naming registry derived from the compiled model catalogue and
SDK provider registry, with standard aliases when defined. Share it across
runner metrics and daemon/runner traces. Keep arbitrary private provider names
and endpoint URLs out of labels. MiniMax through Fireworks must retain Fireworks
as provider and MiniMax as model; model names never determine the provider.

The provider regression fails on the previous normalizer: `fireworks` returns
no attribution. The shared registry now derives public identities from the
existing curated catalogue and the SDK, with canonical standard aliases and
no new authored provider inventory. The first combined producer/export run
passes 195 tests, including Fireworks attribution in delivery spans and runner
turn/call/token exports. Public aliases are deduplicated before counting label
products; existing runner cardinality ceilings still pass unchanged. Private
names and endpoint URLs remain absent or `_other`. Historical omissions remain
historical; the daemon and runner binaries must be deployed for new attribution.


### Owner-requested closeout

Indy requested: "Can you move the spec to done? Send a another PR".
He added: "Indy will eyeball bad confirm later".
This closes the implementation record and leaves his visual confirmation pending.

The six dashboards and their 57 panel names are live, as recorded above.
The provider fix is in #801; it needs deployed daemon and runner binaries before new samples can prove attribution.
Historical provider omissions cannot be recovered from the missing attributes.

The following acceptance work remains visible after closure:

- Indy reviews the six dashboards and confirms the displayed measurements.
- Post-deployment checks confirm new producer samples, including Fireworks attribution for hosted MiniMax calls.
- The managed Tempo datasource still needs a resolution for its zero-value display defect.
- The actual-sandbox admission witness remains pending under Indy’s earlier acceptance: "i think we are good, the sandbox wont reach yet".
- Final repository and coverage results must be recorded for the exact reviewed revision.

The latest local integration gate passed on `1bb7e7fcf` in 16 minutes, 23 seconds.
The focused provider run passed 195 tests.
Three coverage reports put all 38 measured changed Rust files above 99%; the missing kernel report prevents a final coverage claim.

At this checkpoint, the kernel reproducibility and toolbox checks failed.
These results remain incomplete; this documentation move does not waive either gate.
M233_002 stays active for its separate unfinished timing requirements.


### Owner-requested fixture cleanup

| Surface | Action | Reason |
|---|---|---|
| Dev evidence folder and four listed test/fixture files | DELETE | Owner-requested cleanup |
| `make/quality.mk`, `make/check-safety-gates.mk` | EDIT | Remove the deleted test target and prerequisite |
| Observability playbooks | EDIT | Remove commands and local evidence links that no longer exist |
| M233_002, M238_001 and M225_001 | EDIT | Keep open requirements and measured results readable after cleanup |

Indy requested: "remove these", naming the dev evidence folder, both dashboard regression suites, the collector conversion test and its Dockerfile.
Remove their Make wiring and update live instructions that referred to those files.
The six dashboard definitions, alert definitions, asset validator and live drift check remain.

The measurement summaries stay in M238_001 and M225_001 Discovery.
Their original screenshots and raw records remain in Git history.
Historical test commands above describe earlier verification, not the current check inventory.

M233_002 remains active because per-request daemon stages and browser/edge timing were never implemented.
Its header now states the remaining work explicitly.


### Single-PR consolidation

Indy requested: "push it in PR801 and close 804."
Keep the closeout, wording fixes and requested fixture cleanup in #801.
GitHub marked #804 merged when its commits landed in its base branch, the branch for #801.
The main branch was not merged; #801 remains the single open implementation PR.


### Greptile review repairs

The merge waits for both review findings to be fixed and verified.

- **Dimension 5.1** — IN_PROGRESS — Daemon counters and histograms accumulate in the source process before the shared collector endpoint.
  Real exported frames keep cumulative counts and one start time when requests reach different collector receivers.
- **Dimension 5.2** — IN_PROGRESS — Dashboard drift checks compare panel display options, including percentile reducer settings.
  Matching resources pass; changing reducer values or calculations fails.

Additional review scope: `docs/metrics.census.tsv`, the OTLP pipeline documentation and native-export regression in `rustd/crates/afd_otlp/`, and collector comments.
The existing census-to-provider routing supplies cumulative export without a new setting or a second accumulation layer.
Runner delta histograms keep their isolated host collector.
