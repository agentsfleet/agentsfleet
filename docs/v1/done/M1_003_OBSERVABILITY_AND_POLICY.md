# M1_003: Observability and Policy

Date: Mar 2, 2026 (updated Mar 3, 2026)
Status: IN PROGRESS — types/schema written, 5 gaps block AC sign-off (see Gap Analysis below)
Depends on: M1_000 (control plane + NullClaw baseline), M1_002 (event envelope)

## Goal

Specify M1 observability and guardrail behavior so operators can explain every run outcome, measure reliability/cost, and enforce action safety boundaries (`safe`, `sensitive`, `critical`) consistently across web and agent API interfaces.

## Explicit Assumptions

1. Observability is product-critical, not a debug-only log sink.
2. Policy checks occur before every state-changing action.
3. M1 does not include chat/voice ingress; those channels inherit the same policy engine in M2+.
4. Run-level metrics must be queryable within seconds for operator dashboards.
5. Sensitive/critical actions require explicit identity binding.
6. M1 uses Langfuse + OpenTelemetry. Signal-based observability (LLM-as-judge, friction detection) is M2 scope per ADR-008.

## In-Scope

1. Event taxonomy and envelope requirements for lifecycle, cost, and policy decisions.
2. Dashboard views and minimum M1 SLO definitions.
3. Policy matrix for safe/sensitive/critical action classes.
4. Alerting triggers for blocked runs, SLO breaches, and high-risk policy denies.

## Out-of-Scope

1. Full anomaly detection/ML forecasting.
2. Fine-grained billing reconciliation.
3. Enterprise-only compliance packs (SOC2 export automation, legal hold workflows).
4. Fully autonomous critical actions without human confirmation.
5. Signal-based observability (LLM-as-judge friction/delight detection) — M2 scope.

## Interfaces and Contracts

### 1) Observability Event Taxonomy

Core event types:
1. `transition`: state changes with `state_from/state_to/reason_code`
2. `policy_decision`: allow/deny/require_confirmation with rule ID
3. `validation_result`: pass/fail with tiered findings (T1-T4)
4. `cost_snapshot`: token/runtime totals per attempt
5. `notification_sent`: operator notifications and delivery outcomes

Optional diagnostic event type:
1. `tool_execution`: tool name, duration, exit status (bounded verbosity)

NullClaw-specific telemetry:
1. `nullclaw_run`: agent role, config file, result status, wall-clock duration, peak memory, token count
2. Emitted by zombie binary after each NullClaw invocation (Echo, Scout, Warden)

Envelope requirements:
1. Stable `event_id` and monotonic `sequence` per run.
2. Correlation keys: `tenant_id`, `workspace_id`, `run_id`, `attempt`, `request_id`.
3. Time fields in UTC RFC3339.
4. Reason codes from controlled enum.

### 2) Operator Views Contract

1. **Run Replay:** ordered timeline + linked artifacts + policy decisions + NullClaw run events.
2. **Reliability Dashboard:** success rate, retry rate, blocked-run rate, median spec-to-PR time.
3. **Cost Dashboard:** cost per successful PR, top expensive workspaces, spike detection summary. Includes NullClaw wall-clock time and token consumption per agent role.
4. **Policy Dashboard:** deny volume, override count, highest-risk attempted actions.

### 3) SLO and Alert Contract

M1 target SLOs:
1. 90% of runs reach `PR_OPENED` within 30 minutes.
2. Blocked-run rate stays below 10% over rolling 7 days.
3. Median retries per successful run is <= 1.
4. Blocked-run alert emission latency is < 60 seconds.

Alert severities:
1. `P1`: control plane unavailable or run admission failures > 25% in 15m.
2. `P2`: blocked-run rate breach for 1h window.
3. `P3`: sustained cost spike above configured workspace budget threshold.

### 4) Policy Guardrails Contract

Action classes:
1. `safe`: read-only/status/list operations.
2. `sensitive`: run start/retry/pause and approval-affecting operations.
3. `critical`: destructive or permission-changing actions.

Enforcement model:
1. Safe: allow by default if authenticated and workspace-scoped.
2. Sensitive: require single explicit confirmation + policy allow.
3. Critical: require double confirmation + policy allow + stronger identity proof.
4. Every deny/override emits `policy_decision` event with rule context.

Channel requirements:
1. Web and agent API share policy outcomes.
2. Policy decisions are auditable in run replay.
3. Future chat/voice channels must consume this same policy engine and reason-code catalog.

### 5) Signal-Based Observability (M2 scope — design notes)

M2 will add Factory.ai Signals-inspired analysis per ADR-008:
- LLM-as-judge on completed runs (friction/delight detection)
- Embedding-based clustering for pattern discovery
- Self-improving feedback loops (friction threshold → auto-filed tickets)
- Privacy-preserving: abstract patterns, never raw session content

M1 lays the foundation by emitting structured events that M2's signal pipeline can consume.

## Acceptance Criteria

1. Every state-changing operation emits both `transition` and policy context events.
2. Dashboards can reconstruct a complete run timeline from events + artifacts.
3. SLOs and alert thresholds are encoded and testable in configuration.
4. Safe/sensitive/critical classification exists for all exposed control actions.
5. Web and API paths cannot bypass sensitive/critical policy gates.
6. NullClaw run events captured with exit code, duration, and memory usage.

## Risks and Mitigations

1. Risk: high event volume affects storage/query latency.
Mitigation: partition by time/workspace and cap verbose tool events.
2. Risk: inconsistent policy outcomes across channels.
Mitigation: single policy engine service with channel as input attribute only.
3. Risk: operator confusion from unclear reason codes.
Mitigation: controlled reason-code catalog with human-readable mapping in docs.
4. Risk: false-positive alerts create fatigue.
Mitigation: rolling-window thresholds and per-workspace suppression controls.
5. Risk: NullClaw stderr output not captured in observability events.
Mitigation: zombie binary pipes NullClaw stderr to structured log events.

## Test/Verification Commands

```bash
# Required section presence
rg -n "^## (Goal|Explicit|In-Scope|Out-of-Scope|Interfaces|Acceptance|Risks|Test)" docs/spec/v1/M1_003_OBSERVABILITY_AND_POLICY.md

# Policy class and SLO terms
rg -n "safe|sensitive|critical|PR_OPENED|blocked-run|policy_decision|transition" docs/spec/v1/M1_003_OBSERVABILITY_AND_POLICY.md

# Verify no stale references
rg -n "Sprint 1|PI SDK|PI Agent|OpenClaw Gateway|Phase 2" docs/spec/v1/M1_003_OBSERVABILITY_AND_POLICY.md && echo "FAIL: stale refs" || echo "PASS: clean"
```

## Gap Analysis (Oracle review Mar 3, 2026)

Foundational types exist: `EventType`, `Event`, `PolicyEvent`, `ActionClass`, `PolicyDecision` are all defined in `src/types.zig`. Schema tables (`policy_events`, `usage_ledger`, `run_transitions`) exist. The wire between request handling and actual event emission/enforcement is missing.

### Gap 1 — AC#1: No `policy_decision` event is ever emitted [BLOCKER]

**What the AC says:** Every state-changing operation emits both `transition` and policy context events.

**What is missing:** `state.transition()` in `src/state/machine.zig` writes a transition row and logs, but **nothing writes to the `policy_events` table**. The `PolicyEvent` struct and schema table exist but the insert is never called anywhere in the codebase.

**Fix required:**
- Create `src/state/policy.zig` (or add to `machine.zig`) with a `recordPolicyEvent(conn, workspace_id, run_id, action_class, decision, rule_id, actor)` function that inserts into `policy_events`.
- Call it from `handleStartRun`, `handleRetryRun`, `handlePauseWorkspace` in `src/http/handler.zig` — before the state change, record a `policy_decision` event with the appropriate `ActionClass` and `PolicyDecision.allow`.
- For blocked/denied paths, record `PolicyDecision.deny` instead of executing.

---

### Gap 2 — AC#4 and AC#5: No safe/sensitive/critical gate enforcement in handlers [BLOCKER]

**What the AC says:** Safe/sensitive/critical classification exists for all exposed control actions. Web and API paths cannot bypass sensitive/critical policy gates.

**What is missing:** `src/http/handler.zig` handlers authenticate (Bearer token) and execute — there is no action class classification or policy gate between auth and execution. `ActionClass` is defined in `src/types.zig` but unused in handlers.

**Classification mapping (implement this):**
| Endpoint | Class |
|---|---|
| `GET /v1/runs/{run_id}` | `safe` |
| `GET /v1/specs` | `safe` |
| `GET /healthz` | `safe` |
| `POST /v1/runs` | `sensitive` |
| `POST /v1/runs/{run_id}:retry` | `sensitive` |
| `POST /v1/workspaces/{workspace_id}:pause` | `sensitive` |
| `POST /v1/workspaces/{workspace_id}:sync` | `safe` |

**Fix required:**
- M1 minimum: classify each handler and record a `policy_decision` event (allow) before execution. This satisfies AC#4 (classification exists) and creates the audit trail AC#5 requires.
- Full enforcement (sensitive = require explicit confirmation) can be gated behind a `POLICY_ENFORCE=strict` env var for M1 — record the classification even in permissive mode.

---

### Gap 3 — AC#3: No SLO or alert threshold configuration [MAJOR]

**What the AC says:** SLOs and alert thresholds are encoded and testable in configuration.

**What is missing:** `config/policy.json` has `retry_budget`, `timeouts`, `rate_limits` — but no SLO targets and no alert thresholds.

**Fix required:** Add an `slos` block and `alerts` block to `config/policy.json`:

```json
{
  "retry_budget": 3,
  "timeouts": { ... },
  "rate_limits": { ... },
  "slos": {
    "pr_opened_within_minutes": 30,
    "blocked_run_rate_max_pct": 10,
    "median_retries_per_run_max": 1,
    "blocked_alert_latency_seconds": 60
  },
  "alerts": {
    "p1_admission_failure_rate_pct": 25,
    "p1_window_minutes": 15,
    "p2_blocked_run_rate_pct": 10,
    "p2_window_hours": 1,
    "p3_cost_spike_multiplier": 3
  }
}
```

The worker loop should load and expose these values; enforcement/alerting can be deferred to M2 but the config must be testable (parseable, with validated ranges) in M1.

---

### Gap 4 — AC#6: NullClaw run events missing exit code and peak memory [MAJOR]

**What the AC says:** NullClaw run events captured with exit code, duration, and memory usage.

**What is missing:** `src/pipeline/agents.zig` `AgentResult` captures `token_count` and `wall_seconds`. `writeUsage()` in `src/state/machine.zig` records both in `usage_ledger`. But:
1. Exit code is not captured — `runSingle()` result is not checked for error status beyond the Zig error union.
2. Peak memory is not captured — NullClaw does not expose this directly; capture RSS via `/proc/self/status` or skip and document as N/A for M1.
3. No `nullclaw_run` event row is written — only a `usage_ledger` row exists. The event envelope requires a `nullclaw_run` event type (defined in `src/types.zig`) to be emitted.

**Fix required:**
- Extend `AgentResult` in `src/pipeline/agents.zig` with `exit_ok: bool`.
- After `agent.runSingle()`, record success/failure in `AgentResult.exit_ok`.
- Add an `emitNullclawRunEvent(conn, run_id, attempt, actor, result)` helper that inserts a row into a `nullclaw_run_events` table (or reuses `policy_events` with `event_type = 'nullclaw_run'`). For M1, this can be a structured log line at minimum; a DB row is preferred.

---

### Gap 5 — AC#2: Dashboards require `artifacts[]` + `policy_events[]` in `GET /v1/runs` [MAJOR]

**What the AC says:** Dashboards can reconstruct a complete run timeline from events + artifacts.

**What is missing:** `GET /v1/runs/{run_id}` in `src/http/handler.zig` returns `transitions[]` only. `artifacts[]` and `policy_events[]` are absent from the response. This is shared with M1_002 Gap 3.

**Fix required:** (same as M1_002 Gap 3) — query and include both arrays in `handleGetRun`.

---

### Implementation checklist (M1_003 only)

- [ ] Create `recordPolicyEvent()` helper in `src/state/policy.zig` (or `machine.zig`)
- [ ] Call `recordPolicyEvent` from `handleStartRun`, `handleRetryRun`, `handlePauseWorkspace`
- [ ] Add action class annotation to each handler (table above)
- [ ] Add `slos` and `alerts` blocks to `config/policy.json`
- [ ] Extend `AgentResult` with `exit_ok: bool`, populate after `runSingle()`
- [ ] Emit `nullclaw_run` event (DB row or structured log) after each agent call
- [ ] `GET /v1/runs` — include `policy_events[]` array (shared fix with M1_002)

---

## Runtime Guardrail Instrumentation (Mar 05, 2026)

Thread/allocator guardrails now expose explicit worker-runtime signals:

1. `zombie_worker_in_flight_runs` (gauge)
   - Current number of runs actively executing across worker threads.
   - Updated at run begin/end boundaries in worker orchestration.
   - Shutdown expectation: converge to `0` before process exit.

2. `zombie_worker_allocator_leaks_total` (counter)
   - Incremented when worker `GeneralPurposeAllocator` reports `.leak` on teardown.
   - Used to detect allocator lifecycle regressions on shutdown/error paths.

Operational use:

1. During deploy drain/restart, verify in-flight gauge trends to zero.
2. Treat allocator leak counter increments as release-blocking until triaged.
3. Correlate with `run_id`/request logs when in-flight is non-zero at exit.
