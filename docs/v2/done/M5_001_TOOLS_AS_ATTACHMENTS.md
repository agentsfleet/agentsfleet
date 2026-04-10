# M5_001: Tools as Attachments — sandbox-agnostic tool architecture with per-Zombie scoping

**Prototype:** v0.7.0
**Milestone:** M5
**Workstream:** 001
**Date:** Apr 09, 2026
**Status:** DONE
**Priority:** P0 — v2 architecture rule; enables every future Zombie and ClawHub (clawhub.ai/skills) skill
**Batch:** B2 — parallel with M4 (shares tool registry from M3, but no execution dependency)
**Branch:** feat/m5-tools-as-attachments
**Depends on:** M3_001 (tool registry, initial tool implementations)

---

## Overview

**Goal (testable):** The sandbox (`src/executor/`) has zero knowledge of specific tools (git, Slack, GitHub). All tool-specific logic lives in `src/zombie/tools/`. The executor provides a generic tool invocation interface: receive tool call from NullClaw agent → resolve tool via registry → inject credential from vault → execute tool → return result → strip credential echo from response. A Zombie's TRIGGER.md declares exactly which tools it needs; the agent can only call those tools. Adding a new tool requires only: (1) implement `Tool` interface in `src/zombie/tools/`, (2) register in tool registry, (3) add domain to skill-domain map. Zero changes to the executor, event loop, or sandbox.

**Problem:** v1 baked git into the sandbox — the executor was built around clone → worktree → gate → push → PR. This made the sandbox git-specific and unusable for non-git Zombies (Lead Zombie with email, Ops Zombie with Cloudflare). M3 introduces the tool registry and three tool implementations, but the architecture isn't formalized: the executor still has git-aware code paths, the `SandboxShellTool` mixes tool logic with sandbox concerns, and there's no clean interface contract that new tool authors follow. Without a clean architecture, every new tool requires touching the executor (blast radius grows, bugs compound).

**Solution summary:** Refactor the tool invocation path into three layers: (1) **Tool interface** — a vtable contract in `src/zombie/tools/tool.zig` that every tool implements (`name`, `description`, `parameters`, `domains`, `execute`), (2) **Tool executor bridge** — a thin adapter in `src/executor/tool_bridge.zig` that the executor calls for any tool invocation (resolve → credential inject → execute → strip credential echo → return), (3) **Sandbox shell tool refactor** — extract the existing `SandboxShellTool` into the new interface, moving git-specific logic to `git_tool.zig`. Delete git-aware code paths from the executor. Verify: adding a hypothetical "linear" tool requires only files in `src/zombie/tools/` and a registry entry.

---

## 1.0 Tool Interface Contract

**Status:** PENDING

Define the canonical `Tool` interface that every tool must implement. This is the contract between the tool system and the executor. The interface is a Zig vtable (comptime interface pattern used elsewhere in the codebase). Every tool provides: metadata (name, description, parameter schema, required domains), execution (takes action + params + credential, returns ToolResult), and cleanup (release any resources). The interface file is the single source of truth for tool authors.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/zombie/tools/tool.zig:Tool`
  - input: `SlackTool struct conforming to Tool interface`
  - expected: `Compiles, vtable dispatch works, executeFn called with correct args`
  - test_type: unit (compile-time verification + runtime dispatch test)
- 1.2 PENDING
  - target: `src/zombie/tools/tool.zig:Tool`
  - input: `Struct missing required executeFn`
  - expected: `Compile error with descriptive message`
  - test_type: unit (compile-time — verify error message is helpful)
- 1.3 PENDING
  - target: `src/zombie/tools/tool.zig:ToolResult`
  - input: `Successful tool execution`
  - expected: `ToolResult{success=true, output="...", credential_echo=false}`
  - test_type: unit
- 1.4 PENDING
  - target: `src/zombie/tools/tool.zig:ToolResult`
  - input: `Failed tool execution with error code`
  - expected: `ToolResult{success=false, error_code="UZ-TOOL-001", error_message="..."}`
  - test_type: unit

---

## 2.0 Tool Executor Bridge

**Status:** PENDING

A thin adapter between the executor and the tool system. The executor calls `toolBridge.invoke(tool_name, action, params)` — the bridge resolves the tool from the registry, fetches the credential from the vault, calls the tool's `executeFn`, inspects the response for credential leakage (string match against the injected credential value), strips any echo, and returns the sanitized `ToolResult`. This is the credential isolation enforcement point. The bridge is the only code that ever sees the raw credential value in-process.

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/executor/tool_bridge.zig:invoke`
  - input: `tool_name="slack", action="post_message", params={channel, text}, credential in vault`
  - expected: `Credential fetched from vault, passed to SlackTool.execute, result returned`
  - test_type: integration (vault mock)
- 2.2 PENDING
  - target: `src/executor/tool_bridge.zig:invoke`
  - input: `tool response body contains the credential value (echo)`
  - expected: `Credential value stripped from output, ToolResult.credential_echo=true, activity warning logged`
  - test_type: unit
- 2.3 PENDING
  - target: `src/executor/tool_bridge.zig:invoke`
  - input: `tool_name="git" but Zombie's skills list only has ["slack"]`
  - expected: `error code UZ-TOOL-004: "Tool 'git' not attached to this Zombie"`
  - test_type: unit
- 2.4 PENDING
  - target: `src/executor/tool_bridge.zig:invoke`
  - input: `credential not found in vault for tool`
  - expected: `error code UZ-TOOL-001 with actionable message, tool NOT executed`
  - test_type: unit

---

## 3.0 Sandbox Shell Tool Refactor

**Status:** PENDING

The existing `SandboxShellTool` in `src/pipeline/sandbox_shell_tool.zig` mixes sandbox concerns (bwrap args, path validation, network policy) with tool execution logic. Refactor: extract the generic sandbox execution capability into the executor's session layer (already exists as `runner.zig`), and convert `SandboxShellTool` into the new `Tool` interface. Git-specific logic (clone, branch, push patterns) moves to `git_tool.zig`. The shell tool becomes a generic "run command in sandbox" capability that other tools can use internally.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `src/zombie/tools/git_tool.zig`
  - input: `action="clone", uses sandbox shell internally`
  - expected: `Git clone runs inside bwrap sandbox with correct network policy, Tool interface satisfied`
  - test_type: integration (sandbox)
- 3.2 PENDING
  - target: `src/pipeline/sandbox_shell_tool.zig`
  - input: `Verify no git-specific logic remains after refactor`
  - expected: `grep for "git", "clone", "push", "branch" returns zero matches (except generic "git" in comments/docs)`
  - test_type: unit (static analysis)
- 3.3 PENDING
  - target: `src/executor/handler.zig`
  - input: `Verify no tool-specific code paths remain`
  - expected: `Handler dispatches all tool calls through tool_bridge.invoke, no if/switch on tool name`
  - test_type: unit (static analysis)
- 3.4 PENDING
  - target: `src/zombie/tools/git_tool.zig`
  - input: `All existing git-related tests from v1`
  - expected: `Tests still pass after refactor (no regression)`
  - test_type: integration

---

## 4.0 Credential Isolation Verification

**Status:** PENDING

Prove the credential isolation guarantee end-to-end. The NullClaw agent conversation context must never contain raw credential values. The tool bridge injects credentials at the HTTP call layer and strips echoes from responses. This section adds explicit verification tests that would catch a regression where credentials leak into the agent context.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `src/executor/tool_bridge.zig` + `src/zombie/event_loop.zig`
  - input: `Zombie processes event, tool uses credential, agent conversation captured`
  - expected: `grep agent conversation history for credential value → zero matches`
  - test_type: integration (Executor + vault)
- 4.2 PENDING
  - target: `src/executor/tool_bridge.zig:stripCredentialEcho`
  - input: `response body = "Authorization: Bearer sk-test-1234..."` where `sk-test-1234...` is the credential
  - expected: `response body = "Authorization: Bearer [REDACTED]", credential_echo=true`
  - test_type: unit
- 4.3 PENDING
  - target: `src/executor/tool_bridge.zig:stripCredentialEcho`
  - input: `response body contains partial credential match (e.g., first 8 chars only)`
  - expected: `No redaction (avoid false positives on short substrings), credential_echo=false`
  - test_type: unit
- 4.4 PENDING
  - target: `src/executor/tool_bridge.zig:stripCredentialEcho`
  - input: `response body with base64-encoded credential`
  - expected: `Base64 variant also stripped (check both raw and base64 forms)`
  - test_type: unit

---

## 5.0 New Tool Author Contract (Documentation + Verification)

**Status:** PENDING

Document and verify that adding a new tool requires zero changes outside `src/zombie/tools/` and the tool registry. This is the "extensibility proof" — if this section passes, the architecture is clean.

**Dimensions (test blueprints):**
- 5.1 PENDING
  - target: `src/zombie/tools/linear_tool.zig` (hypothetical — created only for this verification, deleted after)
  - input: `Implement a minimal Linear tool (create_issue, list_issues) following Tool interface`
  - expected: `Compiles, tool_registry resolves it, executor invokes it, zero changes to executor/ or event_loop.zig`
  - test_type: integration (compilation + smoke test)
- 5.2 PENDING
  - target: `src/zombie/tool_registry.zig`
  - input: `Add "linear" to registry with LinearTool + domains ["api.linear.app"]`
  - expected: `resolveTools(["linear"]) returns LinearTool, domainsForSkills(["linear"]) returns ["api.linear.app"]`
  - test_type: unit
- 5.3 PENDING
  - target: `git diff --stat` after adding linear tool
  - input: `diff of all changed files`
  - expected: `Only files changed: src/zombie/tools/linear_tool.zig (new), src/zombie/tool_registry.zig (1 line added). Zero changes elsewhere.`
  - test_type: manual (diff inspection)

---

## 6.0 Interfaces

**Status:** PENDING

### 6.1 Public Functions

```zig
// src/zombie/tools/tool.zig — THE interface contract
pub const Tool = struct {
    name: []const u8,
    description: []const u8,
    parameters_schema: []const u8,    // JSON Schema for NullClaw
    required_domains: []const []const u8,

    // vtable
    executeFn: *const fn (
        self: *anyopaque,
        alloc: Allocator,
        action: []const u8,
        params: std.json.Value,
        credential: []const u8,       // injected by bridge, NOT by the tool
    ) ToolError!ToolResult,
    deinitFn: *const fn (self: *anyopaque) void,
};

pub const ToolResult = struct {
    success: bool,
    output: []const u8,              // returned to agent
    error_code: ?[]const u8 = null,  // UZ-TOOL-xxx
    error_message: ?[]const u8 = null,
    credential_echo: bool = false,   // set by bridge if echo detected + stripped
};

pub const ToolError = error{
    CredentialNotFound,
    ToolNotAttached,
    ApiCallFailed,
    Timeout,
    InvalidParams,
};

// src/executor/tool_bridge.zig
pub const ToolBridge = struct {
    registry: *ToolRegistry,
    vault: *VaultClient,
    allowed_skills: []const []const u8,  // from Zombie's TRIGGER.md
};

pub fn invoke(
    self: *ToolBridge,
    alloc: Allocator,
    tool_name: []const u8,
    action: []const u8,
    params: std.json.Value,
) ToolError!ToolResult;

pub fn stripCredentialEcho(output: []const u8, credential: []const u8) StripResult;
```

### 6.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `tool_name` | Text | Must exist in registry AND in Zombie's skills list | `"slack"` |
| `action` | Text | Must be one of tool's declared actions | `"post_message"` |
| `params` | JSON | Must validate against tool's parameters_schema | `{channel: "C01...", text: "..."}` |
| `credential` | Text | Resolved from vault, never from agent | `"xoxb-..."` |

### 6.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `ToolResult.output` | Text | On success | `"Message posted to #bugs"` |
| `ToolResult.credential_echo` | bool | When response contained credential | `true` (stripped) |
| `ToolResult.error_code` | Text | On failure | `"UZ-TOOL-002"` |

### 6.4 Error Contracts

| Error condition | Code | Developer sees | HTTP |
|----------------|------|---------------|------|
| Tool not in registry | `UZ-TOOL-005` | "Unknown tool '{name}'. Available: agentmail, slack, git, github" | -- |
| Tool not in Zombie's skills | `UZ-TOOL-004` | "Tool '{name}' not attached to this Zombie. Add to TRIGGER.md skills." | -- |
| Credential not found | `UZ-TOOL-001` | "Credential '{name}' not found. Add with: zombiectl credential add {skill}" | -- |
| Tool API error | `UZ-TOOL-002` | "{Tool} API error: {detail}. Check permissions." | -- |
| Tool timeout | `UZ-TOOL-006` | "{Tool} timed out after {n}s. Check network or API status." | -- |
| Credential echo detected | (warning) | Activity: "Credential echo stripped from {tool} response" | -- |

---

## 7.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Vault unavailable | Vault service down | Tool invocation fails with UZ-TOOL-001, event retried | Activity: "Vault unavailable — retrying in 5s" |
| Tool implementation panic | Bug in tool code | Executor catches panic, returns error result | Activity: "Tool '{name}' crashed: {message}" — Zombie continues |
| Credential rotation mid-invocation | Credential updated while tool is executing | Current invocation uses old credential, next uses new | Transparent — no user impact |
| Tool response too large | Tool returns > 1MB | Truncated to 1MB, warning logged | Agent sees truncated output + "[truncated]" |
| DNS resolution failure for tool domain | Network issue | Tool returns error after timeout | Activity: "Cannot reach {domain} — check network" |

**Platform constraints:**
- Tool execution timeout inherits from executor's `lease_timeout_ms` (default 30s per tool call)
- Credential echo stripping is string-match based — may false-positive on credentials that are common substrings. Min credential length for echo check: 16 chars.
- Tools run inside the same bwrap sandbox as the agent — no separate isolation per tool

---

## 8.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| tool.zig interface file < 100 lines | `wc -l src/zombie/tools/tool.zig` |
| tool_bridge.zig < 300 lines | `wc -l src/executor/tool_bridge.zig` |
| Each tool implementation < 500 lines | `wc -l src/zombie/tools/*.zig` |
| Zero tool-specific code in executor/ (except tool_bridge.zig) | `grep -r "slack\|github\|agentmail" src/executor/ --include="*.zig" -l` returns only tool_bridge.zig |
| Adding a new tool changes exactly 2 files | Verified by section 5.0 (new tool + registry entry) |
| Credential never passed to NullClaw agent context | Integration test 4.1 |
| Credential echo stripped for all credential formats (raw + base64) | Unit tests 4.2, 4.3, 4.4 |
| Cross-compiles on x86_64-linux, aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| drain() before deinit() on all pg query results | `make check-pg-drain` |
| No regression in existing executor tests | `make test-integration` passes |

---

## 9.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| tool_interface_compiles | 1.1 | tool.zig:Tool | SlackTool | vtable dispatch works |
| tool_result_success | 1.3 | tool.zig:ToolResult | success case | correct fields |
| tool_result_error | 1.4 | tool.zig:ToolResult | error case | error_code set |
| bridge_credential_echo | 2.2 | tool_bridge.zig:stripCredentialEcho | echo in response | stripped |
| bridge_tool_not_attached | 2.3 | tool_bridge.zig:invoke | tool not in skills | UZ-TOOL-004 |
| bridge_cred_not_found | 2.4 | tool_bridge.zig:invoke | cred missing | UZ-TOOL-001 |
| no_git_in_sandbox_shell | 3.2 | sandbox_shell_tool.zig | grep | zero matches |
| no_tool_specific_in_executor | 3.3 | handler.zig | grep | zero matches |
| strip_partial_no_match | 4.3 | tool_bridge.zig:stripCredentialEcho | 8 char match | no redaction |
| strip_base64 | 4.4 | tool_bridge.zig:stripCredentialEcho | base64 encoded | stripped |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| bridge_invoke_success | 2.1 | Vault mock | slack post_message | result returned |
| git_tool_refactored | 3.1 | Sandbox | git clone | works via Tool interface |
| git_regression | 3.4 | Sandbox | existing v1 git tests | all pass |
| credential_isolation_e2e | 4.1 | Executor + vault | event processing | cred not in conversation |
| new_tool_zero_executor_changes | 5.1 | Compilation | add linear tool | compiles, works |
| new_tool_registry_only | 5.2 | None | registry lookup | resolves correctly |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| "sandbox has zero knowledge of specific tools" | no_tool_specific_in_executor, no_git_in_sandbox_shell | unit (static) |
| "all tool-specific logic lives in src/zombie/tools/" | new_tool_zero_executor_changes | integration |
| "agent can only call tools declared in TRIGGER.md" | bridge_tool_not_attached | unit |
| "adding a new tool requires zero changes to executor" | new_tool_zero_executor_changes, diff inspection | integration + manual |
| "credential echo stripped from response" | bridge_credential_echo, strip_base64 | unit |
| "credential never enters NullClaw conversation" | credential_isolation_e2e | integration |

---

## 10.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Define Tool interface in src/zombie/tools/tool.zig | Compiles, SlackTool conforms |
| 2 | Implement tool_bridge.zig (invoke, stripCredentialEcho) | Unit tests 2.2, 2.3, 2.4 pass |
| 3 | Refactor SlackTool, GitTool, GitHubTool to conform to Tool interface | Existing tests pass |
| 4 | Extract git-specific logic from SandboxShellTool to GitTool | Test 3.2 pass (grep zero) |
| 5 | Remove tool-specific code paths from executor/handler.zig | Test 3.3 pass (grep zero) |
| 6 | Wire tool_bridge into executor session (all tool calls route through bridge) | Integration test 2.1 pass |
| 7 | Add credential isolation verification tests | Test 4.1 pass |
| 8 | Add credential echo stripping tests (raw + base64) | Tests 4.2, 4.3, 4.4 pass |
| 9 | Verify extensibility: add + remove hypothetical linear_tool | Tests 5.1, 5.2, 5.3 pass |
| 10 | Full regression suite | `make test && make test-integration && make lint` |
| 11 | Cross-compile check | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## 11.0 Acceptance Criteria

**Status:** PENDING

- [ ] Tool interface defined, all existing tools conform — verify: `zig build` compiles
- [ ] tool_bridge.invoke routes all tool calls, no tool-specific paths in executor — verify: grep
- [ ] Credential echo stripped from tool responses — verify: unit tests
- [ ] Credential never in NullClaw agent conversation — verify: integration test
- [ ] Git-specific logic removed from SandboxShellTool — verify: grep
- [ ] Adding a new tool requires only 2 files changed — verify: diff of linear_tool addition
- [ ] No regression in existing tests — verify: `make test && make test-integration`
- [ ] `make lint` passes
- [ ] Cross-compile passes for both targets
- [ ] `make check-pg-drain` passes
- [ ] All new files < 500 lines

### Integration Test Prerequisites

Integration tests (`make test-integration`) require local services running. Sequence:

```bash
# 1. Start Postgres + Redis (docker compose)
make up

# 2. Wait for services to be healthy (~10s)
docker compose ps   # verify STATUS = running

# 3. Run integration tests (DB + Redis)
#    Env vars auto-resolved from make/test-integration.mk:
#      TEST_DATABASE_URL_LOCAL = postgres://usezombie:usezombie@localhost:5432/usezombiedb
#      TEST_REDIS_TLS_URL_LOCAL = rediss://:usezombie@localhost:6379
#    TLS CA cert extracted from Redis container automatically.
make test-integration

# 4. Individual suites (when debugging):
make test-integration-db      # DB-backed only
make test-integration-redis   # Redis-backed only

# 5. Tear down after
make down
```

**CI note:** GitHub Actions runs `make up` in the workflow before `make test-integration`. Local dev must do the same manually. Tests will fail with connection errors if services are not running.
---

## 12.0 Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Services up | `make up && docker compose ps` | | |
| Unit tests | `make test` | | |
| Integration tests (DB) | `make test-integration-db` | | |
| Integration tests (Redis) | `make test-integration-redis` | | |
| Integration tests (full) | `make test-integration` | | |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| 500L gate | `wc -l` on new/changed files | | |
| pg-drain | `make check-pg-drain` | | |
| Extensibility proof | `git diff --stat` after linear_tool addition | | |
| Credential isolation | grep agent conversation for credential values | | |
| Services down | `make down` | | |

---

## 13.0 Out of Scope

- Dynamic tool loading from ClawHub (clawhub.ai/skills) registry (Phase 3 — tools are compiled-in for now)
- Tool sandboxing (each tool runs inside the same bwrap sandbox — no per-tool isolation)
- Tool versioning (all tools at v1, no version negotiation)
- Tool composition (tools can't call other tools — agent orchestrates)
- Tool marketplace / third-party tool authoring
- Tool telemetry / per-tool metrics (activity stream logs are sufficient for now)
- MCP server integration (tools use native Zig, not MCP protocol)
