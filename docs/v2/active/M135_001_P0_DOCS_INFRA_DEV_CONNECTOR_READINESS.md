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

# M135_001: Development connectors prove the GitHub reviewer path

**Prototype:** v2.0.0
**Milestone:** M135
**Workstream:** 001
**Date:** Jul 20, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — production release evidence is invalid while the flagship fleet lacks its provider connections and real repository proof.
**Categories:** DOCS, INFRA
**Batch:** B1 — runs beside M135_002 before either acceptance lane is tuned
**Branch:** `feat/m135-release-readiness`
**Test Baseline:** unit=2802 integration=369
**Depends on:** none
**Provenance:** human-directed, Oracle-authored from the Jul 20, 2026 app-dev state and run evidence
**Canonical architecture:** `docs/architecture/connectors.md` §GitHub App: platform setup to fleet execution; `docs/architecture/scenarios/github-pr-reviewer.md` §Remaining proof punch list

---

## Overview

**Goal (testable):** The designated platform operator can connect the app-dev workspace to GitHub and Slack, install `github-pr-reviewer`, approve its declared grants, and prove one real Pull Request (PR) delivery creates exactly one review without exposing provider credentials.
**Problem:** The default platform model library and QStash are configured, and one platform fleet library exists, but GitHub is disconnected in Integrations and `github-pr-reviewer` has no usable GitHub or Slack connection. The architecture still marks the external repository proof incomplete.
**Solution summary:** Verify the running app sees the already-configured model and QStash prerequisites, complete both browser-mediated Open Authorization (OAuth) connector flows idempotently, bind only the credentials and repository access declared by the installed fleet, run the repository-level GitHub proof, and record whether Slack is a fleet dependency or a separately verified workspace connector. Never paste provider tokens into commands, chat, logs, or repository files.

## PR Intent & comprehension handshake

- **PR title (eventual):** docs(m135): prove development connector readiness
- **Intent (one sentence):** A release operator can demonstrate that app-dev has working provider connections and that `github-pr-reviewer` performs one real review with replay safety.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/AUTH.md` §OAuth connectors — provider app bags, callback trust, workspace handles, and integration grants.
2. `docs/architecture/connectors.md` §GitHub App: platform setup to fleet execution — installation, repository subscription, grant, and replay boundaries.
3. `docs/architecture/scenarios/github-pr-reviewer.md` §Remaining proof punch list — the exact real-repository evidence still missing.
4. `playbooks/operations/github_app_registration/001_playbook.md` and `playbooks/operations/slack_app_registration/001_playbook.md` — supported operator setup and verification paths.
5. `tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md` — the checked-in bundle declares GitHub only; inspect the live platform-library snapshot before claiming Slack is a fleet requirement.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT | Record any discovered app-dev-safe, idempotent connection or verification correction. |
| `playbooks/operations/slack_app_registration/001_playbook.md` | EDIT | Record any discovered app-dev-safe, idempotent connection or verification correction. |
| `playbooks/operations/jira_app_registration/001_playbook.md` | EDIT (discovered) | Pin resource-level authorization and the issue plus Service Management reply scopes selected during development registration. |
| `playbooks/operations/linear_app_registration/001_playbook.md` | EDIT (discovered) | Align the application controls and targeted comment scope with Linear's current refresh-token contract. |
| `playbooks/founding/02_preflight/001_playbook.md` | EDIT | Inventory the actual connector bags and agentsfleet callback signer required before promotion. |
| `playbooks/founding/02_preflight/02_credentials.sh` | EDIT | Fail loud on missing canonical GitHub, Slack, OAuth, QStash, and callback-state credentials. |
| `playbooks/founding/02_preflight/credentials_test.sh` | EDIT | Pin canonical field checks and missing callback-signer failure. |
| `playbooks/founding/01_bootstrap/001_playbook.md` | EDIT (review) | Remove retired GitHub Fly secrets and load the agentsfleet callback signer. |
| `playbooks/founding/03_priming_infra/001_playbook.md` | EDIT (review) | Keep the manual Fly bootstrap contract aligned with the deployment workflow. |
| `.github/workflows/deploy-dev.yml` | EDIT | Load the callback signer and remove retired GitHub Fly secrets. |
| `.github/workflows/release.yml` | EDIT | Make the same production path reproducible without activating or deploying it. |
| `deploy/fly/agentsfleetd-dev/fly.toml` | EDIT (discovered) | Pin the development API and app origins so OAuth redemption and browser redirects use the same environment. |
| `src/agentsfleetd/http/handlers/connectors/callback.zig` | EDIT (discovered) | Redirect successful connector callbacks to the workspace-scoped integrations route. |
| `src/agentsfleetd/http/handlers/connectors/github/callback_integration_test.zig` | EDIT (discovered) | Prevent regression to the stale unscoped integrations redirect. |
| `src/agentsfleetd/http/handlers/connectors/slack/oauth_callback_integration_test.zig` | EDIT (review) | Cover the OAuth callback path and trailing-slash-safe workspace redirect. |
| `src/agentsfleetd/http/handlers/connectors/jira/spec.zig` | EDIT (discovered) | Request and pin the least-privilege read/write scopes needed to reply on Jira and Service Management tickets. |
| `src/agentsfleetd/http/handlers/connectors/linear/spec.zig` | EDIT (discovered) | Remove the retired offline scope and request targeted Linear comment replies. |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | Replace the outstanding external proof marker only after the real repository run and replay check pass. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — No Dead Code (NDC), No Legacy Retained (NLR), No Legacy compatibility shims (NLG), Orphan Sweep (ORP), Prompt-injection Resistance (PRI): no dead setup branch, stale setup wording, legacy framing, orphaned references, or leaked private material.
- **`dispatch/write_documentation.md` and `docs/DOCUMENTATION_RULES.md`** — operational instructions remain literal, safe, and user-verifiable.
- **`dispatch/name_architecture.md`** — connector, integration, grant, and event-flow names follow the canonical architecture.

## Applicable Gates

N/A — docs/markdown only. External mutations remain governed by the secret and provider safety rules.

## Prior-Art / Reference Implementations

- **Reference:** `playbooks/operations/github_app_registration/001_playbook.md` — platform app bag and real GitHub verification.
- **Reference:** `playbooks/operations/slack_app_registration/001_playbook.md` — Slack Open Authorization connection and signed event verification.

## Sections (implementation slices)

### §1 — Prerequisites are runtime facts

Prove app-dev, not merely the vault, reports a default platform model and usable QStash configuration before provider work begins.

- **Dimension 1.1** — a platform-library install preflight no longer returns `UZ-SCHED-007` and identifies the configured default model → Test `test_dev_platform_prerequisites_are_runtime_visible`
- **Dimension 1.2** — missing runtime prerequisites fail before provider mutation with typed, redacted diagnostics → Test `test_dev_connector_preflight_fails_redacted`

### §2 — Workspace connectors are connected safely

Use the existing browser-mediated GitHub App and Slack Open Authorization flows. Platform app bags are checked, not recreated when already valid.

- **Dimension 2.1** — GitHub status is connected to the intended installation and repository set → Test `test_dev_github_connector_is_connected`
- **Dimension 2.2** — Slack status is connected to the intended workspace and a signed mention round-trip succeeds → Test `test_dev_slack_connector_is_connected`
- **Dimension 2.3** — reconnect is idempotent and never moves an installation owned by another workspace → Test `test_dev_connector_reconnect_is_safe`

### §3 — The fleet has least-privilege bindings

Inspect the live platform-library snapshot. GitHub must be granted because the checked-in trigger declares it. Slack is granted to the fleet only if the live snapshot declares it; otherwise Slack remains a separately proven workspace connector and the evidence says so.

- **Dimension 3.1** — `github-pr-reviewer` installs from the platform library with an approved GitHub grant and an explicit repository subscription → Test `test_github_reviewer_install_has_declared_grants`
- **Dimension 3.2** — no undeclared Slack secret is injected into a fleet that does not request it → Test `test_github_reviewer_rejects_undeclared_slack_binding`

### §4 — Real repository proof closes the architecture punch list

Open a harmless PR in a dedicated repository, observe one fleet event and one posted review, then replay the same delivery.

- **Dimension 4.1** — one signed GitHub delivery produces one queued event and one review through a short-lived installation token → Test `test_external_github_reviewer_posts_one_review`
- **Dimension 4.2** — replay produces neither a second event nor a second review → Test `test_external_github_reviewer_replay_is_idempotent`

## Interfaces

```text
POST /v1/workspaces/{workspace_id}/connectors/{provider}/connect
GET  /v1/workspaces/{workspace_id}/connectors/{provider}
POST /v1/ingress/github

Provider secrets: platform app bag -> workspace connector handle -> approved fleet grant.
No raw provider secret crosses into the repository, shell history, or evidence artifact.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Platform provider absent | `github-app` or `slack-app` bag is missing | Stop with `UZ-CONN-001`; list the missing bag name only. |
| Wrong workspace | OAuth callback returns for another workspace | Signed state or installation ownership check rejects with no mutation. |
| Missing grant | Fleet receives an event but lacks GitHub approval | No credential mint and no fleet wake; operator sees the missing grant. |
| Provider outage | GitHub or Slack is unavailable | Bounded failure with redacted provider diagnostics; retry remains idempotent. |
| Duplicate delivery | GitHub retries the same signed payload | Existing replay slot prevents a second event and review. |

## Invariants

1. Provider credentials are resolved only through 1Password and platform vault handles — gitleaks plus redacted evidence enforce the boundary.
2. Event receipt never implies credential access — the approved integration grant is rechecked at mint time.
3. A fleet receives only repositories and events explicitly declared by its trigger and installation selection.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `connector_readiness_proof` | ops | a provider status or repository proof is checked | provider, workspace identifier, outcome, duration | no email, token, callback code, or payload body | `test_connector_evidence_is_redacted` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | end-to-end | `test_dev_platform_prerequisites_are_runtime_visible` | live install preflight sees model and QStash and returns no scheduler-config error |
| 1.2 | integration | `test_dev_connector_preflight_fails_redacted` | absent prerequisite stops before connector mutation without secret output |
| 2.1 | end-to-end | `test_dev_github_connector_is_connected` | intended installation and repository set report connected |
| 2.2 | end-to-end | `test_dev_slack_connector_is_connected` | intended Slack workspace reports connected and signed mention succeeds |
| 2.3 | integration | `test_dev_connector_reconnect_is_safe` | repeat connect is idempotent; cross-workspace move is refused |
| 3.1 | end-to-end | `test_github_reviewer_install_has_declared_grants` | installed fleet has GitHub grant and repository subscription |
| 3.2 | integration | `test_github_reviewer_rejects_undeclared_slack_binding` | undeclared connector material is not delivered |
| 4.1 | end-to-end | `test_external_github_reviewer_posts_one_review` | real PR yields one event and one review |
| 4.2 | end-to-end | `test_external_github_reviewer_replay_is_idempotent` | replay leaves event and review counts unchanged |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | GitHub and Slack report connected | `agentsfleet connector list --workspace "$WORKSPACE_ID" --json` | github and slack each have `connected` status | P0 | |
| R2 | Reviewer installs from the platform library | `agentsfleet list --workspace-id "$WORKSPACE_ID" --json | jq -e '.. | objects | select(.name? == "github-pr-reviewer")'` | exit 0 with installed reviewer fleet | P0 | |
| R3 | Real PR is reviewed exactly once | `gh pr view "$PROOF_PR" --json reviews,comments` | one fleet-authored review for the proof delivery | P0 | |
| R4 | Architecture proof is closed | `rg -n 'External .github-pr-reviewer. repository test.*✅' docs/architecture/scenarios/github-pr-reviewer.md` | one match | P0 | |
| S1 | Repository checks pass | `make lint-all` | exit 0 | P0 | |
| S2 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S3 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Changing connector protocol, provider registry, or OAuth trust design.
- Adding Slack to the fleet bundle unless the canonical external bundle already declares it.
- Configuring production provider connections.

## Product Clarity (authoring record)

1. **Successful user moment** — a real GitHub PR receives one useful review from `github-pr-reviewer`.
2. **Preserved user behaviour** — existing workspace connector and fleet-install flows remain unchanged.
3. **Optimal-way check** — real provider proof is the shortest release-grade evidence; synthetic ingress alone is insufficient.
4. **Rebuild-vs-iterate** — iterate on shipped connector plumbing; no provider rewrite is justified.
5. **What we build** — app-dev connections, least-privilege bindings, real repository evidence, corrected playbooks.
6. **What we do NOT build** — new providers, static provider keys, or unrelated fleet behavior.
7. **Fit with existing features** — compounds with platform libraries, grants, QStash scheduling, and runner execution.
8. **Surface order** — UI-first for provider authorization, CLI for inspection and repeatable proof.
9. **Dashboard restraint** — connected labels appear only after provider status confirms persisted handles.
10. **Confused-user next step** — the typed connector status and playbook identify the missing bag, grant, or repository selection.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one readiness workstream owns provider setup and real-repository proof because both are one user journey.
- **Alternatives considered:** replacing browser authorization with pasted tokens was rejected because it weakens the connector trust boundary.
- **Patch-vs-refactor verdict:** this is a **patch** because the architecture is shipped and the missing work is environment binding plus proof.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
