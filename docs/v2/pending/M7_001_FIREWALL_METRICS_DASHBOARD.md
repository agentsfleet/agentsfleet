# M7_001: Firewall Metrics Dashboard — proof your agents are behaving

**Prototype:** v0.8.0
**Milestone:** M7
**Workstream:** 001
**Date:** Apr 09, 2026
**Status:** PENDING
**Priority:** P1 — Customer-facing proof of value; powers surfaces.md "AI Firewall Dashboard"
**Batch:** B3 — after M6 (firewall emits events that dashboard aggregates)
**Branch:** feat/m7-firewall-dashboard
**Depends on:** M6_001 (firewall events in activity stream)

---

## Overview

**Goal (testable):** `GET /v1/workspaces/{ws}/firewall/metrics` returns aggregated firewall metrics for a workspace: requests_proxied, credentials_injected, injections_blocked, policy_violations, anomaly_kills, domains_blocked, budget_kills, and a 7-day trust score trend. `GET /v1/workspaces/{ws}/firewall/events` returns paginated firewall events with filtering by type, tool, and time range. `zombiectl firewall` prints a summary. `zombiectl firewall blocked` shows blocked requests with details. The app dashboard (M12) consumes these APIs.

**Problem:** M6 logs every firewall decision to the activity stream, but there's no aggregation or query API. A customer can't answer "how many injections were blocked this week?" without scrolling raw events. The surfaces.md positions the Firewall Dashboard as a headline feature — "Not just logs — actionable security metrics." Without aggregation endpoints, the dashboard has nothing to render.

**Solution summary:** Add two new API endpoints: `/firewall/metrics` (aggregate counters from `core.activity_events` where `event_type LIKE 'firewall_%'`, grouped by type and time bucket) and `/firewall/events` (filtered, paginated query on firewall events). Add a materialized view or query for the 7-day trust score (ratio of allowed/total requests, weighted by severity). Add two CLI commands: `zombiectl firewall` (summary) and `zombiectl firewall blocked` (details). All data comes from existing `core.activity_events` — no new tables.

---

## 1.0 Metrics Aggregation API

**Status:** PENDING

`GET /v1/workspaces/{ws}/firewall/metrics?period=7d` returns aggregated counters. Metrics are computed from `core.activity_events` with `event_type IN ('firewall_allow', 'firewall_block', 'firewall_injection', 'firewall_flag', 'firewall_approval')`. Period supports: `1h`, `24h`, `7d`, `30d`. Response includes total counts and time-bucketed series (hourly for 1h/24h, daily for 7d/30d).

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/http/handlers/firewall_metrics.zig:handleGetMetrics`
  - input: `GET /v1/workspaces/{ws}/firewall/metrics?period=7d with 100 events in DB`
  - expected: `JSON with requests_proxied, injections_blocked, policy_violations, domains_blocked, trust_score, daily_series[]`
  - test_type: integration (DB)
- 1.2 PENDING
  - target: `src/http/handlers/firewall_metrics.zig:computeTrustScore`
  - input: `70 allowed, 20 blocked, 10 flagged`
  - expected: `trust_score = 0.70 (allowed / total), trend = [daily scores for 7 days]`
  - test_type: unit
- 1.3 PENDING
  - target: `src/http/handlers/firewall_metrics.zig:handleGetMetrics`
  - input: `period=24h, no events in time range`
  - expected: `All counters = 0, trust_score = 1.0 (no bad events = full trust), empty series`
  - test_type: integration (DB)
- 1.4 PENDING
  - target: `src/http/handlers/firewall_metrics.zig:handleGetMetrics`
  - input: `invalid period="99x"`
  - expected: `HTTP 400, error: "Invalid period. Supported: 1h, 24h, 7d, 30d"`
  - test_type: unit

---

## 2.0 Firewall Events API

**Status:** PENDING

`GET /v1/workspaces/{ws}/firewall/events?type=block&tool=slack&cursor=...&limit=50` returns paginated firewall events. Reuses `activity_stream.zig`'s cursor-based pagination. Filters: `type` (allow/block/injection/flag/approval), `tool` (slack/git/github/etc), `zombie_id`, `after` (timestamp). Default: all types, all tools, last 24h, limit 50.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/http/handlers/firewall_events.zig:handleGetEvents`
  - input: `GET /v1/workspaces/{ws}/firewall/events?type=block&limit=10`
  - expected: `JSON array of up to 10 firewall_block events, cursor for next page`
  - test_type: integration (DB)
- 2.2 PENDING
  - target: `src/http/handlers/firewall_events.zig:handleGetEvents`
  - input: `GET with cursor from previous page`
  - expected: `Next page of results, no duplicates with previous page`
  - test_type: integration (DB)
- 2.3 PENDING
  - target: `src/http/handlers/firewall_events.zig:handleGetEvents`
  - input: `GET with tool=slack filter`
  - expected: `Only events where tool_name = "slack", other tool events excluded`
  - test_type: integration (DB)
- 2.4 PENDING
  - target: `src/http/handlers/firewall_events.zig:handleGetEvents`
  - input: `GET with no events matching filter`
  - expected: `Empty array, no cursor, HTTP 200`
  - test_type: integration (DB)

---

## 3.0 CLI Commands

**Status:** PENDING

Two new CLI commands for firewall visibility from the terminal.

`zombiectl firewall` — summary view:
```
AI Firewall (last 24h)
  Requests proxied:      147
  Credentials injected:   89
  Injections blocked:      3
  Policy violations:       1
  Domains blocked:         0
  Trust score:          0.97 ↑
```

`zombiectl firewall blocked` — detail view:
```
Blocked requests (last 24h)
  1. [10:47] slack.post_message → evil.com — Domain not in allowlist
  2. [11:23] github.create_pr → api.github.com — Injection pattern detected
  3. [14:01] slack.read_message → api.slack.com — Endpoint policy: deny
```

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `zombiectl/src/commands/firewall.js:commandFirewall`
  - input: `zombiectl firewall` with API returning metrics
  - expected: `Formatted summary printed with all metric fields`
  - test_type: unit (mocked API)
- 3.2 PENDING
  - target: `zombiectl/src/commands/firewall.js:commandFirewall`
  - input: `zombiectl firewall blocked` with API returning 3 blocked events
  - expected: `Formatted list with timestamp, tool, target, reason for each`
  - test_type: unit (mocked API)
- 3.3 PENDING
  - target: `zombiectl/src/commands/firewall.js:commandFirewall`
  - input: `zombiectl firewall --json`
  - expected: `Raw JSON from API, no formatting`
  - test_type: unit (mocked API)

---

## 4.0 Interfaces

**Status:** PENDING

### 4.1 Public Functions

```zig
// src/http/handlers/firewall_metrics.zig
pub fn handleGetMetrics(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void

pub const FirewallMetrics = struct {
    requests_proxied: u64,
    credentials_injected: u64,
    injections_blocked: u64,
    policy_violations: u64,
    anomaly_kills: u64,
    domains_blocked: u64,
    budget_kills: u64,
    trust_score: f64,            // 0.0 - 1.0
    trust_trend: []DailyScore,   // 7-day array
    period: []const u8,
};

pub fn computeTrustScore(allowed: u64, total: u64) f64
pub fn aggregateMetrics(pool: *pg.Pool, workspace_id: []const u8, period: Period) !FirewallMetrics

// src/http/handlers/firewall_events.zig
pub fn handleGetEvents(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void
```

### 4.2 API Endpoints

```
GET /v1/workspaces/{ws}/firewall/metrics?period={1h|24h|7d|30d}
GET /v1/workspaces/{ws}/firewall/events?type={type}&tool={tool}&cursor={cursor}&limit={limit}
```

### 4.3 Error Contracts

| Error condition | Code | Developer sees | HTTP |
|----------------|------|---------------|------|
| Invalid period | `UZ-FW-010` | "Invalid period. Supported: 1h, 24h, 7d, 30d" | 400 |
| Workspace not found | `UZ-WS-001` | "Workspace not found" | 404 |
| Invalid cursor | `UZ-FW-011` | "Invalid cursor format" | 400 |

---

## 5.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Metrics query < 100ms for 10K events | Benchmark with seeded DB |
| Each handler file < 300 lines | `wc -l` |
| No new database tables (uses existing activity_events) | `ls schema/` — no new migration files |
| Trust score defaults to 1.0 when no events | Unit test |
| Cross-compiles | both targets |
| drain() before deinit() | `make check-pg-drain` |

---

## 6.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| trust_score_calc | 1.2 | computeTrustScore | 70/100 | 0.70 |
| trust_score_no_events | 1.3 | computeTrustScore | 0/0 | 1.0 |
| invalid_period | 1.4 | handleGetMetrics | "99x" | 400 |
| cli_summary | 3.1 | commandFirewall | metrics JSON | formatted output |
| cli_blocked | 3.2 | commandFirewall | events JSON | formatted list |
| cli_json | 3.3 | commandFirewall | --json flag | raw JSON |

### Integration Tests

| Test name | Dimension | Infra | Input | Expected |
|-----------|-----------|-------|-------|----------|
| metrics_7d | 1.1 | DB | 100 events | aggregated JSON |
| events_filter_type | 2.1 | DB | type=block | only blocks |
| events_pagination | 2.2 | DB | cursor | next page |
| events_filter_tool | 2.3 | DB | tool=slack | only slack |
| events_empty | 2.4 | DB | no matches | empty array |

---

## 7.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Implement aggregateMetrics + computeTrustScore | Unit tests pass |
| 2 | Implement handleGetMetrics endpoint + register route | Integration test 1.1 pass |
| 3 | Implement handleGetEvents endpoint + register route | Integration tests 2.1-2.4 pass |
| 4 | Implement CLI commands (firewall, firewall blocked) | Unit tests 3.1-3.3 pass |
| 5 | Cross-compile + full suite | `make test && make lint` |

---

## 8.0 Acceptance Criteria

**Status:** PENDING

- [ ] `/firewall/metrics` returns correct aggregated counters — verify: integration test
- [ ] `/firewall/events` returns filtered, paginated events — verify: integration test
- [ ] `zombiectl firewall` prints formatted summary — verify: unit test
- [ ] `zombiectl firewall blocked` shows blocked details — verify: unit test
- [ ] Trust score = 1.0 when no events — verify: unit test
- [ ] Metrics query < 100ms for 10K events — verify: benchmark
- [ ] `make test && make lint` pass
- [ ] Cross-compile passes

---

## 9.0 Applicable Rules

RULE XCC (cross-compile check), RULE FLL (full lint gate), RULE ORP (cross-layer orphan sweep), RULE DRN (drain before deinit), RULE 350L (line-length gate).

---

## 9.1 Invariants

N/A — no compile-time guardrails.

---

## 9.2 Eval Commands

```bash
# E1: Build
zig build 2>&1 | head -5; echo "build=$?"

# E2: Tests
make test 2>&1 | tail -5; echo "test=$?"

# E3: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E4: Gitleaks
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E5: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'

# E6: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "xc_x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "xc_arm=$?"

# E7: Memory leak check
make check-pg-drain 2>&1 | tail -3; echo "drain=$?"
```

---

## 9.3 Dead Code Sweep

N/A — no files deleted.

---

## 9.4 Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| 350L gate | see E5 | | |
| drain check | `make check-pg-drain` | | |
| Gitleaks | `gitleaks detect` | | |

---

## 10.0 Out of Scope

- Real-time WebSocket streaming of firewall events (API polling is sufficient for v1)
- Historical trend storage beyond activity_events retention (30 days default)
- Custom metric dashboards or alerting rules
- Export to Grafana/Datadog (API is the interface — external tools can poll it)
- App dashboard UI rendering (M12 — this milestone ships the API)
