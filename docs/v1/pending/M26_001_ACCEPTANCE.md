# M26_001: Acceptance Gate for DEV and PROD Operator Flows

**Prototype:** v1.0.0
**Milestone:** M26
**Workstream:** 001
**Date:** Apr 06, 2026
**Status:** PENDING
**Priority:** P0 — release gate; v1 cannot be considered operator-ready until DEV and PROD flows are executed with evidence
**Batch:** B1
**Depends on:** M7_001_DEV_ACCEPTANCE_ENVIRONMENT (DONE), M7_003_PROD_ACCEPTANCE_ENVIRONMENT (DONE), M7_005_NETWORK_CONNECTIVITY (DONE), M21_001 (interrupt flow), M22_001 (watch/replay), M28_001 (billing/observability)

---

## Overview

**Goal (testable):** A human operator can use the current repo-local `zombiectl` and deployed surfaces to complete the canonical acceptance flow on DEV and PROD, with worker-fleet evidence, release evidence, UI smoke evidence, and captured artifacts sufficient for sign-off.

**Problem:** The current pending spec is a carry-forward checklist from older acceptance milestones. It predates the current CLI contract, still uses stale assumptions like `docs/spec/`, and does not lock the exact evidence, commands, metrics, and failure handling required by the current repo. That makes the acceptance gate ambiguous and easy to "pass" without durable proof.

**Solution summary:** Rewrite the acceptance gate as a current-format operational spec. The gate covers four verifiable slices: worker fleet observability, DEV CLI acceptance, PROD release/fleet/UI acceptance, and evidence capture. All commands and contracts reference the current repo-local CLI, current endpoints, current watch/replay flows, and actual observability signals.

---

## 1.0 Worker Fleet Observability Gate

**Status:** PENDING

Before operator acceptance can be trusted, the platform must expose worker-fleet liveness without requiring ad hoc SSH inspection. The existing `zombie_worker_running` gauge reflects only local process state and is insufficient for bare-metal fleet verification.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/observability/metrics_render.zig` and worker heartbeat/registration path
  - input: `DEV and PROD workers polling normally`
  - expected: `/metrics` exposes a fleet-level gauge such as `zombie_active_workers` or equivalent documented replacement`
  - test_type: integration
- 1.2 PENDING
  - target: `https://api-dev.usezombie.com/metrics`
  - input: `live DEV metrics scrape`
  - expected: `fleet-level worker gauge reports >= 1 connected worker`
  - test_type: contract
- 1.3 PENDING
  - target: `https://api.usezombie.com/metrics`
  - input: `live PROD metrics scrape`
  - expected: `fleet-level worker gauge reports >= 2 connected workers or documented current production target`
  - test_type: contract
- 1.4 PENDING
  - target: `docs/observability/overview — see docs/grafana.md and docs.usezombie.com/operator/observability/overview`
  - input: `worker metric documentation review`
  - expected: `fleet-level worker metric name, semantics, and operator interpretation are documented`
  - test_type: contract

---

## 2.0 DEV CLI Acceptance Flow

**Status:** PENDING

Execute the canonical operator flow against DEV using the current repo-local CLI, not a hypothetical published package and not stale path assumptions.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `./zombiectl/bin/zombiectl.js login`
  - input: `DEV API base URL, valid operator account`
  - expected: `token saved locally and login completes without manual config edits`
  - test_type: integration
- 2.2 PENDING
  - target: `./zombiectl/bin/zombiectl.js workspace add <repo_url> --json`
  - input: `acceptance repository URL`
  - expected: `JSON output includes workspace_id and install_url; workspace becomes current workspace`
  - test_type: integration
- 2.3 PENDING
  - target: `./zombiectl/bin/zombiectl.js specs sync --workspace-id <id> --json`
  - input: `workspace_id from 2.2`
  - expected: `JSON output includes synced_count, total_pending, and no auth/billing failure`
  - test_type: integration
- 2.4 PENDING
  - target: `./zombiectl/bin/zombiectl.js run --workspace-id <id> --spec-id <spec_id> --watch --json` or documented equivalent split flow
  - input: `workspace_id plus a known synced spec_id`
  - expected: `run is queued, reaches terminal state, and yields observable completion evidence without manual DB inspection`
  - test_type: integration
- 2.5 PENDING
  - target: `./zombiectl/bin/zombiectl.js runs list --workspace-id <id> --json` and `run status <run_id> --json`
  - input: `workspace_id and run_id from 2.4`
  - expected: `run record includes terminal state and PR linkage or explicit terminal failure reason`
  - test_type: integration
- 2.6 PENDING
  - target: `./zombiectl/bin/zombiectl.js doctor --json`
  - input: `authenticated local CLI environment`
  - expected: `machine-parseable diagnostics payload with no blocking failures for acceptance prerequisites`
  - test_type: contract

---

## 3.0 PROD Release, Worker, and UI Gate

**Status:** PENDING

PROD acceptance is not just "the API is up". It requires release artifact proof, worker rollout proof, and user-facing smoke proof.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `release.yml` and GitHub release artifacts
  - input: `release tag matching VERSION`
  - expected: `verify-tag, binaries, docker publish, npm publish, and GitHub release jobs all succeed`
  - test_type: integration
- 3.2 PENDING
  - target: `worker rollout on PROD nodes`
  - input: `deploy-prod flow or documented replacement`
  - expected: `workers start, connect to dependencies, and consume queueable work after rollout`
  - test_type: integration
- 3.3 PENDING
  - target: `https://app.usezombie.com` and `https://usezombie.com`
  - input: `live PROD URLs`
  - expected: `app loads, auth entry point is reachable, website assets load without broken bundles`
  - test_type: integration
- 3.4 PENDING
  - target: `./zombiectl/bin/zombiectl.js` against `https://api.usezombie.com`
  - input: `same canonical flow as DEV`
  - expected: `login, workspace add, specs sync, run, run status/list, and doctor all complete with evidence`
  - test_type: integration

---

## 4.0 Evidence Capture and Sign-Off Artifact

**Status:** PENDING

Acceptance is incomplete until another agent can audit the run from files alone.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `docs/v1/evidence/` acceptance artifact for this milestone
  - input: `DEV and PROD command transcripts, CI links, release URLs, metrics snapshots`
  - expected: `single evidence document records commands, outputs, timestamps, and links`
  - test_type: contract
- 4.2 PENDING
  - target: `DEV evidence set`
  - input: `login/workspace/specs/run/list/doctor outputs plus metrics scrape`
  - expected: `all DEV evidence stored with command context and redacted secrets`
  - test_type: contract
- 4.3 PENDING
  - target: `PROD evidence set`
  - input: `release job IDs, release URL, worker evidence, UI smoke, CLI flow outputs, metrics scrape`
  - expected: `all PROD evidence stored with command context and redacted secrets`
  - test_type: contract
- 4.4 PENDING
  - target: `latency proof`
  - input: `spec sync timestamp and PR creation timestamp for acceptance run`
  - expected: `spec-to-PR latency is recorded and compared to the gate threshold`
  - test_type: contract

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 Operator Command Contract

```bash
export ZOMBIE_API_URL=https://api-dev.usezombie.com
./zombiectl/bin/zombiectl.js login
./zombiectl/bin/zombiectl.js workspace add <ACCEPTANCE_REPO_URL> --json
./zombiectl/bin/zombiectl.js specs sync --workspace-id <WORKSPACE_ID> --json
./zombiectl/bin/zombiectl.js run --workspace-id <WORKSPACE_ID> --spec-id <SPEC_ID> --watch
./zombiectl/bin/zombiectl.js run status <RUN_ID> --json
./zombiectl/bin/zombiectl.js runs list --workspace-id <WORKSPACE_ID> --json
./zombiectl/bin/zombiectl.js doctor --json
```

### 5.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `ACCEPTANCE_REPO_URL` | Git remote URL | GitHub repo reachable by the GitHub App install flow | `https://github.com/usezombie/acceptance-repo` |
| `WORKSPACE_ID` | UUIDv7 string | Returned by `workspace add` or `workspace list` | `0195...` |
| `SPEC_ID` | UUIDv7 string | Must belong to synced spec in the target workspace | `0195...` |
| `RUN_ID` | UUIDv7 string | Returned by `run` | `0195...` |
| `metrics_url` | HTTPS URL | Must point at environment API `/metrics` endpoint | `https://api-dev.usezombie.com/metrics` |

### 5.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `workspace_id` | string | `workspace add --json` success | `0195...` |
| `install_url` | string | `workspace add --json` success | `https://github.com/apps/...` |
| `synced_count` | integer | `specs sync --json` success | `3` |
| `run_id` | string | `run` success | `0195...` |
| `state` / `current_state` | string | `run` / `run status` / `runs list` | `DONE` |
| `pr_url` | string or null | terminal successful run | `https://github.com/usezombie/.../pull/123` |
| `doctor` payload | JSON object | `doctor --json` success | `{ checks: [...], ok: true }` |

### 5.4 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Auth missing or expired | CLI/API flow halts before mutation | structured CLI/API auth error |
| Billing or credit block | acceptance run rejected | explicit error code/message in JSON or stderr |
| Workspace not bound after GitHub install | `specs sync` / `run` cannot proceed | workspace/scm binding error |
| Worker fleet unavailable | run stays queued or fails to progress | queue/worker evidence fails, acceptance gate blocked |
| UI smoke regression | app or website load fails | smoke evidence records failing URL/build |

---

## 6.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Stale CLI flow | Spec uses obsolete flags or paths | operator follows wrong commands | acceptance evidence is invalid or irreproducible |
| False worker health | only `zombie_worker_running` is observed | API reports local liveness but not fleet state | operator cannot prove bare-metal consumption |
| Release-only pass | CI release is green but no live operator flow is run | artifacts exist without end-to-end validation | acceptance gate remains open |
| UI-only pass | URLs load but worker/CLI path is broken | smoke appears green while operator flow fails | acceptance gate remains open |
| Evidence gap | commands are run but outputs are not captured durably | future agent cannot verify the gate | sign-off blocked |

**Platform constraints:**
- Acceptance evidence must never include raw secrets, tokens, or credential values.
- PROD acceptance must use the deployed PROD surfaces; DEV acceptance cannot stand in for PROD.

---

## 7.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Acceptance flow uses current repo-local CLI syntax | `./zombiectl/bin/zombiectl.js --help` and command-specific help/output |
| Worker fleet proof comes from metrics or equivalent documented API signal | `curl -sf $ZOMBIE_API_URL/metrics | grep <worker_metric_name>` |
| Evidence is stored in repo docs, not chat | verify acceptance evidence file exists under `docs/v1/evidence/` |
| Latency gate is measured from captured timestamps, not anecdote | compare recorded run/PR timestamps in evidence artifact |
| CLI machine output is parseable where specified | pipe `--json` commands through `jq` during acceptance |

---

## 8.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `worker_metric_documented` | 1.4 | observability docs | metric doc review | fleet worker metric documented |

### Integration Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `dev_cli_acceptance_flow` | 2.1-2.6 | DEV API + repo-local CLI | acceptance repo + operator account | canonical flow completes with evidence |
| `prod_release_gate` | 3.1 | GitHub Actions release pipeline | matching tag/version | release jobs all green |
| `prod_worker_rollout_gate` | 3.2 | PROD worker rollout | deployed worker nodes | workers connect and consume work |
| `prod_ui_smoke_gate` | 3.3 | PROD app + website | live URLs | smoke checks pass |
| `prod_cli_acceptance_flow` | 3.4 | PROD API + repo-local CLI | acceptance repo + operator account | canonical flow completes with evidence |

### Contract Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `metrics_exposes_worker_fleet_signal` | 1.1-1.3 | `/metrics` endpoint | live scrape | active worker count is observable |
| `evidence_artifact_complete` | 4.1-4.4 | evidence document | captured outputs/links | another agent can audit gate from file alone |

### Spec-Claim Tracing

| Claim | Proved by |
|------|-----------|
| DEV operator flow works end to end | `dev_cli_acceptance_flow` |
| PROD release is real, not just merged code | `prod_release_gate` |
| PROD workers are actually consuming queue work | `prod_worker_rollout_gate`, `metrics_exposes_worker_fleet_signal` |
| Acceptance evidence is durable | `evidence_artifact_complete` |

---

## 9.0 Verification Evidence

**Status:** PENDING

| Command | Purpose | Evidence placeholder |
|---------|---------|----------------------|
| `./zombiectl/bin/zombiectl.js workspace add <url> --json | jq .` | DEV/PROD workspace create parseability | PENDING |
| `./zombiectl/bin/zombiectl.js specs sync --workspace-id <id> --json | jq .` | specs sync parseability | PENDING |
| `./zombiectl/bin/zombiectl.js run status <run_id> --json | jq .` | run terminal proof | PENDING |
| `./zombiectl/bin/zombiectl.js doctor --json | jq .` | doctor parseability and environment readiness | PENDING |
| `curl -sf https://api-dev.usezombie.com/metrics | grep zombie_` | DEV metrics proof | PENDING |
| `curl -sf https://api.usezombie.com/metrics | grep zombie_` | PROD metrics proof | PENDING |
| `gh run view <run-id>` | release pipeline proof | PENDING |

---

## 10.0 Acceptance Criteria

**Status:** PENDING

- [ ] 10.1 Worker fleet observability exposes a documented environment-level worker count signal
- [ ] 10.2 DEV operator flow completes with current repo-local CLI: login → workspace add → specs sync → run → run status/list → doctor
- [ ] 10.3 PROD release pipeline is green with release artifacts, binaries, image publish, and npm publish evidence
- [ ] 10.4 PROD worker rollout is verified by both rollout evidence and live worker-fleet signal
- [ ] 10.5 PROD app and website smoke checks pass
- [ ] 10.6 PROD operator flow completes with current repo-local CLI
- [ ] 10.7 Acceptance evidence artifact is complete and auditable
- [ ] 10.8 Spec-to-PR latency is captured and meets the target threshold or is explicitly recorded as a blocker

---

## 11.0 Out of Scope

- New feature implementation unrelated to acceptance proof
- Multi-region rollout beyond current v1 deployment contract
- Long-duration soak testing
- Desktop/mobile-specific acceptance flows beyond current web/CLI/operator surfaces
