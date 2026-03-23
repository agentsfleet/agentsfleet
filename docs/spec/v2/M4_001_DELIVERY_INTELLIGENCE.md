# M4_001: Delivery Intelligence

**Version:** v2
**Milestone:** M4
**Workstream:** 001
**Date:** Mar 23, 2026
**Status:** PENDING
**Priority:** P2 — builds on v1 M13 delivery observation layer
**Depends on:** v1/M13_001 (delivery state), v1/M13_002 (outbound webhooks), v1/M13_003 (delivery observability), v1/M9_001 (scoring engine)

---

## Problem

The v1 M13 milestone establishes delivery *observation* — the control plane knows whether PRs are merged, closed, or revised. This milestone adds delivery *intelligence* — autonomous actions and scoring based on delivery outcomes. These capabilities were explicitly deferred during the M13 CEO Review (Mar 23, 2026) to ship the observation layer first and design actions from real usage data.

---

## Deferred Capabilities (from M13 CEO Review)

### 1. Webhook Payload Includes Agent Score

**Origin:** CEO Review Issue 19, Option A — deferred because payload shape should be designed once and documented in `openapi.json` before shipping.

**Scope:** When constructing outbound webhook payloads (M13_002 §3.3), include the M9 agent score: `{ "score": { "total": 87, "tier": "Gold", "formula_version": "v3" } }`. Requires joining `agent_run_scores` during payload construction. Non-breaking addition to payload contract.

### 2. Auto-Revision on CHANGES_REQUESTED

**Origin:** CEO Review Issue 6, Option C — deferred because review comments may be conversational rather than actionable, and the revision run input format needs design.

**Scope:** When `delivery_state` transitions to `CHANGES_REQUESTED`, the control plane:
1. Extracts review comments via GitHub API (already best-effort enriched in M13_002 §3.3)
2. Constructs a revision spec: original spec + review comments as structured defects
3. Enqueues a new run with `parent_run_id` linking to the original
4. New run's Scout receives review comments as context (similar to Warden defect injection in retry loop)
5. Resulting commits pushed to same branch, updating existing PR

**Design questions to resolve from real data:**
- What percentage of review comments are actionable vs. conversational?
- Should auto-revision require opt-in per workspace?
- Does Scout need a new input format for review-feedback-as-defects?
- Push to same branch (force-push or additional commits)?

### 3. Staleness Detector for Stuck PRs

**Origin:** CEO Review Issue 14, Option C — deferred to v2 as low frequency at v1 scale.

**Scope:** Background reconciler (hourly) queries `runs WHERE delivery_state = 'PR_OPEN' AND updated_at < now() - interval '7 days'`. For each stale run, uses GitHub API to check actual PR status. Updates `delivery_state` accordingly. If GitHub API returns 404 (app uninstalled, repo deleted), transitions to `CLOSED` with `reason=github_unreachable`.

**Implementation:** Extend existing `src/cmd/reconcile` with a `--stale-prs` flag. Reuses GitHub API client and installation token minting from `auth/github.zig`.

### 4. Delivery Scoring Axis in M9 Engine

**Origin:** Dream state delta — "scoring includes delivery outcome."

**Scope:** Add a 5th scoring axis: **Delivery (weight TBD)**. Dimensions:
- Time-to-merge (faster = higher score)
- Revision count (fewer revisions = higher score)
- Auto-merge eligibility (trusted + CI passed + approved = bonus)
- Merge rate (merged vs. closed/abandoned)

**Constraint:** Delivery scoring can only run after `delivery_state` reaches a terminal state (`MERGED` or `CLOSED`). This means the score is updated retroactively — the initial M9 score (computed at run completion) is augmented with a delivery component when the PR resolves. Requires a `rescore` or `score_amendment` pattern.

### 5. Auto-Merge for Trusted Agents

**Origin:** Phase 2 trajectory from CEO Review Section 10.

**Scope:** When a run's agent has `trust_level = TRUSTED` (M9_004) and the PR reaches `APPROVED` + last CI check passed, the control plane calls the GitHub merge API to auto-merge the PR.

**Prerequisites:**
- Delivery scoring axis (§4) validates that trusted agents produce PRs worth merging
- Workspace-level opt-in: `auto_merge_enabled: bool` on workspace settings
- Merge method configuration: `merge`, `squash`, or `rebase` per workspace
- Human override: operators can disable auto-merge per run via `zombiectl runs lock {run_id}`

**Safety:** Auto-merge only fires if:
1. Agent trust_level = TRUSTED (10+ consecutive Gold+ runs)
2. PR is APPROVED by at least one human reviewer
3. CI suite passed (check_suite conclusion = success)
4. Workspace has auto_merge_enabled = true
5. No `lock` flag on the run

### 6. Spec Chaining with Dependency Graph

**Origin:** Phase 3 trajectory from CEO Review Section 10.

**Scope:** Specs can declare dependencies: `depends_on: run_xyz789` in spec frontmatter. A run with dependencies is held in `SPEC_QUEUED` until all dependency runs have `delivery_state = MERGED`. If a dependency reaches `CLOSED`, the dependent run is transitioned to `BLOCKED` with `reason=dependency_closed`.

**Design:** This requires a `run_dependencies` table and a dependency resolver in the worker claim path. The resolver checks all dependencies before claiming the run from Redis.

### 7. API-Served Discovery Files

**Origin:** CEO Review Issue 7, fast follow — serve machine-readable contracts from the API domain.

**Scope:** Add routes to `zombied` that serve static files at:
- `GET /openapi.json` — OpenAPI 3.1 spec
- `GET /agent-manifest.json` — JSON-LD capability manifest
- `GET /skill.md` — agent onboarding instructions
- `GET /llms.txt` — LLM-friendly index

**Implementation:** Either embed files in the Zig binary at compile time (`@embedFile`) or serve from the `config/` directory. Embedding is preferred — single binary, no file system dependency.

---

## Sequencing

```
  v1/M13 (observation)          v2/M4_001 (intelligence)
  ──────────────────            ────────────────────────
  Delivery state tracking  ──►  Delivery scoring axis
  Outbound webhooks        ──►  Webhook payload with score
  PostHog pr.* events      ──►  Auto-revision on CHANGES_REQUESTED
  Time-to-merge metric     ──►  Auto-merge for trusted agents
  CLI delivery state       ──►  Staleness detector
                           ──►  Spec chaining
                           ──►  API-served discovery
```

Each capability in this spec can be shipped independently. No ordering constraints between items except: delivery scoring axis (§4) should precede auto-merge (§5) so that trust decisions include delivery quality.
