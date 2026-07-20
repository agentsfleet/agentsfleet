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

# M135_002: Development runner advances from registered to online

**Prototype:** v2.0.0
**Milestone:** M135
**Workstream:** 002
**Date:** Jul 20, 2026
**Status:** PENDING
**Priority:** P0 — acceptance cannot prove execution while the only development runner has never authenticated a heartbeat.
**Categories:** DOCS, INFRA
**Batch:** B1 — runs beside M135_001 before acceptance optimization
**Branch:** set at CHORE(open)
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none
**Provenance:** human-directed, Oracle-authored from the Jul 20, 2026 app-dev state and runner bootstrap code
**Canonical architecture:** `docs/architecture/runner_fleet.md` §Registering a runner and §Runner state

---

## Overview

**Goal (testable):** A freshly minted development runner token is stored only in the approved vault and runner host, the service authenticates a heartbeat, and `GET /v1/fleets/runners` reports `online` with advancing `last_seen_at` instead of `registered`.
**Problem:** The runner record exists but remains `registered`, which means its `last_seen_at` sentinel has never advanced. The current operational playbook still names retired authorization metadata and an obsolete runner-list route, making a safe recovery needlessly ambiguous.
**Solution summary:** Mint or rotate the runner token through the current scoped operator surface, write the raw `agt_r` value directly to the approved 1Password item without displaying it, provision the host with the existing bootstrap script, verify service logs and live heartbeats, and repair the operational playbook. `agentsfleetd` stores only the token hash; the raw token is never added to its job environment.

## PR Intent & comprehension handshake

- **PR title (eventual):** docs(m135): make development runner activation repeatable
- **Intent (one sentence):** Operators can safely activate and prove a development runner without putting its raw machine credential on the control-plane host.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/AUTH.md` §Runner token — machine-principal scope, storage, and raw-token boundary.
2. `docs/architecture/runner_fleet.md` §Registering a runner and §Runner state — one-time reveal, hash storage, heartbeat-derived liveness.
3. `playbooks/founding/06_runner_bootstrap_dev/04_provision_runner_env.sh` — canonical vault-to-host provisioning path.
4. `playbooks/operations/runner_onboarding/001_playbook.md` — repair its retired authorization and route guidance while preserving safe operator steps.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `playbooks/operations/runner_onboarding/001_playbook.md` | EDIT | Replace retired platform-admin metadata and `/v1/fleet/runners` guidance with current scopes and `/v1/fleets/runners`. |
| `playbooks/founding/06_runner_bootstrap_dev/04_provision_runner_env.sh` | EDIT (conditional) | Add redacted diagnosis only if the existing script cannot distinguish vault, host, service, and heartbeat failures. |
| `playbooks/founding/06_runner_bootstrap_dev/provision_runner_env_test.sh` | EDIT (conditional) | Pin every provisioning behavior changed in the script. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — No Dead Code (NDC), No Legacy Retained (NLR), No Legacy compatibility shims (NLG), Prompt-injection Resistance (PRI), Unified Form for Symbols (UFS): no stale auth branch, legacy wording, secret output, or repeated semantic literal.
- **`dispatch/write_shell.md`** — quote expansions, preserve cleanup, avoid secret-bearing argument lists, retain macOS shell compatibility.
- **`dispatch/write_documentation.md` and `docs/DOCUMENTATION_RULES.md`** — safe, current operator instructions.
- **`dispatch/name_architecture.md`** — runner identity, liveness, token, and control-plane boundaries follow canonical docs.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length | yes if shell changes | keep diagnosis helpers focused and within repository caps |
| UFS | yes if shell changes | reuse named vault, service, route, and token-prefix constants |
| LOGGING / LIFECYCLE | yes if shell changes | redact token values and clean every temporary file on all exits |

## Prior-Art / Reference Implementations

- **Reference:** `playbooks/founding/06_runner_bootstrap_dev/04_provision_runner_env.sh` — approved 1Password read, host env synchronization, service restart, and validation.
- **Reference:** `ui/packages/app/lib/api/runners.ts` — current `/v1/fleets/runners` read surface and liveness vocabulary.

## Sections (implementation slices)

### §1 — Identify the exact broken link without revealing the token

Distinguish an unminted record, stale vault value, stale host environment, rejected service credential, and missing heartbeat using hashes or status only.

- **Dimension 1.1** — diagnostics identify the first failed boundary while emitting no raw token → Test `test_runner_activation_diagnostics_are_redacted`
- **Dimension 1.2** — current operator scope and runner-list route are used; retired metadata and route have zero operational references → Test `test_runner_onboarding_uses_current_authorization`

### §2 — Rotate and provision the machine credential

Mint through the dashboard or current API as the scoped platform operator, store the one-time value directly in `op://ZMB_CD_DEV/zombie-dev-worker-ant/runner-token`, and run the canonical provisioner.

- **Dimension 2.1** — a non-placeholder `agt_r` token reaches `/opt/agentsfleet/.env` and `/etc/default/agentsfleet-runner` without appearing in process arguments or logs → Test `test_runner_token_provisions_without_disclosure`
- **Dimension 2.2** — service restart is idempotent and a failed write leaves neither partial env file nor running stale credential → Test `test_runner_provision_failure_is_atomic`

### §3 — Liveness is proven by a fresh heartbeat

Service-active is necessary but insufficient. The acceptance proof observes `last_seen_at` advance and derived liveness become `online`.

- **Dimension 3.1** — the runner journal shows successful heartbeat and lease polling with no authorization loop → Test `test_runner_service_authenticates_heartbeat`
- **Dimension 3.2** — the API reports `online` and `last_seen_at` advances across two reads → Test `test_runner_liveness_advances_after_provision`

### §4 — Obsolete identities are resolved deliberately

If token rotation creates a second runner identity, preserve both until the online identity is unambiguous. Revoke or delete the obsolete row only with Indy's explicit approval.

- **Dimension 4.1** — the online host maps to exactly one intended runner identity and duplicate cleanup is never automatic → Test `test_runner_identity_cleanup_requires_confirmation`

## Interfaces

```text
POST /v1/runners                         scoped human operator mints agt_r once
POST /v1/runners/me/heartbeats           runner authenticates with raw agt_r
GET  /v1/fleets/runners                  operator reads derived liveness

Control plane: stores sha256(agt_r), never the raw token.
Runner host: receives raw agt_r from 1Password as AGENTSFLEET_RUNNER_TOKEN.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Placeholder token | Vault still contains fake bootstrap value | Provisioner refuses before any host write. |
| Token mismatch | Stored hash and host token do not correspond | Journal shows redacted 401 class; rotate and reprovision, never print either value. |
| Partial host write | Remote write or permission failure | Temporary file is removed and prior service configuration remains intact. |
| Service-only false green | systemd is active but heartbeat fails | Workstream remains failed until `last_seen_at` advances. |
| Duplicate identity | a new mint creates another row | Keep both, identify online row, request approval before destructive cleanup. |

## Invariants

1. The raw runner token exists only in the one-time response, approved 1Password item, and runner host environment.
2. `registered` means no successful heartbeat; no check may relabel service-active as runner-online.
3. Destructive cleanup of an obsolete runner row requires explicit user approval.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `runner_activation_check` | ops | a provisioning boundary or heartbeat is checked | runner identifier, boundary, outcome, liveness | no token, hash, environment contents, or email | `test_runner_activation_diagnostics_are_redacted` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_runner_activation_diagnostics_are_redacted` | each injected boundary failure names only its class |
| 1.2 | unit | `test_runner_onboarding_uses_current_authorization` | retired auth key and singular route have zero current playbook matches |
| 2.1 | integration | `test_runner_token_provisions_without_disclosure` | valid secret reaches both env files while captured output omits it |
| 2.2 | integration | `test_runner_provision_failure_is_atomic` | injected remote write failure retains prior config and cleans temporary files |
| 3.1 | end-to-end | `test_runner_service_authenticates_heartbeat` | journal shows successful heartbeat and no repeated 401 |
| 3.2 | end-to-end | `test_runner_liveness_advances_after_provision` | two API reads show online and increasing last-seen value |
| 4.1 | integration | `test_runner_identity_cleanup_requires_confirmation` | duplicate discovery performs no revoke or delete by default |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Runner service is active | `ssh root@runbooksdev systemctl is-active agentsfleet-runner` | `active` | P0 | |
| R2 | Runner has no authorization loop | `ssh root@runbooksdev journalctl -u agentsfleet-runner -n 100 --no-pager` | heartbeat success and 0 repeated 401 entries | P0 | |
| R3 | API liveness is online | `curl -fsS -H "Authorization: Bearer $ADMIN_JWT" https://api-dev.agentsfleet.net/v1/fleets/runners | jq -e '.items[] | select(.host_id == env.RUNNER_HOST_ID and .liveness == "online")'` | exit 0 with intended runner online | P0 | |
| R4 | Operational docs use current auth and route | `rg -n 'publicMetadata\.platform_admin|/v1/fleet/runners' playbooks/operations/runner_onboarding/001_playbook.md` | no output | P0 | |
| S1 | Provisioner tests pass | `bash playbooks/founding/06_runner_bootstrap_dev/provision_runner_env_test.sh` | exit 0 | P0 | |
| S2 | Repository checks pass | `make lint-all` | exit 0 | P0 | |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S4 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted.

## Out of Scope

- Adding the raw runner token to an `agentsfleetd` deployment or Continuous Integration (CI) environment.
- Changing runner authentication protocol or liveness derivation.
- Deleting or revoking duplicate runner identities without explicit approval.

## Product Clarity (authoring record)

1. **Successful user moment** — the Runners screen changes from registered to online after a real heartbeat.
2. **Preserved user behaviour** — one-time dashboard mint and vault-to-host provisioning remain the operator flow.
3. **Optimal-way check** — proving API liveness is more direct than trusting systemd alone.
4. **Rebuild-vs-iterate** — repair configuration and stale docs; no runner redesign is needed.
5. **What we build** — redacted diagnosis, safe reprovisioning, heartbeat proof, corrected playbook.
6. **What we do NOT build** — token echo tools, self-enrollment, or hidden duplicate cleanup.
7. **Fit with existing features** — unblocks fleet leases, live acceptance, and deployment readiness.
8. **Surface order** — operator UI mints; shell provisioning installs; CLI or API proves.
9. **Dashboard restraint** — online appears only from fresh heartbeat-derived liveness.
10. **Confused-user next step** — the first failed boundary points to mint, vault, host env, service, or heartbeat.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream owns the credential boundary through liveness because splitting it would allow another service-active false green.
- **Alternatives considered:** injecting the raw token into the control-plane deployment was rejected because it violates machine-credential ownership.
- **Patch-vs-refactor verdict:** this is a **patch** because existing mint, provision, and heartbeat paths are correct and need synchronized state plus current guidance.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
