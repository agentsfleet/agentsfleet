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
| `src/executor/rpc.zig` | EXTEND | `CreateExecutionRequest`: add `network_policy`, `tools`, `secrets_map`, `context` fields. In-place additive extension per RULE NLG — no `V2` twin, no legacy fallback branch. Every caller updated in the same commit. |
| `src/executor/client.zig` | EXTEND | `createExecution(...)` call site grows the four new params. The single in-tree caller (the worker) lands in `event_loop_helpers.zig` (this commit). |
| `src/executor/session.zig` | EXTEND | Session stores per-execution policy + context knobs. |
| `src/executor/tool_builders.zig` | EXTEND | `http_request` builder reads `network_policy.allow`, rejects off-list with `host_not_allowed`. |
| `src/executor/runner_progress.zig` | EXTEND | Substitution layer lives here, NOT in a new `tool_bridge.zig`. The existing `Adapter` already intercepts tool-use frames for redaction (M42_002 pending); §2 adds the `${secrets.NAME.FIELD}` → resolved-value pass on the same path. Coordinate with M42_002 on the same struct. |
| `src/executor/tool_bridge.zig` | (unchanged) | Existing tool *registry* — not touched by M41. |
| `src/executor/registry/model_registry.zig` | NEW | Compile-time `const` table mapping model name → `{ context_cap_tokens, tier }`. Drives §6 fill estimation and §8 auto-defaults. Operator (`nkishore@megam.io`) extends by PR; M48 (BYOK) layers tenant validation against the same table. |
| `src/executor/runtime/context_lifecycle.zig` | NEW | L1+L2+L3 enforcement: rolling-window state, memory-checkpoint nudge, stage-chunk threshold detection. New file lands in `runtime/` subfolder; existing `runner.zig`/`runner_progress.zig`/`session.zig` stay put for this PR (mass-move is a follow-up hygiene spec). |
| `src/executor/runner.zig` | EXTEND | NullClaw integration: pass context knobs into `Agent.runSingle`. Wire L1/L2/L3 hooks via `runtime/context_lifecycle`. |
| `src/zombie/event_loop_helpers.zig` | EXTEND | Replace `resolveFirstCredential` (line 242) with `resolveSecretsMap` (already shipped in `event_loop_secrets.zig:34`). Resolved `[]ResolvedSecret` flows directly into the extended `createExecution` payload. Also passes context knobs from zombie config. |
| `src/zombie/continuation.zig` | NEW | Worker-side: when stage returns `exit_ok=false` with `checkpoint_id`, XADD synthetic event with `event_type='continuation'`, `actor='continuation:<original_actor>'`, `request_json={checkpoint_id, original_event_id}`. Counts prior continuation events on the incident to enforce the max-10 cap (no new state column — derived at re-enqueue time from `core.zombie_events`). |
| `src/zombie/config_parser.zig` | EXTEND | Parse `x-usezombie.context` knobs from frontmatter; apply tier-aware defaults if `auto`. (Note: M46 also touches this file later — sequencing watch.) |
| `tests/integration/executor_policy_test.zig` | NEW | E2E: install zombie with structured creds → tool call substitutes → real bytes never in event log. |
| `tests/integration/context_lifecycle_test.zig` | NEW | E2E: 30-tool-call zombie → memory_store nudges fire → tool_window drops → stage chunks → continuation event → recovery. |

---

## Sections (implementation slices)

### §1 — RPC schema extension (in-place, per RULE NLG)
`CreateExecutionRequest` in `src/executor/rpc.zig` grows four fields. **No `V2` twin, no legacy `secrets_map=null` fallback** — pre-v2.0.0 has no external consumers, the in-tree caller (`src/zombie/event_loop_helpers.zig`) is updated in the same commit.

```
CreateExecutionRequest {
  workspace_path:   string,                     // existing
  correlation:      CorrelationContext,         // existing
  network_policy:   { allow: []string },        // NEW
  tools:            []string,                   // NEW (per-zombie tool allowlist)
  secrets_map:      []ResolvedSecret,           // NEW (M45 shape: { name, parsed: std.json.Value })
  context:          ExecutionContextBudget,     // NEW
}
ExecutionContextBudget {
  tool_window:               u32 | "auto",
  memory_checkpoint_every:   u32,
  stage_chunk_threshold:     f32,   // 0.0..1.0
  model:                     []const u8,    // looked up against registry/model_registry.zig
}
```

`ResolvedSecret` is the type already returned by `src/zombie/event_loop_secrets.zig:34`. Substitution-time field lookup walks `parsed` (`std.json.Value`) — the spec's earlier `{ [field]: string }` shape was wrong; M45 ships the whole JSON value per credential and lets the substitution path do field traversal at use-time.

### §2 — Substitution lives in `runner_progress.Adapter`
The substitution pass lands on the **existing** `runner_progress.Adapter` (`src/executor/runner_progress.zig:9-15`), which already intercepts tool-use frames in the observer/stream-callback path for M42_002's redaction work. Adding the `${secrets.NAME.FIELD}` resolver here means one observer pipeline owns both invariants (redaction inbound to the agent's context, real-byte substitution outbound to the network). Substitution timing is unchanged: AFTER the sandbox is established (Landlock + cgroups + bwrap closed), BEFORE the outbound `std.http.Client.fetch` call.

**Implementation pattern** (mirrored from `bun/src/bun.zig:919-1040` and `bun/src/StaticHashMap.zig`, not lifted):
- Headers: a small case-insensitive ASCII string hashmap (custom `Context` over `std.HashMapUnmanaged`) for `[]const u8 → []const u8` header lookups.
- Body: hand-rolled scanner over the JSON-stringified payload for the `\$\{secrets\.[a-zA-Z_][a-zA-Z0-9_]*\.[a-zA-Z_][a-zA-Z0-9_]*\}` shape; rewrite into a fresh `std.ArrayList(u8)` to avoid parse → walk → restringify.
- Module shape follows file-as-struct (`const This = @This()` + mixin) per the PUB GATE / `docs/ZIG_RULES.md` conventions.

The agent's view of the tool call (the frame that flows back into context) keeps the placeholder string. Real bytes never appear in the agent context, in `core.zombie_events`, or in any log line.

### §3 — Network policy enforcement
`http_request` tool builder reads `network_policy.allow` from session. Outbound URLs must match an entry. Reject with `tool_call_failed: host not in allowlist`. Agent reasons over the error and either reformulates or gives up — visible failure beats silent egress.

### §4 — Context lifecycle (L1: memory_checkpoint_every)
After every N tool calls (default 5), NullClaw inserts a soft frame in the agent's context: *"You have called N tools. Consider whether you should snapshot your findings via `memory_store(\"incident:<id>:findings\", <summary>)` before continuing."* The agent decides whether to act. If the SKILL.md prose includes the explicit nudge ("after every 5 tool calls, snapshot findings"), this becomes near-deterministic.

### §5 — Context lifecycle (L2: tool_window)
Maintain a deque of the last N tool results in the agent's active context. When N is exceeded, oldest results are summarized to a single `[tool result for <tool>:<args> dropped — see event log]` line. The full result stays in `core.zombie_events` (M42). Agent reasons over the summary if needed via `memory_recall`.

### §6 — Context lifecycle (L3: stage_chunk_threshold)
Before each tool call, check estimated context fill: `current_tokens / model_context_cap`. `current_tokens` already flows on every stage response (`src/executor/client.zig:230`). `model_context_cap` is a NEW lookup against `src/executor/registry/model_registry.zig` keyed by the resolved model name (which is part of `ExecutionContextBudget.model`). If the ratio ≥ `stage_chunk_threshold` (default 0.75), nudge the agent: *"Context approaching budget. Snapshot findings now and return for continuation."* If the next tool call would exceed the threshold, force the agent to call `memory_store` and return `{exit_ok: false, checkpoint_id: <auto>, content: <agent's final reasoning>}`. Worker re-enqueues a continuation event (§7).

### §7 — Continuation event flow
`src/zombie/continuation.zig`: when stage returns `exit_ok=false` with `checkpoint_id`, the worker:
1. Counts existing continuation events for this incident — `SELECT count(*) FROM core.zombie_events WHERE incident_id = $1 AND event_type = 'continuation'`. If ≥ 10, force-stop with `incident_chunk_loop` error (Invariant 4) — no XADD, surface to operator.
2. Otherwise XADD to `zombie:{id}:events` with the **shipped column names** (per `schema/019_zombie_events.sql:20-22`):
   - `event_type = 'continuation'`
   - `actor = 'continuation:<original_actor>'`  (flat string, colon-compound — `event_envelope.zig` already accepts this shape)
   - `request_json = { checkpoint_id, original_event_id }`  (merges into the existing JSONB column; no separate `data` column exists)
3. Next stage opens with a synthetic prompt: *"You are continuing incident `<id>`. Call `memory_recall(\"<checkpoint_id>\")` first to load your prior reasoning. Then continue."*

### §8 — Auto-defaults and override resolution
On `createExecution`, if `context.tool_window == "auto"`, look up the resolved model in `src/executor/registry/model_registry.zig` and pick from the same table that drives §6 fill estimation:

| Tier | Context cap | Default `tool_window` |
|---|---|---|
| LARGE | ≥ 1M tokens | 30 |
| MEDIUM | 200k - 300k tokens | 20 |
| SMALL | ≤ 200k tokens | 10 |

User overrides in `x-usezombie.context` win over auto. The registry is a compile-time `const` table; new model entries land via PR (admin: `nkishore@megam.io`). M48 (BYOK) extends the validation surface — tenant-supplied model names are checked against the same table. Surface the resolved knobs in the event log for observability.

### §9 — Config hot-reload
When watcher receives `zombie_config_changed` (M40), the per-zombie thread loads the new config revision. In-flight execution keeps old config; next event uses new. Test: PATCH config → in-flight finishes with old context budget → next event uses new budget.

---

## Interfaces

```
RPC: executor.createExecution    (extended in place — no V2 twin, no legacy fallback)
  request: {
    workspace_path: string,
    correlation:    CorrelationContext,           // existing
    network_policy: { allow: []string },          // NEW
    tools:          []string,                     // NEW
    secrets_map:    []ResolvedSecret,             // NEW — {name, parsed: std.json.Value}
    context: {                                    // NEW
      tool_window:               u32 | "auto",
      memory_checkpoint_every:   u32,
      stage_chunk_threshold:     f32,
      model:                     []const u8,
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
| `test_secret_no_leak_into_event_log` | Run a tool call → grep all JSONB columns of `core.zombie_events` (`request_json`, response columns) for token bytes → 0 matches |
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

---

## Discovery (PLAN-phase audit, Apr 29, 2026)

Cross-checked the spec against what M40/M42/M45 actually shipped. Six amendments landed in the same commit as this section:

1. **`createExecution` RPC extension strategy.** Spec was silent between in-place vs `V2`. Locked in-place per new `RULE NLG` (`docs/greptile-learnings/RULES.md`) — no legacy compat shims pre-v2.0.0. Single in-tree caller (worker) updates in the same commit.
2. **`secrets_map` shape.** M45's `resolveSecretsMap` (`src/zombie/event_loop_secrets.zig:34`) returns `[]ResolvedSecret { name, parsed: std.json.Value }`. Spec previously assumed `{ [name]: { [field]: string } }`. Substitution path now does field traversal against `parsed` at use-time. Also caught: `event_loop_helpers.zig:242` still calls `resolveFirstCredential` — replaced as part of this milestone.
3. **Substitution module.** Lives in the existing `runner_progress.Adapter` (`src/executor/runner_progress.zig:9-15`), not a new `tool_bridge.zig`. M42_002 (pending) shares the same struct for redaction; coordinate.
4. **Model context cap.** `model_context_cap` doesn't exist anywhere today. Added `src/executor/registry/model_registry.zig` as compile-time `const` table; same table drives §6 fill estimation and §8 auto-defaults. Admin: `nkishore@megam.io` extends by PR. M48 (BYOK) layers tenant validation against it.
5. **Continuation max-10 enforcement.** Derived at re-enqueue time from `core.zombie_events` (count rows where `event_type='continuation' AND incident_id=$1`). No new state column.
6. **Schema column names.** Spec used `type` and `data`; shipped schema (`schema/019_zombie_events.sql:20-22`) has `event_type TEXT` + `request_json JSONB`. §7 + Test Specification updated.

Folder hygiene: M41's NEW files land in `src/executor/registry/` and `src/executor/runtime/` to seed a future reorg. Existing files stay put for this PR — mass-move during a security-critical change is a bad idea. Follow-up hygiene spec (TBD) does the full executor + zombie reorg.

Bun reference (research only, no code lifted): mirroring patterns from `bun/src/bun.zig:919-1040` (case-insensitive ASCII string hashmap context) and `bun/src/StaticHashMap.zig:40` (file-as-struct + mixin), implemented from scratch over `std.HashMapUnmanaged` and `std.json.Stringify`.
