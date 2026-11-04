# M238_002: Grafana accepts the daemon's delta metrics

**Status:** DONE
**Priority:** P1
**Branch:** fix/m238-delta-metrics-export
**Baseline revision:** 11883fdaba2197adf8931d334396ba8b3bb9eb5b

## Overview

Dev's collector forwards delta histograms and sums to Grafana unchanged. During
the ten-chat M238 measurement, Grafana returned HTTP 400 and rejected delivery,
lease-history, invocation and billing metrics. Convert delta metrics at the
collector boundary, keeping producer names, labels and measurements intact.

## PR Intent & comprehension handshake

Add the existing OpenTelemetry Collector `delta_to_cumulative` processor to both
environments' metrics pipelines. Assumptions: conversion happens after memory
admission and before batching; traces and logs retain their current pipelines;
the daemon keeps its delta producers. No live deployment or merge is requested.

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `deploy/fly/otelcol-dev/config.yml` | EDIT | Convert metrics before Grafana ingest |
| `deploy/fly/otelcol-prod/config.yml` | EDIT | Apply the same conversion boundary |
| `scripts/collector_temporality_test.py` | CREATE | Exercise the declared pipelines with real collector input/output |
| `make/quality.mk` | EDIT | Provide the focused collector verification command |
| `make/check-safety-gates.mk` | EDIT | Run conversion proofs in the existing deployment-safety lane |
| `scripts/fixtures/collector-temporality.Dockerfile` | CREATE | Supply a portable isolated test runtime |
| `playbooks/operations/observability/001_playbook.md` | EDIT | Explain conversion, state bounds and verification |

## Applicable Rules

Repository instructions; Python, tests, documentation and pull-request standards.

## Applicable Gates

`orly doctor`, `orly gate work`, `orly gate verify`, `orly gate pr` and the focused
collector test. Full verification receipts belong in pull-request Session Notes.

## Prior-Art / Reference Implementations

Use contrib's existing `delta_to_cumulative` processor, verified with the running
collector release 0.164.0. No conversion logic is implemented in the daemon.

## Sections

### §1 — Convert at the existing collector hop

- **Dimension 1.1** — DONE — Delta histogram and monotonic sum windows accumulate from 2 to 5, with original labels and names preserved → `test_delta_windows_accumulate` (integration).
- **Dimension 1.2** — DONE — An already cumulative counter stays 7 then 9; a gauge stays 11 then 13 → `test_cumulative_and_gauge_are_unchanged` (integration).
- **Dimension 1.3** — DONE — The same proofs hold for development and production configurations → both tests execute both declared pipelines.

## Interfaces

No API changes. The Grafana-facing metrics change from delta to cumulative;
producer temporality remains unchanged.

## Failure Modes

A collector restart loses conversion state and starts a new cumulative series
window. The processor refuses additional streams beyond its cap; existing
memory admission remains first. An exporter failure retains the existing queue
and retry behavior. This change does not make the queue durable.

## Invariants

Only metrics use conversion. Names, attributes and existing cumulative readings
are preserved. The conversion retains at most 10,000 streams and expires idle
state after five minutes.

## Metrics & Observability

No new metric families. The intended result is that Grafana accepts existing
delta families instead of rejecting the export request.

## Test Specification

The integration test runs Collector 0.164.0 with the actual declared processors
and metrics pipeline, replacing transport credentials with loopback fixtures.
It asserts structured OpenTelemetry output over two windows, including exact
names, attributes, counts, sums, buckets, cumulative readings and gauges.

## Acceptance Rubric

| ID | Criterion | Verify | Expected | Priority | Graded |
|---|---|---|---|---|---|
| R1 | Both collector configurations convert delta windows and preserve cumulative metrics | `make check-collector-temporality COLLECTOR_BINARY=/tmp/otelcol-contrib-0.164.0` | 4 tests pass across both environments | P0 | ✅ `Ran 4 tests; OK` |
| R2 | Deployment configurations remain valid | `make check-collector-temporality COLLECTOR_BINARY=/tmp/otelcol-contrib-0.164.0` | exit 0 for both | P0 | ✅ both `validate` invocations exit 0 |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version | `make check-version` | exit 0 | P0 | |
| S5 | Secrets | `gitleaks detect --redact --no-banner` | no leaks | P0 | |

## Dead Code Sweep

Remove comments claiming every signal is forwarded unchanged; metrics now
retain conversion state at this boundary.

## Out of Scope

Dashboard assets, Tempo metrics-generator configuration and live deployments.

## Product Clarity

An operator can read the delivery histogram after deployment. A successful
local collector test does not prove Grafana ingestion after deployment.

## Decomposition & alternatives

Convert at the collector rather than changing the daemon's delta metric
semantics. The existing contrib processor handles sums and histograms together.

## Discovery

Regression proof: the test against `origin/main` collector configurations fails four delta-window assertions; the corrected configurations pass all three tests across both environments. Existing cumulative values and gauges are preserved, and malformed input returns HTTP 400 / OpenTelemetry error code 3 without reaching the exporter. Both full collector configurations pass Collector 0.164.0 `validate` with fixture credentials.

Indy approved deployment-config edits on Nov 04, 2026: "2. Yes go". The original
request requires a real defect to receive its own fix pull request through
`orly-build` and `orly-ship`. Merging remains Indy's.

The canonical `delta_to_cumulative` name is used; Collector 0.164.0 warns that the older `deltatocumulative` alias is deprecated. The final parent-config run reports 4 tests with four failing delta-window subtests; the corrected configuration reports 4 tests passing.

The existing deployment-safety lane invokes the collector test automatically. Its default Docker fixture has no network access at runtime; a local collector binary can be selected for development. The test uses the collector itself to parse YAML, without another parser dependency.
