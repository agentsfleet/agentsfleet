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

# M159_001: Runtime metrics reach Grafana over the OpenTelemetry Protocol (OTLP) exporter, and the pull endpoint is retired

**Prototype:** v2.0.0
**Milestone:** M159
**Workstream:** 001
**Date:** Aug 11, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — operator dashboards and six alert rules are dark until runtime families reach the metric store
**Categories:** API, DOCS, INFRA, OBS
**Batch:** B1 — §1/§2 and §4 run concurrently; §3/§5/§6 follow §1
**Branch:** feat/m159-otlp-runtime-metrics
**Test Baseline:** unit=3512 integration=589
**Depends on:** none
**Provenance:** LLM-drafted (Claude Opus 5, Aug 11, 2026) from a `/plan-eng-review` against the live development environment
**Canonical architecture:** `docs/architecture/observability.md` §The four signal paths, §Metric family census

---

## Overview

**Goal (testable):** every metric family the Grafana dashboard and its alert rules query returns at least one live series from `grafanacloud-prom`, with no Prometheus pull endpoint remaining in the daemon.

**Problem:** operators have no runtime dashboard. The Grafana provisioning gate refuses to advance with `ERROR: Prometheus does not scrape agentsfleet_api_in_flight_requests`, so no folder, dashboard, or alert rule is installed for development or production. Runner saturation, Redis pool exhaustion, backpressure shedding, and lease-poll cost are all unobservable, and the development bootstrap cannot reach its production step.

**Solution summary:** `agentsfleetd` already pushes logs, traces, and a narrow generative-artificial-intelligence cost metric set to Grafana Cloud over one OTLP connection. This milestone widens that metrics signal to carry the runtime families the operator assets query, adds the gauge metric kind the exporter lacks, replaces a hand-picked series ceiling with a derived one so new families cannot silently evict cost attribution, and deletes the Prometheus pull endpoint together with its rendering layer. The daemon is private on Fly.io, so an outbound push is the only collection shape that works without new infrastructure; after this change there is exactly one production egress path for every signal.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(obs): carry runtime metrics over OTLP and retire the /metrics pull endpoint
- **Intent (one sentence):** operators get a live runtime dashboard and working alerts, delivered through the single telemetry connection the daemon already maintains.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.
- **Restatement (Orly, at PLAN):** the one telemetry pipe the daemon already holds starts carrying every metric family the daemon knows, the dashboards and alerts light up from the store, and the never-scraped pull endpoint disappears from code and deploy config alike. `ASSUMPTIONS I'M MAKING:` (1) Dimension 5.2 means the FULL renderer census (~35 families) moves onto the wire, not only the 16 the assets query; (2) fixed-label runtime families snapshot once per flush through the aggregator, while per-runner families stream straight from the pre-aggregated slot table (their ceiling term reuses `MAX_SLOTS` — re-aggregating an aggregate would only add memory); (3) snapshot counters export as CUMULATIVE sums stamped with process start (no per-flush delta memos), evented cost families stay DELTA; (4) the attribution budget derives from the unchanged cost sub-budget (256), so runtime growth provably cannot shrink it; (5) the family registry lives in a new `otel_metrics_families.zig` and the collector in a new `otel_metrics_runtime.zig` — the length gate's sanctioned split.

## Implementing agent — read these first

1. `src/agentsfleetd/observability/otel_metrics_payload.zig` — the closed `MetricId` enum plus `MetricMeta` is the registry new families join; `MetricKind` is where the gauge variant lands.
2. `src/agentsfleetd/observability/otel_metrics_aggregate.zig` — windowed-delta coalescing and the series ceiling; the gauge path diverges here and nowhere else.
3. `src/agentsfleetd/observability/otel_metrics_cardinality.zig` — explains why the attribution budget is derived, and why it currently derives from a hand-picked root.
4. `playbooks/operations/observability/providers/grafana/assets/alerts.json` and `dashboard.json` — the exact family names and label sets the operator surface queries.
5. `docs/architecture/observability.md` — the metric family census is the single documented list of tracked metrics and must stay current in the same commit as any family change.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/observability/otel_metrics_payload.zig` | EDIT | gains the gauge wire shape, per-family temporality, and exact unit scaling; family identity moves out |
| `src/agentsfleetd/observability/otel_metrics_families.zig` | CREATE | the closed family registry: every identity, kind, temporality, and the derived ceiling arithmetic |
| `src/agentsfleetd/observability/otel_metrics_runtime.zig` | CREATE | flush-time collector for runtime families + streamed per-runner serialization |
| `src/agentsfleetd/observability/otel_metrics_aggregate.zig` | EDIT | gains last-value-wins folding and the derived series ceiling |
| `src/agentsfleetd/observability/otel_metrics_cardinality.zig` | EDIT | attribution budget derives from the cost sub-budget the registry declares |
| `src/agentsfleetd/observability/otel_metrics.zig` | EDIT | collects and serializes the runtime families alongside the cost families |
| `src/agentsfleetd/observability/semconv.zig` | EDIT | gains the runtime family names whose single source was the deleted renderer |
| `src/agentsfleetd/observability/metrics_counters.zig` | EDIT | signup-failure reason labels become a declared wire list |
| `src/agentsfleetd/observability/metrics_trace.zig` | EDIT | suppression reason labels become a declared wire list |
| `src/agentsfleetd/observability/metrics_render.zig` | DELETE | the Prometheus rendering layer and its format constants lose their only consumer |
| `src/agentsfleetd/observability/metrics_runner.zig` | EDIT | drops its rendering entry point, keeps the per-runner slot table |
| `src/agentsfleetd/observability/metrics_sensitive_memory.zig` | EDIT | drops its rendering entry point, keeps the resident-memory probe |
| `src/agentsfleetd/observability/metrics_memory.zig` | EDIT | drops the rendering section, keeps the memory family state |
| `src/agentsfleetd/observability/metrics_redis_pool.zig` | EDIT | pool snapshot rewires to the OTLP collector; its stated caller no longer exists |
| `src/agentsfleetd/observability/metrics.zig` | EDIT | facade stops re-exporting the renderer; module description no longer claims Prometheus text |
| `src/agentsfleetd/http/routes.zig` | EDIT | the metrics route identity is removed |
| `src/agentsfleetd/http/router.zig` | EDIT | the path match is removed |
| `src/agentsfleetd/http/route_template.zig` | EDIT | the path template is removed |
| `src/agentsfleetd/http/route_scopes.zig` | EDIT | the scope registration is removed |
| `src/agentsfleetd/http/route_table.zig` | EDIT | the table registration is removed |
| `src/agentsfleetd/http/route_trace.zig` | EDIT | the trace registration is removed |
| `src/agentsfleetd/http/route_admission.zig` | EDIT | the admission class switch follows the removed variant |
| `src/agentsfleetd/http/route_table_invoke.zig` | EDIT | the invoke registration is removed |
| `src/agentsfleetd/http/handlers/health.zig` | EDIT | the pull handler and its renderer import are removed |
| `src/agentsfleetd/observability/semantic_schema_test.zig` | EDIT | namespace and superseded-name guards move from renderer sources to the OTLP payload source |
| `src/agentsfleetd/observability/metrics_counters_test.zig` | EDIT | assertions move to the OTLP payload observation window |
| `src/agentsfleetd/observability/metrics_runner_test.zig` | EDIT | same observation-window move |
| `src/agentsfleetd/observability/metrics_memory_test.zig` | EDIT | same observation-window move |
| `src/agentsfleetd/observability/metrics_otel_test.zig` | EDIT | same observation-window move |
| `src/agentsfleetd/observability/metrics_sensitive_memory_test.zig` | EDIT | same observation-window move |
| `src/agentsfleetd/observability/otel_metrics_aggregate_test.zig` | EDIT | gains gauge folding and ceiling-derivation coverage |
| `src/agentsfleetd/observability/otel_metrics_test.zig` | EDIT | follows the wire-times surface; gauge serialization and cost-family regression coverage |
| `src/agentsfleetd/observability/otel_metrics_window_test.zig` | CREATE | the shared payload observation helper the rebuilt suite asserts through |
| `src/agentsfleetd/observability/otel_metrics_flush_test.zig` | CREATE | flush-behavior slice of the split exporter suite (length cap) |
| `src/agentsfleetd/observability/otel_metrics_attribution_test.zig` | CREATE | attribution slice of the split exporter suite (length cap) |
| `src/agentsfleetd/observability/otel_metrics_egress_test.zig` | CREATE | removed-surface and liveness-rule declaration tests |
| `src/agentsfleetd/observability/library_stages_window_test.zig` | CREATE | window slice of the library-stages suite (length cap) |
| `src/agentsfleetd/observability/semconv_test.zig` | EDIT | follows metaFor to the registry and the scale enum |
| `src/agentsfleetd/http/router_test.zig` | EDIT | the former pull path is proven unrouted |
| `src/agentsfleetd/http/handlers/grant_surface_integration_test.zig` | EDIT | unauthenticated-route census follows the removed identity |
| `src/agentsfleetd/observability/otel_metrics_census_test.zig` | CREATE | asset/census/route-absence/liveness-rule declaration tests |
| `src/agentsfleetd/observability/library_stages_test.zig` | EDIT | same observation-window move |
| `src/agentsfleetd/queue/redis_pool_test.zig` | EDIT | same observation-window move |
| `src/agentsfleetd/fleet/integration_roundtrip_test.zig` | EDIT | stops referencing the removed pull route |
| `src/agentsfleetd/http/handlers/runner/memory_loop_integration_test.zig` | EDIT | stops scraping the removed pull route |
| `playbooks/operations/observability/001_playbook.md` | EDIT | the scrape instruction becomes the push-path reality |
| `src/agentsfleetd/http/handlers/fleets/backpressure_integration_test.zig` | EDIT | stops scraping a route that no longer exists |
| `src/agentsfleetd/tests.zig` | EDIT | registration list follows the deleted module |
| `src/agentsfleetd/queue/redis_pool.zig` | EDIT | comment naming the pull endpoint as the export path is corrected |
| `deploy/fly/agentsfleetd-dev/fly.toml` | EDIT | the dead scrape block is removed |
| `deploy/fly/agentsfleetd-prod/fly.toml` | EDIT | the dead scrape block is removed |
| `playbooks/operations/observability/providers/grafana/assets/alerts.json` | EDIT | gains the exporter-liveness rule |
| `docs/architecture/observability.md` | EDIT | census gains the newly exported families; the pull-path description is corrected |

## Applicable Rules

- **`~/Projects/dotfiles/docs/greptile-learnings/RULES.md`** — **NDC** (no dead code at write time), **NLR** (touch-it-fix-it on every module the render layer leaves behind), **ORP** (orphan sweep across the route identity and the deleted symbols), **UFS** (family names and label keys are named constants shared verbatim with the assets), **FLL** (file and function length caps on the widened exporter), **TST-NAM** (rebuilt test identifiers stay milestone-free), **TGU** (the metric kind is a tagged union, never an optional-field struct), **XCC** (cross-compile before commit).
- `dispatch/write_zig.md` — memory safety, `errdefer` placement, public-surface shape verdict for every new exported symbol, and the both-target cross-compile.
- `~/Projects/dotfiles/docs/LOGGING_STANDARD.md` — the exporter emits its own health records; any new severity or error code must register.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — every source file is Zig | cross-compile both linux targets; tagged-union metric kind; `errdefer` on any new allocation in the collector |
| PUB / Struct-Shape | yes — the gauge kind and new identities widen a public surface | print a shape verdict per new public symbol; the metric kind stays an enum, the accumulator stays a value type owned by the flush thread |
| File & Function Length (≤350/≤50/≤70) | yes — the exporter and aggregator both grow | split the collector by family group before either file approaches the cap; the deleted render layer frees budget elsewhere |
| UFS (repeated/semantic literals) | yes — family names and label keys appear in source and in the operator assets | every family name is a named constant; the derived ceiling replaces a bare numeric literal |
| UI Substitution / DESIGN TOKEN | no — no frontend surface changes | not applicable |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING and LIFECYCLE yes; ERROR REGISTRY and SCHEMA no | exporter health records follow the logging standard; the aggregator is a transient owned by the flush thread with no new lifecycle pairing; no error codes and no schema change |

## Prior-Art / Reference Implementations

- **Reference:** `src/agentsfleetd/observability/otel_metrics.zig` and its cost families — the existing sum-and-histogram path is the pattern the runtime families mirror; the gauge kind is the one genuinely new shape, and it diverges only in how a flush window folds samples.
- **Reference:** `src/agentsfleetd/observability/metrics_runner.zig` — fixed-capacity slot table with an overflow bucket is the established answer to unbounded per-entity labels; the derived ceiling reuses its capacity constant rather than inventing a second bound.

## Sections (implementation slices)

### §1 — A gauge is a level, not a running total

The exporter understands running totals and distributions. Most runtime families are levels — requests in flight right now, idle pool connections right now — and folding a level by addition reports a number that is wrong in a way that reads as plausible. This slice teaches the wire format and the flush-time fold that a third kind exists. It unblocks every gauge family in §3.

**Implementation default:** the gauge serializes as a native OTLP gauge rather than a non-monotonic sum, because the reader should be able to tell a level from a counter by its type alone.

- **Dimension 1.1** — the metric kind admits a gauge alongside sum and histogram → Test `test_metric_kind_admits_gauge` — **DONE**
- **Dimension 1.2** — repeated samples of one gauge label set within a flush window fold to the newest value, not their sum → Test `test_gauge_folds_to_last_value` — **DONE**
- **Dimension 1.3** — a gauge and a sum sharing a flush window each fold by their own rule → Test `test_mixed_kinds_fold_independently` — **DONE**
- **Dimension 1.4** — a gauge series serializes in the OTLP gauge shape → Test `test_gauge_serializes_as_gauge` — **DONE**

### §2 — The series ceiling is derived, never chosen

The distinct-series cap is a hand-picked number, and the per-model cost attribution budget derives from it. Widening the exported family set therefore spends a budget nobody declared, and the overflow lands on whichever label set arrives last — possibly cost attribution rather than the family just added. This slice makes the ceiling a computed consequence of what is actually declared, so the arithmetic cannot silently go wrong again.

**Implementation default:** the runner-family term reuses the existing per-runner slot capacity as its bound rather than introducing a second, independently drifting constant.

- **Dimension 2.1** — the ceiling is computed at compile time from declared families, their maximum label combinations, the runner capacity, and the attribution budget → Test `test_series_ceiling_is_derived_from_declarations` — **DONE**
- **Dimension 2.2** — a compile-time assertion fails the build when the declared worst case exceeds the ceiling → Test `test_declared_worst_case_fits_under_ceiling` — **DONE**
- **Dimension 2.3** — the attribution budget re-derives from the new ceiling and does not shrink when runtime families are added → Test `test_attribution_budget_survives_family_growth` — **DONE**

### §3 — Runtime families reach the wire

The operator assets query families the exporter has never carried. This slice adds each family the dashboard and alert rules name, with the label sets those queries expect, so the provisioning gate can advance and the dashboard renders real data.

- **Dimension 3.1** — every family named by the dashboard and alert assets exists as a declared metric identity → Test `test_every_asset_family_is_declared` — **DONE**
- **Dimension 3.2** — saturation and pool levels export as gauges carrying their live values → Test `test_saturation_families_export_current_level` — **DONE**
- **Dimension 3.3** — cumulative families export as monotonic sums → Test `test_cumulative_families_export_as_sums` — **DONE**
- **Dimension 3.4** — per-runner families carry the runner label and route overflow to the shared bucket → Test `test_runner_families_carry_identity_and_overflow` — **DONE**
- **Dimension 3.5** — the Redis pool snapshot reaches the collector without the deleted renderer → Test `test_pool_snapshot_reaches_the_collector` — **DONE**

### §4 — The pull endpoint and its rendering layer are removed

A private daemon cannot be scraped, the deployment configuration has pointed at an unbound port in both environments since it was written, and after §3 the rendering layer has no production consumer. Leaving any part of it is configuration and code that reads as live and is not. This slice removes the route identity everywhere it is registered, the rendering entry points, and the deployment blocks.

- **Dimension 4.1** — the daemon answers no route at the former metrics path → Test `test_metrics_path_is_not_routed` — **DONE**
- **Dimension 4.2** — the route identity is absent from every registration surface → Test `test_metrics_route_identity_is_absent` — **DONE**
- **Dimension 4.3** — no rendering entry point survives in any observability module → Test `test_no_prometheus_rendering_entry_point_remains` — **DONE**
- **Dimension 4.4** — neither deployment configuration declares a scrape block → Test `test_deploy_configs_declare_no_scrape_block` — **DONE**

### §5 — The test observation window moves to the wire format

Metric tests deliberately observe instrumentation through the rendered output rather than reaching into internal state, and that window is being deleted. This slice rebuilds the window on the exported payload so the suite keeps its no-internals discipline and, for the first time, asserts against the path production actually uses.

**Implementation default:** one shared test helper renders a flush window to an inspectable form, mirroring how the deleted renderer was shared, so individual tests change their target and not their shape.

- **Dimension 5.1** — a shared helper exposes a flush window for assertion without reaching into instrumentation internals → Test `test_payload_observation_helper_exposes_a_window` — **DONE**
- **Dimension 5.2** — every previously rendered family is asserted through the new window with its prior behavioural claim intact → Test `test_rebuilt_suite_covers_every_previously_rendered_family` — **DONE**
- **Dimension 5.3** — the namespace guard rejects any family outside the project namespace on the payload source → Test `test_namespace_guard_runs_against_the_payload_source` — **DONE**
- **Dimension 5.4** — the superseded-name guard scans the exporter source rather than the deleted renderer sources → Test `test_superseded_name_guard_scans_the_exporter` — **DONE**

### §6 — The operator can tell when the pipe itself is dead

Deleting the pull endpoint removes the out-of-band way to observe the daemon, and the existing exporter-health alerts travel through the exporter they watch. This slice adds an absence-based rule that evaluates in the metric store rather than in the process, plus the documentation the census promises.

- **Dimension 6.1** — an alert rule fires on absence of the saturation family rather than on a threshold over it → Test `test_liveness_rule_fires_on_absent_series` — **DONE**
- **Dimension 6.2** — the alert asset validates and carries the project service label used by the routing policy → Test `test_liveness_rule_validates_and_carries_service_label` — **DONE**
- **Dimension 6.3** — the census lists every exported family and the four-signal description no longer claims a pull path → Test `test_census_matches_exported_families` — **DONE**

## Interfaces

```
OTLP metrics export (outbound, unchanged endpoint):
  POST {GRAFANA_OTLP_ENDPOINT}/v1/metrics
  Adds: gauge dataPoints alongside existing sum and histogram dataPoints.
  Metric kind is one of: sum | histogram | gauge.

Removed HTTP surface:
  GET /metrics — route identity, path template, scope, table entry, and trace
  registration all removed. No replacement path is introduced.

Grafana query surface (unchanged, now answered):
  Datasource unique identifier (UID) grafanacloud-prom returns >= 1 series for
  every family named in alerts.json and dashboard.json.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Gauge folded by addition | a level family routed through the sum path | compile-time kind dispatch makes the sum path unreachable for a gauge; a fold test observes the newest value, never the total |
| Series ceiling exceeded | declared families plus runner capacity plus attribution budget outgrow the ceiling | the build fails at the compile-time assertion before any deployment; the operator never sees partial data |
| Runner label unbounded | more live runners than the slot table holds | overflow routes to the shared bucket with its outcome preserved; the dashboard shows aggregate rather than losing the sample |
| Exporter dead | network partition, credential rotation, or a crashed flush thread | the absence rule fires from the metric store, which does not depend on the exporter; the operator is paged rather than seeing a flat dashboard |
| Stale asset family | an asset queries a family the exporter no longer declares | the declaration test fails in Continuous Integration (CI), naming the family, before the dashboard is provisioned |
| Orphaned route identity | a registration surface keeps the removed identity | the absence test fails, naming the surviving surface |

## Invariants

1. A metric family is exported through exactly one encoder — enforced by the deletion of the rendering layer and the guard test that no rendering entry point survives.
2. The declared worst-case series count never exceeds the ceiling — enforced by a compile-time assertion, not by review.
3. Every family the operator assets query is declared in the exporter — enforced by the declaration test that reads the asset files.
4. Every exported family appears in the architecture census — enforced by the census test that compares the documented list against the declared identities.
5. A level never folds by addition — enforced by compile-time kind dispatch; the sum path is unreachable for a gauge.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| runtime saturation, pool, lease, and runner families | ops | continuously, folded once per flush window | runner identity, outcome, failure class, pool result | no credential, token, endpoint secret, or workspace-identifying value on any label | `test_every_asset_family_is_declared` |
| exporter liveness alert | ops | the saturation family reports no series for the evaluation window | none — the rule is absence-based | not applicable — no sample payload | `test_liveness_rule_fires_on_absent_series` |

No product analytics event changes. No funnel changes, so no analytics playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_metric_kind_admits_gauge` | the kind enumeration carries a gauge variant distinct from sum and histogram |
| 1.2 | unit | `test_gauge_folds_to_last_value` | ten samples of the same label set with values ending at seven fold to seven, not seventy |
| 1.3 | unit | `test_mixed_kinds_fold_independently` | a sum and a gauge in one window yield the sum's total and the gauge's newest value |
| 1.4 | unit | `test_gauge_serializes_as_gauge` | the emitted payload places the series under the gauge form, not a non-monotonic sum |
| 2.1 | unit | `test_series_ceiling_is_derived_from_declarations` | the ceiling equals the sum of declared terms; changing a declaration changes the ceiling |
| 2.2 | unit | `test_declared_worst_case_fits_under_ceiling` | the declared worst case is below the ceiling; a deliberately inflated declaration fails the assertion |
| 2.3 | unit | `test_attribution_budget_survives_family_growth` | the attribution budget after adding the runtime families is not smaller than before |
| 3.1 | unit | `test_every_asset_family_is_declared` | every family name read from the alert and dashboard assets resolves to a declared identity |
| 3.2 | unit | `test_saturation_families_export_current_level` | a set in-flight count of three exports as a gauge valued three |
| 3.3 | unit | `test_cumulative_families_export_as_sums` | two increments export as a monotonic sum valued two |
| 3.4 | unit | `test_runner_families_carry_identity_and_overflow` | a known runner keeps its identity label; one beyond capacity lands in the shared bucket with its outcome preserved |
| 3.5 | integration | `test_pool_snapshot_reaches_the_collector` | a registered pool's live statistics appear in a flush window with no renderer present |
| 4.1 | integration | `test_metrics_path_is_not_routed` | a request to the former path is unrouted; the daemon answers no successful response |
| 4.2 | unit | `test_metrics_route_identity_is_absent` | the route identity resolves in none of the six registration surfaces |
| 4.3 | unit | `test_no_prometheus_rendering_entry_point_remains` | no observability module exposes a rendering entry point |
| 4.4 | unit | `test_deploy_configs_declare_no_scrape_block` | neither deployment configuration declares a scrape block |
| 5.1 | unit | `test_payload_observation_helper_exposes_a_window` | the helper returns an inspectable window without reading instrumentation internals |
| 5.2 | unit | `test_rebuilt_suite_covers_every_previously_rendered_family` | every family the deleted renderer emitted has an assertion in the rebuilt suite |
| 5.3 | unit | `test_namespace_guard_runs_against_the_payload_source` | a family outside the project namespace fails the guard |
| 5.4 | unit | `test_superseded_name_guard_scans_the_exporter` | a superseded family name introduced into the exporter fails the guard |
| 6.1 | unit | `test_liveness_rule_fires_on_absent_series` | the rule evaluates absence; a present series does not fire it |
| 6.2 | unit | `test_liveness_rule_validates_and_carries_service_label` | the asset validates and carries the service label the routing policy matches |
| 6.3 | unit | `test_census_matches_exported_families` | the documented census and the declared identities agree in both directions |
| regression | integration | `test_cost_families_unchanged_after_widening` | the pre-existing cost families keep their kinds, names, and label sets |
| regression | integration | `test_traces_and_logs_unaffected_by_metric_widening` | the other two signals continue to export after the metric set grows |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | The observability gate advances past the datasource probe (§3, §6) | `ALLOW_VAULT_READS=1 ./playbooks/operations/observability/00_gate.sh check dev grafana` | exit 0 | P0 | |
| R2 | No pull endpoint remains anywhere in the daemon (§4) | `grep -rn '"/metrics"' src/ \| wc -l` | `0` | P0 | |
| R3 | No rendering entry point survives (§4) | `grep -rn 'renderPrometheus' src/ \| wc -l` | `0` | P0 | |
| R4 | Neither deployment configuration declares a scrape block (§4) | `grep -rn '\[\[metrics\]\]' deploy/ \| wc -l` | `0` | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps below | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `src/agentsfleetd/observability/metrics_render.zig` | `test ! -f src/agentsfleetd/observability/metrics_render.zig` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `renderPrometheus` | `grep -rn "renderPrometheus" src/ \| head` | 0 matches |
| `metrics_render` | `grep -rn "metrics_render" src/ \| head` | 0 matches |
| metrics route identity | `grep -rn "\.metrics\b" src/agentsfleetd/http/ \| head` | 0 matches |
| scrape block | `grep -rn "\[\[metrics\]\]" deploy/ \| head` | 0 matches |

## Out of Scope

- Restricting datastore ingress to the platform's egress addresses — a separate operational gate blocked on a paid static allocation decision; unrelated to metric collection.
- Reviewing whether the six existing alert rules are the right alerts — this milestone makes them evaluable; their thresholds and choice are taken as given.
- Collecting metrics from runner hosts — per-runner families are recorded daemon-side, so no runner-side collection exists or is needed.

---

## Product Clarity (authoring record)

1. **Successful user moment** — an operator opens the development Grafana folder, sees in-flight requests and Redis pool depth moving in real time, and watches a lease-poll spike explain a latency complaint without opening a terminal.
2. **Preserved user behaviour** — the daemon keeps serving every existing route and answering readiness; logs and traces keep arriving in Grafana unchanged; per-model cost attribution keeps its precision.
3. **Optimal-way check** — the most direct shape is a single outbound pipe carrying all three signals, which is what this delivers. The gap from unconstrained-optimal is that the daemon still holds instrumentation state in fixed-capacity tables rather than an arbitrary-cardinality store; that is acceptable and deliberate, since bounded memory on a small machine is worth more than unbounded label freedom.
4. **Rebuild-vs-iterate** — iterate. The exporter already owns the ring, the flush thread, aggregation, and the cardinality budget; the shortfall is coverage and one missing kind, not structure. Determinism is preserved because folding stays inside the single-owner flush thread.
5. **What we build** — a gauge kind with its own fold, a derived series ceiling, the runtime families on the wire, the removal of the pull path, a rebuilt observation window for tests, an absence-based liveness rule, and a current census.
6. **What we do NOT build** — no sidecar collector, no second datasource, no scrape path through the tunnel, no alert-threshold redesign, no runner-side collection.
7. **Fit with existing features** — compounds with the logs and traces already flowing to the same destination, so an operator can pivot from a trace to the saturation that caused it. The one feature it must not destabilize is per-model cost attribution, which is why the ceiling becomes derived rather than merely larger.
8. **Surface order** — operator-surface-first, justified: the consumer is a provisioned dashboard and alert set, and there is no command-line or web equivalent for this data today.
9. **Dashboard restraint** — no panel ships for a family that has no live series behind it, and the liveness rule exists precisely so an empty panel is distinguishable from a dead pipe.
10. **Confused-user next step** — the provisioning gate names the exact missing family in its failure output, so an operator who sees an empty dashboard runs the check arm and is told which family is absent rather than filing a ticket.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six sections split along the dependency edge rather than by file. The wire format and the ceiling are prerequisites; family coverage, endpoint removal, the test window, and the operator surface follow. Endpoint removal shares no module with the exporter work, so it runs concurrently.
- **Alternatives considered:** registering the platform's managed metric store as a second datasource, which was rejected because it splits runtime health from cost data into two stores that cannot be joined in one query; and running a collector process beside the daemon to scrape and forward, which was rejected because it adds a process to own, configure, and upgrade in exchange for a pipe that already exists.
- **Patch-vs-refactor verdict:** this is a **refactor**, because the shortfall is structural — one signal path was never wired and a second, unwired path was left in place — and a minimal patch would have meant fixing a port number to feed a store the operator assets do not query.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`.
