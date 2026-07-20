<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M136_001: Live Slack and GitHub reviewer proof

**Prototype:** v2.0.0
**Milestone:** M136
**Workstream:** 001
**Date:** Jul 20, 2026
**Status:** PENDING
**Priority:** P0 — the flagship reviewer scenario remains incomplete until real provider authorization and replay safety pass.
**Categories:** DOCS, INFRA
**Batch:** B1 — starts after the development runner is online
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) via `make _lint_zig_test_depth`
**Depends on:** M135_002 (online runner with advancing heartbeat); M135_001 (provider bags, callback routes, and registration grants)
**Provenance:** human-directed successor to M135_001 after the Jul 20, 2026 scope decision
**Canonical architecture:** `docs/architecture/scenarios/github-pr-reviewer.md` §Remaining proof punch list

---

## Overview

**Goal (testable):** A real Slack mention succeeds and one signed GitHub delivery creates exactly one reviewer event and review while replay creates neither again.
**Problem:** Provider applications and callbacks are deployment-ready, but Slack workspace authorization and the real repository reviewer path lack release-grade evidence, and the development runner must be online before the fleet can execute.
**Solution summary:** After M135_002 proves runner liveness, complete the existing browser-mediated Slack and GitHub authorization flows, prove Slack independently at workspace level, run `github-pr-reviewer` against the dedicated proof repository, and replay the exact GitHub delivery. Keep provider material in vaulted handles and record only redacted identifiers and counts.

## PR Intent & comprehension handshake

- **PR title (eventual):** test(connectors): prove live reviewer integrations
- **Intent (one sentence):** A release operator can trust that real Slack and GitHub integrations execute once, use only declared grants, and remain replay-safe.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/architecture/scenarios/github-pr-reviewer.md` — exact external proof still open.
2. `docs/architecture/connectors.md` — installation, repository subscription, grant, mint, and replay boundaries.
3. `docs/architecture/runner_fleet.md` — online heartbeat prerequisite and execution boundary.
4. `tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md` — GitHub-only provider dependency.
5. `playbooks/operations/slack_app_registration/001_playbook.md` — workspace OAuth and signed event verification.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | Mark external proof complete only after every live rubric row passes. |
| `playbooks/operations/slack_app_registration/001_playbook.md` | EDIT if discovered | Record corrections proven during the live workspace flow. |
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT if discovered | Record corrections proven during the live repository flow. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC, NLR, NLG, PRI, and ORP: no stale setup path, compatibility alias, untrusted provider payload, orphaned marker, or secret-bearing evidence.
- **`dispatch/write_documentation.md` and `docs/DOCUMENTATION_RULES.md`** — operator steps remain literal, redacted, and independently verifiable.
- **`dispatch/name_architecture.md`** — connector, grant, event, review, and replay terminology remains canonical.

## Applicable Gates

N/A — docs/markdown and external proof only. Provider, secret, repository, and runner mutations remain governed by their operational safety rules.

## Prior-Art / Reference Implementations

- **Reference:** `docs/architecture/scenarios/github-pr-reviewer.md` — local datastore proof defines the event, grant, mint, and replay behavior the external run must match.

## Sections (implementation slices)

### §1 — Preconditions are live facts

Verify the deployed prerequisites and that M135_002 reports an online runner whose `last_seen_at` advances before provider proof begins.

- **Dimension 1.1** — readiness fails before external mutation when the runner is not online → Test `test_connector_proof_requires_online_runner`
- **Dimension 1.2** — two bounded reads observe `last_seen_at` advance → Test `test_runner_last_seen_advances`

### §2 — Slack is independently proven

Authorize the intended Slack workspace through the existing browser flow. Because the reviewer bundle declares GitHub only, Slack remains a workspace proof and is never injected into that fleet.

- **Dimension 2.1** — status identifies the intended Slack workspace as connected → Test `test_slack_workspace_connection_status`
- **Dimension 2.2** — one signed mention produces one accepted workspace event without secret output → Test `test_slack_signed_mention_once`

### §3 — GitHub reviewer executes exactly once

Use the existing installation, selected proof repository, installed fleet, and approved GitHub grant.

- **Dimension 3.1** — one signed delivery creates one fleet event and one fleet-authored review through a short-lived installation token → Test `test_github_reviewer_posts_once`
- **Dimension 3.2** — the fleet receives no Slack material because its trigger does not declare Slack → Test `test_reviewer_declared_connectors_only`

### §4 — Replay closes the architecture marker

Replay the exact delivery only after recording original event and review identifiers.

- **Dimension 4.1** — replay creates no second fleet event → Test `test_github_replay_no_event`
- **Dimension 4.2** — replay creates no second review and only then closes the architecture marker → Test `test_github_replay_no_review`

## Interfaces

```text
GET  /v1/fleets/runners
GET  /v1/workspaces/{workspace_id}/connectors/{provider}
POST /v1/ingress/slack
POST /v1/ingress/github

Provider credentials remain platform bag -> workspace handle -> approved fleet grant.
Evidence records redacted resource identifiers, delivery identifiers, and counts only.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Runner stale | service exists but heartbeat does not advance | Stop before provider mutation; report runner as offline. |
| Consent denied | operator or provider declines OAuth | Preserve prior connector state and record no connected claim. |
| Wrong target | grant does not match the intended workspace or repository | Stop before delivery or review and preserve ownership. |
| Provider unavailable | Slack or GitHub request fails | Bounded failure with redacted diagnostics; retry remains idempotent. |
| Duplicate delivery | GitHub retries the same delivery id | Existing replay boundary creates no second event or review. |

## Invariants

1. Raw provider and runner credentials never enter evidence — gitleaks and redacted status-only commands enforce the boundary.
2. Slack is not granted to `github-pr-reviewer` unless its bundle declares Slack — the grant resolver rejects undeclared provider access.
3. Architecture status becomes complete only after original and replay counts are observed — rubric R4 gates the marker edit.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `connector_readiness_proof` | ops | each provider or replay proof completes | provider, redacted resource id, outcome, duration | no email, token, code, signature, or payload body | `test_connector_evidence_redaction` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | end-to-end | `test_connector_proof_requires_online_runner` | stale runner prevents external mutation |
| 1.2 | end-to-end | `test_runner_last_seen_advances` | bounded reads show advancing `last_seen_at` |
| 2.1 | end-to-end | `test_slack_workspace_connection_status` | connector status matches intended workspace |
| 2.2 | end-to-end | `test_slack_signed_mention_once` | signed mention is accepted exactly once |
| 3.1 | end-to-end | `test_github_reviewer_posts_once` | signed delivery yields one event and one review |
| 3.2 | integration | `test_reviewer_declared_connectors_only` | reviewer has GitHub but no undeclared Slack grant |
| 4.1 | end-to-end | `test_github_replay_no_event` | replay leaves event count unchanged |
| 4.2 | end-to-end | `test_github_replay_no_review` | replay leaves review count unchanged before marker closes |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Runner heartbeat is fresh | `agentsfleet runner list --json | jq -e '.[] | select(.status == "online" and .last_seen_at != null)'` | exit 0 across two reads with advancing `last_seen_at` | P0 | |
| R2 | Slack workspace is connected | `agentsfleet connector list --workspace "$WORKSPACE_ID" --json | jq -e '.[] | select(.provider == "slack" and .status == "connected")'` | exit 0 plus one redacted mention event id | P0 | |
| R3 | Real Pull Request is reviewed exactly once | `gh pr view "$PROOF_PR" --json reviews,comments` | exactly one fleet-authored review | P0 | |
| R4 | Replay is idempotent and architecture proof closes | `rg -n 'External .github-pr-reviewer. repository test.*✅' docs/architecture/scenarios/github-pr-reviewer.md` | one match after unchanged event and review counts | P0 | |
| S1 | Repository checks pass | `make lint-all` | exit 0 | P0 | |
| S2 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | |
| S3 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Runner credential provisioning or heartbeat repair — M135_002.
- Adding Slack to the GitHub reviewer bundle when it is not declared.
- Merging the proof Pull Request or changing unrelated repositories.
- Production deployment, release, or tag creation.

---

## Product Clarity (authoring record)

1. **Successful user moment** — one real Pull Request receives one fleet-authored review and a replay changes nothing.
2. **Preserved user behaviour** — connector authorization and fleet grant approval remain the existing browser and app flows.
3. **Optimal-way check** — external evidence is required because local tests cannot prove consent, ownership, or review posting.
4. **Rebuild-vs-iterate** — iterate on shipped plumbing; no connector rewrite is justified.
5. **What we build** — redacted Slack mention, GitHub review, and replay evidence plus the architecture marker update.
6. **What we do NOT build** — new providers, pasted tokens, a Slack fleet dependency, or runner provisioning.
7. **Fit with existing features** — compounds with integration grants, short-lived provider tokens, fleet events, and runner execution.
8. **Surface order** — UI-first for consent, CLI and provider interfaces for repeatable observation.
9. **Dashboard restraint** — connected and completed claims appear only after provider and replay status confirm them.
10. **Confused-user next step** — readiness identifies the stale runner, disconnected provider, missing grant, or mismatched repository before mutation.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one successor owns both external providers and replay evidence because together they close one reviewer scenario.
- **Alternatives considered:** folding proof into M135_001 was rejected after runner availability blocked execution; claiming registration as proof was rejected as false evidence.
- **Patch-vs-refactor verdict:** this is a **patch** because the architecture is implemented and locally tested; only external binding and evidence remain.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
  > Indy (2026-07-20 22:23): "And move th 2,3,4 to the next milestone and read and move this milestone to done?" — context: live Slack authorization/signed mention and real GitHub review/replay proof move from M135_001 to this successor; runner activation remains M135_002 and is this workstream's prerequisite.
