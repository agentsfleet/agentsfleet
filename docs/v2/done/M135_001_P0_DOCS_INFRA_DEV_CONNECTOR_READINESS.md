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

# M135_001: Provider registration and callback readiness

**Prototype:** v2.0.0
**Milestone:** M135
**Workstream:** 001
**Date:** Jul 20, 2026
**Status:** DONE
**Priority:** P0 — development and production promotion are unsafe while provider credentials, callback origins, and registration grants drift between environments.
**Categories:** DOCS, INFRA
**Batch:** B1 — runs beside M135_002 before either acceptance lane is tuned
**Branch:** `feat/m135-release-readiness`
**Test Baseline:** unit=2802 integration=369
**Depends on:** none; live provider proof continues in M136_001 after M135_002 establishes an online runner
**Provenance:** human-directed, Oracle-authored from the Jul 20, 2026 app-dev state and run evidence
**Canonical architecture:** `docs/architecture/connectors.md` §GitHub App: platform setup to fleet execution; `docs/architecture/scenarios/github-pr-reviewer.md` §Remaining proof punch list (open until M136_001)

---

## Overview

**Goal (testable):** Development and production deployment paths consume canonical, validated provider credential bags and return successful Open Authorization (OAuth) callbacks to the environment-correct workspace Integrations route without exposing secrets.
**Problem:** Provider setup was split across stale Fly aliases, incomplete preflight checks, and an API-origin browser redirect, so a valid provider authorization could end on the wrong host while deployments silently omitted the callback-state signer.
**Solution summary:** Make 1Password provider bags and the agentsfleet approval signer explicit deployment inputs, fail preflight on missing canonical fields, keep API and app origins distinct, return successful callbacks to the workspace-scoped app route, and document the current GitHub, Slack, Zoho Desk, Jira, and Linear registration contracts. Live Slack authorization and the real GitHub reviewer/replay proof remain visibly open in M136_001 rather than being claimed by this infrastructure milestone.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(connectors): align provider deployment readiness
- **Intent (one sentence):** A release operator can promote provider applications without credential, scope, callback-origin, or browser-redirect drift between development and production.
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
| `src/agentsfleetd/http/handlers/connectors/registry_integration_test.zig` | EDIT (review) | Prove connect and callback paths fail closed without the approval signer. |
| `src/agentsfleetd/http/handlers/connectors/github/callback_integration_test.zig` | EDIT (discovered) | Prevent regression to the stale unscoped integrations redirect. |
| `src/agentsfleetd/http/handlers/connectors/slack/oauth_callback_integration_test.zig` | EDIT (review) | Cover the OAuth callback path and trailing-slash-safe workspace redirect. |
| `src/agentsfleetd/http/handlers/connectors/jira/spec.zig` | EDIT (discovered) | Request and pin the least-privilege read/write scopes needed to reply on Jira and Service Management tickets. |
| `src/agentsfleetd/http/handlers/connectors/linear/spec.zig` | EDIT (discovered) | Remove the retired offline scope and request targeted Linear comment replies. |
| `docs/architecture/scenarios/github-pr-reviewer.md` | EDIT | Keep the external proof visibly open without citing a pending milestone from canonical architecture. |
| `docs/v2/done/M135_001_P0_DOCS_INFRA_DEV_CONNECTOR_READINESS.md` | EDIT | Grade and close this amended readiness contract. |
| `docs/v2/active/M135_002_P0_DOCS_INFRA_DEV_RUNNER_ACTIVATION.md` | NO EDIT (parked) | Keep the active runner workstream open as M136_001's prerequisite. |
| `docs/v2/active/M135_003_P0_CLI_INFRA_CLI_ACCEPTANCE_TRUTH_AND_SPEED.md` | NO EDIT (parked) | Keep the active CLI workstream open for later execution. |
| `docs/v2/active/M135_004_P0_INFRA_UI_DEV_RELEASE_ACCEPTANCE_GATE.md` | NO EDIT (parked) | Keep the active UI workstream open for later execution. |
| `docs/v2/pending/M136_001_P0_DOCS_INFRA_LIVE_CONNECTOR_PROOF.md` | CREATE | Carry the explicitly deferred Slack and real-repository proof without claiming it passed here. |

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

### §1 — Canonical provider inputs are deployment facts — **DONE**

Prove app-dev, not merely the vault, reports a default platform model and usable QStash configuration before provider work begins.

- **Dimension 1.1 — DONE** — development and production workflows read the canonical provider bags and agentsfleet approval signer from their environment vault → Test `test_deployment_workflows_load_canonical_connector_credentials`
- **Dimension 1.2 — DONE** — a missing bag or required field fails preflight with the item and field name but no value → Test `test_connector_preflight_fails_redacted`

### §2 — OAuth callbacks preserve environment and workspace — **DONE**

Keep the provider exchange on the API origin and return the browser to the app origin's workspace-scoped Integrations route.

- **Dimension 2.1 — DONE** — GitHub callback success returns to `/w/{workspace_id}/integrations` on the configured app origin → Test `test_github_callback_redirects_to_workspace_integrations`
- **Dimension 2.2 — DONE** — Slack callback success uses the same workspace-aware redirect and handles a trailing-slash origin → Test `test_slack_callback_redirects_to_workspace_integrations`
- **Dimension 2.3 — DONE** — callback-state signing is unavailable when the agentsfleet-owned signer is absent rather than accepting an unsigned fallback → Test `test_connector_callback_requires_approval_signing_secret`

### §3 — Provider registration is reproducible and least privilege — **DONE**

Pin the provider-owned settings that an operator must reproduce while distinguishing authorization grants from outbound capabilities that are not implemented yet.

- **Dimension 3.1 — DONE** — GitHub and Slack playbooks use environment-correct callbacks and canonical provider bags → Test `test_github_and_slack_registration_contracts`
- **Dimension 3.2 — DONE** — Jira and Linear request only the selected read/write grants while stating that registration does not implement outbound posting → Test `test_issue_tracker_registration_contracts`
- **Dimension 3.3 — DONE** — Zoho registration describes the implemented Desk connector without claiming Recruit or Sprints support → Test `test_zoho_registration_contract`

### §4 — Live proof remains explicit — **DONE**

The architecture marker remains open and points to the pending successor. This milestone does not relabel an unrun external proof as infrastructure success.

- **Dimension 4.1 — DONE** — M136_001 owns Slack workspace authorization and signed mention proof after runner activation → Test `test_live_slack_proof_has_successor`
- **Dimension 4.2 — DONE** — M136_001 owns the real Pull Request review and replay-deduplication proof → Test `test_external_github_proof_has_successor`

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
| 1.1 | integration | `test_deployment_workflows_load_canonical_connector_credentials` | workflow contracts select the environment vault and stage every required connector input |
| 1.2 | integration | `test_connector_preflight_fails_redacted` | an absent bag or field fails without emitting its value |
| 2.1 | integration | `test_github_callback_redirects_to_workspace_integrations` | successful GitHub callback returns the browser to the app origin and workspace route |
| 2.2 | integration | `test_slack_callback_redirects_to_workspace_integrations` | successful Slack callback returns the browser to the app origin and workspace route |
| 2.3 | integration | `test_connector_callback_requires_approval_signing_secret` | missing signer prevents callback state issuance or redemption |
| 3.1 | integration | `test_github_and_slack_registration_contracts` | playbooks name canonical bags, callbacks, and environment-specific app distribution |
| 3.2 | integration | `test_issue_tracker_registration_contracts` | Jira and Linear grants match source constants without claiming an outbound poster exists |
| 3.3 | integration | `test_zoho_registration_contract` | playbook limits the shipped connector claim to Zoho Desk |
| 4.1 | documentation | `test_live_slack_proof_has_successor` | pending M136_001 depends on M135_002 and contains the signed mention proof |
| 4.2 | documentation | `test_external_github_proof_has_successor` | active spec names M136_001 while the architecture marker remains open |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Development credential gate accepts every canonical bag | `OP_BIOMETRIC_UNLOCK_ENABLED=false ENV=dev ./playbooks/founding/02_preflight/00_gate.sh` | exit 0 and `002_preflight check complete (env: dev)` | P0 | ✅ `002_preflight check complete (env: dev)` |
| R2 | Provider callbacks return to workspace-scoped app routes | `zig build test -Dtest-filter='OAuth callback' && zig build test -Dtest-filter='GitHub callback'` | exit 0 | P0 | ✅ both filtered test commands exited 0 |
| R3 | Deployment workflow contracts and registration playbooks agree | `bash playbooks/founding/02_preflight/credentials_test.sh && make check-playbooks` | exit 0 and `8 passed, 0 failed` | P0 | ✅ `8 passed, 0 failed`; playbook checks exited 0 |
| R4 | Architecture proof remains visibly open | `rg -n 'External .github-pr-reviewer. repository test.*external proof remains open' docs/architecture/scenarios/github-pr-reviewer.md` | one match | P0 | ✅ one match at line 108 |
| S1 | Repository checks pass | `make lint-all` | exit 0 | P0 | ✅ `All lint checks passed` |
| S2 | No secrets | `gitleaks detect --no-banner` | exit 0 | P0 | ✅ `no leaks found` |
| S3 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ comparison emitted 0 paths |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Changing connector protocol, provider registry, or OAuth trust design.
- Adding Slack to the fleet bundle unless the canonical external bundle already declares it.
- Configuring production provider connections.
- Completing live Slack authorization, signed mention, real repository review, or replay proof — M136_001.

## Product Clarity (authoring record)

1. **Successful user moment** — a successful provider callback returns the operator to that workspace's Integrations page in the same environment.
2. **Preserved user behaviour** — existing workspace connector and fleet-install flows remain unchanged.
3. **Optimal-way check** — real provider proof is the shortest release-grade evidence; synthetic ingress alone is insufficient.
4. **Rebuild-vs-iterate** — iterate on shipped connector plumbing; no provider rewrite is justified.
5. **What we build** — canonical provider inputs, fail-closed preflight, environment-correct redirects, least-privilege registration guidance, and a named live-proof successor.
6. **What we do NOT build** — new providers, static provider keys, outbound Jira/Linear posters, or fabricated live evidence.
7. **Fit with existing features** — compounds with platform libraries, grants, QStash scheduling, and runner execution.
8. **Surface order** — UI-first for provider authorization, CLI for inspection and repeatable proof.
9. **Dashboard restraint** — connected labels appear only after provider status confirms persisted handles.
10. **Confused-user next step** — the typed connector status and playbook identify the missing bag, grant, or repository selection.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** this workstream closes deterministic provider registration and callback readiness; M136_001 owns external proof after M135_002 supplies the execution dependency.
- **Alternatives considered:** replacing browser authorization with pasted tokens was rejected because it weakens the connector trust boundary.
- **Patch-vs-refactor verdict:** this is a **patch** because the architecture is shipped and the missing work is environment binding plus proof.

## Discovery (consult log)

- **Consults** — Architecture marker remains open because the real repository proof has not run; Jira and Linear registration grants are not described as implemented outbound posters.
- **Metrics review** — no analytics or funnel event changes; this work changes deployment inputs, callback redirects, and operator playbooks.
- **Skill-chain outcomes** — `/write-unit-test`: PASS after adding provider-secret redaction, exact dev/prod signer workflow contracts, fail-closed connect/callback signer coverage, and Jira/Linear source-to-playbook assertions; `8 passed, 0 failed`. Final gstack `/review`: CLEAN after correcting the Jira/Linear capability overstatement and one mechanical parked-lifecycle wording fix; 0 unresolved findings, quality score 10/10, adversarial sources both clean. `make lint-all`, `make harness-verify`, and gitleaks passed; test depth advanced from unit=2802/integration=369 to unit=2806/integration=371. `kishore-babysit-prs` runs after the closure push.
- **Deferrals** —
  > Indy (2026-07-20 22:23): "And move th 2,3,4 to the next milestone and read and move this milestone to done?" — context: runner activation remains M135_002; live Slack authorization/signed mention and real GitHub review/replay proof move to M136_001 rather than being claimed passed by M135_001.

**Jul 20, 2026 — CHORE(close).** M135_001 closes the amended provider registration and callback-readiness contract. M135_002–004 remain active and parked. M136_001 is pending and owns the unrun live Slack and GitHub proof; the canonical architecture marker remains visibly open. No external proof was relabelled as passed. No user-facing changelog entry is required because this is internal deployment, callback routing, test, and operator-playbook readiness work.
