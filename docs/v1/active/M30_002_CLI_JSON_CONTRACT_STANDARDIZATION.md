# M30_002: CLI JSON Contract Standardization

**Prototype:** v1.0.0
**Milestone:** M30
**Workstream:** 002
**Date:** Apr 06, 2026
**Status:** IN_PROGRESS
**Branch:** feat/m30-cli-json-contract
**Priority:** P1 — agent and automation consumers need stable machine output across the real current `zombiectl` command surface
**Batch:** B2
**Depends on:** M30_001 (DB schema migration gate), M4_001 (CLI baseline), M21_001 (interrupt surface), M22_001 (watch/replay), M28_001 (workspace billing JSON)

---

## Overview

**Goal (testable):** Every current machine-usable `zombiectl` command has a documented JSON contract, stable success/error envelope rules, and contract tests proving parseability without banners, ANSI, or route mismatches.

**Problem:** The current pending spec is generic and does not describe the actual CLI surface that exists today. The repo already has partial JSON support across `workspace`, `run`, `runs list/cancel/replay`, `doctor`, `harness`, `agent`, and `admin`, but the surface is inconsistent: some commands are explicitly JSON-tested, some only incidentally print raw API payloads, and at least one subcommand path (`runs interrupt`) has implementation code but no route registration. Without a current contract, agents cannot depend on `zombiectl` safely.

**Solution summary:** Rewrite the spec around the real command surface. First inventory the currently routed commands and their JSON behavior. Then define a uniform contract for success, error, and stream-incompatible commands. Finally, implement missing JSON support and route wiring, add golden tests, and document which commands are guaranteed stable for automation.

---

## 1.0 Current CLI JSON Inventory

**Status:** PENDING

The first step is to inventory the routed command surface, not just commands that happen to mention `--json` in help strings.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `zombiectl/src/program/routes.js` and `zombiectl/src/program/command-registry.js`
  - input: `current routed command tree`
  - expected: `inventory lists every routed command and whether JSON mode is reachable`
  - test_type: contract
- 1.2 PENDING
  - target: `zombiectl/src/commands/*.js`
  - input: `all currently implemented command handlers`
  - expected: `inventory distinguishes routed commands from implemented-but-unreachable commands such as route mismatches`
  - test_type: contract
- 1.3 PENDING
  - target: `zombiectl/README.md` and help text
  - input: `documented command surface`
  - expected: `README/help inventory matches routed command inventory or the mismatch is explicitly called out as a defect`
  - test_type: contract
- 1.4 PENDING
  - target: `inventory matrix artifact in spec/docs`
  - input: `command groups: workspace, specs, run, runs, doctor, harness, agent, admin`
  - expected: `each command classified as list/status/mutation/streaming/interactive and marked required/excluded for JSON`
  - test_type: contract

---

## 2.0 Canonical JSON Contract

**Status:** PENDING

Define the machine-output rules for the current CLI instead of assuming "whatever the API returned" is good enough.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `zombiectl/src/program/io.js`, `zombiectl/src/cli.js`, and command output policy
  - input: `global --json mode`
  - expected: `JSON mode suppresses banners, pre-release prose, ANSI tables, and human-only hints on stdout`
  - test_type: unit
- 2.2 PENDING
  - target: `all machine-usable command handlers`
  - input: `successful command execution in JSON mode`
  - expected: `success shape is documented per command and either follows a common envelope or an explicitly versioned raw payload policy`
  - test_type: contract
- 2.3 PENDING
  - target: `CLI error printing path`
  - input: `usage error, auth error, API error, validation error`
  - expected: `JSON mode emits structured error payloads with stable error.code and non-zero exit status`
  - test_type: unit
- 2.4 PENDING
  - target: `streaming and interactive commands`
  - input: `commands like --watch or interactive login flows`
  - expected: `spec explicitly states whether JSON mode is supported, degraded, or prohibited for each command type`
  - test_type: contract

---

## 3.0 Command Surface Standardization

**Status:** PENDING

This section closes the actual gaps found in inventory rather than adding `--json` indiscriminately.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `zombiectl/src/program/routes.js`, `zombiectl/src/program/command-registry.js`, `zombiectl/src/commands/runs.js`
  - input: `runs subcommand surface`
  - expected: `implemented JSON-capable subcommands such as `runs interrupt` are either routed and tested or explicitly removed from the supported contract`
  - test_type: integration
- 3.2 PENDING
  - target: `workspace`, `run`, `runs`, `doctor`, `harness`, `agent`, and `admin` command handlers
  - input: `JSON mode execution`
  - expected: `all agent-usable commands have stable JSON output and no human-only noise on stdout`
  - test_type: integration
- 3.3 PENDING
  - target: `usage/validation/auth failure paths`
  - input: `missing args, invalid IDs, missing auth, API failures`
  - expected: `every supported JSON command returns structured JSON errors and non-zero exit codes`
  - test_type: integration
- 3.4 PENDING
  - target: `zombiectl/test/*`
  - input: `golden output and parseability checks`
  - expected: `contract tests cover both success and negative paths for each supported JSON command group`
  - test_type: contract

---

## 4.0 Documentation and Automation Guidance

**Status:** PENDING

Once the contract exists, the repo documentation must tell operators and agents which commands are safe to automate.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `zombiectl/README.md`
  - input: `supported JSON command inventory`
  - expected: `README lists supported JSON commands and notes any exclusions for interactive/streaming flows`
  - test_type: contract
- 4.2 PENDING
  - target: `CLI help output and/or docs page`
  - input: `--help surface`
  - expected: `help text does not advertise unsupported JSON behavior`
  - test_type: contract
- 4.3 PENDING
  - target: `automation guidance doc or README section`
  - input: `agent/operator consumption guidance`
  - expected: `docs recommend stable commands for scripts and explain exit-code + stderr/stdout behavior in JSON mode`
  - test_type: contract

---

## 5.0 Interfaces

**Status:** PENDING

### 5.1 Supported JSON Command Classes

```text
workspace add|list|remove|billing|upgrade-scale
spec init
specs sync
run
run status
runs list|cancel|replay|interrupt
doctor
harness ...
agent ...
admin ...
```

### 5.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `--json` | global flag | Applies to routed command unless explicitly excluded by spec | `zombiectl --json runs list` |
| command args | route-specific | Must satisfy existing validation rules | `run status <run_id>` |
| auth token | credential | Required for non-auth-exempt routes | `~/.config/zombiectl/credentials.json` |

### 5.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| success payload | JSON object | supported command succeeds | `{ "run_id": "...", "state": "QUEUED" }` |
| error payload | JSON object | supported command fails in JSON mode | `{ "error": { "code": "AUTH_REQUIRED", "message": "..." } }` |
| stdout behavior | stream | JSON mode | stdout contains only JSON payload |
| stderr behavior | stream | JSON mode error or diagnostics | stderr may contain debug/log noise only if explicitly documented; otherwise structured JSON error policy applies |

### 5.4 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Unknown route | CLI exits non-zero | structured JSON error or documented parser-safe stderr behavior |
| Missing required args | CLI exits `2` | structured JSON usage error |
| Auth required | CLI exits `1` | stable `AUTH_REQUIRED` code in JSON mode |
| API error | CLI exits non-zero | stable error payload with code/message from API layer |
| Unsupported JSON for a route | CLI exits non-zero or documented fallback | explicit machine-readable explanation |

---

## 6.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Help/docs drift | README/help advertises JSON support not reachable via routes | automation chooses invalid command path | command fails despite documented support |
| Route mismatch | command implementation exists but no route registration | tests miss command unless inventory checks routing | command is unreachable in real CLI |
| Raw payload drift | command prints raw API response with no contract | API shape changes silently break agents | parser breaks without CLI-level version signal |
| Human text leakage | banner/prose/table output appears in JSON mode | stdout is no longer parseable | `jq`/agent consumer fails |
| Error inconsistency | one command returns JSON errors, another prints prose | automation cannot handle failures uniformly | brittle agent logic |

**Platform constraints:**
- JSON mode must coexist with current human-readable output; non-JSON UX must not regress.
- Streaming watch flows are allowed to be excluded or specially scoped, but the exclusion must be explicit and documented.

---

## 7.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Inventory is based on routed commands, not aspirational commands | inspect `routes.js` and `command-registry.js` |
| JSON stdout is parseable | pipe supported commands through `jq` in tests |
| Human banners/prose suppressed in JSON mode | run banner/help/command tests under `--json` |
| Error payloads have stable codes | golden tests for validation/auth/API failures |
| Touched files stay under 500 lines | `wc -l <file>` |

---

## 8.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `json_mode_suppresses_banner_and_prose` | 2.1 | CLI banner/output path | `--json --version`, `--json doctor` | parseable JSON only |
| `json_error_shape_is_stable` | 2.3 | CLI error printing path | auth/usage/validation failure | structured error payload |

### Integration Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `json_inventory_matches_routes` | 1.1-1.4 | routed commands | current route table | inventory artifact matches real surface |
| `runs_interrupt_route_and_json_contract` | 3.1 | runs subcommand surface | `runs interrupt ... --json` | command is routed and parseable or explicitly unsupported |
| `supported_json_commands_parse_with_jq` | 3.2 | supported command matrix | success executions | every supported command parses cleanly |
| `supported_json_errors_parse_with_jq` | 3.3 | supported command matrix | failure executions | every supported command emits structured error JSON |

### Contract Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `readme_help_and_routes_agree_on_json_surface` | 1.3, 4.1, 4.2 | README/help/routes | current docs and routes | no advertised unsupported JSON paths |
| `automation_guidance_matches_supported_commands` | 4.3 | docs | supported JSON matrix | guidance matches real contract |

### Spec-Claim Tracing

| Claim | Proved by |
|------|-----------|
| JSON support is defined against the real routed CLI surface | `json_inventory_matches_routes` |
| Supported commands produce parseable machine output | `supported_json_commands_parse_with_jq` |
| Supported commands fail in a machine-handleable way | `supported_json_errors_parse_with_jq`, `json_error_shape_is_stable` |
| Docs and help reflect reality | `readme_help_and_routes_agree_on_json_surface`, `automation_guidance_matches_supported_commands` |

---

## 9.0 Verification Evidence

**Status:** PENDING

| Command | Purpose | Evidence placeholder |
|---------|---------|----------------------|
| `bun test zombiectl/test` | CLI unit/integration coverage | PENDING |
| `zombiectl --json doctor | jq .` | parseability smoke | PENDING |
| `zombiectl --json workspace add <repo> | jq .` | mutation JSON smoke | PENDING |
| `zombiectl --json runs list | jq .` | listing JSON smoke | PENDING |
| `zombiectl --json run status <run_id> | jq .` | status JSON smoke | PENDING |

---

## 10.0 Acceptance Criteria

**Status:** PENDING

- [ ] 10.1 JSON support inventory is based on the real routed CLI surface and checked into repo docs/spec
- [ ] 10.2 Every supported machine-usable command has documented JSON success behavior
- [ ] 10.3 Every supported machine-usable command has documented JSON error behavior with stable codes
- [ ] 10.4 `runs interrupt` is either fully routed and covered or explicitly excluded from the supported JSON contract
- [ ] 10.5 Supported JSON commands parse cleanly with `jq` in tests
- [ ] 10.6 Human-readable output outside JSON mode remains unchanged
- [ ] 10.7 README/help automation guidance matches the supported JSON surface

---

## 11.0 Out of Scope

- Replacing streaming watch mode with a separate RPC protocol
- Guaranteeing JSON support for inherently interactive/browser-driven flows unless explicitly scoped
- Redesigning non-JSON CLI UX
- API response redesign beyond what is needed to make CLI JSON output stable
