# M4_004: Delivery UX Surface

**Version:** v2
**Milestone:** M4
**Workstream:** 004
**Date:** Mar 23, 2026
**Status:** PENDING
**Priority:** P1 — makes delivery state visible to operators and PR reviewers
**Batch:** B2 — parallel with M4_002, M4_003 after M4_001
**Depends on:** M4_001 (delivery_state column), M9_001 (scoring engine for PR badge)

---

## Problem

Delivery state data flows into the system (M4_001) and out to external agents (M4_002) but is not visible to operators using the CLI or humans reviewing PRs on GitHub. The control plane sees delivery outcomes — operators should too.

---

## 1.0 Score Badge in PR Body

**Status:** PENDING

Append agent quality score and tier to the PR description body. Social proof for reviewers.

**Dimensions:**
- 1.1 In `handleDoneOutcome()` (`worker_stage_outcomes.zig`), after scoring completes, construct a badge line: `"\n\n---\nProduced by a {tier}-tier agent ({score}/100) | [UseZombie](https://usezombie.com)"`. Tier and score are read from `scoring_state` after `scoreRunIfTerminal()` completes.
- 1.2 If scoring failed or is disabled (`score = null`), omit the badge line entirely. The PR body is the `final_stage_output` (Warden verdict) — the badge is appended, never replacing content.
- 1.3 Badge format is plain markdown — renders correctly on GitHub, GitLab, and Bitbucket. No HTML, no images, no external badge service dependencies.
- 1.4 Badge is append-only — if the PR body is updated later (e.g., revision run pushes new commits), the badge from the latest run replaces the previous one. The badge line is identified by the `Produced by a` prefix for replacement.

---

## 2.0 Delivery State in `zombiectl runs list`

**Status:** PENDING

Add delivery state column to CLI table output so operators can see PR lifecycle status alongside run state.

**Dimensions:**
- 2.1 `zombiectl runs list` output adds a `DELIVERY` column after `STATE`. Values: blank (no PR yet), `PR_OPEN`, `APPROVED`, `CHANGES_REQUESTED`, `CI_FAILED`, `MERGED`, `CLOSED`. Column is right-aligned, max width 18 chars.
- 2.2 Compound display format: `DONE (MERGED)`, `DONE (PR_OPEN)`, `DONE (CHANGES_REQUESTED)` for quick scanning. The compound format is used when `--compact` flag is set or terminal width < 100 chars. Full separate columns used otherwise.
- 2.3 `zombiectl runs get {run_id}` detail view includes `delivery_state` field with timestamp of last delivery state change.
- 2.4 API response for `GET /v1/runs/{run_id}` includes `delivery_state` field (nullable string). No breaking change — new field is additive.

---

## 3.0 Fix Static Discovery File URLs

**Status:** PENDING

Correct the API base URL and endpoint references in the static machine-readable files served at `usezombie.sh`.

**Dimensions:**
- 3.1 `llms.txt` line 11: change `https://usezombie.com/v1` to `https://api.usezombie.com/v1`. This is the actual API base URL that agents should use for API calls.
- 3.2 `agent-manifest.json`: add `"apiBase": "https://api.usezombie.com"` top-level field. Agents reading the manifest can construct full API URLs from `apiBase + operation.path`.
- 3.3 `skill.md` line 29: change `Agent manifest (JSON-LD): /agents` to `Agent manifest (JSON-LD): /agent-manifest.json` — the `/agents` route is an SPA page, not the JSON file.
- 3.4 Verify all `sameAs` URLs in `agent-manifest.json` resolve correctly from `usezombie.sh` domain.

---

## 4.0 Stale Diagram Updates

**Status:** PENDING

Update lifecycle diagrams in machine-readable files to reflect the new delivery states.

**Dimensions:**
- 4.1 `skill.md` lines 38-42: append delivery state addendum after existing lifecycle:
  ```
  Post-PR delivery states (tracked via GitHub webhooks):
  PR_OPEN → APPROVED | CHANGES_REQUESTED | CI_FAILED → MERGED | CLOSED
  ```
- 4.2 `llms.txt` lines 25-27: append same delivery state addendum.
- 4.3 `docs/ARCHITECTURE.md`: add "Post-PR Delivery Lifecycle" subsection after the canonical sequence diagram (line 212). Include the 6-state delivery machine, the parallel `delivery_state` column design rationale, and the webhook-to-state mapping table.
- 4.4 `docs/CONFIGURATION.md`: add `GITHUB_WEBHOOK_SECRET` to Auth partition table with `Required: Conditional`, `Override Source: Process env only`, `Notes: Required when webhook ingestion is enabled for delivery state tracking`.
