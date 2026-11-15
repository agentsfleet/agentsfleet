# M249 Rust coverage validation

Status: IN_PROGRESS

## Recovery and refusal tests after the rollup rebase

All six shards passed on `a11a0b942aceffce51730ad572c193ab55b79c0c`.
The [combined job](https://github.com/agentsfleet/agentsfleet/actions/runs/43750366060/job/132928514989) still failed: 106,924 of 109,296 lines covered, or 97.8297 percent against the unchanged 98 percent floor.
Its changed-line grade passed at 1,223 of 1,223, or 100 percent.
The six original reports and their common revision sidecars remain separate from earlier runs.

The follow-up adds component tests for authenticated recovery cursors, frozen-sandbox ownership, refused preparation, retained receipts, bounded manifests, nonempty history and files, and failed authentication tags.
Publication tests also cover refused receipt reads, lost replies without duplicate writes, invalid candidates and source replies for another task generation.
Policy tests stop work after unusable assignments or permanent heartbeat failure.
Authoring observations retain uncertainty and refuse interrupted updates.
A direct malformed-cost test checks the meter leaf's defensive refusal; the native client normally rejects that input first.

The test-only HTTPS fixture supplies its pinned certificate and client to the existing provider client and recovery session helpers.
It does not replace the production constructors or establish complete object-store acceptance through `LeaseRun`.
Provider tests verify refusal classification and retry metadata; they do not establish the complete two-attempt logical retry loop.

The scoped instrumented diagnostic passed 208 agent and 473 supervisor tests.
It exercises 190 formerly missed lines at source positions unchanged from the six-shard report.
That comparison identifies newly exercised code; it is not an overall coverage grade and does not combine reports from different revisions.
The pushed revision still needs its complete six-shard measurement before merge.

Source review strengthened the failed-thaw test to assert an attempt before refusal and preserve the original restore error when thaw also fails.
The shared scratch-Git helper now removes inherited `GIT_*` variables.
Its subprocess regression verifies the sentinel repository's HEAD, index and files, and requires the scratch repository's own index.

Receipts include `recovery-native-unit-final.log`, `recovery-reviewed-coverage.log`, `recovery-reviewed.info` and `recovery-reviewed-gain-analysis.json` under `/workspace/artifacts/m249-runtime/`.
The final native `make lint-rustd` passed source checks, formatting, all-feature/all-target Clippy and the dependency scan; its receipt is `recovery-rust-lint-complete.log`.
Review removed two accepted token-overflow seeds because they exceeded the wire schema's `u32` bound; they provide no native-path proof.
The earlier diagnostic's mistaken 500-response retry expectation was corrected to the existing provider behavior: only 429 and 529 retain retry-delay metadata.
No production reporting behavior, SQL, schema, threshold, gate selection or workflow changed.

## Exact parent and local gate limits

At exact parent `6edd08d8402747b3468820c50ce1102bb79fe2fc`, native Rust unit testing passed 1,731 runner, 172 daemon and 3,674 daemon-library tests.
Its integration lane passed 924 cases and failed the inherited rollup-table grant inventory; later daemon and exclusive selections were not reached.
The repaired inventory on this branch includes both existing tables and retains all role and verb probes.

The parent's app coverage run passed 4,630 tests but failed its unchanged statement and branch floors at 99.98 percent.
The separate native website, CLI and design-system baselines passed 133, 2,335 and 516 tests respectively.
The parent's app build passed; its size check failed the existing server-only `effect` import check.

The first local gate on `a11a0b942` passed 1,387 integration cases and failed one: the 100-caller task-decision test exhausted its API connection-pool wait after five seconds.
The same case passed in the six-shard CI run.
The superseded gate's unit command was stopped deliberately while implementing the additional coverage tests.
Its old draft-opening integration override does not establish a passing integration lane or authorize merging a failed result.
The final native gate remains pending at this checkpoint; no assertion or deadline was relaxed.

Baseline receipts are `rollup-exact-parent-unit-rust.log`, `rollup-exact-parent-integration.log`, `rollup-exact-parent-typescript-coverage.log` and `rollup-exact-parent-other-typescript-units.log`.
The production-build receipts are `rollup-parent-app-build.log` and `rollup-parent-app-size.log`.
The interrupted branch gate is retained as `final-rebased-pr-gate.log`.

The remaining merge blocker on PR #845 is Rust line coverage. The owner authorized this repair after the UI checks passed:

> "Move the test-integration coverage up, so we could merge"

## Recorded failure

The six successful shards on `5c3bd896f23bbbd727a05df83ab754949dc860d2` measured 106,183 of 109,225 lines covered, 97.2149 percent. The required floor remains 98 percent. At that denominator, another 858 covered lines would meet it.

The [aggregate job](https://github.com/agentsfleet/agentsfleet/actions/runs/43711893401/job/132810468611) failed after all shards completed. Its reports and revision sidecars were retained unchanged. Historical #844 reports were downloaded separately for diagnosis; they were never mixed into the current grade.

The original Rust tree matched parent `ab4fe8937831ce341474c03a86333997a7c6a4ff`. That parent moved Rust from 1.99.1 to 1.100.0.

## Reproduction

A standalone package pins tracing 0.1.44 and tracing-subscriber 0.3.23. With tracing/log enabled, a subscriber captures and asserts one event's exact event, item, reason and message fields. Both toolchains pass those assertions.

Rust 1.99.1 reports 28/28 covered lines; Rust 1.100.0 reports 28/32 and marks only the emitted field lines as zero.

The workspace manifest already describes the cause. The optional log bridge expands the fields twice; the alternate copy does not run once a tracing subscriber is installed.

SQLx requests that feature. The newer mapping now also affects bare values and literals.

A second probe compares the original event with field shorthand. It asserts identical structured dictionaries. The shorthand path has no zero-hit field lines under Rust 1.100.

These diagnostics establish particular false misses. They do not establish that every missed branch is synthetic. Receipts are retained under `/workspace/artifacts/m249-runtime/coverage-probe/`, with the original reproduction in `rust-1100-coverage-reproduction.md`.

## Repair

Affected emit sites bind simple scalar, literal or path values and use tracing's documented field shorthand. Borrowed aliases stay in a lexical block around the emit. Simultaneous tuple binding preserves references to outer variables when an event field and an input alias share a name.

Literal messages remain `format_args!` values and stay first in the field set. Event names, field names and order, formatting modes, levels and logging features are retained.

The generated aliases contain no allocation or new I/O. Expressions containing calls or captured formatting are outside this mechanical pass.

The initial inventory contains 260 reached events across 169 files, including 642 zero-hit field lines. This inventory selects source to inspect; it is not a measurement of the repaired percentage.

No coverage threshold, ignore pattern, judge, workflow or test selection changed. Billing queries, response fields and producer behavior are unchanged.

## Validation

The standalone equivalence probe passed. The standards review accepted the constrained mechanism under LOG-HOIST and LOG-EVENT.

The adversarial reviewer inspected all 169 formatted source files.
It checked all 260 emit conversions against the original field names, values, order, levels and formatting modes, and reported no findings.

Reference forwarding and the message's recording type were also checked against tracing 0.1.44 and tracing-core 0.1.36.

`make lint-source` passed 44 checks. The first Rust lint run found a missing statement semicolon in the generated blocks; the correction added the semicolons without changing values.

The complete `make lint-rustd` retry passed, including workspace Clippy with all targets and features, formatting and the dependency scan.

The local `make test-coverage-rustd` run passed all 7,178 tests across its seven selections.
The previously failing wall-tick case passed under instrumentation; its earlier non-instrumented failures remain historical results.

The command still exited 2 because coverage failed: 106,613 of 109,546 lines covered, 97.3226 percent against the unchanged 98 percent floor.
This unsharded local report excludes GitHub's separate kernel report. Its percentage is not directly comparable with the earlier six-shard CI union.

The command also printed patch coverage of 131/138, 94.9275 percent against 95 percent.
That comparison used committed `HEAD` while the source repair was uncommitted, so its changed-line positions do not validate the new patch.
The pushed revision requires its own complete CI measurement before merge.

| Instrumented selection | Passed |
|---|---:|
| Runner crates | 1,189 |
| Runner against daemon | 4 |
| Daemon | 284 |
| Native runner streams | 41 |
| API and fleet | 1,328 |
| Substrate | 4,329 |
| Exclusive substrate cases | 3 |

Receipts: `trace-shorthand-coverage.log`, `trace-shorthand-local.info`, `trace-shorthand-local-gaps.json` and `trace-shorthand-tested-rust.patch` under `/workspace/artifacts/m249-runtime/`.
The tested Rust patch has SHA256 `e3ec67464e7e5f4e3f964b2051612ac000ce4182585df9844657586d7ee07c07`.
The 98 percent requirement remains unmet; this is a reviewed checkpoint, not a passing final gate or merge claim.

## Source inventory

The following source paths contain the mechanical emit changes. The workspace manifest commentary and the active M249_004 scope also reference this evidence.

- `rustd/crates/daemon/afd_admission/src/admit.rs`
- `rustd/crates/daemon/afd_api/src/router/mod.rs`
- `rustd/crates/daemon/afd_api/src/router/probes.rs`
- `rustd/crates/daemon/afd_api/src/server.rs`
- `rustd/crates/daemon/afd_api_ingress/src/handler/events/mod.rs`
- `rustd/crates/daemon/afd_api_ingress/src/handler/mention/admit.rs`
- `rustd/crates/daemon/afd_api_ingress/src/handler/webhook/app_route.rs`
- `rustd/crates/daemon/afd_api_ingress/src/handler/webhook/identity_route.rs`
- `rustd/crates/daemon/afd_api_ingress/src/handler/webhook/qstash_route/mod.rs`
- `rustd/crates/daemon/afd_api_operator/src/handler/admin/platform_keys/mod.rs`
- `rustd/crates/daemon/afd_api_tenant/src/handler/auth/session/dashboard.rs`
- `rustd/crates/daemon/afd_api_tenant/src/handler/fleet/memory.rs`
- `rustd/crates/daemon/afd_api_tenant/src/handler/stream/wall/mod.rs`
- `rustd/crates/daemon/afd_approval/src/request/mod.rs`
- `rustd/crates/daemon/afd_authoring/src/creation/system.rs`
- `rustd/crates/daemon/afd_authoring/src/lease/claim/mod.rs`
- `rustd/crates/daemon/afd_authoring/src/model/progress/trace.rs`
- `rustd/crates/daemon/afd_billing/src/ledger/settle.rs`
- `rustd/crates/daemon/afd_billing/src/ledger/sweep/mod.rs`
- `rustd/crates/daemon/afd_billing/src/ledger/unreadable.rs`
- `rustd/crates/daemon/afd_bus/src/lib.rs`
- `rustd/crates/daemon/afd_bus/src/nats/mod.rs`
- `rustd/crates/daemon/afd_bus/src/nats/signals.rs`
- `rustd/crates/daemon/afd_bus/src/wake/publish.rs`
- `rustd/crates/daemon/afd_connector/src/complete.rs`
- `rustd/crates/daemon/afd_connector/src/connect.rs`
- `rustd/crates/daemon/afd_connector/src/grant/holding.rs`
- `rustd/crates/daemon/afd_credential/src/credential/github/exchange.rs`
- `rustd/crates/daemon/afd_credential/src/credential/platform/mod.rs`
- `rustd/crates/daemon/afd_credential/src/secrets/mod.rs`
- `rustd/crates/daemon/afd_cron/src/fire/mod.rs`
- `rustd/crates/daemon/afd_cron/src/qstash/retry.rs`
- `rustd/crates/daemon/afd_db/src/config/tls.rs`
- `rustd/crates/daemon/afd_db/src/migrate/lock.rs`
- `rustd/crates/daemon/afd_db/src/pool/acquire.rs`
- `rustd/crates/daemon/afd_db/src/pool/mod.rs`
- `rustd/crates/daemon/afd_db/src/test_util/platform_default.rs`
- `rustd/crates/daemon/afd_events/src/steer/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/activity/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/admit/fault.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/admit/gates.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/admit/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/affinity.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/billing_sweep.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/bracket.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/commit/steps.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/coverage.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/deliver/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/deliver/window.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/issue.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/memory.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/mint/admission.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/obligation.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/pull/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/pull/refuse.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/pull/step.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/renew/batch/frame.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/renew/batch/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/renew/batch/verdict.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/report/memory.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/report/steps.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/scope.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/sql/report_statement.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/standing/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/tool_detail/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/tool_trace/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/lease/usage/mod.rs`
- `rustd/crates/daemon/afd_fleet/src/task/decision/maintenance.rs`
- `rustd/crates/daemon/afd_fleet/src/task/observe.rs`
- `rustd/crates/daemon/afd_fleet/src/task/waits/fanout.rs`
- `rustd/crates/daemon/afd_gate/src/gate/anomaly.rs`
- `rustd/crates/daemon/afd_gate/src/gate/archive.rs`
- `rustd/crates/daemon/afd_gate/src/gate/first.rs`
- `rustd/crates/daemon/afd_gate/src/gate/park.rs`
- `rustd/crates/daemon/afd_gate/src/gate/pass.rs`
- `rustd/crates/daemon/afd_gateway/src/observations.rs`
- `rustd/crates/daemon/afd_http/src/admission/mod.rs`
- `rustd/crates/daemon/afd_http/src/auth/identity/mod.rs`
- `rustd/crates/daemon/afd_http/src/auth/ownership/extract.rs`
- `rustd/crates/daemon/afd_http/src/auth/ownership/mod.rs`
- `rustd/crates/daemon/afd_http/src/auth/person.rs`
- `rustd/crates/daemon/afd_http/src/handler/mod.rs`
- `rustd/crates/daemon/afd_http/src/handler/refusable/mod.rs`
- `rustd/crates/daemon/afd_http/src/handler/refusal/mod.rs`
- `rustd/crates/daemon/afd_http/src/request_id.rs`
- `rustd/crates/daemon/afd_identity/src/capability.rs`
- `rustd/crates/daemon/afd_identity/src/jwks/cache.rs`
- `rustd/crates/daemon/afd_identity/src/jwks/http.rs`
- `rustd/crates/daemon/afd_identity/src/jwks/verifier.rs`
- `rustd/crates/daemon/afd_identity/src/provider.rs`
- `rustd/crates/daemon/afd_ingress/src/deliver.rs`
- `rustd/crates/daemon/afd_mail/src/mailer/mod.rs`
- `rustd/crates/daemon/afd_memory/src/hydrate.rs`
- `rustd/crates/daemon/afd_memory/src/memories.rs`
- `rustd/crates/daemon/afd_outbound/src/abandon.rs`
- `rustd/crates/daemon/afd_outbound/src/interim.rs`
- `rustd/crates/daemon/afd_outbound/src/lanes/claim.rs`
- `rustd/crates/daemon/afd_outbound/src/lanes/mod.rs`
- `rustd/crates/daemon/afd_outbound/src/poster/mod.rs`
- `rustd/crates/daemon/afd_outbound/src/slack/mod.rs`
- `rustd/crates/daemon/afd_outbound/src/worker/mod.rs`
- `rustd/crates/daemon/afd_runner/src/heartbeat/mod.rs`
- `rustd/crates/daemon/afd_runner/src/heartbeat/verdict.rs`
- `rustd/crates/daemon/afd_runner/src/rotate.rs`
- `rustd/crates/daemon/afd_runner/src/store.rs`
- `rustd/crates/daemon/afd_runner/src/sweep/census/mod.rs`
- `rustd/crates/daemon/afd_runner/src/sweep/mod.rs`
- `rustd/crates/daemon/afd_runner/src/sweep/rebuild/mod.rs`
- `rustd/crates/daemon/afd_sse/src/live.rs`
- `rustd/crates/daemon/afd_state/src/credentials/mod.rs`
- `rustd/crates/daemon/afd_state/src/credentials/rows.rs`
- `rustd/crates/daemon/afd_tenant/src/cli_credential/mod.rs`
- `rustd/crates/daemon/afd_tenant/src/signup/mod.rs`
- `rustd/crates/daemon/afd_tenant/src/team/invitation/lifecycle.rs`
- `rustd/crates/daemon/afd_tenant/src/workspace/crossing.rs`
- `rustd/crates/daemon/afd_vault/src/classified.rs`
- `rustd/crates/daemon/afd_vault/src/delete.rs`
- `rustd/crates/daemon/afd_vault/src/load.rs`
- `rustd/crates/daemon/afd_vault/src/write.rs`
- `rustd/crates/daemon/main/src/credentials.rs`
- `rustd/crates/daemon/main/src/daemon.rs`
- `rustd/crates/daemon/main/src/identity/mod.rs`
- `rustd/crates/daemon/main/src/serve/accept/mod.rs`
- `rustd/crates/daemon/main/src/serve/drain/mod.rs`
- `rustd/crates/daemon/main/src/serve/exporting.rs`
- `rustd/crates/daemon/main/src/serve/optional.rs`
- `rustd/crates/daemon/main/src/serve/runner_stream.rs`
- `rustd/crates/daemon/main/src/supervisor/mod.rs`
- `rustd/crates/daemon/main/src/sweepers/authoring.rs`
- `rustd/crates/daemon/main/src/sweepers/billing.rs`
- `rustd/crates/daemon/main/src/sweepers/checkpoints.rs`
- `rustd/crates/daemon/main/src/sweepers/expiry.rs`
- `rustd/crates/daemon/main/src/telemetry/mod.rs`
- `rustd/crates/harness/af_egress/src/redirect/mod.rs`
- `rustd/crates/harness/af_providers/src/decision/observe.rs`
- `rustd/crates/protocol/af_context/src/trace.rs`
- `rustd/crates/protocol/af_inference/src/count/mod.rs`
- `rustd/crates/protocol/af_inference/src/stream/trace.rs`
- `rustd/crates/protocol/af_observability/src/delivery/mod.rs`
- `rustd/crates/protocol/af_observability/src/export/mod.rs`
- `rustd/crates/protocol/af_observability/src/metrics/export/mod.rs`
- `rustd/crates/protocol/af_otlp/src/pipelines/exports.rs`
- `rustd/crates/protocol/af_stream/src/server/mod.rs`
- `rustd/crates/protocol/af_stream/src/telemetry.rs`
- `rustd/crates/runner/afr_executor/src/client/call/mod.rs`
- `rustd/crates/runner/afr_executor/src/client/link.rs`
- `rustd/crates/runner/afr_executor/src/server/process/mod.rs`
- `rustd/crates/runner/afr_memory/src/hydrated.rs`
- `rustd/crates/runner/afr_sandbox/src/bubblewrap_engine/boot.rs`
- `rustd/crates/runner/afr_sandbox/src/bubblewrap_engine/mod.rs`
- `rustd/crates/runner/afr_sandbox/src/bubblewrap_engine/parts/teardown.rs`
- `rustd/crates/runner/afr_sandbox/src/bubblewrap_engine/sweep.rs`
- `rustd/crates/runner/afr_sandbox/src/cgroup/mod.rs`
- `rustd/crates/runner/afr_sandbox/src/egress/mod.rs`
- `rustd/crates/runner/afr_sandbox/src/warm_slots/mod.rs`
- `rustd/crates/runner/afr_sandbox/src/workspace_disk/mod.rs`
- `rustd/crates/runner/afr_supervisor/src/activity/mod.rs`
- `rustd/crates/runner/afr_supervisor/src/halt/mod.rs`
- `rustd/crates/runner/afr_supervisor/src/holds/keeper.rs`
- `rustd/crates/runner/afr_supervisor/src/lease_loop/build.rs`
- `rustd/crates/runner/afr_supervisor/src/lease_loop/drive.rs`
- `rustd/crates/runner/afr_supervisor/src/lease_loop/hold.rs`
- `rustd/crates/runner/afr_supervisor/src/lease_loop/settle.rs`
- `rustd/crates/runner/afr_supervisor/src/renew/mod.rs`
- `rustd/crates/runner/afr_supervisor/src/shutdown.rs`
- `rustd/crates/runner/afr_supervisor/src/uplink/queues.rs`
- `rustd/crates/runner/afr_telemetry/src/telemetry/mod.rs`
- `rustd/crates/runner/afr_tools/src/egress/mod.rs`
- `rustd/crates/runner/afr_tools/src/sandbox/oneshot/mod.rs`

## Rebased boundary checks

The UI branch now uses parent `6edd08d8402747b3468820c50ce1102bb79fe2fc`.
Earlier coverage totals describe their named revisions; they do not grade the new daily-rollup source.
This follow-up adds no SQL, schema or billing producer changes against that parent.

Six new supervisor tests check task input, binding, receipt, publication, effect-page and effect-read boundaries.
Each verifies the supervisor's fence, one attempted request and exact refusal or unavailable outcomes.
Structured fixture inputs are validated before calls.

Three resolver tests check public numeric addresses, blocked loopback/private/metadata destinations and an unspecified address.
The existing resolver moved into `resolver/mod.rs` without a production behavior change.
These tests exercise its address policy, not HTTP authorization or external DNS availability.

A scoped diagnostic changed the authoritative input fence and then reversed the resolver's address decision.
Both mutations failed at test assertions, rather than compilation.
The diagnostic restored both files byte-for-byte before the green checks.

The restored scoped suites passed six supervisor and three resolver tests with all features enabled.
These Cargo diagnostics do not replace the native repository checks.
Their logs are `task-guards-mutation-summary.log`, `task-guards-mutation-results.json` and the two `*-guards-green.log` files.

The native `make lint-rustd` passed source checks, formatting, all-target/all-feature Clippy and the dependency scan.
Its log is `rollup-rust-lint-final.log` under `/workspace/artifacts/m249-runtime/`.
Mechanical parent lint repairs use lazy optional defaults and a one-item iterator; no lint rule changed.

## Rebased integration fixtures

The exact parent `6edd08d84` passed all three native unit selections: 1,731 runner, 172 daemon and 3,674 daemon-library tests.
The baseline ran in isolated `/workspace/m249-rollup-baseline` through `make test-unit-rustd`, with every feature enabled.
Its integration result remains separate from these datastore-free counts.

The daemon coverage shard failed the same unsupported-event assertion on `5f7729f12` and `cfa2c5fe0`.
Each run passed 192 cases and failed one, at `integration_runner_e2e/refusals.rs:32`.
The ordinary producer commits an admission and then publishes its wake; the unsupported-event fixture only inserted its row.

The recovery probe runs every five seconds; the refusal window stays at two seconds.
The fixture now awaits the shared post-commit wake after its insert.
It retains the same admission identifier, refusal check, no-offer assertion and deadline.

This uses `afd_bus::wake::publish`, as production admission does; it changes no production producer, retry policy or recovery interval.
The standards review confirmed the dispatch/refusal path and the bounded fixture repair.
Native integration and new-head coverage must verify the outcome before merge.

The substrate shard on `cfa2c5fe0` passed 102 database cases and failed the table-grant inventory.
The catalogue contained `billing.usage_daily` and `billing.runs_daily`, while the expected inventory omitted both.
The inventory now includes both tables with their existing `api_runtime` read/write grants.

The existing test probes every runtime role and verb against both tables and still rejects truncation and unrelated roles.
No grant, SQL file, schema, table or permission changed.
Their canonical declarations remain in `20261108124610_billing_usage_daily.sql` and `20261108124620_billing_runs_daily.sql`.

The failed aggregate explicitly refused to grade incomplete shard reports.
No new overall percentage is claimed from that run; the 98 percent floor remains unchanged.

The untouched parent integration run also failed the missing grant inventory: 924 passes and one failure before the lane stopped.
The daemon end-to-end case and exclusive selection were not reached, so this baseline does not establish their outcomes.
Its log is `rollup-exact-parent-integration.log`; the clean baseline checkout remains at `6edd08d84`.
