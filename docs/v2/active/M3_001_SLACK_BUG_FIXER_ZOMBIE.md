# M3_001: Slack Bug Fixer Zombie — message in #bugs produces a PR and thread reply

**Prototype:** v0.7.0
**Milestone:** M3
**Workstream:** 001
**Date:** Apr 09, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — "Shut up and take my money" hero demo for engineering teams
**Batch:** B2 — after M2 E2E and ClawHub format land
**Branch:** feat/m3-slack-bug-fixer
**Depends on:** M2_001 (live event loop + executor integration), M2_002 (SKILL.md + TRIGGER.md format)

---

## Overview

**Goal (testable):** A developer runs `zombiectl install slack-bug-fixer && zombiectl credential add slack && zombiectl credential add github && zombiectl up` and has a live Zombie that receives Slack webhook events from a configured channel, uses a NullClaw agent in the bwrap+landlock sandbox to read the bug report, clone the repo via git tool, find and fix the bug, run `make lint && make test`, open a GitHub PR, and reply in the original Slack thread with the PR link. The approval gate fires before push-to-main (M4 dependency — stubbed here as auto-approve). Tools (slack, git, github) are attached per-Zombie, not baked into the sandbox.

**Problem:** The Lead Zombie (M1/M2) proves the architecture works for a single-tool webhook flow (email in → reply out). But the product pitch for engineering teams requires multi-tool orchestration: Slack message → git clone → code fix → PR → Slack reply. This is the demo that converts $29/mo customers. No multi-tool Zombie exists. Slack webhook ingestion doesn't exist. Git and GitHub tool attachments don't exist as sandbox-external tools. The Zombie can't reply in a Slack thread today.

**Solution summary:** Build the `slack-bug-fixer` Zombie template (SKILL.md + TRIGGER.md) using M2_002's directory format. Implement three new tool modules in `src/zombie/tools/`: `slack_tool.zig` (read/post messages via Slack API), `git_tool.zig` (clone/branch/commit/push via libgit2), `github_tool.zig` (create PR via GitHub API). Each tool is a `NullClaw.Tool` implementation that the executor invokes inside the sandbox with credential injection from the vault. Add Slack webhook verification (signing secret validation) to the webhook handler. Add a tool registry that maps skill names to tool implementations. The Zombie's TRIGGER.md declares `skills: [slack, git, github]` and the event loop attaches only those tools to the NullClaw agent.

---

## 1.0 Slack Webhook Ingestion

**Status:** PENDING

Extend the webhook handler (`src/http/handlers/webhooks.zig`) to support Slack Events API. Slack sends a `url_verification` challenge on setup and `event_callback` payloads for messages. The handler must validate the Slack signing secret (`X-Slack-Signature` + `X-Slack-Request-Timestamp`) using HMAC-SHA256 before accepting any event. Signing secret comes from the vault, not from env vars. Rate limiting reuses existing `RATE_LIMIT_CAPACITY`. Events are filtered by channel ID (from TRIGGER.md config) before enqueuing.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `src/http/handlers/webhooks.zig:verifySlackSignature`
  - input: `request with valid X-Slack-Signature, X-Slack-Request-Timestamp, raw body, signing_secret from vault`
  - expected: `HMAC-SHA256 matches, returns true`
  - test_type: unit
- 1.2 PENDING
  - target: `src/http/handlers/webhooks.zig:verifySlackSignature`
  - input: `request with tampered body (signature mismatch)`
  - expected: `returns false, HTTP 401 with error code UZ-WH-010`
  - test_type: unit
- 1.3 PENDING
  - target: `src/http/handlers/webhooks.zig:handleReceiveWebhook`
  - input: `Slack url_verification challenge payload`
  - expected: `HTTP 200 with JSON { "challenge": "<value>" }, no event enqueued`
  - test_type: unit
- 1.4 PENDING
  - target: `src/http/handlers/webhooks.zig:handleReceiveWebhook`
  - input: `Slack event_callback with type=message, channel=C_CONFIGURED`
  - expected: `HTTP 202, event enqueued on zombie:{zombie_id}:events stream`
  - test_type: integration (Redis)

---

## 2.0 Tool Registry and Attachment

**Status:** PENDING

A tool registry maps skill names (from TRIGGER.md `skills:` field) to `NullClaw.Tool` implementations. When the event loop claims a Zombie, it reads the skills list and attaches only the matching tools to the NullClaw agent. Tools not in the skills list are unavailable — the agent cannot call them. Each tool receives its credentials from the vault at invocation time (not at startup). The registry is a compile-time map; adding a new tool requires a code change (ClawHub dynamic loading is Phase 3).

**Dimensions (test blueprints):**
- 2.1 PENDING
  - target: `src/zombie/tool_registry.zig:resolveTools`
  - input: `skills = ["slack", "github"]`
  - expected: `returns [SlackTool, GitHubTool], no GitTool (not requested)`
  - test_type: unit
- 2.2 PENDING
  - target: `src/zombie/tool_registry.zig:resolveTools`
  - input: `skills = ["unknown_tool"]`
  - expected: `error.UnknownSkill with tool name in message`
  - test_type: unit
- 2.3 PENDING
  - target: `src/zombie/event_loop.zig:claimZombie`
  - input: `Zombie config with skills: [slack, git, github]`
  - expected: `NullClaw agent initialized with exactly 3 tool definitions, no more`
  - test_type: integration (Executor)
- 2.4 PENDING
  - target: `src/zombie/tool_registry.zig:resolveTools`
  - input: `skills = []` (empty, like security-gate Zombie)
  - expected: `returns empty tool list, agent runs with no external tools`
  - test_type: unit

---

## 3.0 Tool Implementations (Slack, Git, GitHub)

**Status:** PENDING

Three new `NullClaw.Tool` implementations in `src/zombie/tools/`. Each tool follows the existing `SandboxShellTool` vtable pattern. Credential injection happens per-invocation: the tool reads the vault reference from the Zombie config's `credentials:` list, resolves it at call time, and injects it into the outbound HTTP request. The credential never enters the NullClaw conversation context.

### 3.1 Slack Tool

Capabilities: `read_message` (fetch thread/channel messages), `post_message` (reply in thread), `react` (add emoji reaction). All calls go through Slack Web API (`api.slack.com`). Domain `api.slack.com` added to network allowlist for Zombies with `slack` skill.

### 3.2 Git Tool

Capabilities: `clone` (shallow clone into sandbox workspace), `branch` (create branch), `commit` (stage + commit), `push` (push branch to remote). Uses libgit2 (already linked in Zig build). Authentication via vault-injected GitHub PAT or SSH key. Clone target directory is inside sandbox `workspace_path`.

### 3.3 GitHub Tool

Capabilities: `create_pr` (open pull request), `get_pr` (read PR details), `list_files` (list changed files). All calls go through GitHub REST API (`api.github.com`). Auth via vault-injected token in Authorization header.

**Dimensions (test blueprints):**
- 3.1 PENDING
  - target: `src/zombie/tools/slack_tool.zig:execute`
  - input: `action="post_message", params={channel, thread_ts, text}, credential from vault`
  - expected: `HTTP POST to api.slack.com/chat.postMessage with Bearer token, returns message_ts`
  - test_type: integration (HTTP mock)
- 3.2 PENDING
  - target: `src/zombie/tools/git_tool.zig:execute`
  - input: `action="clone", params={repo_url, branch="main"}, credential from vault`
  - expected: `Repo cloned into sandbox workspace_path, .git directory exists, HEAD at branch tip`
  - test_type: integration (local git repo)
- 3.3 PENDING
  - target: `src/zombie/tools/github_tool.zig:execute`
  - input: `action="create_pr", params={owner, repo, head, base, title, body}, credential from vault`
  - expected: `HTTP POST to api.github.com/repos/{owner}/{repo}/pulls, returns PR number and URL`
  - test_type: integration (HTTP mock)
- 3.4 PENDING
  - target: `src/zombie/tools/slack_tool.zig:execute`
  - input: `action="post_message" with missing credential in vault`
  - expected: `error with code UZ-TOOL-001, message: "Credential 'slack_bot_token' not found. Add with: zombiectl credential add slack"`
  - test_type: unit

---

## 4.0 Network Policy Extension

**Status:** PENDING

Extend `src/executor/executor_network_policy.zig` to support per-Zombie domain allowlists in addition to the static `REGISTRY_ALLOWLIST`. When a Zombie declares `skills: [slack, github]`, the tool registry produces a domain allowlist: `["api.slack.com", "api.github.com", "github.com"]`. This per-Zombie allowlist is merged with the static registry allowlist and passed to `appendBwrapNetworkArgs`. Phase 2 nftables enforcement (out of scope) will use this merged list.

**Dimensions (test blueprints):**
- 4.1 PENDING
  - target: `src/executor/executor_network_policy.zig:mergeAllowlists`
  - input: `static REGISTRY_ALLOWLIST + zombie_domains=["api.slack.com", "api.github.com"]`
  - expected: `merged list contains all 10 entries (8 registry + 2 zombie), no duplicates`
  - test_type: unit
- 4.2 PENDING
  - target: `src/executor/executor_network_policy.zig:mergeAllowlists`
  - input: `zombie_domains=["evil.com; rm -rf /"]` (injection attempt)
  - expected: `error.InvalidDomain, domain rejected`
  - test_type: unit
- 4.3 PENDING
  - target: `src/zombie/tool_registry.zig:domainsForSkills`
  - input: `skills=["slack", "git", "github"]`
  - expected: `["api.slack.com", "github.com", "api.github.com"]`
  - test_type: unit

---

## 5.0 Slack Bug Fixer Template

**Status:** PENDING

Ship a bundled `slack-bug-fixer/` directory inside the zombiectl npm package with SKILL.md and TRIGGER.md. SKILL.md contains the agent instructions (read bug report, clone repo, find bug, fix, lint, test, PR, reply). TRIGGER.md declares skills, trigger, credentials, budget, and network config.

```
zombiectl/templates/slack-bug-fixer/
  SKILL.md    — agent instructions (ClaHub-compatible)
  TRIGGER.md  — platform config (trigger: webhook, skills: [slack, git, github])
```

**Dimensions (test blueprints):**
- 5.1 PENDING
  - target: `zombiectl/src/commands/zombie.js:commandZombie`
  - input: `zombiectl install slack-bug-fixer`
  - expected: `Directory created with SKILL.md + TRIGGER.md, success message printed`
  - test_type: unit
- 5.2 PENDING
  - target: `zombiectl/templates/slack-bug-fixer/TRIGGER.md`
  - input: `parse TRIGGER.md frontmatter`
  - expected: `trigger.type=webhook, trigger.source=slack, skills=[slack, git, github], credentials=[slack_bot_token, github_token]`
  - test_type: unit
- 5.3 PENDING
  - target: `zombiectl/src/commands/zombie.js:commandZombie`
  - input: `zombiectl up` with slack-bug-fixer config
  - expected: `Zombie deployed with 3 tools attached, webhook URL printed for Slack app config`
  - test_type: unit (mocked API)

---

## 6.0 Interfaces

**Status:** PENDING

### 6.1 Public Functions

```zig
// src/zombie/tool_registry.zig
pub const ToolDefinition = struct {
    name: []const u8,
    description: []const u8,
    parameters: []const u8,          // JSON Schema
    domains: []const []const u8,     // required network domains
};

pub fn resolveTools(alloc: Allocator, skills: []const []const u8) ![]ToolDefinition
pub fn domainsForSkills(alloc: Allocator, skills: []const []const u8) ![]const []const u8

// src/zombie/tools/slack_tool.zig
pub fn execute(alloc: Allocator, action: []const u8, params: std.json.Value, credential: []const u8) !ToolResult

// src/zombie/tools/git_tool.zig
pub fn execute(alloc: Allocator, action: []const u8, params: std.json.Value, credential: []const u8, workspace_path: []const u8) !ToolResult

// src/zombie/tools/github_tool.zig
pub fn execute(alloc: Allocator, action: []const u8, params: std.json.Value, credential: []const u8) !ToolResult

// src/http/handlers/webhooks.zig (additions)
pub fn verifySlackSignature(signing_secret: []const u8, timestamp: []const u8, body: []const u8, signature: []const u8) bool
```

### 6.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `action` | Text | One of tool's declared actions | `"post_message"` |
| `params.channel` | Text | Slack channel ID, starts with C | `"C01ABCDEF"` |
| `params.thread_ts` | Text | Slack message timestamp | `"1234567890.123456"` |
| `params.repo_url` | Text | HTTPS git URL | `"https://github.com/org/repo.git"` |
| `params.owner` | Text | GitHub org/user | `"usezombie"` |
| `params.repo` | Text | GitHub repo name | `"usezombie"` |
| `params.head` | Text | PR head branch | `"zombie/fix-null-check"` |
| `params.base` | Text | PR base branch | `"main"` |

### 6.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `tool_result.success` | bool | Always | `true` |
| `tool_result.output` | Text | On success | `"PR #247 created: https://github.com/org/repo/pull/247"` |
| `tool_result.error_code` | Text | On failure | `"UZ-TOOL-001"` |
| `tool_result.error_message` | Text | On failure | `"Credential 'slack_bot_token' not found"` |

### 6.4 Error Contracts

| Error condition | Code | Developer sees | HTTP |
|----------------|------|---------------|------|
| Credential not in vault | `UZ-TOOL-001` | "Credential '{name}' not found. Add with: zombiectl credential add {skill}" | -- |
| Tool API call failed | `UZ-TOOL-002` | "Slack API error: {api_error}. Check bot permissions." | -- |
| Git clone failed | `UZ-TOOL-003` | "Clone failed: {reason}. Check repo URL and credentials." | -- |
| Slack signature invalid | `UZ-WH-010` | "Slack signature verification failed. Check signing secret." | 401 |
| Slack timestamp stale | `UZ-WH-011` | "Slack request too old (>5 min). Replay attack rejected." | 401 |
| Tool not in skills list | `UZ-TOOL-004` | "Tool '{name}' not attached to this Zombie. Add to TRIGGER.md skills." | -- |

---

## 7.0 Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Slack signing secret missing | Vault ref not set | Zombie refuses to start, error logged | CLI: "Credential 'slack_signing_secret' not found" |
| Slack API rate limit | Too many messages | Tool retries with exponential backoff (3 attempts, 1s/2s/4s) | Brief delay, then succeeds or activity: "Slack rate limited after 3 retries" |
| Git clone timeout | Large repo or network issue | Tool timeout after 120s | Activity: "Clone timed out after 120s for repo {url}" |
| PR creation conflict | Branch already has open PR | GitHub API returns 422 | Activity: "PR already exists for branch {head}" — Zombie posts existing PR link |
| Lint/test failure | Agent's fix doesn't pass checks | Agent reads output, retries fix (max 3 attempts) | Activity: "Fix attempt 2/3 — lint failed: {error}" |
| Multi-message flood | Rapid messages in channel | Dedup by message_ts in Redis SET NX EX | Only first event processed, duplicates skipped |

**Platform constraints:**
- Slack Events API retry: Slack retries unacknowledged events 3 times. Handler must respond 200/202 within 3 seconds. Event processing is async (enqueue then ack).
- Slack signing secret timestamp must be within 5 minutes of current time (replay protection).
- libgit2 must be linked with OpenSSL for HTTPS clone support (verify in build.zig).

---

## 8.0 Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Each tool file < 500 lines | `wc -l src/zombie/tools/*.zig` |
| tool_registry.zig < 200 lines | `wc -l src/zombie/tool_registry.zig` |
| Credentials never in NullClaw conversation context | Grep for credential values in agent message history — must be zero |
| No new heap allocations in webhook hot path | Benchmark: receive → enqueue < 1ms p99 |
| Cross-compiles on x86_64-linux, aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| drain() before deinit() on all pg query results | `make check-pg-drain` passes |
| Slack signature verified before any processing | Code path: first check after body read, before JSON parse |
| All new error codes registered in codes.zig | grep UZ-TOOL and UZ-WH-01x in codes.zig |

---

## 9.0 Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| verify_slack_sig_valid | 1.1 | webhooks.zig:verifySlackSignature | Valid HMAC | true |
| verify_slack_sig_tampered | 1.2 | webhooks.zig:verifySlackSignature | Tampered body | false |
| slack_url_verification | 1.3 | webhooks.zig:handleReceiveWebhook | challenge payload | HTTP 200 + challenge |
| resolve_tools_subset | 2.1 | tool_registry.zig:resolveTools | ["slack","github"] | 2 tools |
| resolve_tools_unknown | 2.2 | tool_registry.zig:resolveTools | ["bad"] | error.UnknownSkill |
| resolve_tools_empty | 2.4 | tool_registry.zig:resolveTools | [] | empty list |
| slack_post_missing_cred | 3.4 | slack_tool.zig:execute | missing credential | UZ-TOOL-001 |
| merge_allowlists | 4.1 | network_policy.zig:mergeAllowlists | static + zombie | merged, no dupes |
| merge_allowlists_injection | 4.2 | network_policy.zig:mergeAllowlists | evil domain | error.InvalidDomain |
| domains_for_skills | 4.3 | tool_registry.zig:domainsForSkills | [slack,git,github] | 3 domains |
| install_slack_bug_fixer | 5.1 | zombie.js:commandZombie | install slack-bug-fixer | dir created |
| parse_trigger_md | 5.2 | TRIGGER.md | parse frontmatter | correct skills/trigger |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| slack_webhook_enqueue | 1.4 | Redis | Valid Slack event | Event on stream |
| claim_with_tools | 2.3 | DB + Redis + Executor | Zombie with 3 skills | Agent has 3 tools |
| slack_post_message | 3.1 | HTTP mock | post_message action | Slack API called |
| git_clone | 3.2 | Local git repo | clone action | Repo in workspace |
| github_create_pr | 3.3 | HTTP mock | create_pr action | PR created |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| "receives Slack webhook events" | slack_webhook_enqueue | integration (Redis) |
| "clone the repo via git tool" | git_clone | integration (local) |
| "open a GitHub PR" | github_create_pr | integration (HTTP mock) |
| "reply in the original Slack thread" | slack_post_message | integration (HTTP mock) |
| "tools attached per-Zombie, not baked into sandbox" | claim_with_tools, resolve_tools_subset | integration + unit |
| "credential never enters NullClaw conversation" | credential_isolation | integration (Executor) |
| "Slack signing secret validation" | verify_slack_sig_valid, verify_slack_sig_tampered | unit |

---

## 10.0 Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify |
|------|--------|--------|
| 1 | Implement tool_registry.zig (resolveTools, domainsForSkills) | Unit tests 2.1, 2.2, 2.4 pass |
| 2 | Implement slack_tool.zig (read_message, post_message, react) | Unit test 3.4 pass |
| 3 | Implement git_tool.zig (clone, branch, commit, push) | Integration test 3.2 pass |
| 4 | Implement github_tool.zig (create_pr, get_pr, list_files) | Integration test 3.3 pass |
| 5 | Extend webhook handler with Slack signature verification | Unit tests 1.1, 1.2, 1.3 pass |
| 6 | Extend webhook handler to enqueue Slack events | Integration test 1.4 pass |
| 7 | Extend network policy with per-Zombie domain merge | Unit tests 4.1, 4.2, 4.3 pass |
| 8 | Wire tool registry into event loop (claimZombie attaches resolved tools) | Integration test 2.3 pass |
| 9 | Create slack-bug-fixer template (SKILL.md + TRIGGER.md) | Unit tests 5.1, 5.2 pass |
| 10 | Cross-compile check | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| 11 | Full test suite | `make test && make test-integration && make lint` |

---

## 11.0 Acceptance Criteria

**Status:** PENDING

- [ ] `zombiectl install slack-bug-fixer` creates directory with SKILL.md + TRIGGER.md — verify: `ls` output
- [ ] Slack webhook with valid signature enqueues event — verify: `make test-integration` (webhook tests)
- [ ] Slack webhook with invalid signature returns 401 — verify: unit test
- [ ] Tool registry resolves skills to tool implementations — verify: `make test` (unit tests)
- [ ] Agent in sandbox has only declared tools available — verify: integration test
- [ ] Credentials never appear in NullClaw conversation context — verify: grep test
- [ ] Per-Zombie network allowlist merges correctly — verify: unit test
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compile passes for both targets
- [ ] `make check-pg-drain` passes
- [ ] All new files < 500 lines — verify: `wc -l`

---

## 12.0 Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Cross-compile x86_64 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile aarch64 | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| 500L gate | `wc -l` on new files | | |
| pg-drain | `make check-pg-drain` | | |

---

## 13.0 Out of Scope

- Approval gate before push (M4 — stubbed as auto-approve in this milestone)
- Slack App installation flow / OAuth (M8 — Slack Plugin Acquisition)
- Dynamic tool loading from ClawHub registry (Phase 3)
- Multi-repo support (agent works on one repo per invocation)
- Slack interactive components beyond thread reply (modals, home tab)
- nftables egress enforcement (Phase 2 of network policy)
- Agent retry logic tuning (agent decides retry count via SKILL.md instructions)
