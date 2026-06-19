# M94_001: Push OTLP metrics to Grafana, completing the OpenTelemetry triad

**Prototype:** v2.0.0
**Milestone:** M94
**Workstream:** 001
**Date:** Jun 19, 2026
**Status:** PENDING
**Priority:** P1 — operator-facing observability; moves analytical/dashboard load off the control-plane Postgres
**Categories:** API, OBS
**Batch:** B1
**Branch:** {feat/m94-otlp-metrics — added when work begins}
**Depends on:** none (otel_traces / otel_logs already ship the push pipeline)
**Provenance:** LLM-drafted (Claude Opus 4.8, Jun 19 2026) — design captured in a live session; cross-check every claim against the codebase before EXECUTE.

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §3–4 (the money path this spec must NOT touch). The observability exporter is greenfield in shape — mirror `src/agentsfleetd/observability/otel_traces.zig`.

---

## Implementing agent — read these first

1. `src/agentsfleetd/observability/otel_traces.zig` — the exact module to mirror: bounded batch queue, background flush thread, OTLP-JSON builder, `install`/`uninstall` gated on config, fire-and-forget POST.
2. `src/agentsfleetd/observability/otel_logs.zig` — `GrafanaOtlpConfig`, `configFromEnv` (reads `GRAFANA_OTLP_ENDPOINT`/`INSTANCE_ID`/`API_KEY`), and `postWithBasicAuth` — reuse all three verbatim; do not duplicate config or auth.
3. `src/agentsfleetd/cmd/preflight.zig` — where `otel_traces.install` / `otel_logs.install` are wired (and their `uninstall`); the metrics exporter installs/uninstalls in the same place under the same gate.
4. `src/agentsfleetd/agent/metering.zig` + `src/agentsfleetd/fleet/renewal_settle.zig` — the post-commit points where credit-drain and token/latency data already exist; emit there, after the money transaction commits.
5. `docs/architecture/billing_and_provider_keys.md` §3–4 — the atomic wallet+ledger contract that defines the hard boundary: nothing money-bearing moves.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Push OTLP metrics to Grafana Cloud, completing the OTel triad
- **Intent (one sentence):** operators get token-throughput, credit-drain, and run-latency dashboards in Grafana without any dashboard query touching the control-plane Postgres.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent and list `ASSUMPTIONS I'M MAKING: …`. A mismatch with the Intent above → STOP and reconcile.

---

## Product Clarity

1. **Successful user moment** — an operator opens Grafana and watches run-latency p95 and token throughput by model update live during a busy window, while Postgres CPU stays flat — the dashboard never queried the control-plane DB.
2. **Preserved user behaviour** — the Prometheus scrape `/metrics` keeps working unchanged; the money path (wallet debit + charge ledger) and the customer per-event drill-down API (`GET /v1/tenants/me/billing/charges/{event_id}/telemetry`) are byte-for-byte unchanged.
3. **Optimal-way check** — the daemon already pushes two OTLP signals to Grafana Cloud (traces→Tempo, logs→Loki). The most direct way to dashboard observability off-Postgres is to complete the triad: add metrics→Mimir over the same pipeline. No material gap to the unconstrained-optimal shape.
4. **Rebuild-vs-iterate** — iterate (add one module). The refactor — moving the charge ledger / `fleet.metering_periods` to an external store — was considered and **rejected**: it severs the atomic wallet+ledger transaction (`metering.zig`), a money-correctness regression. Verdict: iterate; full rationale below.
5. **What we build** — `otel_metrics.zig` (OTLP metrics exporter) + emit sites at the metering hot paths + install wiring in preflight + a Grafana dashboard JSON.
6. **What we do NOT build** — no move of money/ledger/`metering_periods` out of Postgres; no runner-side emission; no Prometheus-histogram retrofit on the scrape path; no new database; no Grafana alerting.
7. **Fit with existing features** — compounds with the existing Tempo/Loki dashboards (same Grafana stack, same OTLP config). Must not destabilize the metering transaction.
8. **Surface order** — daemon-first (the emitter); the Grafana dashboard is the read surface, built after the series emit real values.
9. **Dashboard restraint** — no panel ships before its emit site is live and counters move; the high-cardinality `workspace` label is cardinality-guarded before it leaves the process.
10. **Confused-user next step** — if metrics don't appear, the operator checks `GRAFANA_OTLP_ENDPOINT` is set (the same gate as traces/logs); preflight logs a one-line `metrics exporter disabled (no GRAFANA_OTLP_ENDPOINT)` so the self-serve signal is in the startup log, not a ticket.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal (UFS, NDC, NLR, NLG always apply).
- **`dispatch/write_zig.md`** — `*.zig`: single-type-module / file-as-struct shape for the exporter; tagged-union/`errdefer` lifecycle; the flush-thread **shutdown-must-wake** rule (Listener/Hang section); atomic-ordering comments on the queue coordination; PUB surface; cross-compile both linux targets.
- **`dispatch/write_any.md`** — UFS (metric names / units / label keys / `/v1/metrics` path as named consts), LENGTH (≤350), LOGGING (scoped log), MILESTONE-ID.
- **`docs/LOGGING_STANDARD.md`** — the exporter's fire-and-forget failure logs (`EVENT_IGNORED_ERROR`, scoped `.otel_metrics`).
- **`docs/LIFECYCLE_PATTERNS.md`** — `install`/`uninstall` + flush-thread `join` cleanup.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | read `dispatch/write_zig.md`; cross-compile `x86_64-linux` + `aarch64-linux`; verify the flush-thread shutdown wakes its wait (Linux accept/wait hang rule). |
| PUB / Struct-Shape | yes | new module — file-as-struct verdict for the exporter type; pub surface limited to `install`/`uninstall`/`isInstalled` + the narrow `record*` emit fns, mirroring `otel_traces`. |
| File & Function Length | yes | keep `otel_metrics.zig` ≤350; if the OTLP-JSON builder pushes over, split the payload builder into a sibling exactly as `otel_traces` does. |
| UFS | yes | metric names, units, label keys, and `"/v1/metrics"` are named consts; any name shared with a Grafana dashboard JSON is the single source. |
| LOGGING | yes | `logging.scoped(.otel_metrics)`; export failure is a single `warn` (`EVENT_IGNORED_ERROR`), never an error that blocks. |
| LIFECYCLE | yes | `install` starts the flush thread; `uninstall` signals stop, wakes the wait, and `join`s — no orphan thread, no leak. |
| ERROR REGISTRY / SCHEMA / UI / DESIGN TOKEN | no | no error codes, no schema, no frontend. |

---

## Overview

**Goal (testable):** `otel_metrics.zig` batches in-process metric samples and POSTs OTLP-JSON to `GRAFANA_OTLP_ENDPOINT/v1/metrics` on a background flush thread, fire-and-forget, installed only when the shared `GRAFANA_OTLP_*` config is present; the metering hot paths emit credit-drain sums, token-throughput sums, and a run-latency histogram dimensioned by `{model, posture}` (+ a cardinality-guarded `workspace`); no dashboard read ever touches the control-plane Postgres.

**Problem:** operators cannot watch token throughput, run latency, or credit-drain trends without querying the control-plane Postgres, which (a) puts analytical load on the DB that serves live agent operations and (b) grows unbounded as `agent_execution_telemetry` / `fleet.metering_periods` accumulate.

**Solution summary:** complete the OTLP triad already shipped for traces and logs — add a metrics exporter mirroring `otel_traces.zig`, instrument the metering hot paths post-commit, and dashboard the series in Grafana Cloud (Mimir). The money path and the customer drill-down API stay in Postgres, untouched.

---

## Prior-Art / Reference Implementations

- **API/daemon** → `src/agentsfleetd/observability/otel_traces.zig` is the exact mirror: async bounded queue, background flush thread, OTLP-JSON serialization, `install`/`uninstall` lifecycle, `postWithBasicAuth`. **Alignment:** same `GrafanaOtlpConfig`, same auth/post helper, same fire-and-forget discipline, same preflight wiring. **Divergence:** the OTLP body is the *metrics* schema (`resourceMetrics → scopeMetrics → metric → {sum|histogram} → dataPoints`), not spans; the run-latency **histogram** is net-new (the existing `metrics_runner.zig` "histogram" is a categorical FailureClass bucket, not an `le=` latency histogram — do not reuse it as a latency source; carry explicit bucket bounds in the exporter).
- Greenfield otherwise — shape defined by the mirror above; no `docs/architecture/` observability doc exists yet (a one-paragraph note may be added to the architecture directory at DOCUMENT).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/agentsfleetd/observability/otel_metrics.zig` | CREATE | the OTLP metrics exporter (mirror of otel_traces). |
| `src/agentsfleetd/observability/otel_metrics_test.zig` | CREATE | unit tests (payload shape, gating, drop-on-full, shutdown). |
| `src/agentsfleetd/observability/metrics.zig` | EDIT | (if needed) shared metric-sample type / registry hook for the new series. |
| `src/agentsfleetd/cmd/preflight.zig` | EDIT | install/uninstall the metrics exporter under the same `GRAFANA_OTLP_*` gate as traces/logs. |
| `src/agentsfleetd/agent/metering.zig` | EDIT | emit credit-drain + token sums post-commit (receive + stage debits). |
| `src/agentsfleetd/fleet/renewal_settle.zig` | EDIT | emit token-delta sum + run-latency histogram at settle. |
| `deploy/grafana/agent-observability.json` | CREATE | Grafana dashboard for the three series (config, not code). |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, four Sections (exporter module / instrumentation / wiring+config / cardinality-guard+dashboard). The exporter is the only non-trivial new code; instrumentation is small post-commit emit calls.
- **Alternatives considered:** (a) **move the charge ledger / `metering_periods` to an external store** — rejected: severs the atomic wallet+ledger transaction (`metering.zig`), introducing debited-but-unrecorded money risk; (b) **retrofit Prometheus histograms on the scrape `/metrics`** — rejected: the scrape path has no latency-histogram primitive and doesn't reach Grafana Cloud where Tempo/Loki already live; OTLP unifies the three signals on one pipeline.
- **Patch-vs-refactor verdict:** this is a **patch/iterate** (add one module + post-commit emit sites), not a refactor. The money-store refactor is explicitly **out of scope** and money-unsafe; no follow-up spec is implied — the boundary is intentional, not deferred work.

---

## Sections (implementation slices)

### §1 — OTLP metrics exporter module

Deliver `otel_metrics.zig` mirroring `otel_traces.zig`: a bounded queue of metric samples, a background flush thread that batches and POSTs OTLP-JSON metrics to `/v1/metrics`, install/uninstall gated on `GrafanaOtlpConfig`, fire-and-forget on error. **Implementation default:** reuse `otel_logs.postWithBasicAuth` and `GrafanaOtlpConfig` rather than re-deriving config/auth.

- **Dimension 1.1** — exporter installs only when `GrafanaOtlpConfig` is present; absent → emit calls are cheap no-ops → Test `test_disabled_when_no_config`.
- **Dimension 1.2** — `record*` enqueue is non-blocking and bounded: a full queue drops the sample and increments a drop counter, never blocks the caller → Test `test_enqueue_drops_on_full_never_blocks`.
- **Dimension 1.3** — flush serializes a sum data point and a histogram data point into valid OTLP-JSON and POSTs to `/v1/metrics` → Test `test_otlp_payload_shape` (assert against a captured fixture).
- **Dimension 1.4** — `uninstall` signals stop, wakes the flush wait, and joins the thread with no hang and no leak → Test `test_uninstall_joins_cleanly`.

### §2 — Metric instrumentation at the metering hot paths

Emit the series at the same call sites that already increment Prometheus counters, **after** the money transaction commits.

- **Dimension 2.1** — a credit-drain sum is emitted on each committed debit (receive + stage), labelled `{posture, model}` (+ guarded `workspace`) → Test `test_emits_credit_drain_on_debit`.
- **Dimension 2.2** — a token-throughput sum is emitted on each renew settle from the token delta, by direction `{input, cached, output}` → Test `test_emits_token_throughput_on_settle`.
- **Dimension 2.3** — a run-latency histogram observes `wall_ms` on settle → Test `test_observes_run_latency_histogram`.

### §3 — Wiring & config

Install/uninstall in `preflight.zig` alongside traces/logs, reusing `configFromEnv`; the metrics exporter shares the single `GRAFANA_OTLP_*` enablement gate.

- **Dimension 3.1** — preflight installs the metrics exporter iff config present, and uninstalls on shutdown → Test `test_preflight_installs_under_gate`.
- **Dimension 3.2** — the disabled path logs one line `metrics exporter disabled (no GRAFANA_OTLP_ENDPOINT)` at startup → Test `test_disabled_logs_once`.

### §4 — Cardinality guard & dashboard

The `workspace` label is high-cardinality; guard it. Provide the Grafana dashboard JSON.

- **Dimension 4.1** — above a configured cardinality cap the `workspace` label is dropped/aggregated; below it, retained → Test `test_workspace_label_cardinality_capped`.
- **Dimension 4.2** — `deploy/grafana/agent-observability.json` is valid JSON whose panels reference exactly the emitted metric-name constants → Test `test_dashboard_metric_names_match_constants` (panel names vs the UFS metric-name consts).

---

## Interfaces

```
Env gate (shared with traces/logs):
  GRAFANA_OTLP_ENDPOINT, GRAFANA_OTLP_INSTANCE_ID, GRAFANA_OTLP_API_KEY
OTLP push target: {GRAFANA_OTLP_ENDPOINT}/v1/metrics  (Grafana Cloud Mimir)

Emitted metrics (names are the contract; units OTLP-standard):
  agentsfleet.credit.drained_nanos   sum (monotonic)   labels: posture, model[, workspace*]
  agentsfleet.tokens.processed       sum (monotonic)   labels: direction{input|cached|output}, posture, model
  agentsfleet.run.duration_ms        histogram         labels: posture, model
  * workspace label is cardinality-guarded (§4)

Pub surface (mirror otel_traces): install(cfg), uninstall(), isInstalled(),
  recordCreditDrain(...), recordTokens(...), observeRunDuration(...)
```

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Endpoint unreachable | Grafana Cloud down / network blip | flush logs one `warn`, increments an export-error counter; metering path unaffected; samples for that batch are lost (acceptable — observability, not money). |
| Queue full | emit burst outpaces flush | sample dropped, drop counter incremented; caller never blocks. |
| Partial config | `GRAFANA_OTLP_ENDPOINT` set but instance/key missing | `configFromEnv` returns null → exporter not installed; startup logs disabled. |
| Shutdown mid-POST | `uninstall` during in-flight flush | stop signal + wake + `join`; in-flight POST completes or is abandoned; no hang, no leak. |
| Unbounded `workspace` cardinality | many workspaces active | label dropped/aggregated above the cap (§4 invariant); never emits an unbounded label set. |
| Exporter stall affecting money | a slow/blocked exporter | impossible by construction: emits are post-commit + non-blocking; a stalled exporter leaves the debit result unchanged. |

---

## Invariants

1. **The exporter never blocks or fails a metering/debit/settle operation.** Enforced by: emit sites placed AFTER the money transaction commits + non-blocking bounded enqueue; a test stalls/fails the exporter and asserts the debit result is unchanged.
2. **No money/ledger/`metering_periods` read or write leaves Postgres.** Enforced by: the diff touches no `schema/*` and no billing SQL read path; a grep check that `get_tenant_billing_charges/{event_id}/telemetry` still reads `fleet.metering_periods` from PG.
3. **The metrics exporter installs iff the same `GRAFANA_OTLP_*` config that gates traces/logs is present** — no separate enablement flag. Enforced by: `configFromEnv` reuse + `test_preflight_installs_under_gate`.
4. **The `workspace` label cardinality is bounded.** Enforced by: a runtime cap guard + `test_workspace_label_cardinality_capped`.
5. **`otel_metrics.zig` ≤ 350 lines.** Enforced by the File-Length gate.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_disabled_when_no_config` | no config → `isInstalled()==false`; `record*` is a no-op. |
| 1.2 | unit | `test_enqueue_drops_on_full_never_blocks` | full queue → sample dropped, drop counter +1, call returns immediately. |
| 1.3 | unit | `test_otlp_payload_shape` | a sum + histogram sample serialize to OTLP-JSON matching `samples/fixtures/m94-fixtures/otlp_metrics.json`. |
| 1.4 | unit | `test_uninstall_joins_cleanly` | install→uninstall completes without hang; no leaked thread/allocation (testing allocator). |
| 2.1 | unit | `test_emits_credit_drain_on_debit` | a committed receive+stage debit records a credit-drain sum with `{posture,model}`. |
| 2.2 | unit | `test_emits_token_throughput_on_settle` | a settle with a token delta records token sums per direction. |
| 2.3 | unit | `test_observes_run_latency_histogram` | a settle with `wall_ms=N` observes N into the duration histogram. |
| 3.1 | integration | `test_preflight_installs_under_gate` | preflight with config present installs the exporter; absent → not installed; shutdown uninstalls. |
| 3.2 | unit | `test_disabled_logs_once` | disabled startup logs the one-line disabled message exactly once. |
| 4.1 | unit | `test_workspace_label_cardinality_capped` | above cap → `workspace` omitted/aggregated; below → retained. |
| 4.2 | unit | `test_dashboard_metric_names_match_constants` | every metric name referenced in `deploy/grafana/agent-observability.json` equals a UFS metric-name const. |
| — | integration | `test_money_path_unaffected_by_exporter` | with the exporter stalled/failing, a debit commits and returns the same outcome (Invariant 1). |
| — | regression | `test_scrape_metrics_unchanged` | `/metrics` (Prometheus scrape) output is unchanged by this milestone. |

Idempotency/replay: N/A — metrics are additive samples, not deduplicated wire events.

---

## Acceptance Criteria

- [ ] Exporter installs only under the shared `GRAFANA_OTLP_*` gate — verify: `test_preflight_installs_under_gate`
- [ ] Money path provably unaffected by a stalled exporter — verify: `test_money_path_unaffected_by_exporter`
- [ ] OTLP payload shape matches the fixture — verify: `test_otlp_payload_shape`
- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (preflight install path)
- [ ] `make memleak` clean (flush-thread + queue allocator wiring)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: exporter gating + money-safety
zig build test -Dtest-filter="otel_metrics" 2>&1 | tail -3
# E2: Build
zig build 2>&1 | tail -3
# E3: Tests — make test
# E4: Lint — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — zig build -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: money boundary intact (must still read metering_periods from PG) —
grep -rn "fleet.metering_periods" src/agentsfleetd/http/handlers/ | head
```

---

## Dead Code Sweep

N/A — purely additive; no files deleted.

---

## Discovery (consult log)

- **Architecture consult (Jun 19 2026):** evaluated moving telemetry out of Postgres. Found `agent_execution_telemetry` is the revenue **ledger**, written in the same transaction as the wallet debit (`agent/metering.zig`, `billing_and_provider_keys.md` §3–4). Decision: only the **observability/dashboard** load leaves PG; wallet + ledger + the per-event drill-down stay transactional in PG.
- **Tooling decision (Jun 19 2026):** Grafana, **not** ClickHouse, **not** a new row store. Reuse the existing OTLP push pipeline (traces→Tempo, logs→Loki); add metrics→Mimir as the third signal.
- **Driver:** write-volume relief + operational isolation (keep heavy analytical load off the control-plane DB).
- **Skill chain outcomes** — populate `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs` results here as work proceeds.
- **Deferrals** — none. The money-store boundary is intentional scope, not a deferral.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | audits diff coverage vs this Test Specification | clean; iteration count in Discovery |
| After tests pass, before CHORE(close) | `/review` | adversarial diff review vs this spec, `dispatch/write_zig.md`, Failure Modes, Invariants | clean OR every finding dispositioned |
| After `gh pr create` | `/review-pr` | review-comments the open PR | comments addressed before merge |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste} | |
| Integration | `make test-integration` | {paste} | |
| Lint | `make lint` | {paste} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {paste} | |
| Memleak | `make memleak` | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |
| Money boundary intact | `grep -rn "fleet.metering_periods" src/agentsfleetd/http/handlers/` | {paste} | |

---

## Out of Scope

- Moving the wallet, charge ledger, or `fleet.metering_periods` out of Postgres (money-safe boundary — not a deferral, an intentional constant).
- Runner-plane OTLP emission (the runner reports cumulatives to agentsfleetd, which emits centrally).
- Retrofitting Prometheus histograms on the scrape `/metrics` endpoint.
- Any change to the customer-facing billing drill-down API.
- Grafana alerting rules (dashboards only).
