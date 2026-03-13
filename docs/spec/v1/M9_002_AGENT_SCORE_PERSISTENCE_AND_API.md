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

## 1.0 Data Model

**Status:** PENDING

Persist scores in the workspace data model using UUIDv7 keys (M8_001 contract).

**Dimensions:**
- 1.1 PENDING Add `agent_run_scores` table: `id` (uuidv7), `run_id` (uuidv7 FK), `agent_id` (uuidv7 FK), `workspace_id` (uuidv7 FK), `score` (int 0–100), `tier` (enum), `axis_scores` (jsonb), `weight_snapshot` (jsonb — weights at time of scoring), `scored_at` (timestamptz)
- 1.2 PENDING Add `agent_profiles` table: `agent_id` (uuidv7 PK), `workspace_id`, `current_tier`, `lifetime_runs`, `lifetime_score_avg`, `streak_days` (consecutive days with at least one Gold+ run), `last_scored_at` — updated by outbox consumer after each score event
- 1.3 PENDING Migration is additive (new tables only); zero downtime; no existing table mutations
- 1.4 PENDING Index on `(agent_id, scored_at DESC)` for trajectory queries; index on `(workspace_id, score DESC)` for leaderboard queries

---

## 2.0 API Endpoints

**Status:** PENDING

Expose score data via the existing zombied HTTP API following current auth and error-code conventions.

**Dimensions:**
- 2.1 PENDING `GET /v1/agents/{agent_id}/scores?limit=50&cursor=` — paginated run score history, newest first; response includes `score`, `tier`, `axis_scores`, `scored_at` per entry
- 2.2 PENDING `GET /v1/agents/{agent_id}/profile` — returns `agent_profiles` row: current tier, lifetime stats, streak
- 2.3 PENDING `GET /v1/workspaces/{workspace_id}/leaderboard?limit=20` — top agents by `lifetime_score_avg` within workspace; returns agent_id, display name, tier, avg score, streak
- 2.4 PENDING All three endpoints are read-only; require workspace-scoped auth token; no cross-workspace data leakage

---

## 3.0 CLI Surface

**Status:** PENDING

Expose score data through `zombiectl` with structured and human-readable output.

**Dimensions:**
- 3.1 PENDING `zombiectl agent scores <agent-id> [--limit 20] [--json]` — prints score history table or JSON
- 3.2 PENDING `zombiectl agent profile <agent-id>` — prints current tier, streak, lifetime avg
- 3.3 PENDING `zombiectl workspace leaderboard [--json]` — prints workspace leaderboard

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 Score row written within 2 seconds of `agent.run.scored` event consumed from outbox
- [ ] 4.2 `agent_profiles` row reflects correct tier and streak after 10 sequential scored runs
- [ ] 4.3 Leaderboard returns correct ordering for a workspace with 5 agents and mixed scores
- [ ] 4.4 Cross-workspace isolation: agent in workspace A never appears in workspace B leaderboard
- [ ] 4.5 CLI commands return `--json` output parseable by `jq` with no extra prose

---

## 5.0 Out of Scope

- Global cross-workspace leaderboard (privacy concern, deferred)
- Score history charts or UI visualization (deferred to website milestone)
- Score export / CSV download
- Agent display name management (uses agent_id for now)
