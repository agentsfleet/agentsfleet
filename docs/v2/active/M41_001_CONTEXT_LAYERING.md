# M41_001: Context Layering — Per-Execution Policy + Bounded Context Lifecycle

**Prototype:** v2.0.0
**Milestone:** M41
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — launch-blocking. Without this the "secrets never in agent context" claim is hollow (creds flow as raw strings) AND long-running incidents crash the model when context overflows. Two independent guarantees that compose at the same boundary: `executor.createExecution`.
**Categories:** API
**Batch:** B1 — parallel with M40 (worker), M42 (streaming), M44 (install contract), M45 (vault).
**Branch:** feat/m41-context-layering
**Depends on:** M40_001 (control stream — control stream + `zombie_config_changed` signal landed Apr 25, 2026). M45_001 (vault structured creds — landed; `secrets_map` resolution uses the structured form directly, no single-string fallback needed).

**Canonical architecture:** `docs/ARCHITECHTURE.md` §10 (Capabilities — per-execution policy row + context lifecycle row), §11 (Context Lifecycle — full L1+L2+L3 mechanics), §12 step 8a/8b (executor invocation + tool-bridge substitution).

---

## Overview

**Goal (testable):** Given a zombie whose `x-usezombie:` declares `credentials: [fly, upstash, slack]`, `network.allow: [api.machines.dev, api.upstash.com, slack.com]`, `tools: [http_request, memory_store, memory_recall, cron_add]`, and `context: { tool_window: auto, memory_checkpoint_every: 5, stage_chunk_threshold: 0.75 }` — when the worker calls `executor.createExecution`, the executor session receives the resolved `secrets_map`, the per-zombie `network_policy` + `tools` list, AND the context budget knobs. When the NullClaw agent emits an `http_request` tool call containing `${secrets.fly.api_token}`, the tool bridge substitutes the real bytes **after** sandbox entry, **before** the HTTPS request fires. The substituted bytes never appear in the agent's context, in any log, or in any row of `core.zombie_events`. When a long stage exceeds the `tool_window` count, oldest tool results drop from context. When the agent has called 5 tools, NullClaw nudges it to call `memory_store`. When context fill exceeds `stage_chunk_threshold`, the agent writes a snapshot, returns `{exit_ok: false, content: "needs continuation"}`, and the worker re-enqueues a continuation event.

**Problem:** Today the executor uses a process-wide `EXECUTOR_NETWORK_POLICY` env var. There is no per-zombie isolation of network egress. Credentials flow through `resolveFirstCredential` as a single `api_key` string (the LLM provider key); tool-level credentials don't flow at all. The agent would receive them inline in its prompt, defeating the security claim. Context grows unbounded — a 30+ tool-call incident exhausts the model's context window with no failsafe. There are no continuation events; a stage that hits a wall returns degraded output.

**Solution summary:** Extend the executor RPC: `createExecution` grows `network_policy`, `tools`, `secrets_map`, and `context` (the three knobs) fields. Session stores them. The tool bridge wraps NullClaw's `http_request` with a substitution pass that finds `${secrets.NAME.FIELD}` in headers + body fragments and replaces with `secrets_map[NAME][FIELD]` **inside** the sandbox, after isolation. NullClaw's reasoning loop respects three new layers: (L1) every N tool calls the agent is prompted to call `memory_store("findings_so_far")` via SKILL.md prose hook plus a soft frame from NullClaw; (L2) results past `tool_window` count drop from context (still in the event log); (L3) when context fill exceeds `stage_chunk_threshold`, the agent writes a final snapshot and returns `{exit_ok: false, checkpoint_id, content: "needs continuation"}` — the worker re-enqueues the same incident as a synthetic event with `actor=continuation` and the next stage starts fresh, opening with a `memory_recall` of the snapshot.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/executor/rpc.zig` | EXTEND | `CreateExecutionRequest`: add `network_policy`, `tools`, `secrets_map`, `context` fields. Versioned schema. |
| `src/executor/session.zig` | EXTEND | Session stores per-execution policy + context knobs |
| `src/executor/tool_builders.zig` | EXTEND | `http_request` builder reads `network_policy.allow`, rejects off-list. Adds substitution wrapper. |
| `src/executor/tool_bridge.zig` | NEW | Substitution layer: pre-flight pass on outbound request builder, replaces `${secrets.x.y}` with `secrets_map[x][y]`. Substitution happens INSIDE the sandbox. |
| `src/executor/runner.zig` | EXTEND | NullClaw integration: pass context knobs into Agent.runSingle. Wire L1/L2/L3 hooks. |
| `src/executor/context_lifecycle.zig` | NEW | The L1+L2+L3 enforcement: rolling-window state, memory-checkpoint nudge, stage-chunk threshold detection. |
| `src/zombie/event_loop_helpers.zig` | EXTEND | `processEvent`: resolves `secrets_map` from vault before `createExecution`; passes context knobs from zombie config. |
| `src/zombie/continuation.zig` | NEW | Worker-side: when stage returns `exit_ok=false` with `checkpoint_id`, XADD synthetic event `actor=continuation` to `zombie:{id}:events`. |
| `src/zombie/config_parser.zig` | EXTEND | Parse `x-usezombie.context` knobs from frontmatter; apply tier-aware defaults if `auto`. |
| `tests/integration/executor_policy_test.zig` | NEW | E2E: install zombie with structured creds → tool call substitutes → real bytes never in event log |
| `tests/integration/context_lifecycle_test.zig` | NEW | E2E: 30-tool-call zombie → memory_store nudges fire → tool_window drops → stage chunks → continuation event → recovery |

---

## Sections (implementation slices)

### §1 — RPC schema extension
`createExecution` adds:
```
{
  network_policy: { allow: [string] },
  tools: [string],
  secrets_map: { [name: string]: { [field: string]: string } },
  context: { tool_window: int|"auto", memory_checkpoint_every: int, stage_chunk_threshold: float },
}
```
Versioned: existing `secrets_map=null` callers get the old single-string LLM key path (legacy).

### §2 — Tool bridge substitution
Inside `src/executor/tool_bridge.zig`: just before the outbound `std.http.Client.fetch` call, scan request headers + body for `${secrets.NAME.FIELD}` patterns. Replace with `secrets_map[NAME][FIELD]`. Substitution happens AFTER the sandbox is established (Landlock + cgroups + bwrap have closed). The agent's view of the request never contains the real bytes — it sees the placeholder string in tool call records that flow back into context.

### §3 — Network policy enforcement
`http_request` tool builder reads `network_policy.allow` from session. Outbound URLs must match an entry. Reject with `tool_call_failed: host not in allowlist`. Agent reasons over the error and either reformulates or gives up — visible failure beats silent egress.

### §4 — Context lifecycle (L1: memory_checkpoint_every)
After every N tool calls (default 5), NullClaw inserts a soft frame in the agent's context: *"You have called N tools. Consider whether you should snapshot your findings via `memory_store(\"incident:<id>:findings\", <summary>)` before continuing."* The agent decides whether to act. If the SKILL.md prose includes the explicit nudge ("after every 5 tool calls, snapshot findings"), this becomes near-deterministic.

### §5 — Context lifecycle (L2: tool_window)
Maintain a deque of the last N tool results in the agent's active context. When N is exceeded, oldest results are summarized to a single `[tool result for <tool>:<args> dropped — see event log]` line. The full result stays in `core.zombie_events` (M42). Agent reasons over the summary if needed via `memory_recall`.

### §6 — Context lifecycle (L3: stage_chunk_threshold)
Before each tool call, check estimated context fill (current_tokens / model_context_cap). If >= `stage_chunk_threshold` (default 0.75), nudge the agent: *"Context approaching budget. Snapshot findings now and return for continuation."* If the next tool call would exceed the threshold, force the agent to call `memory_store` and return `{exit_ok: false, checkpoint_id: <auto>, content: <agent's final reasoning>}`. Worker re-enqueues a continuation event.

### §7 — Continuation event flow
`src/zombie/continuation.zig`: when stage returns `exit_ok=false` with `checkpoint_id`, XADD `zombie:{id}:events` with `type=continuation`, `actor=continuation:<original_actor>`, `data={checkpoint_id, original_event_id}`. Next stage opens with a synthetic prompt: *"You are continuing incident <id>. Call `memory_recall(\"<checkpoint_id>\")` first to load your prior reasoning. Then continue."*

### §8 — Auto-defaults and override resolution
On `createExecution`, if `context.tool_window == "auto"`, pick from a tier table based on the active model: ≥1M → 30, 200-300k → 20, ≤200k → 10. User overrides in `x-usezombie.context` win over auto. Surface the resolved knobs in event log for observability.

### §9 — Config hot-reload
When watcher receives `zombie_config_changed` (M40), the per-zombie thread loads the new config revision. In-flight execution keeps old config; next event uses new. Test: PATCH config → in-flight finishes with old context budget → next event uses new budget.

---

## Interfaces

```
RPC: executor.createExecution
  request: {
    workspace_path: string,
    network_policy: { allow: [string] },
    tools: [string],
    secrets_map: { [name]: { [field]: string } },
    context: {
      tool_window: int | "auto",
      memory_checkpoint_every: int,
      stage_chunk_threshold: float,
    },
  }
  response: { execution_id: string }

RPC: executor.startStage
  request: { execution_id, message, context_continuation? }
  response: { content, tokens, wall_s, exit_ok, checkpoint_id? }
  invariant: when exit_ok=false, checkpoint_id is set; worker re-enqueues continuation

Substitution contract:
  - placeholder syntax: ${secrets.NAME.FIELD}
  - applied to: request headers (all values), request body (string scan)
  - NOT applied to: response bodies (no substitution on receive — agent sees real
    response bodies; but secrets shouldn't appear in responses anyway)
```

---

## Failure Modes

| Mode | Cause | Handling |
|---|---|---|
| `secrets_map` missing requested key | Zombie config references credential not in vault | Tool call fails with `secret_not_found`; agent sees the error, can reason about it |
| Substitution leaves placeholder in outbound request | Pattern miss (e.g., escaped) | Pre-flight test: request body MUST NOT contain `${secrets.` after substitution. Refuse to fire if found. |
| Outbound URL not in `allow` list | Agent invented an endpoint | Tool call returns `host_not_allowed`; agent reformulates |
| Stage chunk threshold reached but agent refuses to snapshot | LLM ignored the nudge | Force-cancel stage with `exit_ok=false`, `content="agent did not snapshot — manual intervention needed"`. Operator sees this in event log. |
| `memory_store` quota exceeded | Too many checkpoints in tight loop | Hard cap (1MB per zombie, 100 keys); reject with clear error; agent gets a chance to compact |
| Continuation event arrives but worker restarted | Worker thread missing in new process | Watcher (M40) re-spawns thread on `zombie_status_changed`; continuation event waits in pending |

---

## Invariants

1. **Substituted secret bytes never enter agent context.** Tool call records sent back to the agent show placeholder strings, not real bytes.
2. **Substituted bytes never appear in `core.zombie_events`.** Storage layer asserts on insert.
3. **Outbound HTTPS rejected if host not in allow list.** No silent off-allowlist egress.
4. **Continuation event preserves outcome.** A continuation chain MUST eventually terminate with `exit_ok=true` or operator-killed. No infinite chunk loops (max 10 continuations per incident; after that, force-stop and surface).

---

## Test Specification

| Test | Asserts |
|---|---|
| `test_secret_substitution_real_bytes_outbound` | Mock HTTPS server captures Authorization header → asserts real bytes match vault entry |
| `test_secret_no_leak_into_event_log` | Run a tool call → grep `core.zombie_events.response_text` for token bytes → 0 matches |
| `test_secret_no_leak_into_agent_context` | Capture agent's tool-call record → asserts placeholder string only |
| `test_network_allow_blocks_off_list` | Agent calls http_request to `evil.com` (not in allow) → tool returns `host_not_allowed` |
| `test_tool_window_drops_old_results` | Run 25 tool calls with `tool_window=10` → assert oldest 15 are summarized in active context |
| `test_memory_checkpoint_nudge_fires` | Run 6 tool calls with `memory_checkpoint_every=5` → assert nudge appears in agent prompt at call 5 |
| `test_stage_chunk_at_threshold` | Force context fill to 80% with `stage_chunk_threshold=0.75` → assert stage returns `exit_ok=false` with checkpoint_id |
| `test_continuation_event_resumes` | After chunk, assert continuation event lands → next stage opens → memory_recall called → new tokens generated |
| `test_max_continuation_chain_10` | Force 11 chunks in a row → 11th force-stops with `incident_chunk_loop` error |
| `test_config_hot_reload_next_event` | PATCH `tool_window` from 30 to 5 → in-flight uses 30 → next event uses 5 |

All tests in `tests/integration/executor_policy_test.zig` and `tests/integration/context_lifecycle_test.zig`.

---

## Acceptance Criteria

- [ ] `make test-integration` passes the 10 tests above
- [ ] Audit grep: install platform-ops sample, run a chat steer → grep DB dump + worker logs + executor logs for token bytes → 0 matches
- [ ] A 25-tool-call test scenario completes without context overflow; event log shows ≥1 memory_store call and 0 stage chunks
- [ ] A 50-tool-call adversarial scenario chunks at least once and resumes via continuation event
- [ ] `make memleak` clean
- [ ] Cross-compile clean: x86_64-linux + aarch64-linux
