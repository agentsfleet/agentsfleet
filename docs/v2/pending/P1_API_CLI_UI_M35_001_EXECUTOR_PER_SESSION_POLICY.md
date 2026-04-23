# M35_001: Executor Per-Session Policy + Credential Templating + Config Hot-Reload

**Prototype:** v2.0.0
**Milestone:** M35
**Workstream:** 001
**Date:** Apr 23, 2026
**Status:** PENDING
**Priority:** P1 — the "secrets never in agent context" claim is the headline security property of the product. Without this workstream, credentials are static env vars in the executor process and the agent sees real token bytes. Also blocks M37_001 §2.4 (credential non-leak) and the whole "no vendor connector" promise of platform-ops.
**Batch:** B1 — parallel with M33_001 (control stream) and M34_001 (event history). Closes out the M31 worker/executor rework alongside them.
**Branch:** feat/m35-executor-policy (to be created)
**Depends on:** M13_001 (credential vault supports structured `{host, api_token}` / `{host, bot_token}` creds), M33_001 (control-stream `zombie_config_changed` producer — config hot-reload consumes that signal).

**Canonical architecture:** `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` §5 (processNext resolve-secrets step), §6 (inside the executor — tool-bridge substitution), §8 invariant 2 and 6.

---

## Overview

**Goal (testable):** Given a zombie whose TRIGGER.md declares `credentials: [fly, upstash, slack]`, when the worker calls `executor.createExecution`, the executor session receives the resolved `secrets_map` and the per-zombie `network_policy` + `tools` list. When the NullClaw agent emits an `http_request` tool call containing the literal string `${secrets.fly.api_token}` in a header, the executor's tool bridge substitutes the real byte value **after** sandbox entry, **before** the HTTPS request fires — and the substituted bytes never appear in the agent's context, in any log, or in any row of `core.zombie_events`. When an operator runs `zombiectl zombie tools add {id} shell` (or the UI equivalent), the zombie's config is updated, `zombie_config_changed` fires on `zombie:control`, and the next event the zombie processes uses the new tool set.

**Problem:** Today the executor uses a process-wide `EXECUTOR_NETWORK_POLICY` env var and a fixed tool bridge. There is no per-zombie isolation of network egress. Credentials are fetched by the worker via `resolveFirstCredential` and passed to `startStage` as a single `api_key` string (the LLM provider key) — but *tool-level credentials* (fly, upstash, slack) don't flow at all. The agent would have to receive them inline in its prompt, which defeats the security claim. And config changes require a worker restart.

**Solution summary:** Extend the executor RPC protocol: `createExecution` grows `network_policy`, `tools`, and `secrets_map` fields. Session stores them; tool bridge wraps NullClaw's `http_request` with a substitution pass that finds `${secrets.NAME.FIELD}` in header values and body fragments and replaces with `secrets_map[NAME][FIELD]` **in the outbound request builder**, after the sandbox isolates the process. The agent never sees the real bytes; logs never see them; responses never echo them (HTTP response bodies don't include request Authorization header values). For tools and firewall config mutation, add `PATCH /v1/workspaces/{ws}/zombies/{id}` that updates `core.zombies.config_json` and publishes `zombie_config_changed` on `zombie:control`. The worker's per-zombie thread reads a cached config revision and reloads config on mismatch, so the change takes effect on the next event (not the current in-flight one). CLI and UI surface tools/firewall editing as thin PATCH wrappers.

---

## Files Changed (blast radius)

### Executor (Zig)

| File | Action | Why |
|---|---|---|
| `src/executor/protocol.zig` | MODIFY | `createExecution` params gain optional `network_policy`, `tools` (array of tool names), `secrets_map` (obj keyed by credential name → obj of field→value). Backwards-compatible: existing callers keep working without the fields. |
| `src/executor/handler.zig` | MODIFY | `handleCreateExecution` stores the new fields on the session. |
| `src/executor/session.zig` | MODIFY | Session struct grows `network_policy`, `tools_allowlist`, `secrets_map` (owned, zeroized on destroy). |
| `src/executor/tool_bridge.zig` | CREATE (or extend existing) | Wraps NullClaw tools. For `http_request`: takes the JSON tool call, walks headers/body/query for `${secrets.NAME.FIELD}`, substitutes from `session.secrets_map`. Returns the rewritten tool call to NullClaw for dispatch. **The substitution is the ONLY place in the whole stack where real credential bytes meet the outbound request.** ≤250 lines. |
| `src/executor/network.zig` | MODIFY | Apply `session.network_policy` per-session (bwrap rule set) instead of the single process-wide env var. |
| `src/executor/runner.zig` | MODIFY | When building tools, filter by `session.tools_allowlist`. Reject startStage if the agent's tool call references a tool not in the list (defense in depth). |

### Worker (Zig)

| File | Action | Why |
|---|---|---|
| `src/zombie/event_loop_helpers.zig` | MODIFY | `executeInSandbox` resolves the zombie's credential list from vault just-in-time, builds `secrets_map`, passes it + `network_policy` + `tools` into `createExecution`. |
| `src/secrets/crypto_store.zig` | MODIFY | Add `loadJson(conn, ws_id, name) -> std.json.Value` convenience for structured creds. (Underlying bytes already encrypted; just adds a JSON-parse pass.) |
| `src/zombie/config_reload.zig` | CREATE | `maybeReloadConfig(session, redis, pool)` — compare `zombie:{id}:config_rev` Redis key to session's cached rev. On mismatch: SELECT config_json + source_markdown, re-parse, replace session.config in-place. Runs at top of each `processEvent` iteration. |
| `src/cmd/worker/watcher.zig` (M33_001) | MODIFY | On `zombie_config_changed` control message: `SET zombie:{id}:config_rev <now-ms> EX 3600` — worker threads pick up on next iteration. |

### HTTP (Zig)

| File | Action | Why |
|---|---|---|
| `src/http/handlers/zombies/patch.zig` | CREATE | `PATCH /v1/workspaces/{ws}/zombies/{id}` with body `{config?: {...}, name?: string, description?: string}`. Validates, UPDATE `core.zombies.config_json` (jsonb_set or full replace), XADD `zombie:control` type=`zombie_config_changed`. ≤200 lines. Absorbs the prior lifecycle-mutations PATCH surface (the old `M19_002` spec was retired alongside this workstream) — minus the `schedule_cron` field, which NullClaw owns via `cron_add`. |
| `src/http/handlers/zombies/credentials.zig` | MODIFY or CREATE | `POST /v1/.../zombies/{id}/credentials` — declare which vault credentials this zombie uses. Just updates `config_json.credentials` array via the PATCH path above. |
| `src/http/router.zig` + `route_table.zig` + `route_manifest.zig` | MODIFY | Register PATCH + credentials endpoints. |

### CLI (JavaScript)

| File | Action | Why |
|---|---|---|
| `zombiectl/src/commands/zombie_tools.js` | CREATE | `zombiectl zombie tools list|add|remove {id} <tool>`. Thin PATCH wrappers. |
| `zombiectl/src/commands/zombie_firewall.js` | CREATE | `zombiectl zombie firewall list|add|remove {id} <host>`. Thin PATCH wrappers. |
| `zombiectl/src/commands/zombie_credentials.js` | CREATE | `zombiectl zombie credentials list|link|unlink {id} <name>` — attach a vault credential to this zombie's config. |

### UI (TypeScript/React)

| File | Action | Why |
|---|---|---|
| `ui/packages/app/lib/api/zombies.ts` | MODIFY | `patchZombie(wsId, zId, patch)`. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/ToolsEditor.tsx` | CREATE | Add/remove tools from NullClaw catalog; renders allowlist. ≤200 lines. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/FirewallEditor.tsx` | CREATE | Add/remove allowlisted hosts. ≤150 lines. |
| `ui/packages/app/app/(dashboard)/zombies/[id]/components/CredentialsLinker.tsx` | CREATE | Link existing vault credentials to this zombie. ≤150 lines. |

### Schema

No new table. `core.zombies.config_json` is already jsonb.

### Tests

| File | Action | Why |
|---|---|---|
| `src/executor/tool_bridge_test.zig` | CREATE | Unit: substitution covers header values, body fragments, nested JSON; leaves unmatched placeholders unchanged; handles empty secrets_map; throws on reference to unknown secret. ≤250 lines. |
| `src/executor/session_test.zig` | MODIFY | Session stores + zeroizes secrets_map on destroy. |
| `src/executor/network_policy_test.zig` | CREATE | Per-session firewall applies (integration with bwrap). |
| `src/zombie/config_reload_test.zig` | CREATE | Reload on rev mismatch, skip on match. |
| `src/http/handlers/zombies/patch_test.zig` | CREATE | PATCH validation + control-stream publish. |
| `zombiectl/test/zombie-tools.unit.test.js` | CREATE | CLI flag parsing + API mock. |
| `ui/packages/app/tests/tools-editor.test.tsx` | CREATE | Component behavior. |
| `src/zombie/credential_nonleak_integration_test.zig` | CREATE | **The acceptance test for §2.4 of M37_001.** Seeds a test token; runs a chat; greps all persistence layers for the token bytes; asserts 0 hits. Also asserts bytes are zeroized from session memory after destroyExecution. |

---

## Applicable Rules

**ZIG-DRAIN**, **TST-NAM**, **FLL** (tool_bridge ≤250, patch ≤200 — split if growing). **Memory-safety** (zeroize secrets_map on session destroy — no lingering credential bytes after zombie thread idle).

---

## Sections

### §1 — Executor protocol + session policy

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `createExecution` with new fields | call with `network_policy`, `tools`, `secrets_map` | session stores them; returns execution_id | unit |
| 1.2 | PENDING | backwards compat | call without new fields (existing callers) | defaults (no secrets, default network policy, all tools) preserve current behavior | unit |
| 1.3 | PENDING | network_policy applied per-session | two concurrent executions with different allowlists | each bwrap config matches its session's policy | integration |
| 1.4 | PENDING | tools allowlist enforced | agent calls a tool not in list | runner rejects with structured error; agent sees "tool X not allowed by this zombie's config" | integration |

### §2 — Credential templating on http_request

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | header substitution | tool call with `Authorization: Bearer ${secrets.fly.api_token}` | outbound request has real token; tool call recorded in NullClaw observer does NOT | unit |
| 2.2 | PENDING | body substitution | tool call body `{"token": "${secrets.slack.bot_token}"}` | outbound body has real token; logged body has placeholder | unit |
| 2.3 | PENDING | unknown placeholder | `${secrets.does_not_exist.field}` | structured error returned to agent; no outbound request | unit |
| 2.4 | PENDING | partial match | literal `$secrets.x.y` (missing braces) | treated as literal text; no substitution | unit |
| 2.5 | PENDING | nested ref | `${secrets.a.b}` inside `${secrets.c.d}` (adversarial) | non-recursive — one pass, leaves inner placeholder literal if outer resolves | unit |
| 2.6 | PENDING | zeroization | session.destroy | `secrets_map` memory overwritten with zeros | unit (hex-dump assertion) |

### §3 — Credential non-leak (THE headline test)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | full-stack non-leak | seed `fly.api_token="test-token-SEED-xyz"`; run a chat that triggers http_request; grep all rows/logs | ZERO occurrences of `test-token-SEED-xyz` in: `core.zombie_events.{request_json,response_text}`, `core.zombie_activities.detail`, `core.zombie_sessions.context_json`, zombied-api logs, zombied-worker logs. Token only visible in executor stderr trace (if verbose NULLCLAW_OBSERVER=verbose) and outbound TCP bytes. | integration |

### §4 — Config hot-reload

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `PATCH /zombies/{id}` with tools change | `{config: {tools: ["http_request","shell"]}}` | 200; UPDATE core.zombies; XADD zombie:control type=zombie_config_changed | integration |
| 4.2 | PENDING | watcher sees control msg | — | SET `zombie:{id}:config_rev <now>` | integration |
| 4.3 | PENDING | next event reloads config | after PATCH, send chat | agent's tool list reflects updated value; prior in-flight event not affected | integration |
| 4.4 | PENDING | rev cache | same rev two events in a row | config not re-parsed; perf neutral | unit |

### §5 — CLI + UI surfaces

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 5.1 | PENDING | `zombiectl zombie tools add {id} shell` | CLI | PATCH fires; success line; next `zombiectl zombie chat` reflects new tool set | unit |
| 5.2 | PENDING | `zombiectl zombie firewall add {id} api.stripe.com` | CLI | same shape | unit |
| 5.3 | PENDING | `zombiectl zombie credentials link {id} stripe` | CLI | adds `stripe` to config.credentials | unit |
| 5.4 | PENDING | ToolsEditor.tsx | UI | add/remove tools; optimistic; on save PATCH fires | unit |
| 5.5 | PENDING | FirewallEditor.tsx | UI | add/remove hosts | unit |
| 5.6 | PENDING | CredentialsLinker.tsx | UI | shows vault credentials; link/unlink toggles | unit |

---

## Interfaces

**Produced:**

- Executor RPC (`createExecution` schema growth — additive, backwards compatible).
- `PATCH /v1/workspaces/{ws}/zombies/{id}` HTTP.
- `POST /v1/.../zombies/{id}/credentials` (thin wrapper over PATCH).
- Redis: `zombie:{id}:config_rev` key (set by watcher on control msg, read by worker threads).

**Consumed:**

- NullClaw tools (`http_request`, `shell`, etc.).
- `crypto_store.load` for credential resolution.
- `zombie:control` from M33_001.

**Placeholder syntax specification:**

```
${secrets.CREDENTIAL_NAME.FIELD_NAME}

CREDENTIAL_NAME: [a-z][a-z0-9_]{0,63}
FIELD_NAME:       [a-z][a-z0-9_]{0,63}

Rules:
- exact substring match, case-sensitive
- not recursive; one substitution pass per tool call
- unmatched placeholders: if CREDENTIAL_NAME present but FIELD_NAME missing, structured error
- if CREDENTIAL_NAME not in session.secrets_map, structured error
- substitution happens in: header values, body string fragments, and URL query values
- NOT substituted in: URL paths (avoid accidental cred bytes in URL logs), tool_name, method
```

---

## Failure Modes

| Failure | Trigger | Behavior | Observed |
|---|---|---|---|
| Vault decrypt fails at resolve | KMS key rotation mid-run | `resolveSecretsFromVault` returns err → `processEvent` emits `UZ-GRANT-001` with credential name → zombie_events row completed with `status=agent_error`, `failure_label='credential_resolve_fail'` | clear event; no XACK block |
| Unknown placeholder in tool call | prompt injection or agent bug | tool bridge returns structured error to agent; agent can retry differently; no outbound request fires | agent reasoning logged with error |
| Config PATCH with malformed JSON | client bug | 400 with validation message; no UPDATE; no XADD | structured error |
| Watcher misses `zombie_config_changed` | Redis flap | 30s pg reconcile picks up via config_rev mismatch (watcher compares core.zombies.updated_at against cached set) | brief latency; no permanent divergence |
| Session destroy fails to zeroize secrets_map | bug | regression test (§2.6) catches it; CI blocks | — |

---

## Invariants

| # | Invariant | Enforcement |
|---|---|---|
| 1 | Raw credential bytes never in agent's prompt, observer log, request_json, response_text, activity_events, or any log outside executor stderr | §3.1 full-stack grep-assert |
| 2 | Session.secrets_map is zeroized on destroy | §2.6 hex-dump assertion |
| 3 | Config changes take effect within 1 event tick of PATCH (or 30s reconcile fallback) | §4.3 integration |
| 4 | Tool allowlist enforced in runner regardless of agent bypass attempts | §1.4 integration |
| 5 | Network policy enforced per-session, not process-wide | §1.3 integration |

---

## Test Specification

Per sections. Unit covers the substitution logic edge cases exhaustively (this is security-critical). Integration covers the end-to-end non-leak. The credential non-leak test is the acceptance gate for both this spec and M37_001 §2.4.

### Negative + adversarial tests

- Agent crafts `${secrets.fly.api_token}${secrets.fly.api_token}` — both substituted.
- Agent emits tool call with secret reference inside a URL path — NOT substituted (security property; enforced by §2.x tests).
- Malformed session state (missing secrets_map) — falls back to passthrough; agent sees placeholder bytes verbatim (harmless, documented).

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|---|---|---|
| 1 | CHORE(open): worktree. | `git worktree list` |
| 2 | §1 executor protocol + session fields (backwards compat). | §1.1, §1.2 unit |
| 3 | §2 tool_bridge substitution logic (extensively unit-tested). | §2.x unit |
| 4 | §1.3 + §1.4 integration with real bwrap + runner. | green |
| 5 | §4 config hot-reload: PATCH handler + watcher bridge key + worker reload. | §4 integration |
| 6 | §5 CLI + UI surfaces. | §5 unit |
| 7 | §3.1 credential non-leak acceptance — full stack with real platform-ops sample. | **HEADLINE GATE** |
| 8 | Full gates + memleak (executor session lifecycle) + cross-compile + gitleaks. | green |
| 9 | CHORE(close). | PR green |

---

## Acceptance Criteria

- [ ] Executor session stores per-execution policy (network, tools, secrets) — §1
- [ ] `http_request` substitution works for headers + body; unknown refs error; no recursion — §2
- [ ] **Credential non-leak: ZERO occurrences of seeded token in all persistence layers** — §3.1 (the big one)
- [ ] Config PATCH fires + control msg fires + next event reflects change — §4
- [ ] CLI `zombie tools|firewall|credentials` work — §5.1–5.3
- [ ] UI editors work — §5.4–5.6
- [ ] Memleak gate on session destroy (zeroization) — §2.6
- [ ] All gates green — lint/test/memleak/cross-compile/gitleaks
- [ ] Closes M37_001 §2.4 — verify by running that dim's test

---

## Eval Commands

```bash
# §2 unit suite
zig build test -Dtest-filter=tool_bridge
zig build test -Dtest-filter=session_test

# §3 headline test
SEED_TOKEN="test-token-SEED-$(openssl rand -hex 4)" \
  make test-integration -- -Dtest-filter=credential_nonleak
# expect exit 0 with "ok: zero hits of $SEED_TOKEN"

# §4 hot-reload
# scripted: PATCH tools → chat → assert new tool in agent output

# gitleaks pre-commit
gitleaks detect --no-banner

# memleak
make memleak
```

---

## Discovery (fills during EXECUTE)

---

## Out of Scope

- Argument-level redaction of command lines in shell tool output (separate workstream).
- HTTPS request body recording at TCP layer (pcap-style). Not needed for MVP.
- Policy versioning — PATCH blasts current config; history recoverable from git-like pg audit if we add it later.
- Multi-tenant secret sharing (secret used by multiple zombies). Today each zombie lists its own credentials array; vault row is still shared by workspace_id.
- Rotation webhooks (notify zombies on credential rotation). Out of scope until customer demands it.
