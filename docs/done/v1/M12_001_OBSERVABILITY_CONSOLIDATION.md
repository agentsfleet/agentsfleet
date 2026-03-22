# M12_001: Observability Consolidation — Done

**Milestone:** M12
**Completed:** Mar 22, 2026

## Summary

Consolidated observability from 3 tools (Grafana + PostHog + Langfuse) to 2 tools (Grafana + PostHog).
Completed Grafana 3-signal story: metrics, logs, and traces all export via OTLP/HTTP.

## Workstreams

| WS | Description | Status |
|---|---|---|
| WS1 | Remove Langfuse (~300 lines removed) | DONE |
| WS2 | OTLP Log Exporter to Grafana Loki (341 lines new) | DONE |
| WS3 | OTLP Trace Propagation + Export (~200 lines across 10+ files) | DONE |
| WS4 | Docs + Config Cleanup | DONE |

## Key Changes

### WS1 + WS2 (PR #71)
- Removed Langfuse async exporter, circuit breaker, and all configuration
- New `src/observability/otel_logs.zig` — ring buffer + batch OTLP log export to Grafana Loki
- Net: -300 lines Langfuse, +341 lines OTLP logs

### WS3 (this PR)
- New `src/observability/otel_traces.zig` — ring buffer span exporter to Grafana Tempo via OTLP/HTTP
- `http/server.zig` dispatch: parse `traceparent` header, generate root trace, emit `http.request` spans
- `pipeline/worker_stage_executor.zig`: emit `agent.call` spans with `agent.actor`, `agent.tokens`, `agent.duration_ms`
- `cmd/preflight.zig`: `initOtelTraces`/`deinitOtelTraces` reusing `GRAFANA_OTLP_*` config
- Wired into serve.zig and worker.zig startup

### WS4 (this PR)
- `docs/observability/SIGNAL_CONTRACT.md` → version 2.0 with trace exporter contract and error codes
- `docs/OBSERVABILITY.md` → 3-signal Grafana architecture table
- `playbooks/M2_002_PRIMING_INFRA.md` → Grafana OTLP secrets in Fly config
- `playbooks/gates/m2_001/section-2-procurement-readiness.sh` → Grafana OTLP vault checks
- `.github/workflows/deploy-dev.yml` → Grafana OTLP env vars in 1Password load + flyctl secrets

## Env Vars (Final)

```
POSTHOG_API_KEY              → posthog-zig SDK
GRAFANA_OTLP_ENDPOINT        → Grafana Cloud gateway (logs + traces)
GRAFANA_OTLP_INSTANCE_ID     → Grafana Cloud instance
GRAFANA_OTLP_API_KEY         → Grafana Cloud API key
OTEL_SERVICE_NAME            → service identifier (default: zombied)
```

## Spec Reference

`docs/spec/v1/M12_001_OBSERVABILITY_CONSOLIDATION.md`
