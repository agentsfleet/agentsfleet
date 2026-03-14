# M9_002: Agent Score Persistence And Leaderboard API

**Prototype:** v1.0.0
**Milestone:** M9
**Workstream:** 002
**Date:** Mar 13, 2026
**Status:** PENDING
**Priority:** P0 — exposes scores to CLI and UI; required before feedback injection (M9_003)
**Batch:** B1 — parallel with M9_001 schema work; depends on M9_001 event contract
**Depends on:** M9_001 (scoring engine + event schema)

---

## 0.0 Pre-Requisite Migration: agent_id Rename

**Status:** PENDING

Before M9 tables are created, rename `agent_profiles.profile_id` → `agent_id` across all existing tables and FK references. This is a separate migration (016) isolated from M9 additions.

**Tables affected:**
- `agent_profiles`: `profile_id` → `agent_id` (PK)
- `agent_profile_versions`: `profile_id` → `agent_id` (FK)
- `workspace_active_profile`: `profile_version_id` → `config_version_id` (FK)
- `profile_compile_jobs`: `requested_profile_id` → `requested_agent_id` (FK)
- `agent_profile_versions`: `profile_version_id` → `config_version_id` (PK); table rename to `agent_config_versions`
- `entitlement_policy_audit_snapshots`: `profile_version_id` → `config_version_id`
- `profile_linkage_audit_artifacts`: `profile_version_id` → `config_version_id`

**Source code affected:** All Zig files referencing `profile_id` in harness control plane handlers, entitlements, and profile resolver. Grep for `"profile_id"` in `src/` to find all locations.

---

## 1.0 Data Model

**Status:** PENDING

Persist scores in the workspace data model using UUIDv7 keys (M8_001 contract).

**Dimensions:**
- 1.1 PENDING Add `agent_run_scores` table:
  ```sql
  CREATE TABLE agent_run_scores (
      score_id         UUID PRIMARY KEY,
      run_id           UUID NOT NULL REFERENCES runs(run_id),
      agent_id         UUID NOT NULL REFERENCES agent_profiles(agent_id),
      workspace_id     UUID NOT NULL REFERENCES workspaces(workspace_id),
      score            INTEGER NOT NULL CHECK (score >= 0 AND score <= 100),
      tier             TEXT NOT NULL CHECK (tier IN ('BRONZE', 'SILVER', 'GOLD', 'ELITE')),
      axis_scores      TEXT NOT NULL,   -- JSON: {"completion":95,"error_rate":80,"latency":70,"resource":50}
      weight_snapshot  TEXT NOT NULL,   -- JSON: {"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}
      scored_at        BIGINT NOT NULL,
      UNIQUE (run_id),
      CONSTRAINT ck_agent_run_scores_uuidv7 CHECK (substring(score_id::text from 15 for 1) = '7')
  );
  CREATE INDEX idx_agent_run_scores_agent ON agent_run_scores(agent_id, scored_at DESC);
  CREATE INDEX idx_agent_run_scores_workspace ON agent_run_scores(workspace_id, score DESC);
  ```
- 1.2 PENDING Add columns to existing `agent_profiles` table:
  ```sql
  ALTER TABLE agent_profiles
      ADD COLUMN current_tier TEXT DEFAULT 'UNRANKED',
      ADD COLUMN lifetime_runs INTEGER NOT NULL DEFAULT 0,
      ADD COLUMN lifetime_score_avg NUMERIC(5,2) NOT NULL DEFAULT 0.0,
      ADD COLUMN consecutive_gold_plus_runs INTEGER NOT NULL DEFAULT 0,
      ADD COLUMN trust_level TEXT NOT NULL DEFAULT 'UNEARNED' CHECK (trust_level IN ('UNEARNED', 'TRUSTED')),
      ADD COLUMN last_scored_at BIGINT;
  ```
  Updated by the scoring persistence path after each scored run.
- 1.3 PENDING Add `workspace_latency_baseline` table:
  ```sql
  CREATE TABLE workspace_latency_baseline (
      workspace_id    UUID PRIMARY KEY REFERENCES workspaces(workspace_id),
      p50_seconds     BIGINT NOT NULL,
      p95_seconds     BIGINT NOT NULL,
      sample_count    INTEGER NOT NULL,
      computed_at     BIGINT NOT NULL
  );
  ```
- 1.4 PENDING Migration is additive (new tables + ALTER columns only); zero downtime; no existing table mutations beyond the pre-requisite rename
- 1.5 PENDING Indexes: `(agent_id, scored_at DESC)` for trajectory queries; `(workspace_id, score DESC)` for leaderboard queries
- 1.6 PENDING DB grants:
  ```sql
  GRANT SELECT, INSERT, UPDATE ON agent_run_scores TO worker_accessor;
  GRANT SELECT ON agent_run_scores TO api_accessor;
  GRANT SELECT, INSERT, UPDATE ON workspace_latency_baseline TO worker_accessor;
  GRANT SELECT ON workspace_latency_baseline TO api_accessor;
  ```
  Worker writes scores; API reads them. Agent_profiles grants already exist (schema 006).
- 1.7 PENDING Retention policy: `agent_run_scores` rows older than 365 days may be archived. Aggregate data on `agent_profiles` is sufficient for long-term trend analysis. Retention job is a follow-on, not M9 scope.

---

## 2.0 API Endpoints

**Status:** PENDING

Expose score data via the existing zombied HTTP API following current auth and error-code conventions.

**Dimensions:**
- 2.1 PENDING `GET /v1/agents/{agent_id}/scores?limit=50&cursor=` — paginated run score history, newest first; response includes `score_id`, `run_id`, `score`, `tier`, `axis_scores`, `weight_snapshot`, `scored_at` per entry. Returns `[]` if no scores exist.
- 2.2 PENDING `GET /v1/agents/{agent_id}/profile` — returns agent_profiles row: `agent_id`, `name`, `current_tier`, `lifetime_runs`, `lifetime_score_avg`, `consecutive_gold_plus_runs`, `trust_level`, `last_scored_at`. Returns 404 if agent_id not found.
- 2.3 PENDING `GET /v1/workspaces/{workspace_id}/leaderboard?limit=20` — top agents by `lifetime_score_avg` within workspace; returns `agent_id`, `name`, `current_tier`, `lifetime_score_avg`, `consecutive_gold_plus_runs`. Cached for 5 minutes at the handler level.
- 2.4 PENDING All three endpoints are read-only; require workspace-scoped auth token; workspace_id extracted from auth claims. Cross-workspace data leakage prevented by `WHERE workspace_id = $auth_workspace_id` on every query.

---

## 3.0 CLI Surface

**Status:** PENDING

Expose score data through `zombiectl` with structured and human-readable output.

**Dimensions:**
- 3.1 PENDING `zombiectl agent scores <agent-id> [--limit 20] [--json]` — prints score history table or JSON
- 3.2 PENDING `zombiectl agent profile <agent-id>` — prints current tier, trust level, streak, lifetime avg
- 3.3 PENDING `zombiectl workspace leaderboard [--json]` — prints workspace leaderboard table

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 Score row written within 2 seconds of run reaching terminal state (synchronous path, not outbox)
- [ ] 4.2 `agent_profiles` row reflects correct tier and streak after 10 sequential scored runs
- [ ] 4.3 Leaderboard returns correct ordering for a workspace with 5 agents and mixed scores
- [ ] 4.4 Cross-workspace isolation: agent in workspace A never appears in workspace B leaderboard
- [ ] 4.5 CLI commands return `--json` output parseable by `jq` with no extra prose
- [ ] 4.6 `agent_profiles.consecutive_gold_plus_runs` correctly excludes infrastructure failures (TIMEOUT, OOM, CONTEXT_OVERFLOW) from streak resets — only agent-attributable failures (BAD_OUTPUT, low score) break the streak (depends on M9_003 failure classification)
- [ ] 4.7 DB grants enforce worker-write / api-read separation

---

## 5.0 Out of Scope

- Global cross-workspace leaderboard (privacy concern, deferred)
- Score history charts or UI visualization (deferred to Mission Control v3)
- Score export / CSV download
- Agent display name management (uses agent_profiles.name for now)
- Retention job for old score rows (follow-on)
