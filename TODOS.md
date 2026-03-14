# TODOS

Tracked deferred work items. Each has context, rationale, and priority so anyone
picking it up in 3 months understands the motivation and where to start.

---

## M9 Follow-On: Score/Analysis Table Retention Policy

**What:** Archive `agent_run_scores` and `agent_run_analysis` rows older than 365 days. Preserve aggregate data on `agent_profiles`.
**Why:** Tables grow linearly with runs. At 1000 runs/day = 365K rows/year per table. Without retention, query performance degrades and storage costs grow.
**Pros:** Keeps tables fast, reduces storage.
**Cons:** Small effort. Need to define archival format (cold table? S3 export?).
**Context:** Aggregate data (`agent_profiles.lifetime_score_avg`, `consecutive_gold_plus_runs`) is maintained on the profile row and survives archival. Historical drill-down into old scores would require archived data.
**Effort:** S
**Priority:** P3
**Depends on:** M9_002 shipping

---

## M9 Follow-On: Agent-Scoped Latency Baselines

**What:** Replace workspace-scoped latency baselines (p50/p95) with per-agent baselines when sufficient run volume exists (>50 runs per agent).
**Why:** Workspace baselines blend different agents with different performance profiles. A fast agent looks "average" in a workspace with a slow agent.
**Pros:** More accurate latency scoring per agent. Better trust evaluation.
**Cons:** More DB rows, more complex baseline computation.
**Context:** M9_001 ships with workspace-scoped baselines. This is sufficient for v1 where most workspaces have 1-2 agents. Upgrade when multi-agent workspaces are common.
**Effort:** S
**Priority:** P2
**Depends on:** M9_001 + sufficient run volume data

---

## M9 Follow-On: Resource Efficiency Axis Activation

**What:** Replace the stubbed resource axis (fixed score of 50) with real CPU/memory metrics from Firecracker sandbox.
**Why:** Resource efficiency is 10% of the quality score but currently meaningless. Activating it completes the 4-axis scoring model.
**Pros:** Full scoring model. Can optimize agents for cost-efficiency.
**Cons:** M effort. Need to define scoring formula (actual_usage / sandbox_limit) and test boundary conditions.
**Context:** Depends on M4_008 (Firecracker sandbox) providing per-execution CPU/memory metrics. Scoring engine already reserves the axis and weights. Formula TBD: linear degradation from 100 (used 0% of limit) to 0 (used 100%+).
**Effort:** M
**Priority:** P1 (after M4_008)
**Depends on:** M4_008 (Firecracker sandbox resource governance)

---

## M9 Delight: Score Badge in PR Description

**What:** Append agent quality badge to PR description body: "This PR was produced by a Gold-tier agent (87/100)."
**Why:** Social proof for PR reviewers. Shows quality at a glance without checking the dashboard.
**Pros:** Tiny effort, high visibility. Makes M9 scoring tangible to end users.
**Cons:** None significant. Could be noisy if scores are low.
**Context:** PR creation happens in `src/pipeline/worker_pr_flow.zig`. Score is available synchronously when the PR is created. Just append a markdown line to the PR body.
**Effort:** S (30 min)
**Priority:** P2
**Depends on:** M9_001 (scoring engine)

---

## M9 Delight: ASCII Sparkline in `zombiectl agent profile`

**What:** Show recent score trend as an ASCII sparkline in profile output: `▂▅▇▇▅▇▇▇`
**Why:** Instant visual of quality trajectory without scrolling through score history.
**Pros:** Compelling CLI UX. Terminal-native, no browser needed.
**Cons:** Minimal. Unicode sparkline chars may not render in all terminals (fallback to `_-^` if needed).
**Context:** `zombiectl agent profile` already shows current tier and streak. Add a sparkline from the last 20 scores. Map 0-100 to 8 sparkline characters.
**Effort:** S (30 min)
**Priority:** P3
**Depends on:** M9_002 CLI surface

---

## M9 Delight: Score-Gated Billing Credit

**What:** Runs scoring below Bronze (score < 40) are automatically marked `non_billable` in the credit lifecycle.
**Why:** Operators should never pay for garbage output. This is a powerful trust signal — "we only charge for quality."
**Pros:** Direct revenue signal. Ties scoring to billing, making M9 a revenue-relevant feature. Differentiator.
**Cons:** Revenue impact — some runs that currently bill would become free. Need to model the financial impact.
**Context:** Hook into `billing.finalizeRunForBilling()` in `worker_stage_executor.zig`. If score < 40, change `FinalizeOutcome` to `.non_billable`. Score is computed synchronously before billing finalization.
**Effort:** S (1 hour)
**Priority:** P1
**Depends on:** M9_001 + M6_002 (credit lifecycle)

---

## M9 Delight: Quality Drift Alert

**What:** If workspace average score drops 15% week-over-week, emit `agent.quality.drifting` PostHog event.
**Why:** Proactive quality monitoring. The "pager" for agent quality before operators notice degradation.
**Pros:** Early warning system. Enables PostHog alerts and dashboards.
**Cons:** Needs sufficient scoring data to compute weekly averages (minimum 2 weeks of data).
**Context:** Compute weekly average from `agent_run_scores`. Compare current week to previous week. If delta < -15%, emit event. Could run as part of the periodic background checker (same goroutine as auto-apply).
**Effort:** S (30 min)
**Priority:** P2
**Depends on:** M9_001 + 2 weeks of scoring data

---

## M9 Delight: `zombiectl agent dashboard`

**What:** ASCII table showing all agents in workspace with sparklines, current tier, trust status, and pending proposals in a single command.
**Why:** One command, full situational awareness. Operators shouldn't need to run 4 separate commands.
**Pros:** Compelling CLI UX. Makes M9 feel like a complete product.
**Cons:** 2 hour effort. Needs data from M9_002 (scores/profiles) + M9_004 (proposals).
**Context:** Combines data from `/v1/workspaces/{id}/leaderboard` + pending proposals query. Display as a formatted table with sparklines, tier badges, and proposal counts.
**Effort:** M (2 hours)
**Priority:** P2
**Depends on:** M9_002 + M9_004

---

## M9 Pre-Requisite: Rename profile_id → agent_id

**What:** Rename `profile_id` → `agent_id` and `profile_version_id` → `config_version_id` across all existing tables, FK references, and source code. Separate migration before M9 additions.
**Why:** `agent_id` is the universal identifier for M9. `config_version_id` eliminates the word "profile" from the vocabulary entirely. The rename aligns the existing schema with the M9 entity model.
**Pros:** Clean, consistent naming. No confusion between old "profile" terminology and new "agent" + "config" terminology.
**Cons:** Touches multiple tables and all Zig source references. Pre-launch so no production data to migrate.
**Context:** Renames in migration 016:
- `agent_profiles.profile_id` → `agent_id` (PK)
- `agent_profile_versions.profile_version_id` → `config_version_id` (PK); table rename to `agent_config_versions`
- `agent_profile_versions.profile_id` → `agent_id` (FK)
- `workspace_active_profile.profile_version_id` → `config_version_id` (FK)
- `profile_compile_jobs.requested_profile_id` → `requested_agent_id` (FK)
- `entitlement_policy_audit_snapshots` references updated
- `profile_linkage_audit_artifacts` references updated
- Source files: all harness control plane handlers, entitlements, profile resolver
**Effort:** M
**Priority:** P0 (must ship before M9)
**Depends on:** Nothing — can start immediately

- TODO: Define Bronze/Silver/Gold/Elite as human-coined quality tiers, then add a built-in AI Slop Inspector that reasons about the concrete criteria so the tiers measure less-sloppy agent output instead of arbitrary label drift.
