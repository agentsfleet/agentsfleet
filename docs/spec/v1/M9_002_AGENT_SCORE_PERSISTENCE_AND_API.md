# M9_002: Agent Score Persistence And Leaderboard API

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 002
**Date:** Mar 13, 2026
**Status:** DONE
**Priority:** P0 — exposes scores to CLI and UI; required before feedback injection (M9_003)
**Batch:** B1 — parallel with M9_001 schema work; depends on M9_001 event contract
**Depends on:** M9_001 (scoring engine + event schema)

---

## 0.0 Pre-Requisite Rename

**Status:** DONE

M9_002 depends on the canonical naming defined in [M9_000](./M9_000_AGENT_ID_AND_CONFIG_VERSION_RENAME.md). This workstream assumes the clean-state schema already uses `agent_id`, `config_version_id`, `workspace_active_config`, and `config_compile_jobs`.

No separate rename migration is part of M9_002.

---

## 1.0 Data Model

**Status:** DONE

Persist raw deterministic score outputs in the workspace data model using UUIDv7 keys (M8_001 contract). Do not persist derived labels such as tiers or trust levels in Postgres.

**Dimensions:**
- 1.1 DONE Add `agent_run_scores` table:
  ```sql
  CREATE TABLE agent_run_scores (
      score_id         UUID PRIMARY KEY,
      run_id           UUID NOT NULL REFERENCES runs(run_id),
      agent_id         UUID NOT NULL REFERENCES agent_profiles(agent_id),
      workspace_id     UUID NOT NULL REFERENCES workspaces(workspace_id),
      score            INTEGER NOT NULL CHECK (score >= 0 AND score <= 100),
      axis_scores      TEXT NOT NULL,   -- JSON: {"completion":95,"error_rate":80,"latency":70,"resource":50}
      weight_snapshot  TEXT NOT NULL,   -- JSON: {"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}
      scored_at        BIGINT NOT NULL,
      UNIQUE (run_id),
      CONSTRAINT ck_agent_run_scores_uuidv7 CHECK (substring(score_id::text from 15 for 1) = '7')
  );
  CREATE INDEX idx_agent_run_scores_agent ON agent_run_scores(agent_id, scored_at DESC);
  CREATE INDEX idx_agent_run_scores_workspace ON agent_run_scores(workspace_id, score DESC);
  ```
- 1.2 DONE Do not add score-derived columns to `agent_profiles`. Tiering, trust, and leaderboard-specific aggregates remain application concerns or follow-on work.
- 1.3 DONE Add `workspace_latency_baseline` table:
  ```sql
  CREATE TABLE workspace_latency_baseline (
      workspace_id    UUID PRIMARY KEY REFERENCES workspaces(workspace_id),
      p50_seconds     BIGINT NOT NULL,
      p95_seconds     BIGINT NOT NULL,
      sample_count    INTEGER NOT NULL,
      computed_at     BIGINT NOT NULL
  );
  ```
- 1.4 DONE Canonical schema is updated in-place for this unreleased project; no `ALTER TABLE`-based rename or score-aggregate expansion is introduced in M9_002
- 1.5 DONE Indexes: `(agent_id, scored_at DESC)` for trajectory queries; `(workspace_id, score DESC)` for leaderboard queries
- 1.6 DONE DB grants:
  ```sql
  GRANT SELECT, INSERT, UPDATE ON agent_run_scores TO worker_accessor;
  GRANT SELECT ON agent_run_scores TO api_accessor;
  GRANT SELECT, INSERT, UPDATE ON workspace_latency_baseline TO worker_accessor;
  GRANT SELECT ON workspace_latency_baseline TO api_accessor;
  ```
  Worker writes scores; API reads them. Agent_profiles grants already exist (schema 006).
- 1.7 PENDING Retention policy: `agent_run_scores` rows older than 365 days may be archived. Retention automation is a follow-on, not M9 scope.

---

## 2.0 API Endpoints

**Status:** DONE

Expose score data via the existing zombied HTTP API following current auth and error-code conventions.

**Dimensions:**
- 2.1 DONE `GET /v1/agents/{agent_id}/scores?limit=50&cursor=` — paginated run score history, newest first; response includes `score_id`, `run_id`, `score`, `axis_scores`, `weight_snapshot`, `scored_at` per entry. Returns `[]` if no scores exist.
- 2.2 DONE `GET /v1/agents/{agent_id}` — returns base agent metadata from `agent_profiles` such as `agent_id`, `name`, `status`, `created_at`, and `updated_at`. Returns 404 if agent_id not found.
- 2.3 PENDING Workspace leaderboards and derived tiers are deferred until a separate aggregation design is approved.
- 2.4 DONE All endpoints are read-only; require workspace-scoped auth token; workspace_id resolved from agent_profiles and enforced via `WHERE workspace_id = $auth_workspace_id` on every query.

---

## 3.0 CLI Surface

**Status:** DONE

Expose score data through `zombiectl` with structured and human-readable output.

**Dimensions:**
- 3.1 DONE `zombiectl agent scores <agent-id> [--limit 20] [--json]` — prints score history table or JSON
- 3.2 DONE `zombiectl agent profile <agent-id>` — prints base agent metadata
- 3.3 PENDING Workspace leaderboard CLI is deferred with the API

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 Score row written within 2 seconds of run reaching terminal state (synchronous path, not outbox)
- [x] 4.2 Score persistence writes exactly one row per run and remains idempotent on duplicate scoring attempts
- [x] 4.3 Stored score payload preserves raw `axis_scores` and `weight_snapshot` JSON for later API reads
- [x] 4.4 Cross-workspace isolation: agent scores filtered by `workspace_id` resolved from `agent_profiles`; cross-workspace data leakage prevented
- [x] 4.5 CLI commands return `--json` output parseable by `jq` with no extra prose
- [x] 4.6 DB grants enforce worker-write / api-read separation

---

## 5.0 Out of Scope

- Global cross-workspace leaderboard (privacy concern, deferred)
- Score history charts or UI visualization (deferred to Mission Control v3)
- Score export / CSV download
- Tiering, trust levels, and leaderboard aggregates
- Agent display name management (uses agent_profiles.name for now)
- Retention job for old score rows (follow-on)
