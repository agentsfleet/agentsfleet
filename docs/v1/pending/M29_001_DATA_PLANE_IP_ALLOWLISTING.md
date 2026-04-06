# M29_001: Data-Plane IP Allowlisting

**Prototype:** v1.0.0
**Milestone:** M29
**Workstream:** 001
**Date:** Apr 05, 2026
**Status:** PENDING
**Priority:** P1 — close a high-impact credential-exposure gap on the production data plane
**Batch:** B1
**Depends on:** M7_001 (Fly.io deploy pipeline green), M7_005 (network connectivity baseline)

---

## Overview

**Goal (testable):** PlanetScale and Upstash allowlists accept connections only from approved Fly.io and OVH egress IPs, reject all others, and the apply workflow is idempotent and vault-driven.

**Problem:** Data-plane endpoints (DATABASE_URL, REDIS_URL) currently accept connections from any source IP. A leaked credential grants immediate access without any network-layer gate.

**Solution summary:** Define a deterministic IP allowlist contract backed by 1Password vault, implement an idempotent CLI-first apply+verify workflow for both PlanetScale and Upstash (with strict dev/prod separation), and harden with break-glass and rotation runbooks. The outcome is that only known infrastructure egress IPs can reach data-plane services.

---

## 1.0 Provider Allowlist Contract

**Status:** PENDING

Define one deterministic allowlist contract for PlanetScale and Upstash so only known infra egress IPs can access data-plane endpoints.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `scripts/allowlist-apply.sh:enumerate_egress_sources`
  - input: `op read 'op://ops/fly-egress-ips/ips'` + `op read 'op://ops/ovh-worker-ips/ips'`
  - expected: structured list of all egress IPs with ownership labels (fly-control-plane, ovh-executor) and environment tags (dev, prod)
  - test_type: contract
- 1.2 PENDING
  - target: `op://ops/fly-egress-ips`, `op://ops/ovh-worker-ips`
  - input: vault item read
  - expected: non-empty JSON arrays; every entry is a valid IPv4 CIDR; items have `updated_at` metadata for audit trail
  - test_type: contract
- 1.3 PENDING
  - target: `scripts/allowlist-apply.sh:map_ips_to_providers`
  - input: enumerated IP sets + provider config (PlanetScale org/project, Upstash DB ID)
  - expected: mapping table: each IP → each provider allowlist endpoint, with explicit environment separation (dev IPs → dev allowlist, prod IPs → prod allowlist, no cross-env writes)
  - test_type: unit
- 1.4 PENDING
  - target: PlanetScale/Upstash allowlist enforcement
  - input: connection attempt from non-allowlisted IP
  - expected: connection refused with actionable error (provider-level deny); no silent fallback to open access
  - test_type: integration

---

## 2.0 Agent-Executable Apply Workflow

**Status:** PENDING

Implement an idempotent CLI-first workflow that reads expected IPs from vault, applies provider allowlists, and verifies effective enforcement.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `scripts/allowlist-apply.sh:apply_planetscale`
  - input: IP set from vault + PlanetScale org/project target + environment (dev|prod)
  - expected: PlanetScale allowlist matches vault state; `make` entrypoint exits 0 on success with structured JSON output
  - test_type: integration
- 2.2 PENDING
  - target: `scripts/allowlist-apply.sh:apply_upstash`
  - input: IP set from vault + Upstash DB ID + environment (dev|prod)
  - expected: Upstash allowlist matches vault state; strict env targeting (no mixed-environment writes); `make` entrypoint exits 0
  - test_type: integration
- 2.3 PENDING
  - target: `scripts/allowlist-apply.sh:apply_all`
  - input: run apply twice with identical vault state
  - expected: second run produces no-op; deterministic output; exit 0; diff shows zero changes
  - test_type: integration
- 2.4 PENDING
  - target: `scripts/allowlist-apply.sh:verify`
  - input: post-apply state
  - expected: structured JSON output with fields: `{provider, environment, configured_ips[], expected_ips[], drift: bool, drift_details[]}`
  - test_type: contract

---

## 3.0 Operational Safety and Evidence

**Status:** PENDING

Harden operator workflow to prevent lockout and make recovery explicit.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `docs/runbooks/allowlist-break-glass.md`
  - input: emergency access scenario (e.g., new IP needed immediately)
  - expected: runbook defines: temporary one-time IP entry with mandatory expiration/removal, audit trail via vault, and post-incident cleanup steps
  - test_type: contract
- 3.2 PENDING
  - target: `scripts/allowlist-apply.sh:preflight_check`
  - input: local runner IP + current allowlist state
  - expected: if runner IP is not in allowlist, script exits 1 with error naming the runner IP and instructions to add it before proceeding; never applies restrictive update that would lock out the operator
  - test_type: unit
- 3.3 PENDING
  - target: `docs/v1/evidence/m29_001_allowlist_verification.md`
  - input: post-apply connectivity tests
  - expected: evidence file contains: successful connection logs from allowlisted hosts, denied connection logs from non-allowlisted source, timestamps, and provider names
  - test_type: contract
- 3.4 PENDING
  - target: `docs/runbooks/allowlist-ip-rotation.md`
  - input: IP change scenario (Fly egress change, worker host replacement, DR failover)
  - expected: runbook covers: vault update → apply → verify cycle for each rotation scenario; no manual provider console steps
  - test_type: contract

---

## 4.0 Interfaces

**Status:** PENDING

Lock the API surface for the allowlist workflow.

### 4.1 Public Functions

```bash
# Main entrypoints (make targets)
make allowlist-apply ENV=dev|prod      # Apply allowlists from vault to providers
make allowlist-verify ENV=dev|prod     # Verify current state matches vault
make allowlist-diff ENV=dev|prod       # Show drift without applying

# Script functions
allowlist-apply.sh enumerate_egress_sources  # Read IPs from vault
allowlist-apply.sh apply_planetscale <env>   # Apply to PlanetScale
allowlist-apply.sh apply_upstash <env>       # Apply to Upstash
allowlist-apply.sh verify <env>              # Verify enforcement
allowlist-apply.sh preflight_check           # Check runner IP is allowlisted
```

### 4.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| ENV | string | required; one of `dev`, `prod` | `prod` |
| Vault path (Fly IPs) | op:// ref | must exist and be non-empty JSON array of CIDRs | `op://ops/fly-egress-ips/ips` |
| Vault path (OVH IPs) | op:// ref | must exist and be non-empty JSON array of CIDRs | `op://ops/ovh-worker-ips/ips` |
| PlanetScale target | op:// ref | org + project identifiers per env | `op://ops/planetscale-dev/org` |
| Upstash target | op:// ref | DB identifier per env | `op://ops/upstash-dev/db-id` |

### 4.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| status | string | always | `"applied"`, `"no-op"`, `"drift-detected"` |
| provider | string | always | `"planetscale"`, `"upstash"` |
| environment | string | always | `"dev"`, `"prod"` |
| configured_ips | string[] | always | `["1.2.3.4/32", "5.6.7.8/32"]` |
| drift | bool | on verify | `false` |
| drift_details | string[] | on drift | `["missing: 1.2.3.4/32", "extra: 9.9.9.9/32"]` |

### 4.4 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Vault item missing | Script exits 1 immediately | `ERROR: op://ops/fly-egress-ips/ips not found — add Fly egress IPs to vault` |
| Vault item empty | Script exits 1 immediately | `ERROR: op://ops/fly-egress-ips/ips is empty — populate with CIDR list` |
| Invalid CIDR format | Script exits 1 before apply | `ERROR: '999.0.0.0/33' is not valid CIDR` |
| Provider API auth failure | Script exits 1 | `ERROR: PlanetScale API auth failed — check op://ops/planetscale-dev/api-token` |
| Runner IP not in allowlist | Script exits 1 (preflight) | `ERROR: Runner IP 10.0.0.1 not in allowlist — add before applying restrictive rules` |
| Mixed env write attempt | Script exits 1 | `ERROR: Refusing to write dev IPs to prod allowlist` |

---

## 5.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Vault unreachable | `op` CLI not authenticated or network down | Script exits 1 at enumeration step | `ERROR: op read failed — run 'op signin' first` |
| Partial apply | Provider API timeout mid-apply (e.g., PlanetScale succeeds, Upstash fails) | Script exits 1 with partial state; re-run is safe (idempotent) | `ERROR: Upstash apply failed after PlanetScale succeeded — re-run to converge` |
| Self-lockout | Operator applies restrictive list without own IP | Preflight check prevents this; exits 1 before any apply | `ERROR: Runner IP not in allowlist` |
| Provider API rate limit | Too many API calls in short window | Script retries with backoff (3 attempts), then exits 1 | `ERROR: PlanetScale rate limited — retry in 60s` |
| IP rotation mid-flight | Fly/OVH changes egress IP while apply is running | Idempotent re-run with updated vault state converges | Operator updates vault, re-runs apply |

**Platform constraints:**
- PlanetScale allowlist API is org-scoped; changes affect all databases in the org — environment separation relies on separate orgs or projects per env
- Upstash allowlist is per-database; each DB allowlist is independent

---

## 6.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Zero hardcoded IPs or credentials in scripts/docs | `grep -rn '[0-9]\{1,3\}\.[0-9]\{1,3\}\.[0-9]\{1,3\}\.[0-9]\{1,3\}' scripts/allowlist-*.sh` returns 0 matches |
| All credentials read from vault at runtime | `grep -c 'op read' scripts/allowlist-apply.sh` > 0; no env var credential reads |
| Idempotent reruns | Run apply twice; diff of provider state is empty |
| Strict env separation | Script refuses cross-env writes; test with `ENV=prod` but dev vault paths |
| Script under 500 lines | `wc -l scripts/allowlist-apply.sh` < 500 |
| Structured JSON output | `scripts/allowlist-apply.sh verify dev \| jq .` parses without error |

---

## 7.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| test_cidr_validation | 1.2 | validate_cidr | `"999.0.0.0/33"` | exit 1, error message |
| test_env_separation | 1.3 | map_ips_to_providers | dev IPs + prod target | exit 1, refuses cross-env |
| test_preflight_runner_not_in_list | 3.2 | preflight_check | runner IP absent from allowlist | exit 1, names runner IP |
| test_preflight_runner_in_list | 3.2 | preflight_check | runner IP present in allowlist | exit 0 |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| test_planetscale_apply | 2.1 | PlanetScale dev + vault | vault IPs | allowlist matches vault state |
| test_upstash_apply | 2.2 | Upstash dev + vault | vault IPs | allowlist matches vault state |
| test_idempotent_rerun | 2.3 | both providers + vault | run apply twice | second run is no-op |
| test_deny_unknown_ip | 1.4 | both providers | connect from non-listed IP | connection refused |
| test_verify_no_drift | 2.4 | both providers + vault | post-apply | drift: false |

### Contract Tests

| Test name | Dimension | What it proves |
|-----------|-----------|---------------|
| test_verify_output_schema | 2.4 | verify output matches JSON contract (provider, env, ips, drift) |
| test_vault_ip_format | 1.2 | vault items contain valid CIDR arrays |
| test_enumeration_labels | 1.1 | enumerated IPs have ownership and environment tags |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| Only approved IPs can connect | test_deny_unknown_ip | integration |
| Workflow is idempotent | test_idempotent_rerun | integration |
| Workflow is vault-driven | test_vault_ip_format + no hardcoded IPs constraint | contract + lint |
| Rejects non-allowlisted sources | test_deny_unknown_ip | integration |

---

## 8.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Confirm vault items exist: `op read 'op://ops/fly-egress-ips/ips'`, `op read 'op://ops/ovh-worker-ips/ips'` | non-empty JSON arrays returned |
| 2 | Create `scripts/allowlist-apply.sh` with enumerate, validate, apply, verify, preflight functions | `shellcheck scripts/allowlist-apply.sh` passes |
| 3 | Add `make allowlist-apply`, `make allowlist-verify`, `make allowlist-diff` targets | `make -n allowlist-apply ENV=dev` succeeds |
| 4 | Apply to PlanetScale dev | `make allowlist-verify ENV=dev` shows drift: false for PlanetScale |
| 5 | Apply to Upstash dev | `make allowlist-verify ENV=dev` shows drift: false for Upstash |
| 6 | Test deny from non-listed IP | connection attempt fails with expected error |
| 7 | Apply to prod (PlanetScale + Upstash) | `make allowlist-verify ENV=prod` shows drift: false |
| 8 | Write break-glass and rotation runbooks | files exist at expected paths |
| 9 | Capture evidence in `docs/v1/evidence/` | evidence file contains success + deny logs |

---

## 9.0 Acceptance Criteria

**Status:** PENDING

- [ ] PlanetScale and Upstash allowlists configured for both dev and prod using only approved Fly/OVH source IPs — verify: `make allowlist-verify ENV=dev && make allowlist-verify ENV=prod`
- [ ] Connectivity succeeds from allowlisted sources and fails from non-allowlisted source — verify: evidence file in `docs/v1/evidence/`
- [ ] Re-running apply with identical inputs produces no config drift and exits cleanly — verify: `make allowlist-apply ENV=dev` second run shows `"status": "no-op"`
- [ ] Vault is the single handoff interface; no literal credentials or IPs hardcoded in scripts/docs — verify: `grep -rn '[0-9]\{1,3\}\.[0-9]\{1,3\}' scripts/allowlist-*.sh` returns 0
- [ ] Break-glass and IP rotation runbooks exist — verify: `ls docs/runbooks/allowlist-break-glass.md docs/runbooks/allowlist-ip-rotation.md`

---

## 10.0 Verification Evidence

**Status:** PENDING

Filled in during VERIFY phase.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Vault items populated | `op read 'op://ops/fly-egress-ips/ips' \| jq length` | | |
| Dev allowlist applied | `make allowlist-verify ENV=dev` | | |
| Prod allowlist applied | `make allowlist-verify ENV=prod` | | |
| Idempotent rerun | `make allowlist-apply ENV=dev` (2nd run) | | |
| Deny from unknown IP | connection attempt from non-listed source | | |
| No hardcoded IPs | `grep -rn` check | | |
| Shellcheck | `shellcheck scripts/allowlist-apply.sh` | | |
| 500L gate | `wc -l scripts/allowlist-apply.sh` | | |
| Runbooks exist | `ls docs/runbooks/allowlist-*.md` | | |

---

## 11.0 Out of Scope

- Introducing private link/VPC peering between providers
- Replacing provider-managed auth with mutual TLS
- Runtime secret rotation outside allowlist scope
- Per-workspace data-plane segmentation
