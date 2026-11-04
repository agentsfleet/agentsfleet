# Dev round-trip measurement — Nov 04, 2026

Daemon `/healthz`: `0.73.0`, build `370a86f`. GitHub identifies `370a86f6fcba621e32a244efc69bf08215f20648` as the tested merge of #799 head `6facb05926e5fc7ae79e0f167be45e6be6b0237a` into `fde5859717c73ea52d02d84d2408b8f00efcae82`. Its tree and final merge `11883fdaba2197adf8931d334396ba8b3bb9eb5b` are both `758e6d2c313b194f1f0565c1552b5500ba4f8e34`.

Deployment run https://github.com/agentsfleet/agentsfleet/actions/runs/41751074343 succeeded.

`.github/workflows/deploy-dev-build.yml` promotes the prebuilt candidate for the identical source tree; the build stamp differs from the final merge commit.

`flyctl secrets list --app agentsfleetd-dev` confirms `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS` are deployed. `OBS_ENV=dev ALLOW_VAULT_READS=1 bash playbooks/operations/observability/02_credentials.sh` passed credentials, Prometheus and Loki probes. No credential values are recorded.

Fleet `support-triage`: `01a1a42f-f181-7e1b-9f20-481730f1b7cd`, workspace `01a1551c-987e-74f7-8bd9-b5524913d462`. Ten unique chats settled as `processed`, Nov 04, 2026, 07:03:04–07:03:45 AM Asia/Calcutta.

The first admission was retried with its original operation identifier and replayed the same event. Tempo contains ten unique delivery spans.

## Per-fleet result

Tempo datasource `grafanacloud-traces`; plain search returned ten traces:

```traceql
{ name = "fleet.delivery" && span."gen_ai.agent.id" = "01a1a42f-f181-7e1b-9f20-481730f1b7cd" }
```

`quantile_over_time(duration, .95, .99)` and all seven attribute queries returned empty `series`, with zero engine bytes. The datasource configuration does not expose the metrics-generator local-blocks setting; proxied configuration and generator-status routes returned 404.

Whether local blocks are enabled remains unverified.

Successful search proves daemon trace export and post-deploy settlement.

Values below use Grafana 12.4.0's percentile reducer: sort numeric samples, then select index `Math.round((n−1)q)` (`packages/grafana-data/src/transformations/fieldReducer.ts`, `calculatePercentile`). With ten samples, both p95 and p99 select the maximum. They were computed from datasource results, not witnessed in Explore.

Search with `select(span."agentsfleet.delivery.model_ms")` also returned the model attribute.

Missing attributes are excluded, never replaced by zero.

| Measurement | Samples | p95 (ms) | p99 (ms) |
|---|---:|---:|---:|
| Whole round trip | 10 | 3061.00 | 3061.00 |
| Queue wait | 10 | 223.00 | 223.00 |
| Memory | 0 | unavailable | unavailable |
| Model | 10 | 2661.00 | 2661.00 |
| Tools | 10 | 0.00 | 0.00 |
| Sandbox wait | 0 | unavailable | unavailable |
| Settle | 10 | 0.00 | 0.00 |
| Complete | 10 | 169.00 | 169.00 |

Memory is absent because the lease carries its memory window: `afr_supervisor/src/lease_loop/mod.rs` starts `Segment::Memory` only for fallback memory reads. Sandbox wait is absent because these tool-free chats never wait for preparation (`segments.rs`). Neither is a measured zero.

Screenshot path: **unavailable**. No connected browser-control tool is exposed.

Explore and its Reduce transformation were not visually exercised. Dimension 4.4 stays incomplete for that witness and the two absent segment samples.

## Fleet-wide result and export defect

Both documented `histogram_quantile` queries over `rate(agentsfleet_fleet_delivery_stage_seconds_bucket[5m])` returned empty vectors. Name-regex searches, native-histogram alternatives and ten-minute history also found no delivery family.

`flyctl logs --app otelcol-dev --no-tail` at 01:33:44 and 01:33:49 UTC reports Grafana metrics ingest HTTP 400: `invalid temporality and type combination for metric "agentsfleet_fleet_delivery_stage_seconds"`.

The rejection also names lease-history, invocation-duration, token-usage, cache-read and billing-consumed families.

The census declares these families delta; `deploy/fly/otelcol-dev/config.yml` forwards them unchanged.

A separate fix pull request is required. Empty results do not mean zero latency.

`agentsfleet_runner_wake_answers_total`, instance `48ee32df500958`: before deadline 136 / ring 4; after deadline 145 / ring 15. Differences: deadline +9, ring +11. These are fleet-wide counters and cannot all be attributed to these chats.

## M225 queue pickup

`flyctl logs --app agentsfleetd-dev --no-tail` returned the newest 100 lines after the lease records had left its buffer. Loki recovered the ten original daemon `runner_lease_issued` records using structured metadata filters `event` and `fleet_id`.

The message body is only `a lease was issued`, so body-text filters for the event name returned nothing.

`daemon-lease-lines-loki.json` preserves the records.

This is a different retrieval source from the requested Fly witness.

| Event | Daemon log queue_wait_ms |
|---|---:|
| `01a1a430-310c-7805-ad76-90af7a262e49` | 260 |
| `01a1a430-8c2c-7571-8389-08fa4a0350ec` | 253 |
| `01a1a430-92fc-78ad-919b-29b02af110c3` | 272 |
| `01a1a430-9ea3-720c-83c3-fd268ac0cc20` | 215 |
| `01a1a430-a54b-7544-807b-707ffdf129ec` | 180 |
| `01a1a430-ac12-751e-965c-6b608acf2d41` | 261 |
| `01a1a430-b2c0-75f4-8da5-2124b1fdb942` | 232 |
| `01a1a430-bf89-7468-b786-a8a24b18f961` | 196 |
| `01a1a430-c649-7521-91fa-ad5c38406a4d` | 213 |
| `01a1a430-cd5a-7dc3-8752-58c9e18ecd80` | 265 |

p50 242.5 ms ≤250 ms; max 272 ms ≤1,000 ms. Both numerical limits pass. Delivery-span queue values differ from the later lease-issue log readings and must not replace these ten values.

## Local verification

`observability_verify_test.sh`: 3 passed, 0 failed, including acceptance of saved
variable values and reordered objects, and rejection of changed queries,
reducers or variable definitions. `shellcheck --severity=error -x` passed for
all four changed observability scripts.

`make check-version`: all versions match `0.73.0`. `orly gate work
--accept-dirty` passed. Gitleaks found no leaks in the staged changes or the
50 KB evidence directory.

`make check-playbooks` fails in the new source-assets regression because the
existing gate expects five alerts while `origin/main` has four. The alert set
is unchanged here.

Correcting `EXPECTED_ALERTS` in `01_assets_check.sh` awaits
owner approval under the repository's gate-edit rule. A separate collector
patch and local conversion proof exist outside this branch; deployment-config
edits also await owner approval. No pull request or live apply is claimed.

Review: gstack and repository standards review found zero remaining actionable
issues after the reducer labels and variable-state drift repairs. Review receipt
`4c125fe4-e860-439a-83ad-8e9214f0ef1b` remains incomplete for the manual witnesses
and alert-count blocker. Native adversarial receipt
`74460351-c444-4c32-8e72-95fba3a9f355` completed cleanly.

The full `orly gate verify` passed documentation checks, source-rule tests,
Rust formatting and Clippy, website lint and the version check. It stopped at
app lint because the configured `AGENTSFLEET_UI_ENV_FILE` path did not exist.

The narrow rerun `AGENTSFLEET_UI_ENV_FILE=/tmp/m238-ui-runtime.env make lint-app`
passed, using an empty task-owned file and the already supplied runtime
variables. The original environment file selector was not modified. Full
verification and publication remain blocked by the alert-count decision.

## Owner approval — Nov 04, 2026

Indy: "1. Okay go" and "2. Yes go" approved the alert-count correction and the
separate dev/prod collector conversion fix.

The count is corrected to four
without changing the alert set.

The collector repair ships in its own pull
request; merging remains Indy's.

After the approved count correction, the source-ownership check exposed two
stale `agentsfleet_repair_dispatch_*` targets with no census entry or producer.
Those targets were removed; their panels retain their supported queries.
Asset checks now pass, including 9 positive/negative cases; drift checks pass
3 cases, including saved variable selections.
