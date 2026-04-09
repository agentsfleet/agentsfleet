# M4_001: Approval Gate — 3D Secure model for high-stakes agent actions via Slack

**Prototype:** v0.7.0
**Milestone:** M4
**Workstream:** 001
**Date:** Apr 09, 2026
**Status:** PENDING
**Priority:** P0 — Core trust differentiator; primary acquisition hook for engineering teams
**Batch:** B2 — after M3 tools land (needs tool invocation pipeline)
**Branch:** feat/m4-approval-gate
**Depends on:** M3_001 (tool registry, Slack tool, multi-tool Zombies)

---

## Overview

**Goal (testable):** When a Zombie attempts a high-stakes action (push to main, charge > $50, call a new endpoint for the first time), the event loop pauses execution, sends a Slack message with [Approve] [Deny] buttons to a configured channel, and waits for a human response. On approve: action executes, result logged. On deny: action blocked, Zombie continues with denial context. On timeout (configurable, default 1 hour): default-deny, action blocked. Anomaly patterns (e.g., 10+ identical API calls in 60 seconds) trigger auto-kill without human input. Normal actions pass silently and are logged. Target: 2-3 approvals per Zombie per day, not every action.

**Problem:** The Lead Zombie and Slack Bug Fixer Zombie can execute any action their tools allow. There's no gate between "agent decided to do X" and "X happened." Engineering teams won't deploy a Zombie that can push to main unsupervised. The CEO plan identifies the approval gate as the trust mechanism that converts "I'm scared to run this" into "I trust this because I approved the scary parts." Without it, UseZombie is convenient hosting, not a trust layer.

**Solution summary:** Add an approval gate module (`src/zombie/approval_gate.zig`) that intercepts tool invocations in the event loop. Each tool action is checked against a policy (from TRIGGER.md `gates:` section). Matching actions pause the event loop, emit a Slack interactive message via the Slack tool, and block on a Redis key for the human response. The Slack message includes action details, context, and [Approve] [Deny] buttons. Button clicks hit a new endpoint (`POST /v1/webhooks/{zombie_id}/approval`) that writes the decision to Redis and unblocks the event loop. An anomaly detector runs in-band: if the same action fires N times in T seconds, auto-kill without approval prompt. All gate decisions (approve, deny, timeout, auto-kill) are logged to the activity stream.

---

## 1.0 Gate Policy Engine

**Status:** PENDING

Parse the `gates:` section from TRIGGER.md and evaluate each tool invocation against the policy. A gate policy is a list of rules, each with: tool name, action pattern (glob), condition (threshold expression), and behavior (approve/auto-kill). Rules are evaluated top-to-bottom; first match wins. No match = auto-approve (silent pass-through).

```yaml
# TRIGGER.md gates section example
gates:
  - tool: git
    action: push
    condition: "branch == 'main'"
    behavior: approve
  - tool: github
    action: create_pr
    behavior: approve
  - tool: slack
    action: post_message
    condition: "channel != config.source_channel"
    behavior: approve
  - anomaly:
    pattern: "same_action"
    threshold: "10 in 60s"
    behavior: auto_kill
```

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/zombie/approval_gate.zig:parseGatePolicy`
  - input: `gates JSON from TRIGGER.md with 3 rules + 1 anomaly rule`
  - expected: `GatePolicy struct with 3 GateRule entries + 1 AnomalyRule, parsed correctly`
  - test_type: unit
- 1.2 PENDING
  - target: `src/zombie/approval_gate.zig:evaluateGate`
  - input: `tool="git", action="push", context={branch: "main"}, policy with matching rule`
  - expected: `GateDecision.RequiresApproval with rule reference`
  - test_type: unit
- 1.3 PENDING
  - target: `src/zombie/approval_gate.zig:evaluateGate`
  - input: `tool="slack", action="post_message", context={channel: config.source_channel}, policy with condition`
  - expected: `GateDecision.AutoApprove (condition not met — same channel)`
  - test_type: unit
- 1.4 PENDING
  - target: `src/zombie/approval_gate.zig:evaluateGate`
  - input: `tool="slack", action="react", no matching rule`
  - expected: `GateDecision.AutoApprove (no rule matched)`
  - test_type: unit

---

## 2.0 Approval Flow (Slack Interactive Messages)

**Status:** PENDING

When a gate fires `RequiresApproval`, the event loop: (1) serializes the pending action to Redis (`gate:pending:{zombie_id}:{action_id}`), (2) sends a Slack message via Block Kit with action details and interactive buttons, (3) blocks on Redis `BRPOP gate:response:{action_id}` with TTL = gate timeout. The Slack message includes: Zombie name, tool, action, parameters summary, and [Approve] [Deny] [View details] buttons.

A new webhook endpoint `POST /v1/webhooks/{zombie_id}/approval` receives Slack interactive payloads (button clicks). It validates the Slack signing secret, extracts the action_id and decision, writes to Redis (`LPUSH gate:response:{action_id}`), and returns 200 to Slack within 3 seconds.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/zombie/approval_gate.zig:requestApproval`
  - input: `pending action {tool: "git", action: "push", params: {branch: "main", files: 3}}`
  - expected: `Slack message sent with Block Kit payload, action serialized to Redis, event loop blocked on BRPOP`
  - test_type: integration (Redis + HTTP mock)
- 2.2 PENDING
  - target: `src/http/handlers/webhooks.zig:handleApprovalCallback`
  - input: `Slack interactive payload with action_id and decision="approve"`
  - expected: `Redis LPUSH gate:response:{action_id} with "approve", HTTP 200 to Slack`
  - test_type: integration (Redis)
- 2.3 PENDING
  - target: `src/zombie/approval_gate.zig:waitForDecision`
  - input: `BRPOP returns "deny"`
  - expected: `GateResult.Denied, event loop resumes with denial context passed to agent`
  - test_type: integration (Redis)
- 2.4 PENDING
  - target: `src/zombie/approval_gate.zig:waitForDecision`
  - input: `BRPOP timeout (1 hour, no human response)`
  - expected: `GateResult.TimedOut (default-deny), action blocked, activity logged`
  - test_type: integration (Redis, shortened timeout for test)

---

## 3.0 Anomaly Detection (Auto-Kill)

**Status:** PENDING

In-band anomaly detector that runs before the approval gate check. Maintains a sliding window counter per (zombie_id, tool, action) tuple in Redis (`INCR + EXPIRE`). If count exceeds threshold within window, auto-kill: the Zombie is paused immediately, the in-progress action is cancelled, and an alert is posted to the configured Slack channel. No human approval prompt — the pattern is dangerous enough to kill without asking.

The CEO plan's example: "847 charges in 3 minutes → auto-kill." This catches hallucination retry loops where each individual action is legitimate but the pattern is pathological.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `src/zombie/approval_gate.zig:checkAnomaly`
  - input: `tool="stripe", action="create_charge", window counter at 9/10 threshold`
  - expected: `AnomalyResult.Normal (under threshold)`
  - test_type: unit (mocked Redis)
- 3.2 PENDING
  - target: `src/zombie/approval_gate.zig:checkAnomaly`
  - input: `tool="stripe", action="create_charge", window counter at 11/10 threshold`
  - expected: `AnomalyResult.AutoKill, Zombie paused, alert posted to Slack, activity: "Auto-killed: 11 create_charge calls in 60s (threshold: 10)"`
  - test_type: integration (Redis + HTTP mock)
- 3.3 PENDING
  - target: `src/zombie/approval_gate.zig:checkAnomaly`
  - input: `different actions interleaved (charge, refund, charge, refund) — 5 each in 60s, threshold 10 per action`
  - expected: `AnomalyResult.Normal for each (per-action counting, not aggregate)`
  - test_type: unit (mocked Redis)
- 3.4 PENDING
  - target: `src/zombie/approval_gate.zig:resetAnomalyCounter`
  - input: `Zombie restarted after auto-kill`
  - expected: `Counters cleared, Zombie starts fresh`
  - test_type: unit

---

## 4.0 Event Loop Integration

**Status:** PENDING

Wire the approval gate into the event loop's tool invocation path. Before any tool execution, the event loop calls `evaluateGate()` → if `RequiresApproval`, calls `requestApproval()` → blocks → resumes based on decision. The gate check is inside `deliverEvent`, between "agent decided to call tool X" and "tool X actually executes." This is the enforcement point — the agent cannot bypass the gate.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `src/zombie/event_loop.zig:deliverEvent`
  - input: `agent calls git.push(branch="main"), gate policy requires approval for push to main`
  - expected: `Execution pauses, Slack approval sent, tool call does NOT execute until approved`
  - test_type: integration (Redis + Executor)
- 4.2 PENDING
  - target: `src/zombie/event_loop.zig:deliverEvent`
  - input: `agent calls slack.react(), no gate rule matches`
  - expected: `Tool executes immediately, no approval prompt, action logged silently`
  - test_type: integration (Executor)
- 4.3 PENDING
  - target: `src/zombie/event_loop.zig:deliverEvent`
  - input: `gate fires auto-kill due to anomaly`
  - expected: `Zombie status set to "paused", current event processing cancelled, no more events claimed`
  - test_type: integration (Redis + DB)
- 4.4 PENDING
  - target: `src/zombie/event_loop.zig:deliverEvent`
  - input: `approval denied, agent receives denial context`
  - expected: `Agent gets ToolResult with success=false and reason="Action denied by operator: push to main", agent can decide next steps`
  - test_type: integration (Executor)

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 Public Functions

```zig
// src/zombie/approval_gate.zig
pub const GatePolicy = struct {
    rules: []GateRule,
    anomaly_rules: []AnomalyRule,
};

pub const GateRule = struct {
    tool: []const u8,          // tool name or "*"
    action: []const u8,        // action name or "*"
    condition: ?[]const u8,    // optional expression (e.g., "branch == 'main'")
    behavior: GateBehavior,    // approve | auto_kill
};

pub const AnomalyRule = struct {
    pattern: []const u8,       // "same_action"
    threshold_count: u32,      // e.g., 10
    threshold_window_s: u32,   // e.g., 60
    behavior: GateBehavior,    // auto_kill
};

pub const GateBehavior = enum { approve, auto_kill };

pub const GateDecision = enum { auto_approve, requires_approval, auto_kill };

pub const GateResult = enum { approved, denied, timed_out, auto_killed };

pub fn parseGatePolicy(alloc: Allocator, gates_json: []const u8) !GatePolicy
pub fn evaluateGate(policy: GatePolicy, tool: []const u8, action: []const u8, context: std.json.Value) GateDecision
pub fn checkAnomaly(redis: *Redis, zombie_id: []const u8, tool: []const u8, action: []const u8, rules: []AnomalyRule) !AnomalyResult
pub fn requestApproval(alloc: Allocator, redis: *Redis, slack_tool: *SlackTool, zombie_name: []const u8, action_detail: ActionDetail) ![]const u8  // returns action_id
pub fn waitForDecision(redis: *Redis, action_id: []const u8, timeout_ms: u64) !GateResult
pub fn resetAnomalyCounter(redis: *Redis, zombie_id: []const u8) !void
```

### 5.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `gate_rule.tool` | Text | Known tool name or `"*"` | `"git"` |
| `gate_rule.action` | Text | Tool-specific action or `"*"` | `"push"` |
| `gate_rule.condition` | ?Text | Simple expression, max 256 chars | `"branch == 'main'"` |
| `anomaly_rule.threshold_count` | u32 | 1-10000 | `10` |
| `anomaly_rule.threshold_window_s` | u32 | 1-86400 | `60` |
| `approval_callback.action_id` | Text | UUID, must exist in Redis | `"019abc..."` |
| `approval_callback.decision` | Enum | `"approve"` or `"deny"` | `"approve"` |

### 5.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `gate_decision` | Enum | Every tool invocation | `auto_approve` / `requires_approval` / `auto_kill` |
| `gate_result` | Enum | After approval flow | `approved` / `denied` / `timed_out` / `auto_killed` |
| `action_detail.summary` | Text | In Slack message | `"Push 3 files to main (zombie/fix-null-check)"` |
| `anomaly_alert.message` | Text | On auto-kill | `"Auto-killed: 11 create_charge in 60s (threshold: 10)"` |

### 5.4 Error Contracts

| Error condition | Code | Developer sees | HTTP |
|----------------|------|---------------|------|
| Invalid gate policy syntax | `UZ-GATE-001` | "Gate policy parse error in TRIGGER.md: {detail}" | -- |
| Approval callback for unknown action | `UZ-GATE-002` | "Action {id} not found or already resolved" | 404 |
| Approval callback invalid signature | `UZ-WH-010` | "Slack signature verification failed" | 401 |
| Redis unavailable during gate | `UZ-GATE-003` | "Gate service unavailable — default-deny applied" | -- |
| Condition expression invalid | `UZ-GATE-004` | "Gate condition '{expr}' is not valid. Supported: field == 'value', field != 'value'" | -- |

---

## 6.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Redis down during approval | Network partition | Default-deny, action blocked, logged | Activity: "Gate unavailable — action denied (safe default)" |
| Slack API down during approval | Slack outage | Retry 3x, then default-deny | Activity: "Could not send approval request — action denied" |
| Human never responds | Distracted, AFK | Timeout after configured period (default 1h) → deny | Activity: "Approval timed out after 1h — action denied" |
| Double-click on approve/deny | Slack button clicked twice | Second click gets 200 with "already resolved" | Slack: ephemeral "Already resolved" |
| Zombie killed while waiting for approval | Kill switch used | Pending approval cancelled, Redis key cleaned | Activity: "Zombie killed — pending approval cancelled" |
| Gate condition references unknown field | Bad TRIGGER.md | Zombie fails to start, error logged | CLI: "Gate condition references unknown field '{field}'" |

**Platform constraints:**
- Slack interactive message response must arrive within 3 seconds (respond immediately, process async)
- Redis BRPOP timeout cannot exceed 24 hours (Redis limitation)
- Anomaly counters use INCR + EXPIRE — if Redis restarts, counters reset (acceptable: fail-open for anomaly, fail-closed for approval)

---

## 7.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| approval_gate.zig < 500 lines | `wc -l` |
| Default-deny on all failure paths (Redis down, Slack down, timeout) | Code review + unit tests for each failure path |
| Anomaly check runs before approval check (fast path) | Code path: checkAnomaly() → evaluateGate() → requestApproval() |
| Gate condition evaluator supports only == and != (no code execution) | Unit test with injection attempts |
| No credentials in Slack approval messages | Grep approval message templates for credential patterns |
| Cross-compiles on x86_64-linux, aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| drain() before deinit() on all pg query results | `make check-pg-drain` |
| All gate decisions logged to activity stream | Integration test: every GateResult variant produces an activity event |

---

## 8.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| parse_gate_policy | 1.1 | approval_gate.zig:parseGatePolicy | 3 rules + 1 anomaly | GatePolicy struct |
| evaluate_gate_match | 1.2 | approval_gate.zig:evaluateGate | push to main | RequiresApproval |
| evaluate_gate_condition_miss | 1.3 | approval_gate.zig:evaluateGate | post to source channel | AutoApprove |
| evaluate_gate_no_match | 1.4 | approval_gate.zig:evaluateGate | react emoji | AutoApprove |
| anomaly_under_threshold | 3.1 | approval_gate.zig:checkAnomaly | 9/10 | Normal |
| anomaly_per_action_count | 3.3 | approval_gate.zig:checkAnomaly | interleaved actions | Normal per action |
| anomaly_reset | 3.4 | approval_gate.zig:resetAnomalyCounter | after restart | Counters cleared |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| approval_flow_approve | 2.1+2.2 | Redis + HTTP mock | Slack approve click | Tool executes |
| approval_flow_deny | 2.3 | Redis | deny decision | Tool blocked |
| approval_flow_timeout | 2.4 | Redis | no response | Default-deny |
| anomaly_auto_kill | 3.2 | Redis + HTTP mock | 11/10 threshold | Zombie paused |
| gate_in_event_loop_approve | 4.1 | Redis + Executor | push to main + approve | Push executes |
| gate_in_event_loop_silent | 4.2 | Executor | react (no rule) | Immediate execute |
| gate_in_event_loop_kill | 4.3 | Redis + DB | anomaly triggered | Zombie paused |
| gate_denial_context | 4.4 | Executor | deny + agent continues | Agent gets denial reason |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| "pauses execution on high-stakes action" | gate_in_event_loop_approve | integration |
| "Slack message with [Approve] [Deny]" | approval_flow_approve | integration |
| "on timeout: default-deny" | approval_flow_timeout | integration |
| "anomaly patterns trigger auto-kill" | anomaly_auto_kill | integration |
| "normal actions pass silently" | gate_in_event_loop_silent | integration |
| "2-3 approvals per day, not every action" | evaluate_gate_no_match (most actions auto-approve) | unit |
| "agent receives denial context" | gate_denial_context | integration |

---

## 9.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Implement parseGatePolicy (parse gates: from TRIGGER.md JSON) | Unit tests 1.1 pass |
| 2 | Implement evaluateGate (rule matching + condition evaluation) | Unit tests 1.2, 1.3, 1.4 pass |
| 3 | Implement checkAnomaly (sliding window counter in Redis) | Unit tests 3.1, 3.3, 3.4 pass |
| 4 | Implement requestApproval (Slack Block Kit message + Redis serialization) | Integration test 2.1 pass |
| 5 | Implement handleApprovalCallback (new webhook endpoint) | Integration test 2.2 pass |
| 6 | Implement waitForDecision (BRPOP with timeout) | Integration tests 2.3, 2.4 pass |
| 7 | Wire gate into event loop (between agent decision and tool execution) | Integration tests 4.1-4.4 pass |
| 8 | Add gates: section to slack-bug-fixer TRIGGER.md template | Template updated, parseable |
| 9 | Cross-compile check | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| 10 | Full test suite | `make test && make test-integration && make lint` |

---

## 10.0 Acceptance Criteria

**Status:** PENDING

- [ ] Gate fires on push-to-main with Slack approval message — verify: integration test
- [ ] Gate auto-approves non-matching actions silently — verify: unit test
- [ ] Approve button unblocks tool execution — verify: integration test
- [ ] Deny button blocks tool, agent gets denial context — verify: integration test
- [ ] Timeout defaults to deny — verify: integration test
- [ ] Anomaly detection auto-kills Zombie on threshold breach — verify: integration test
- [ ] All gate decisions logged to activity stream — verify: integration test
- [ ] Default-deny on Redis/Slack failure — verify: unit tests for each failure path
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compile passes for both targets
- [ ] `make check-pg-drain` passes
- [ ] All new files < 500 lines

---

## 11.0 Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| 500L gate | `wc -l` on new files | | |
| pg-drain | `make check-pg-drain` | | |

---

## 12.0 Out of Scope

- Approval via channels other than Slack (email, SMS, phone push — deferred per CEO plan)
- Complex condition expressions (only == and != for v1; full expression language is future)
- Per-user approval permissions (any team member can approve/deny for now)
- Approval audit for compliance export (M13+ when enterprise comes)
- Chained approvals (sequential multi-approver — not needed yet)
- Approval for first-time endpoints (mentioned in CEO plan — deferred to M6 Firewall Policy Engine)
