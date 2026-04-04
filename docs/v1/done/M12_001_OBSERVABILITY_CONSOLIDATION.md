# M12_001: Observability Consolidation — Done

**Milestone:** M12
**Completed:** Mar 22, 2026

## Summary

Consolidated observability from 3 tools (Grafana + PostHog + Langfuse) to 2 tools (Grafana + PostHog).
Completed Grafana 3-signal story: metrics, logs, and traces all export via OTLP/HTTP.

Verified against GitHub PR `#73` (`feat(m12): OTLP trace export + docs — M12_001 WS3+WS4`), merged into `main` on Mar 22, 2026 at 10:27:56 UTC with merge commit `5cb2c22a4055cbd9b8fd463ce6524facbaf5f1af`.

## Workstreams

| WS | Description | Status |
|---|---|---|
| WS1 | Remove Langfuse (~300 lines removed) | DONE |
| WS2 | OTLP Log Exporter to Grafana Loki | DONE |
| WS3 | OTLP Trace Propagation + Export | DONE |
| WS4 | Docs + Config Cleanup | DONE |

## Key Changes

### WS1 + WS2

- Removed Langfuse async exporter, circuit breaker, and all configuration
- Added OTLP log export to Grafana Loki
- Updated observability docs and signal ownership

### WS3

- Added `src/observability/otel_traces.zig`
- Wired HTTP trace propagation and root-trace generation
- Added agent spans with tokens and duration metadata
- Reused shared Grafana OTLP auth config

### WS4

- Updated `docs/observability/SIGNAL_CONTRACT.md`
- Updated `docs/OBSERVABILITY.md`
- Updated `playbooks/M2_002_PRIMING_INFRA.md`
- Updated procurement/vault readiness checks
- Updated deploy configuration

## Acceptance

- [x] Langfuse removed from active observability architecture
- [x] Grafana metrics, logs, and traces all exported
- [x] PostHog retained as the product analytics surface
- [x] Active docs and config updated to the 2-tool model
