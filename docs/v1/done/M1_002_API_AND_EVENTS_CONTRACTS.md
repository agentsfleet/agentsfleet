# M1_002: API and Events Contracts

Date: Mar 2, 2026 (updated Mar 3, 2026)
Status: IN PROGRESS — code written, 4 gaps block AC sign-off (see Gap Analysis below)
Depends on: M1_000 (control plane + NullClaw baseline)

## Goal

Define the external and internal machine contracts for M1: Control APIs, artifact schema, and observability event envelope, with explicit alignment between `openapi.json`, `/agents` JSON-LD, and `/llms.txt`.

## Explicit Assumptions

1. HTTP+JSON is the canonical external interface for M1.
2. Operation IDs match product verbs: `start_run`, `get_run`, `retry_run`, `pause_workspace`, `list_specs`, `sync_specs`.
3. Event stream is append-only and at-least-once delivery; consumers must dedupe by event ID.
4. Artifact files are immutable per attempt and referenced by stable logical names.
5. JSON-LD and `llms.txt` are discovery layers and must never contradict OpenAPI.
6. M1 ingress modes are `web` and `api`; `chat` and `voice` modes are deferred to M2+.

## In-Scope

1. Endpoint contract definitions (request/response/status/idempotency/error model).
2. Artifact contract for run replay package.
3. Event envelope for transitions, failures, and cost visibility.
4. Contract alignment rules across OpenAPI, JSON-LD, and `llms.txt`.

## Out-of-Scope

1. SDK generation and language-specific client ergonomics.
2. WebSocket streaming contract.
3. Long-term event version migration strategy beyond v1.
4. Billing/event export formats to third-party BI platforms.
5. Chat/voice ingress payload contracts.

## Interfaces and Contracts

### 1) Control API (v1)

Base path: `/v1`

1. `POST /v1/runs`
operationId: `start_run`
Required fields: `workspace_id`, `spec_id`, `mode` (`web|api`), `requested_by`, `idempotency_key`
Success: `202 Accepted` with `run_id`, initial `state`, `attempt=1`

2. `GET /v1/runs/{run_id}`
operationId: `get_run`
Success: `200 OK` with `run`, `current_state`, `attempt`, `transitions[]`, `artifacts[]`, `policy_events[]`

3. `POST /v1/runs/{run_id}:retry`
operationId: `retry_run`
Required: `reason`, `retry_token`
Success: `202 Accepted` with incremented `attempt`

4. `POST /v1/workspaces/{workspace_id}:pause`
operationId: `pause_workspace`
Required: `pause` (bool), `reason`, `version`
Success: `200 OK` with new `version` and admission status

5. `GET /v1/specs`
operationId: `list_specs`
Filters: `workspace_id`, `status`, `cursor`, `limit`
Success: `200 OK` with deterministic sorted list + cursor

6. `POST /v1/workspaces/{workspace_id}:sync`
operationId: `sync_specs`
Required fields: (none beyond path param)
Success: `200 OK` with `synced_count`, `total_pending`, `specs[]` summary
Note: Scans `docs/spec/PENDING_*.md` in workspace repo, upserts spec records. Idempotent. Also called lazily by `start_run` if no specs exist for workspace.

### Error Response Contract (all endpoints)

All error responses use a consistent JSON envelope:

```json
{
  "error": {
    "code": "WORKSPACE_PAUSED",
    "message": "Workspace w_456 is paused. Resume before starting new runs.",
    "details": {
      "workspace_id": "w_456",
      "paused_at": "2026-03-02T10:00:00Z",
      "paused_by": "operator"
    }
  },
  "request_id": "req_abc123"
}
```

Fields:
1. `error.code` — stable machine-readable code (UPPER_SNAKE_CASE). Never changes once shipped.
2. `error.message` — human-readable explanation. May change between versions.
3. `error.details` — optional structured data relevant to the error. Schema varies by code.
4. `request_id` — trace correlation ID. Present on all responses (success and error).

### HTTP Status Codes

| Status | When |
|--------|------|
| `200 OK` | Successful read or update |
| `202 Accepted` | Run queued, retry queued |
| `400 Bad Request` | Missing/invalid fields, malformed JSON |
| `401 Unauthorized` | Missing or invalid API key |
| `404 Not Found` | Run, workspace, or spec does not exist |
| `409 Conflict` | Idempotency key reused with different params, or invalid state transition |
| `422 Unprocessable Entity` | Valid JSON but semantically invalid (e.g., retry on a DONE run) |
| `429 Too Many Requests` | Rate limit exceeded |
| `500 Internal Server Error` | Unhandled server error |
| `503 Service Unavailable` | Worker overloaded or DB unreachable |

### Error Codes (M1)

| Code | HTTP | Trigger |
|------|------|---------|
| `WORKSPACE_PAUSED` | 409 | `start_run` when workspace is paused |
| `WORKSPACE_NOT_FOUND` | 404 | Any operation with unknown workspace_id |
| `RUN_NOT_FOUND` | 404 | `get_run` or `retry_run` with unknown run_id |
| `SPEC_NOT_FOUND` | 404 | `start_run` with unknown spec_id |
| `INVALID_STATE_TRANSITION` | 422 | `retry_run` on a run not in retryable state |
| `RETRIES_EXHAUSTED` | 422 | `retry_run` when retry budget is spent |
| `IDEMPOTENCY_CONFLICT` | 409 | Same idempotency_key with different request body |
| `INVALID_REQUEST` | 400 | Missing required fields or malformed input |
| `UNAUTHORIZED` | 401 | Invalid or missing API key |
| `RATE_LIMITED` | 429 | Too many requests from this API key |
| `AGENT_TIMEOUT` | 500 | NullClaw agent exceeded wall-clock deadline |
| `AGENT_CRASH` | 500 | NullClaw agent returned unexpected error |
| `INTERNAL_ERROR` | 500 | Unhandled server error |

### 2) Artifact Contract (run replay package)

Canonical artifact names per run attempt:
1. `plan.json` — produced by NullClaw Echo
2. `implementation.md` — produced by NullClaw Scout
3. `validation.md` — produced by NullClaw Warden (includes tiered findings T1-T4)
4. `attempt_N_defects.md` (`N` is attempt number) — extracted from Warden's FAIL verdict
5. `run_summary.md` — produced by orchestrator at run completion

Artifact metadata fields:
1. `run_id`
2. `attempt`
3. `artifact_name`
4. `object_key` — M1: git path in feature branch (e.g., `docs/runs/<run_id>/plan.json`). M2+: S3-compatible object key.
5. `checksum_sha256`
6. `created_at`
7. `producer` (`echo|scout|warden|orchestrator`)

M1 storage model: artifacts are committed to the feature branch under `docs/runs/<run_id>/` and registered in the `artifacts` table with their git path as `object_key`. M2+ migrates to S3-compatible object storage; the `object_key` field format changes but the artifact index contract remains identical.

### 3) Event Envelope Contract

Minimum envelope:
```json
{
  "event_id": "evt_123",
  "timestamp": "2026-03-02T12:34:56Z",
  "tenant_id": "t_123",
  "workspace_id": "w_456",
  "run_id": "r_789",
  "attempt": 1,
  "actor": "warden",
  "event_type": "transition",
  "state_from": "VERIFICATION_IN_PROGRESS",
  "state_to": "VERIFICATION_FAILED",
  "reason_code": "MISSING_TESTS",
  "cost_tokens": 1432,
  "cost_runtime_seconds": 38
}
```

Required event types in M1:
1. `transition`
2. `policy_decision`
3. `validation_result`
4. `cost_snapshot`
5. `notification_sent`

Optional diagnostic event type:
1. `tool_execution` (bounded verbosity, sampled as needed)

### 4) OpenAPI + JSON-LD + llms.txt Alignment Contract

1. `public/openapi.json` is canonical for endpoint signatures.
2. `/agents` must embed JSON-LD that references the same API base URL and operation IDs.
3. `public/llms.txt` must include canonical links to `/openapi.json`, `/agents`, `/docs`.
4. Any endpoint change requires same-PR updates to all three surfaces.

## Acceptance Criteria

1. OpenAPI defines exactly six required operations with operation IDs above.
2. Every operation has request/response/error schema and status code definitions.
3. Artifact contract includes all five required files with immutable metadata.
4. Event envelope supports lifecycle, failure, and cost fields in one schema.
5. `/agents` JSON-LD and `llms.txt` are verified to match OpenAPI paths and operation IDs.

## Risks and Mitigations

1. Risk: API drift between docs and implementation.
Mitigation: contract tests generated from OpenAPI and checked in CI.
2. Risk: discovery drift across `/agents` and `llms.txt`.
Mitigation: CI check that extracts operation IDs/paths from both and diffs against OpenAPI.
3. Risk: event cardinality explosion and cost.
Mitigation: constrain M1 event types and enforce sampling only on verbose tool telemetry.
4. Risk: replay artifacts missing checksums.
Mitigation: reject artifact registration without checksum.
5. Risk: NullClaw structured output format doesn't match expected artifact schema.
Mitigation: worker orchestrator validates artifact format before committing; fallback parsing layer.

## Test/Verification Commands

```bash
# Required section presence
rg -n "^## (Goal|Explicit|In-Scope|Out-of-Scope|Interfaces|Acceptance|Risks|Test)" docs/spec/v1/M1_002_API_AND_EVENTS_CONTRACTS.md

# Required API operations and artifact names
rg -n "start_run|get_run|retry_run|pause_workspace|list_specs|plan.json|implementation.md|validation.md|attempt_N_defects.md|run_summary.md" docs/spec/v1/M1_002_API_AND_EVENTS_CONTRACTS.md

# Verify no stale references
rg -n "Sprint 1|PI SDK|PI Agent|OpenClaw Gateway|Phase 2" docs/spec/v1/M1_002_API_AND_EVENTS_CONTRACTS.md && echo "FAIL: stale refs" || echo "PASS: clean"
```

## Gap Analysis (Oracle review Mar 3, 2026)

Code is written and compiles. The following gaps block acceptance sign-off. Each item maps to a specific acceptance criterion.

### Gap 1 — AC#5: `public/llms.txt` and `agent-manifest.json` not verified against OpenAPI [BLOCKER]

**What the AC says:** `/agents` JSON-LD and `llms.txt` are verified to match OpenAPI paths and operation IDs.

**What is missing:** `public/llms.txt` and `public/agent-manifest.json` are stale placeholders (acknowledged in KICKOFF step 9). They have not been regenerated to match `public/openapi.json` which defines all 6 operations. This is KICKOFF build step 9 (PUBLIC ASSETS).

**Fix required:**
- Regenerate `public/llms.txt` — must link `/openapi.json`, `/agents`, `/docs` and list all 6 operation IDs.
- Regenerate `public/agent-manifest.json` — JSON-LD must embed the same API base URL and all 6 operationIds from `openapi.json`.
- Regenerate `public/skill.md` — machine-readable skill definition for agent discovery.
- Any future endpoint change requires same-PR updates to all three surfaces.

---

### Gap 2 — AC#3: `run_summary.md` artifact is never produced [MAJOR]

**What the AC says:** Artifact contract includes all five required files: `plan.json`, `implementation.md`, `validation.md`, `attempt_N_defects.md`, `run_summary.md`.

**What is missing:** `src/pipeline/worker.zig` commits the first four artifacts but never produces or commits `run_summary.md`. `ArtifactName.run_summary_md` exists in `src/types.zig` but `commitArtifact` is never called for it.

**Fix required:**
- In `executeRun()` in `src/pipeline/worker.zig`, after the final `DONE` transition, produce a `run_summary.md` that includes: run_id, spec_id, final state, attempt count, PR URL, wall-clock duration, total token count (sum across all agents), list of artifacts produced.
- Register it via `commitArtifact` with `actor = .orchestrator`.

---

### Gap 3 — AC#2: `GET /v1/runs/{run_id}` response missing `artifacts[]` and `policy_events[]` [MAJOR]

**What the AC says:** `get_run` returns `run`, `current_state`, `attempt`, `transitions[]`, `artifacts[]`, `policy_events[]`.

**What is missing:** `handleGetRun` in `src/http/handler.zig` returns `transitions[]` but never queries the `artifacts` table or `policy_events` table. A consumer cannot reconstruct a complete run replay from the API response.

**Fix required:**
- After fetching transitions, query `artifacts WHERE run_id = $1 ORDER BY created_at ASC` and include `artifacts[]` in the response.
- Query `policy_events WHERE run_id = $1 ORDER BY ts ASC` and include `policy_events[]` in the response.
- Add both arrays to the `writeJson` call in `handleGetRun`.

---

### Gap 4 — AC#2: OpenAPI response schemas are stubs, not machine-readable [MINOR]

**What the AC says:** Every operation has request/response/error schema and status code definitions.

**What is missing:** Response bodies for 200/202 in `public/openapi.json` are `{"description": "OK"}` with no `content` schema. Request schemas are complete. Response body schemas are absent — consumers cannot generate typed clients.

**Fix required:**
- Add `content.application/json.schema` for each operation's success response:
  - `POST /v1/runs` 202: `{ run_id, state, attempt, request_id }`
  - `GET /v1/runs/{run_id}` 200: `{ run_id, workspace_id, spec_id, current_state, attempt, mode, requested_by, branch, pr_url, created_at, updated_at, transitions[], artifacts[], policy_events[] }`
  - `POST /v1/runs/{run_id}:retry` 202: `{ run_id, state, attempt, request_id }`
  - `POST /v1/workspaces/{workspace_id}:pause` 200: `{ workspace_id, paused, version, request_id }`
  - `GET /v1/specs` 200: `{ specs[], total, request_id }`
  - `POST /v1/workspaces/{workspace_id}:sync` 200: `{ synced_count, total_pending, specs[], request_id }`

---

### Implementation checklist (M1_002 only)

- [ ] Regenerate `public/llms.txt` with correct operation IDs and canonical links
- [ ] Regenerate `public/agent-manifest.json` with JSON-LD matching OpenAPI operation IDs
- [ ] Regenerate `public/skill.md`
- [ ] Produce and commit `run_summary.md` in worker at run completion
- [ ] Add `artifacts[]` query + response field to `GET /v1/runs`
- [ ] Add `policy_events[]` query + response field to `GET /v1/runs`
- [ ] Add response body schemas to `public/openapi.json` for all 6 operations
