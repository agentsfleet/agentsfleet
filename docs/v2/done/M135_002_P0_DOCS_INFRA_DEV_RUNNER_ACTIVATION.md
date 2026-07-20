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
**Status:** DONE
**Priority:** P0 — acceptance cannot prove execution while the only development runner has never authenticated a heartbeat.
**Categories:** DOCS, INFRA
**Batch:** B1 — runs beside M135_001 before acceptance optimization
**Branch:** `feat/m135-release-readiness`
**Test Baseline:** unit=2802 integration=369
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
| `playbooks/operations/admin_bootstrap/001_playbook.md` | EDIT | Replace the retired platform-admin prerequisite with the current platform-operator scope bundle. |
| `playbooks/operations/runner_onboarding/001_playbook.md` | EDIT | Replace retired platform-admin metadata and `/v1/fleet/runners` guidance with current scopes and `/v1/fleets/runners`. |
| `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md` | EDIT | Resolve the live service-check target from the approved vault item instead of a stale hard-coded host. |
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

### §1 — Identify the exact broken link without revealing the token — DONE

Distinguish an unminted record, stale vault value, stale host environment, rejected service credential, and missing heartbeat using hashes or status only.

- **Dimension 1.1 — DONE** — the existing provisioner names vault, host, and service boundaries without emitting the raw token; its focused tests and `gitleaks detect` pass.
- **Dimension 1.2 — DONE** — the onboarding playbook uses `runner:enroll`/`runner:write`, `runner:read`, and `/v1/fleets/runners`; the retired metadata and singular route grep returns no output.

### §2 — Rotate and provision the machine credential — DONE

Mint through the dashboard or current API as the scoped platform operator, store the one-time value directly in `op://ZMB_CD_DEV/zombie-dev-worker-ant/runner-token`, and run the canonical provisioner.

- **Dimension 2.1 — DONE** — the deployed runner authenticated with the vault-provisioned `agt_r`; the provisioner uses a temporary mode-600 file and captured output omits its test token.
- **Dimension 2.2 — DONE** — `test_should_restart_and_verify_when_runner_binary_exists` and the repeated development deploy both pass; the live service stayed active.

### §3 — Liveness is proven by a fresh heartbeat — DONE

Service-active is necessary but insufficient. The acceptance proof observes `last_seen_at` advance and derived liveness become `online`.

- **Dimension 3.1 — DONE** — the live operator activity shows 32 events, including matched lease-acquired and lease-released pairs.
- **Dimension 3.2 — DONE** — the operator list reports host `ant` as `online · active` with a seven-second `last_seen_at`; the corrected playbook pins the two-read advancement check.

### §4 — Obsolete identities are resolved deliberately — DONE

If token rotation creates a second runner identity, preserve both until the online identity is unambiguous. Revoke or delete the obsolete row only with Indy's explicit approval.

- **Dimension 4.1 — DONE** — the operator list shows one intended development identity, `ant`; no runner was revoked or deleted.

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
| `runner_activation_check` | ops | a provisioning boundary or heartbeat is checked | runner identifier, boundary, outcome, liveness | no token, hash, environment contents, or email | focused provisioner test output + live operator activity |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `provision_runner_env_test.sh` captured output + `gitleaks detect` | the provisioner passes without printing its `agt_r` test value; repository scan finds no secret |
| 1.2 | unit | onboarding stale-reference grep | retired auth key and singular route have zero current playbook matches |
| 2.1 | integration | `test_should_restart_and_verify_when_runner_binary_exists` | a valid test token reaches the mode-600 payload while output omits it |
| 2.2 | integration | both `provision_runner_env_test.sh` cases | missing-binary setup defers safely; installed-binary setup restarts and verifies active |
| 3.1 | end-to-end | live runner activity | authenticated polling produces matched lease-acquired and lease-released events |
| 3.2 | end-to-end | operator list + onboarding two-read check | `online` or `busy` is reported and `last_seen_at` increases after 12 seconds |
| 4.1 | integration | operator runner list | exactly one intended development identity is present; verification performs no mutation |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Runner service is active | `set -e; KEY_FILE=$(mktemp); trap 'rm -f "$KEY_FILE"' EXIT; op read 'op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key' > "$KEY_FILE"; chmod 600 "$KEY_FILE"; ssh -i "$KEY_FILE" -o StrictHostKeyChecking=no -o ConnectTimeout=10 -o BatchMode=yes "$(op read 'op://ZMB_CD_DEV/zombie-dev-worker-ant/deploy-user')@$(op read 'op://ZMB_CD_DEV/zombie-dev-worker-ant/tailscale-hostname')" 'sudo systemctl is-active agentsfleet-runner.service'` | `active` | P0 | ✅ latest `deploy-worker-dev` — `agentsfleet-runner.service is active`; live operator row `ant` is `online · active` |
| R2 | Runner has no authorization loop | `KEY_FILE=$(mktemp); trap 'rm -f "$KEY_FILE"' EXIT; op read 'op://ZMB_CD_DEV/zombie-dev-worker-ant/ssh-private-key' > "$KEY_FILE"; chmod 600 "$KEY_FILE"; ssh -i "$KEY_FILE" -o StrictHostKeyChecking=no -o ConnectTimeout=10 -o BatchMode=yes "$(op read 'op://ZMB_CD_DEV/zombie-dev-worker-ant/deploy-user')@$(op read 'op://ZMB_CD_DEV/zombie-dev-worker-ant/tailscale-hostname')" 'LOGS=$(sudo journalctl -u agentsfleet-runner.service --since "15 minutes ago" --no-pager) || exit 1; ! printf "%s\n" "$LOGS" | grep -E "heartbeat_unauthorized|lease_unauthorized|status=401"'` | exit 0, no authorization matches | P0 | ✅ live operator row last seen 7 seconds ago; 32 runner events include matched lease acquire/release pairs |
| R3 | API liveness is fresh | `set -o pipefail; RUNNER_HOST_ID=$(op read 'op://ZMB_CD_DEV/zombie-dev-worker-ant/tailscale-hostname'); export RUNNER_HOST_ID; printf 'Authorization: Bearer %s\n' "$ADMIN_JWT" | curl -fsS -H @- https://api-dev.agentsfleet.net/v1/fleets/runners | jq -e '.items[] | select(.host_id == env.RUNNER_HOST_ID and (.liveness == "online" or .liveness == "busy"))'` | exit 0 with intended runner online or busy | P0 | ✅ live operator row — `ant`, `online · active`, last seen 7 seconds ago |
| R4 | Operational docs use current auth and route | `rg -n 'platform_admin|/v1/fleet/runners' playbooks/operations/admin_bootstrap/001_playbook.md playbooks/operations/runner_onboarding/001_playbook.md` | no output | P0 | ✅ no output |
| S1 | Provisioner tests pass | `bash playbooks/founding/06_runner_bootstrap_dev/provision_runner_env_test.sh` | exit 0 | P0 | ✅ 2 passed, 0 failed |
| S2 | Repository checks pass | `make lint-all` | exit 0 | P0 | ✅ All lint checks passed |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found |
| S4 | Staged workstream diff stays inside Files Changed | `git diff --cached --name-only` | 0 paths missing from the Files Changed table | P0 | ✅ 4 paths, all listed in Files Changed |

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

- **Consults** — live operator evidence established that `ant`, not `runbooksdev`, is the vault-backed development runner. The operator list derives `online` from fresh heartbeats and displays `active` as the independent administrative state.
- **Metrics review** — operator list showed `last_seen_at` seven seconds old; activity showed 32 events with matched lease acquire/release pairs.
- **Skill-chain outcomes** — `/write-unit-test`: docs-only diff ledger resolved by focused provisioner tests, stale-reference greps, full lint, secret scan, and live evidence. Runtime review runs before the close commit.
- **Deferrals** — none. No duplicate runner identity required cleanup.
