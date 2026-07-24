<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M139_004: Grafana receives one pinned telemetry schema

**Prototype:** v2.0.0
**Milestone:** M139
**Workstream:** 004
**Date:** Jul 23, 2026
**Status:** PENDING
**Priority:** P1 — operator queries currently depend on ambiguous private names, incorrect units, and unbounded identity attributes
**Categories:** Observability (OBS)
**Batch:** B3 — follows bounded exporter reliability so schema migration cannot hide transport loss
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) via `make _lint_zig_test_depth`
**Depends on:** M139_003 — exporter drain, deadline, loss accounting, and current-label restoration must be DONE first
**Provenance:** Large Language Model (LLM)-drafted (Codex, Jul 23, 2026) from Indy's semantic-conventions decision and pinned OpenTelemetry sources
**Canonical architecture:** `docs/architecture/observability.md` §Metrics: off-Postgres dashboards and §Traces stay on the process that creates spans

---

## Overview

**Goal (testable):** Every `agentsfleetd` OpenTelemetry payload and Grafana query agrees on one pinned resource, metric, unit, and attribute schema; no workspace identity reaches metrics; model attribution is bounded and visible when omitted; and no superseded metric name remains live.
**Problem:** The three OpenTelemetry Protocol (OTLP) serializers repeat a resource containing only `service.name`. Run metrics use private keys such as `model`, `workspace`, `posture`, and `direction`; `agentsfleet.credit.drained_nanos` declares the time unit `ns` for a billing quantity; `agentsfleet.run.duration_ms` embeds its unit in its name; token totals represent a whole agent run rather than one provider call; and the Grafana dashboard repeats these private names. The credit series also misses successful renewal debits, so its current name overstates its coverage.
**Solution summary:** Add one source-owned semantic registry pinned to OpenTelemetry core semantic conventions `v1.43.0` and Generative Artificial Intelligence (GenAI) conventions commit `2e994c6d59a93bb4fc1752c5378eedb9b8e14d6b`. Emit standard resource identity and standard attributes wherever the source fact matches. Use `gen_ai.invoke_agent.duration` for the runner-reported single agent invocation, but keep aggregate run tokens and billing under `agentsfleet.*` because they are not individual GenAI client calls. Replace workspace attribution with durable Postgres queries, bound exact model attribution in process, expose omissions, and cut exporter constants, fixtures, audits, architecture, and Grafana over together. The eventual Pull Request (PR) emits only the new delta series; it never emits old and new names in parallel.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(observability): align Grafana telemetry semantics
- **Intent (one sentence):** Operators can read interoperable Grafana telemetry without mistaking product accounting for provider-call data or creating unbounded tenant series.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/observability.md` — signal ownership, queue bounds, current metric labels, and the no-runner-exporter decision.
2. `src/agentsfleetd/observability/otel_metrics_payload.zig`, `otel_metrics.zig`, and `otel_metrics_cardinality.zig` — current wire descriptors, record boundary, aggregation ceiling, and restored workspace guard.
3. `src/agentsfleetd/fleet/service_billing.zig`, `service_renew.zig`, and `service_report.zig` — every committed receive, renewal, and terminal debit plus the cumulative whole-run token facts.
4. [OpenTelemetry naming and resource conventions](https://opentelemetry.io/docs/specs/semconv/general/naming/) — standard namespace, unit, resource, and system-specific naming rules.
5. [Pinned GenAI metric conventions](https://github.com/open-telemetry/semantic-conventions-genai/blob/2e994c6d59a93bb4fc1752c5378eedb9b8e14d6b/docs/gen-ai/gen-ai-metrics.md) — exact agent-duration and token-usage semantics at the selected upstream commit.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/observability/semconv.zig` | CREATE | Own pinned metric, resource, attribute, unit, and provider-normalization constants. |
| `src/agentsfleetd/observability/semconv_test.zig` | CREATE | Prove the registry matches pinned sources and rejects false standard aliases. |
| `src/agentsfleetd/observability/otlp/config.zig` | EDIT | Carry bounded standard resource attributes with existing OTLP configuration ownership. |
| `src/agentsfleetd/observability/otel_logs.zig` | EDIT | Serialize the shared resource and instrumentation scope schema. |
| `src/agentsfleetd/observability/otel_logs_test.zig` | EDIT | Prove resource precedence, escaping, and malformed optional identity handling. |
| `src/agentsfleetd/observability/otel_traces.zig` | EDIT | Serialize shared resources, typed attributes, and correct span kinds. |
| `src/agentsfleetd/observability/otel_traces_test.zig` | EDIT | Prove standard HTTP and GenAI attribute payloads without sensitive content. |
| `src/agentsfleetd/observability/otel_metrics_payload.zig` | EDIT | Replace metric descriptors, seconds buckets, and private label keys. |
| `src/agentsfleetd/observability/otel_metrics.zig` | EDIT | Record exact run observations and committed credit deltas under the new schema. |
| `src/agentsfleetd/observability/otel_metrics_test.zig` | EDIT | Prove names, units, token accounting, omission, replay, and no-op behavior. |
| `src/agentsfleetd/observability/otel_metrics_aggregate_test.zig` | EDIT | Prove standard attribute sets stay distinct and below the series ceiling. |
| `src/agentsfleetd/observability/otel_metrics_cardinality.zig` | EDIT | Replace workspace tracking with a derived provider/model attribution budget. |
| `src/agentsfleetd/observability/metrics_otel.zig` | EDIT | Count fixed attribute-omission reasons without caller-provided labels. |
| `src/agentsfleetd/observability/metrics_otel_test.zig` | EDIT | Prove exact omission accounting and concurrency. |
| `src/agentsfleetd/observability/metrics_render.zig` | EDIT | Render the fixed attribute-omission family for Prometheus. |
| `src/agentsfleetd/fleet/service_billing.zig` | EDIT | Label committed receive debits with their fixed charge class. |
| `src/agentsfleetd/fleet/service_renew.zig` | EDIT | Emit every successful renewal debit after its money write commits. |
| `src/agentsfleetd/fleet/renewal.zig` | EDIT | Return the committed renewal debit needed by service orchestration. |
| `src/agentsfleetd/fleet/service_report.zig` | EDIT | Pass provider, model, outcome, split tokens, and final committed debit once. |
| `src/agentsfleetd/fleet/service_token_splits_wire_test.zig` | EDIT | Reconcile emitted token and credit observations with renewal/report money rows. |
| `src/agentsfleetd/fleet_runtime/metering.zig` | EDIT | Replace private delivery-span keys with standard GenAI and namespaced product keys. |
| `src/agentsfleetd/http/server.zig` | EDIT | Emit standard HTTP server span names, kind, and typed attributes. |
| `src/agentsfleetd/http/route_trace_test.zig` | EDIT | Prove the standard HTTP span shape and raw-URL exclusion. |
| `src/agentsfleetd/tests.zig` | EDIT | Register the semantic registry tests. |
| `tests/fixtures/telemetry/otlp_metrics.json` | EDIT | Pin the complete new OTLP metric payload. |
| `deploy/grafana/agent-observability.json` | EDIT | Query normalized semantic names, seconds, histogram sums, and omission coverage. |
| `deploy/grafana/agent_run_breakdown.json` | EDIT | Repoint four `agent_*` Prometheus queries that no source family satisfies, so no dashboard is left querying a dead name after the cutover. |
| `src/agentsfleetd/observability/semantic_schema_test.zig` | CREATE | Reject schema drift, stale names, workspace metrics, and dashboard mismatch as a real test. The former `audits/signal-routing.sh` was retired for pinning prose and source literals by exact string match; assert the emitted attribute set and its Grafana mapping instead of grepping for text. |
| `docs/architecture/observability.md` | EDIT | Reconcile cardinality policy, resource identity, exact metric boundaries, and Grafana flow. |
| `docs/architecture/runner_fleet.md` | EDIT | Update delivery-span correlation keys without inventing runner spans. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — No Dead Code (RULE NDC), No Legacy Retained (RULE NLR), No Legacy Compatibility Shims (RULE NLG), String Literals Are Always Constants (RULE UFS), Escape Control Characters in JSON (RULE ESC), Cross-Layer Orphan Sweep (RULE ORP), Test Fixtures Use Production Constants (RULE TFX), and Ground in Source Truth (RULE GRD).
- **`dispatch/write_zig.md`** — fixed ownership, typed results, public-surface shape, file length, and both Linux target builds apply.
- **`dispatch/write_any.md`** — logging, error registry, source length, and milestone-free test naming remain active.
- **`dispatch/name_architecture.md`** — the metric cardinality and signal-routing decisions land with the implementation.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| Zig (ZIG) GATE | yes | run formatting, lint, unit tests, and both Linux target builds |
| Public Surface (PUB) / Struct-Shape | yes | declare file shape before adding the semantic registry and resource value type |
| File & Function Length (≤350/≤50/≤70) | yes | keep registry, serialization, aggregation, and orchestration responsibilities split |
| String Literals Are Always Constants (UFS) | yes | one registry owns every live wire name, unit, fixed value, and dashboard assertion |
| User Interface (UI) Substitution / Design Token | no | Grafana JSON changes no TypeScript or design-system surface |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | lifecycle only | preserve owned config slices and exporter teardown; add no error or SQL schema |

## Prior-Art / Reference Implementations

- **OpenTelemetry core `v1.43.0`:** [naming](https://opentelemetry.io/docs/specs/semconv/general/naming/), [metrics](https://opentelemetry.io/docs/specs/semconv/general/metrics/), and [resources](https://opentelemetry.io/docs/specs/semconv/resource/) — use standard names and Unified Code for Units of Measure (UCUM), with `agentsfleet.*` reserved for product-specific facts.
- **OpenTelemetry GenAI commit `2e994c6d59a93bb4fc1752c5378eedb9b8e14d6b`:** [metrics](https://github.com/open-telemetry/semantic-conventions-genai/blob/2e994c6d59a93bb4fc1752c5378eedb9b8e14d6b/docs/gen-ai/gen-ai-metrics.md) and [agent spans](https://github.com/open-telemetry/semantic-conventions-genai/blob/2e994c6d59a93bb4fc1752c5378eedb9b8e14d6b/docs/gen-ai/gen-ai-agent-spans.md) — Development-status source pinned by commit, not followed from `main`.
- **Existing exporter and dashboard:** `otel_metrics_payload.zig` plus `agent-observability.json` — preserve bounded delta aggregation and replace its schema atomically.

## Sections (implementation slices)

### §1 — One pinned resource and semantic registry

Create one registry for standard and product-specific names. Every signal emits `service.name`, `service.namespace=agentsfleet`, `service.version`, and an optional `service.instance.id` only when a trusted instance Identifier (ID) is available. Instrumentation scope carries the build version. Emit the core `https://opentelemetry.io/schemas/1.43.0` schema URL; do not invent a GenAI schema URL while the pinned repository publishes none.

- **Dimension 1.1** — every live name and unit is classified against the pinned source, while false aliases such as client-token usage for aggregate run totals are rejected → Test `test_semantic_registry_matches_pinned_sources`
- **Dimension 1.2** — logs, traces, and metrics serialize identical standard resource identity, valid JSON escaping, and no fabricated optional instance → Test `test_otlp_resources_share_semantic_identity`

### §2 — Metrics state exactly what was measured

Use `gen_ai.invoke_agent.duration` in seconds with the pinned agent-duration histogram boundaries because `ReportTelemetry.wall_ms` bounds one sandboxed NullClaw agent invocation. Aggregate run tokens use `agentsfleet.invoke_agent.token.usage` because the report is cumulative across all provider calls in that invocation. Record input as regular plus cached input, output separately under `gen_ai.token.type`, and keep cached input as the non-additive subset `agentsfleet.invoke_agent.cache_read.token.usage`. Credit uses `agentsfleet.billing.credit.consumed` with unit `{nanocredit}` and fixed `agentsfleet.billing.charge.type`.

Provider values normalize only to exact OpenTelemetry well-known names; unknown providers omit the attribute. Exact `gen_ai.request.model` attribution is admitted only while the derived series budget can preserve the 256-series ceiling; overflow omits the attribute instead of fabricating a standard value. Both cases increment `agentsfleet_otel_attribute_omitted_total` under a fixed attribute key. Workspace never enters an OTLP metric.

- **Dimension 2.1** — metric descriptors, instrument kinds, units, histogram boundaries, and allowed attributes match the table above → Test `test_metric_descriptors_match_semantic_schema`
- **Dimension 2.2** — input includes cached tokens once, output is separate, cached usage remains a subset, and zero values do not create misleading directions → Test `test_invoke_agent_token_usage_never_double_counts_cache`
- **Dimension 2.3** — provider/model attribution stays inside the computed series budget; overflow preserves the sample without the attribute and counts the omission exactly → Test `test_metric_attribute_cardinality_is_bounded_and_visible`
- **Dimension 2.4** — receive, every successful renewal, and final settlement emit their committed credit delta once; stale, failed, or replayed writes emit none → Test `test_credit_metric_reconciles_committed_debits`

### §3 — Spans use standard keys without claiming a runner trace

HTTP ingress spans use server kind, route-based names, `http.request.method`, `http.route`, and typed `http.response.status_code`; query strings, bodies, authorization, and raw caller addresses remain absent. The settled `fleet.delivery` span remains a custom control-plane observation because no runner span or trace context exists. Its attributes become `gen_ai.operation.name=invoke_agent`, `gen_ai.agent.id`, `gen_ai.provider.name`, `gen_ai.request.model`, typed `gen_ai.usage.*` counts, and namespaced `agentsfleet.*` correlation keys.

- **Dimension 3.1** — matched HTTP requests serialize the standard server span shape while unmatched, malformed, and sensitive request data never become attributes → Test `test_http_server_span_uses_standard_semantics`
- **Dimension 3.2** — one accepted report emits one custom delivery span with exact GenAI usage and product correlation keys; replay emits none and no prompt or response content appears → Test `test_delivery_span_uses_semantic_attributes_without_runner_claim`

### §4 — Grafana and drift checks cut over together

Update dashboard queries for Prometheus-normalized dotted names and attribute keys. Token throughput reads the histogram `_sum`; run latency reads seconds buckets; cached input is presented as a subset; credit panels state nanocredits; and model-attribution coverage is visible beside the model selector. Assert this in the semantic-schema test rather than a new make target — and do not reintroduce a prose-pinning shell audit.

The `/metrics` exposition carries two prefixes today: `metrics_render.zig` emits about twenty `fleet_*` families (API in-flight, Server-Sent Events (SSE), Redis pool, signup funnel, worker liveness, trigger totals) while `metrics_runner.zig`, `metrics_otel.zig`, and `metrics_trace.zig` emit `agentsfleet_*`. One process must expose one namespace. Normalize every family to the `agentsfleet_` prefix in the same atomic cutover as the OTLP schema, since both change the names an operator queries and neither has an external consumer before `2.0.0`. No dual emission and no recording-rule alias.

- **Dimension 4.1** — every dashboard query resolves a live descriptor, uses the correct histogram suffix and normalized attribute key, and exposes attribution omissions → Test `test_grafana_queries_match_semantic_metrics`
- **Dimension 4.2** — source, fixture, audit, architecture, and dashboard contain no superseded live name or private metric key after the atomic cutover → Test `test_semantic_schema_has_no_live_legacy_aliases`
- **Dimension 4.3** — every Prometheus family rendered by the daemon shares the `agentsfleet_` prefix, and no `fleet_`-prefixed family remains live in source, audits, or the dashboard → Test `test_prometheus_families_share_one_namespace`

## Interfaces

```text
resource = service.name + service.namespace + service.version + optional service.instance.id

gen_ai.invoke_agent.duration
  histogram, unit s
  attributes: gen_ai.request.model?, error.type?, agentsfleet.execution.posture

agentsfleet.invoke_agent.token.usage
  histogram, unit {token}
  attributes: gen_ai.operation.name, gen_ai.provider.name?, gen_ai.request.model?,
              gen_ai.token.type, agentsfleet.execution.posture

agentsfleet.invoke_agent.cache_read.token.usage
  histogram, unit {token}; subset of input, never an additive third total

agentsfleet.billing.credit.consumed
  monotonic delta sum, unit {nanocredit}
  attributes: agentsfleet.billing.charge.type, gen_ai.provider.name?,
              gen_ai.request.model?, agentsfleet.execution.posture
```

No public API path, request body, response body, Command-Line Interface (CLI), or User Interface (UI) behavior changes.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Upstream convention moves | GenAI Development names change after the pinned commit | Live output remains on the pinned schema until a reviewed spec updates source, fixture, dashboard, and architecture together. |
| False standard metric | aggregate run fact is labeled as one provider call | Registry rejects the alias; unit test fails before export. |
| Unknown provider | configured provider has no exact well-known mapping | Omit `gen_ai.provider.name`, preserve the sample, and increment the fixed omission counter. |
| Model budget exhausted | a new provider/model pair would exceed the derived series budget | Omit `gen_ai.request.model`, preserve the sample, and increment the fixed omission counter. |
| Attribute too long | provider or model exceeds the fixed payload value bound | Omit rather than truncate, count the omission, and preserve the measurement. |
| Cached input counted twice | regular and cached counters are treated as three additive token totals | Input observation equals regular plus cached; cache detail remains a documented subset. |
| Debit emission drift | renewal commits money but no metric, or a replay emits twice | Reconciliation test compares accepted debit results with emitted deltas and fails. |
| Mixed Grafana schema | dashboard queries old and new delta series together | Audit rejects every superseded live name; no compatibility alias ships. |
| Exporter unavailable | OTLP config is absent or backend is dark | Existing non-blocking no-op and M139_003 loss behavior remain; money and reports succeed. |

## Invariants

1. A standard metric name is emitted only when the measured boundary matches the pinned definition; source audits forbid `gen_ai.client.token.usage` for whole-run totals.
2. `workspace`, tenant, fleet, event, lease, runner, prompt, response, credential, and raw error text never enter metric attributes.
3. Provider/model metric attribution has a computed finite ceiling and visible omission; no truncation or invented standard value is allowed.
4. Input token usage equals regular input plus cached input, cached usage is a subset, and output remains separate.
5. Every committed credit debit emits once after commit; an uncommitted, stale, or replayed operation emits zero.
6. All three OTLP signals share one resource serializer and one pinned core schema URL.
7. Superseded names have zero non-historical live hits; no dual emission, compatibility alias, or dashboard union exists.
8. Export remains non-blocking and best effort; Postgres stays authoritative for workspace-level billing and exact money.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `gen_ai.invoke_agent.duration` | ops | accepted terminal report | model when admitted, fixed posture, low-cardinality error type | no workspace, content, or request identity | `test_metric_descriptors_match_semantic_schema` |
| `agentsfleet.invoke_agent.token.usage` | ops | accepted terminal report with known input or output | fixed operation and token type; provider/model when admitted; fixed posture | whole-run aggregate only; no prompt or response | `test_invoke_agent_token_usage_never_double_counts_cache` |
| `agentsfleet.invoke_agent.cache_read.token.usage` | ops | accepted terminal report with cached input | provider/model when admitted; fixed posture | subset is never summed as a third token direction | `test_invoke_agent_token_usage_never_double_counts_cache` |
| `agentsfleet.billing.credit.consumed` | ops | committed receive, renewal, or final debit | fixed charge class and posture; provider/model when admitted | no tenant/workspace identity; Postgres is exact money truth | `test_credit_metric_reconciles_committed_debits` |
| `agentsfleet_otel_attribute_omitted_total` | ops | provider/model attribute cannot be represented safely | fixed attribute key and reason | no rejected value is exported or logged | `test_metric_attribute_cardinality_is_bounded_and_visible` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_semantic_registry_matches_pinned_sources` | pinned descriptors resolve; false client-token alias and fabricated GenAI schema URL do not. |
| 1.2 | unit | `test_otlp_resources_share_semantic_identity` | log, trace, and metric payloads carry equal service identity; absent instance remains absent; hostile strings stay valid JSON. |
| 2.1 | unit | `test_metric_descriptors_match_semantic_schema` | exact names, kinds, units, seconds bounds, and attribute allowlists serialize. |
| 2.2 | unit | `test_invoke_agent_token_usage_never_double_counts_cache` | regular 80, cached 20, output 30 yields input 100, cache subset 20, output 30, never 130 input. |
| 2.3 | unit | `test_metric_attribute_cardinality_is_bounded_and_visible` | concurrent exact pairs fill the derived cap; next pair loses only model attribution and increments one omission. |
| 2.4 | integration | `test_credit_metric_reconciles_committed_debits` | receive plus two renewals plus final settle equals emitted credit; replay, lost fence, and database failure add zero. |
| 3.1 | integration | `test_http_server_span_uses_standard_semantics` | matched request emits server kind and typed route/status; query, authorization, body, and unmatched path are absent. |
| 3.2 | integration | `test_delivery_span_uses_semantic_attributes_without_runner_claim` | accepted report emits exact split usage and namespaced correlations once; replay and content fields emit nothing. |
| 4.1 | unit | `test_grafana_queries_match_semantic_metrics` | every expression uses normalized live names, histogram suffixes, seconds, and the omission family. |
| 4.2 | integration | `test_semantic_schema_has_no_live_legacy_aliases` | source, fixture, audit, architecture, and dashboard have zero live superseded schema hits. |
| 4.3 | unit | `test_prometheus_families_share_one_namespace` | rendering the full `/metrics` body yields zero `fleet_`-prefixed family names; every `# TYPE` line starts `agentsfleet_`. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Three OTLP signals share one standard resource (§1) | `make test-unit-agentsfleetd` | resource identity test passes | P0 | |
| R2 | Metric names, units, and token math match the pinned schema (§2) | `make test-unit-agentsfleetd` | descriptor and token tests pass | P0 | |
| R3 | Credit metrics reconcile every committed debit (§2) | `make test-integration` | credit reconciliation test passes | P0 | |
| R4 | Metric attribution is bounded and omissions are visible (§2) | `make test-unit-agentsfleetd` | cardinality and omission tests pass | P0 | |
| R5 | HTTP and delivery spans use truthful semantic keys (§3) | `make test-integration` | both span integration tests pass | P0 | |
| R6 | Grafana, fixture, source, and architecture have one schema (§4) | `zig build test-lib` (semantic-schema test) | pass | P0 | |
| S1 | Repository conformance passes | `make harness-verify` | exit 0 | P0 | |
| S2 | Repository unit suites pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Both Linux targets build | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | both exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | |
| S5 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

No files are deleted. Grep the four superseded OTLP metric names, private metric keys, old dashboard label selectors, and obsolete resource serializers across non-historical source, fixtures, audits, architecture, and Grafana; every live hit must be replaced in the same commit. Include the already-dead `agent_*` families in `agent_run_breakdown.json` — every Prometheus expression across BOTH dashboards must resolve to a live descriptor, so a query naming no emitted family fails the sweep exactly like a superseded one.

## Out of Scope

- A runner exporter, runner backend credential, runner span producer, or trace-context field in the runner API.
- `gen_ai.client.token.usage`, inference-call counts, or tool-call counts until the runner reports individual provider-call boundaries.
- Workspace-level metric attribution; exact workspace billing remains a Postgres query.
- Parallel old/new emission, compatibility aliases, recording rules, or dashboard unions.
- Provisioning a collector, changing Grafana credentials, or deploying the dashboard.
- Public API, CLI, or UI changes and PostHog event-key migration.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator opens Grafana and sees model-attributed token, duration, and credit data whose names and units match the schema documented in the payload.
2. **Preserved user behaviour** — fleet execution, billing, reports, public APIs, commands, and the nullable exporter keep their current behavior.
3. **Optimal-way check** — one atomic schema registry is the direct path; renaming only the dashboard would preserve ambiguity and drift.
4. **Rebuild-vs-iterate** — refactor the schema boundary while retaining the bounded exporter and report-derived ownership.
5. **What we build** — one registry, shared resources, truthful metric descriptors, bounded attributes, semantic span keys, Grafana queries, fixtures, audits, and architecture.
6. **What we do NOT build** — runner telemetry infrastructure, provider-call transport, workspace metric labels, dual emission, or a new dashboard service.
7. **Fit with existing features** — compounds M139_003's bounded exporter, accepted-report fence, exact renewal metering, and Grafana dashboard without destabilizing money writes.
8. **Surface order** — operator telemetry first; there is no end-user command or interface surface.
9. **Dashboard restraint** — model panels expose attribution omissions and never imply workspace billing or cache tokens are additive.
10. **Confused-user next step** — inspect the dashboard schema description and omission counter, then use Postgres for exact workspace-level cost.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one cross-cutting workstream owns registry, payloads, emit sites, fixtures, audits, architecture, and Grafana because these are one wire schema and must switch atomically.
- **Alternatives considered:** a dashboard-only rename was rejected because source units and meanings would remain false; a generic OpenTelemetry Software Development Kit (SDK) dependency was rejected because the repository already has a bounded native Zig exporter and needs no second lifecycle owner.
- **Patch-vs-refactor verdict:** this is a **refactor** because scattered private descriptors become one source-owned semantic schema while transport and product behavior stay intact.

## Discovery (consult log)

- **Consults** — `docs/architecture/observability.md` currently removes both model and workspace; Indy accepted a separate semantic-conventions workstream that removes workspace while retaining bounded exact model attribution under `gen_ai.request.model`. Architecture lands with implementation.
- **Source finding** — `ReportRequest` carries cumulative whole-run token counts, so `gen_ai.client.token.usage` would falsely claim one provider call; a product-specific invoke-agent metric is required.
- **Source finding** — `ReportTelemetry.wall_ms` bounds one sandboxed NullClaw invocation, so `gen_ai.invoke_agent.duration` is an exact standard metric.
- **Source finding** — the credit exporter observes receive and final-settle debits but not successful renewal debits; semantic naming requires reconciliation with all committed debit results.
- **Source finding** — all three OTLP envelopes currently repeat only `service.name`; model is used by `agent-observability.json`, while workspace is absent from that dashboard and remains queryable in Postgres.
- **Source finding (added Jul 23, 2026 by Indy's direction during the M141 observability scan)** — `agent_run_breakdown.json` is already dead: its four Prometheus panels query `agent_agent_tokens_by_workspace_total`, `agent_runs_completed_by_workspace_total`, `agent_runs_blocked_by_workspace_total`, and `agent_workspace_metrics_overflow_total`, none of which any source family emits. The product rename moved those counters to `fleet_workspace_tokens_total`, `fleet_completed_total`, and `fleet_triggered_total` and the dashboard never followed; `playbooks/operations/observability/002_grafana_setup.md` still documents the dead names too. This is the same prefix-split defect as the finding above, so it lands in this atomic cutover rather than in the M141 lease-fan-out workstream that surfaced it.
- **Upstream finding** — the GenAI conventions are Development status and pin core `v1.43.0`; the selected commit publishes no GenAI schema URL, so none may be fabricated on the wire.
- **Metrics review** — this work renames and corrects operational telemetry only; no PostHog event or analytics funnel changes.
- **Source finding** — the `/metrics` exposition is split between `fleet_*` families in `metrics_render.zig` and `agentsfleet_*` families in `metrics_runner.zig`, `metrics_otel.zig`, and `metrics_trace.zig`. The bounded-signal-export workstream named its new suppression family `agentsfleet_http_trace_suppressed_total` to match its architecture document, which widened the split rather than creating it. Normalizing every family to one prefix belongs in this atomic schema cutover, not in exporter-bounding work.
- **Skill-chain outcomes** —
- **Deferrals** —
  > Indy (2026-07-23 09:27): "Yes create a workstream agree, and move to 139, so the workstream in pending is added in this PR" — context: this Pull Request adds the pending semantic-conventions specification; implementation begins only when M139_004 is opened.
