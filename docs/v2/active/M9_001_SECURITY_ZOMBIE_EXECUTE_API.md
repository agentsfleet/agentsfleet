# M9_001: Security Zombie / Execute API — credential injection as a service for external agents

**Prototype:** v0.9.0
**Milestone:** M9
**Workstream:** 001
**Date:** Apr 09, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Expands TAM to any agent framework (LangGraph, CrewAI, OpenAI SDK)
**Batch:** B4 — after M6 (firewall inspects all requests), M4 (approval gate)
**Branch:** feat/m9-001-execute-api
**Depends on:** M6_001 (firewall policy engine), M4_001 (approval gate), M5_001 (tool bridge)

---

## Overview

**Goal (testable):** An external agent (LangGraph, CrewAI, or any HTTP client) calls `POST api.usezombie.com/v1/execute` with a target URL, method, and body. UseZombie: (1) checks the agent's API key, (2) evaluates the firewall policy (domain allowlist, endpoint rules, injection scan), (3) fires the approval gate if thresholds are met, (4) injects the credential for the target service from the vault, (5) makes the outbound call, (6) strips credential echo from response, (7) logs the full action to the activity stream, (8) returns the response to the agent. The agent never receives the credential. This is the Security Zombie — a packaged workspace that exposes the same trust layer as a first-party API endpoint.

**Problem:** First-party Zombies (Lead Zombie, Slack Bug Fixer) run inside UseZombie's sandbox. But many teams already have agents built with LangGraph, CrewAI, OpenAI Assistants, or custom frameworks. They can't move to UseZombie's sandbox overnight. The Execute API lets them keep their agent code and add UseZombie's trust layer incrementally: replace `requests.post("https://api.stripe.com/...", headers={"Authorization": stripe_key})` with `requests.post("https://api.usezombie.com/v1/execute", json={"target": "api.stripe.com/v1/charges", ...})`. Same functionality, but credentials hidden, request inspected, action logged.

**Solution summary:** New endpoint `POST /v1/execute` that accepts `{target, method, headers, body}`, resolves the workspace from the API key, applies the full firewall + approval gate pipeline, injects credentials, proxies the request, and returns the response. The endpoint reuses the tool bridge (M5), firewall (M6), and approval gate (M4) — no new trust infrastructure. A new `api_keys` table stores workspace API keys with scoped permissions. Rate limiting and budget enforcement apply per-workspace.

---

## 1.0 Execute Endpoint

**Status:** PENDING

`POST /v1/execute` — the core endpoint. Accepts a JSON body describing the outbound request. Authenticates via `Authorization: Bearer zmb_xxx` API key. Returns the proxied response with UseZombie metadata headers (`X-UseZombie-Action-Id`, `X-UseZombie-Firewall-Decision`).

```json
// Request
POST /v1/execute
Authorization: Bearer zmb_abc123
{
  "target": "api.stripe.com/v1/charges",
  "method": "POST",
  "headers": {"Content-Type": "application/x-www-form-urlencoded"},
  "body": "amount=4700&currency=usd",
  "credential_ref": "stripe"
}

// Response
{
  "status": 200,
  "headers": {"...": "..."},
  "body": {"id": "ch_xxx", "amount": 4700},
  "usezombie": {
    "action_id": "019abc...",
    "firewall_decision": "allow",
    "credential_injected": true,
    "approval_required": false
  }
}
```

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute with valid API key, target=api.stripe.com/v1/charges, credential_ref=stripe`
  - expected: `Credential fetched from vault, injected into outbound request, Stripe response returned with UZ metadata`
  - test_type: integration (vault + HTTP mock)
- 1.2 PENDING
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute with invalid API key`
  - expected: `HTTP 401, error code UZ-EXEC-001`
  - test_type: unit
- 1.3 PENDING
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute with target domain not in workspace allowlist`
  - expected: `HTTP 403, firewall blocks, error code UZ-FW-001, action logged`
  - test_type: integration (DB)
- 1.4 PENDING
  - target: `src/http/handlers/execute.zig:handleExecute`
  - input: `POST /v1/execute with body containing injection pattern`
  - expected: `HTTP 403, firewall blocks, error code UZ-FW-003, action logged`
  - test_type: unit

---

## 2.0 API Key Management

**Status:** PENDING

Workspace API keys for programmatic access. Keys are prefixed `zmb_` for easy identification. Each key is scoped to a workspace and has configurable permissions (execute, read_metrics, manage). Keys are hashed (SHA-256) before storage — raw key shown only once at creation. Keys can be created via CLI (`zombiectl api-key create`) or the app dashboard (M12).

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/http/handlers/api_keys.zig:handleCreateKey`
  - input: `POST /v1/workspaces/{ws}/api-keys with name="external-agent", permissions=["execute"]`
  - expected: `Key created, raw key returned once: "zmb_xxx", hash stored in DB`
  - test_type: integration (DB)
- 2.2 PENDING
  - target: `src/http/handlers/api_keys.zig:authenticateApiKey`
  - input: `Authorization: Bearer zmb_xxx`
  - expected: `Hash computed, matched against DB, workspace_id resolved`
  - test_type: integration (DB)
- 2.3 PENDING
  - target: `src/http/handlers/api_keys.zig:authenticateApiKey`
  - input: `Authorization: Bearer zmb_revoked_key`
  - expected: `HTTP 401, no match in DB`
  - test_type: integration (DB)
- 2.4 PENDING
  - target: `schema/026_core_api_keys.sql`
  - input: `CREATE TABLE with id, workspace_id FK, name, key_hash, permissions, created_at, last_used_at`
  - expected: `Table created, unique constraint on key_hash`
  - test_type: integration (DB)

---

## 3.0 Execute Pipeline (Reuse Existing Trust Infrastructure)

**Status:** PENDING

The execute endpoint reuses the same pipeline as first-party Zombies: firewall inspect → approval gate check → credential inject → outbound call → content scan → response return. The only difference: instead of an NullClaw agent triggering tool calls, an external HTTP request triggers the pipeline directly. This proves the architecture: one trust layer, two entry points (sandbox Zombie, API).

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `src/http/handlers/execute.zig:executePipeline`
  - input: `target triggers approval gate (e.g., POST to refund endpoint)`
  - expected: `Execution pauses, Slack approval sent, resumes on approve`
  - test_type: integration (Redis + HTTP mock)
- 3.2 PENDING
  - target: `src/http/handlers/execute.zig:executePipeline`
  - input: `target triggers anomaly detection (10th call in 60s)`
  - expected: `Auto-kill: API key rate-limited, 429 returned, activity logged`
  - test_type: integration (Redis)
- 3.3 PENDING
  - target: `src/http/handlers/execute.zig:executePipeline`
  - input: `response from target API contains credential echo`
  - expected: `Credential stripped from response body before returning to caller`
  - test_type: unit
- 3.4 PENDING
  - target: `src/http/handlers/execute.zig:executePipeline`
  - input: `workspace budget exceeded`
  - expected: `HTTP 402, error code UZ-ZMB-001, action NOT executed`
  - test_type: integration (DB)

---

## 4.0 CLI Commands

**Status:** PENDING

```
zombiectl api-key create --name "my-agent"     → prints zmb_xxx (once)
zombiectl api-key list                          → name, permissions, last_used, created
zombiectl api-key revoke <name>                 → revokes key
```

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `zombiectl/src/commands/api_key.js:commandApiKey`
  - input: `zombiectl api-key create --name "my-agent"`
  - expected: `API called, raw key printed once with warning "Save this key — you won't see it again"`
  - test_type: unit (mocked API)
- 4.2 PENDING
  - target: `zombiectl/src/commands/api_key.js:commandApiKey`
  - input: `zombiectl api-key list`
  - expected: `Table of keys with name, permissions, last_used (key hash NOT shown)`
  - test_type: unit (mocked API)

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 API Endpoints

```
POST /v1/execute                               — proxied outbound call
POST /v1/workspaces/{ws}/api-keys              — create API key
GET  /v1/workspaces/{ws}/api-keys              — list API keys
DELETE /v1/workspaces/{ws}/api-keys/{key_id}   — revoke API key
```

### 5.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `target` | Text | Domain must be in workspace allowlist | `"api.stripe.com/v1/charges"` |
| `method` | Text | GET/POST/PUT/PATCH/DELETE | `"POST"` |
| `headers` | Object | Optional custom headers (auth headers stripped) | `{"Content-Type": "application/json"}` |
| `body` | Text/Object | Request body for POST/PUT/PATCH | `{"amount": 4700}` |
| `credential_ref` | Text | Name of credential in vault | `"stripe"` |

### 5.3 Error Contracts

| Error condition | Code | Developer sees | HTTP |
|----------------|------|---------------|------|
| Invalid API key | `UZ-EXEC-001` | "Invalid API key. Create one with: zombiectl api-key create" | 401 |
| Key lacks execute permission | `UZ-EXEC-002` | "API key lacks 'execute' permission" | 403 |
| Domain not allowed | `UZ-FW-001` | "Domain '{domain}' not in workspace allowlist" | 403 |
| Credential not found | `UZ-TOOL-001` | "Credential '{ref}' not found. Add with: zombiectl credential add {ref}" | 404 |
| Budget exceeded | `UZ-ZMB-001` | "Workspace budget exceeded" | 402 |
| Target API error | `UZ-EXEC-003` | "Target API returned {status}: {body}" | 502 |
| Approval timeout | `UZ-GATE-005` | "Approval timed out — action denied" | 408 |

---

## 6.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Target API unreachable | DNS/network failure | Timeout after 30s, error logged | 502 with UZ-EXEC-003 |
| Approval pending, caller disconnects | External agent times out | Approval still pending in Slack, response lost | Agent must retry; pending approval visible in dashboard |
| Rate limit on execute | High-frequency API calls | Per-workspace rate limit (100/min default) | 429 with retry-after header |
| Large response body | Target returns > 10MB | Response truncated at 10MB | Truncated body + X-UseZombie-Truncated header |

---

## 7.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| execute.zig < 400 lines | `wc -l` |
| API key hash uses SHA-256 (not plaintext) | Code review + test |
| Raw API key shown exactly once (creation response only) | Code path analysis |
| Reuses firewall + approval gate (no duplicate trust logic) | Execute pipeline calls same functions as tool_bridge |
| Schema migration ≤ 100 lines | `wc -l` |
| Cross-compiles | both targets |

---

## 8.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Schema: api_keys table | `zig build` compiles |
| 2 | Implement API key create/auth/list/revoke | Tests 2.1-2.4 pass |
| 3 | Implement execute endpoint | Tests 1.1-1.4 pass |
| 4 | Wire execute pipeline through firewall + approval gate | Tests 3.1-3.4 pass |
| 5 | Implement CLI commands | Tests 4.1-4.2 pass |
| 6 | Full test suite | `make test && make test-integration && make lint` |

---

## 9.0 Acceptance Criteria

**Status:** PENDING

- [ ] `POST /v1/execute` with valid key + target → proxied response with metadata — verify: integration test
- [ ] Invalid API key → 401 — verify: unit test
- [ ] Domain not in allowlist → 403 — verify: integration test
- [ ] Injection in body → 403 — verify: unit test
- [ ] Approval gate fires for gated endpoints — verify: integration test
- [ ] Credential never in response — verify: unit test
- [ ] API key create shows raw key once — verify: unit test
- [ ] Budget enforcement → 402 — verify: integration test
- [ ] `make test && make lint` pass
- [ ] Cross-compile passes

---

## Applicable Rules

RULE XCC (cross-compile check), RULE FLL (350-line gate), RULE ORP (cross-layer orphan sweep), RULE FLS (drain all results), RULE FLL (350-line gate).

---

## Invariants

N/A — no compile-time guardrails.

---

## Eval Commands

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

## Dead Code Sweep

N/A — no files deleted.

---

## Verification Evidence

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

## Out of Scope

- SDK libraries (Python/JS wrappers around POST /v1/execute — future)
- Streaming responses (buffered only for v1)
- Batch execute (one request per call for v1)
- API key rotation (revoke + create new for v1)
- Webhook callbacks (fire-and-forget execution, response returned synchronously)
- GitHub Action integration (separate milestone — uses execute API under the hood)
- Custom rate limits per API key (workspace-level for v1)
