# Architecture — Zombie Event Flow

Date: Apr 23, 2026
Status: Canonical architecture reference for the zombie event path. Specs cite this doc rather than duplicating its contents.

This is the source of truth for **how an event travels from an operator or upstream system through the product, gets processed by an LLM agent, and turns into a side-effect**. It describes three processes, two Redis streams, one Redis key, five Postgres tables, and the order they interact in.

---

## 1. Processes

| Process | Binary | Role |
|---|---|---|
| **zombied-api** | `zombied serve` | HTTP routes; Clerk auth; writes `core.*` tables; produces `zombie:control` and `zombie:{id}:events` (on webhook/slack/svix); writes `zombie:{id}:steer` (on chat). Reads `core.zombie_events` for history. Never runs LLM code. |
| **zombied-worker** | `zombied worker` | Hosts **one watcher thread** (consumer of `zombie:control`) and **N zombie threads** (one per active zombie, each a consumer of its own `zombie:{id}:events` stream). Owns per-zombie cancel flags. Calls zombied-executor over Unix socket. Never runs LLM code. |
| **zombied-executor** (sidecar) | `zombied executor` | Unix-socket RPC server. Hosts NullClaw agent inside Landlock + cgroups + bwrap. Substitutes `${secrets.x.y}` placeholders into `http_request` tool calls at dispatch. The only process that ever holds raw credential bytes post-decryption. |

All three run in the control plane (Fly.io apps in prod). No edge-worker-in-tenant-network model.

---

## 2. Streams, keys, tables — who writes, who reads

| Target | Kind | Producer(s) | Consumer(s) |
|---|---|---|---|
| `zombie:control` | Redis stream + consumer group `zombie_workers_control` | zombied-api: `innerCreateZombie`, `innerStatusChange`, `innerConfigPatch` | zombied-worker watcher thread |
| `zombie:{id}:events` | Redis stream + consumer group `zombie_workers` | zombied-api: webhook/slack/svix ingestors. zombied-worker: steer inject. (Future: NullClaw cron runtime.) | zombied-worker's zombie thread for that id |
| `zombie:{id}:steer` | Redis key (TTL 300s) | zombied-api: `innerSteer` on chat | zombied-worker zombie thread: polls + GETDEL at top of event loop |
| `core.zombies` | Postgres | zombied-api (INSERT/UPDATE) | zombied-worker (SELECT at claim + watcher tick). zombied-api (GET endpoints). |
| `core.zombie_sessions` | Postgres | zombied-worker (UPSERT on checkpoint + execution_id tracking) | zombied-worker (SELECT at claim + kill path). |
| `core.zombie_events` | Postgres | zombied-worker zombie thread (INSERT on receive, UPDATE on complete) | zombied-api (GET `/events` paginated, filterable by `actor`) |
| `core.zombie_activities` | Postgres | zombied-worker (fine-grained event trace). zombied-api (webhook acceptance trace). | zombied-api SSE stream for live watch. |
| `vault.secrets` | Postgres (encrypted via KMS envelope) | zombied-api on `credential add` | zombied-worker resolves just-in-time before each `createExecution`. |

**Invariant.** `core.zombies` and `vault.secrets` are the ONLY durable sources of truth that matter for zombie behavior. Redis streams are the wake-up mechanism; their contents are at-least-once (XACK discipline, XAUTOCLAIM reclaim). On total Redis loss, zombies still exist (pg is truth), events in flight replay once the worker rebuilds consumers.

---

## 3. The watcher thread

One per zombied-worker process. Runs the dynamic-discovery loop.

```
watcher:
  ensureConsumerGroup zombie:control zombie_workers_control         # idempotent
  on startup reconcile:
    rows = SELECT id, status FROM core.zombies WHERE status='active'
    for row: spawn zombie thread
  loop forever:
    msg = XREADGROUP GROUP zombie_workers_control <consumer> BLOCK 5s STREAMS zombie:control >
    switch msg.type:
      zombie_created          → SELECT row; alloc cancel_flag; spawn zombie thread
      zombie_status=active    → if no thread: spawn; else: no-op
      zombie_status=paused    → cancel_flag.store(true); drop entry
      zombie_status=stopped   → cancel_flag.store(true); drop entry
      zombie_status=killed    → cancel_flag.store(true); look up execution_id; executor.cancelExecution
      zombie_config_changed   → set Redis key zombie:{id}:config_rev = now; thread picks up on next event (M35_001)
    XACK zombie:control zombie_workers_control msg_id
  periodic (every 30s) pg reconcile:
    rows = SELECT id, status FROM core.zombies WHERE status IN (active, paused, stopped, killed)
    diff vs in-memory cancels map; correct any drift (missed control message)
```

Watcher is single-threaded, fail-fast on pg errors, relies on process-manager restart for supervision. Registry state is `HashMap(zombie_id → cancel_flag)` — nothing else. Thread handles tracked separately in an `ArrayList` only for shutdown `.join()`.

## 4. The zombie thread

One per active zombie. Spawned by the watcher.

```
zombieWorkerLoop(zombie_id, cancel_flag, shutdown_flag, executor_client):
  redis = connectRedis()                           # own client per thread
  session = claimZombie(alloc, zombie_id, pool)     # loads config + checkpoint + instructions
  ensureZombieConsumerGroup(redis, zombie_id)       # idempotent BUSYGROUP ok
  consumer_id = "worker-{pid}-{rand}"               # unique per process
  spawn watchShutdown(cancel_flag, shutdown_flag, &running)

  while running:
    pollSteerAndInject(alloc, cfg, session)         # GETDEL zombie:{id}:steer → XADD zombie:{id}:events
    maybeReloadConfig(session)                      # compare zombie:{id}:config_rev to session's cached rev (M35_001)
    event = XREADGROUP GROUP zombie_workers <consumer_id> BLOCK 5s STREAMS zombie:{id}:events >
              OR XAUTOCLAIM (every 5 min, reclaim idle > ZOMBIE_RECLAIM_IDLE_MS)
    if event: processNext(session, event)
  # thread exits cleanly; watcher reaps on shutdown
```

`processNext` is the v1-pattern claim → rate-limit → gate → execute → finalize → XACK.

## 5. processNext (per-event pipeline)

```
processNext(session, event):
  # a. Claim (stream delivery already guarantees exclusivity; INSERT makes it durable)
  INSERT core.zombie_events (id, zombie_id, workspace_id, event_id, event_type, source,
                              actor, request_json, status='received', created_at)
                              # UNIQUE(zombie_id, event_id) makes replay idempotent

  # b. Rate-limit (deferred to post-M31 per TenantRateLimiter port)
  # tenant_limiter.acquire(workspace_id) or: XACK + skip with backoff

  # c. Balance gate (existing)
  if metering.shouldBlockDelivery(pool, ws_id, z_id, balance_policy):
    UPDATE zombie_events SET status='balance_blocked'; XACK; return

  # d. Approval gate (existing — event_loop_gate)
  gate = checkApprovalGate(session, event, pool, redis)
  if gate != passed:
    UPDATE zombie_events SET status='agent_error', gate_outcome=<label>; XACK; return

  # e. Executor RPC (M35_001 extends payload)
  secrets_map = resolveSecretsFromVault(session.config.credentials, workspace_id)   # decrypts just-in-time
  execution_id = executor.createExecution(workspace_path, {
      trace_id   = event.event_id,
      zombie_id, workspace_id, session_id = event.event_id,
      network_policy = session.config.firewall,
      tools          = session.config.tools,
      secrets_map,
  })
  setExecutionActive(session, execution_id, pool)    # for steer/kill lookup

  result = executor.startStage(execution_id, {
      agent_config = { system_prompt = session.instructions, api_key = resolveProviderKey(session) },
      message      = event.data_json,
      context      = parsed(session.context_json),
  })
  defer destroyExecution(execution_id)
  defer clearExecutionActive(session, pool)

  # f. Finalize
  updateSessionContext(session, event.event_id, result.content)   # in-memory only
  UPDATE core.zombie_events SET status=<processed|agent_error>, response_text=result.content,
    token_count, wall_ms, ttft_ms, completed_at
  checkpointState(session, pool)                                   # UPSERT core.zombie_sessions
  metering.recordZombieDelivery(...)
  XACK zombie:{id}:events zombie_workers msg_id
```

## 6. Inside the executor (what NullClaw does)

```
handleStartStage(execution_id, payload):
  session = store.get(execution_id)               # has policy, secrets_map, correlation
  check session.isCancelled() → return cancelled
  check session.isLeaseExpired() → return lease_expired
  result = runner.execute(alloc, workspace_path, agent_config, tools_spec, message, context)

runner.execute:
  cfg = Config.load() + overrides from agent_config
  tools = buildTools(tools_spec, session.policy, session.secrets_map)
    # The credential-templating wrap (M35_001) lives here. http_request tool impl wraps
    # NullClaw's own http_request with a pre-dispatch header-substitution pass:
    # find ${secrets.NAME.FIELD} in headers/body, replace with secrets_map[NAME][FIELD].
  agent = Agent.fromConfig(cfg, tools, provider, observer)
  return agent.runSingle(composed_message)        # tool-use loop until stop_reason=end_turn

agent.runSingle loop:
  provider.stream(system_prompt, user_message, tools)
  while tokens arrive:
    if tool_call: tool.execute(args); feed tool_result back into provider.stream
    if stop: break
  return { content, token_count, wall_seconds }
```

---

## 7. End-to-end reference sequence (platform-ops-style)

Abbreviated. See each spec's acceptance walkthrough for concrete inputs/outputs.

```
T=0    Operator  → zombied-api   POST /v1/.../zombies          (install)
                   zombied-api   INSERT core.zombies
                   zombied-api   XGROUP CREATE zombie:{id}:events zombie_workers 0 MKSTREAM
                   zombied-api   XADD zombie:control type=zombie_created
                   zombied-api   ← 201
T=0+ε  zombied-worker watcher unblocks on XREADGROUP
                   SELECT core.zombies WHERE id=<new>
                   spawn zombie thread (cancel_flag allocated)
                   XACK control
T=0+Δ  zombie thread claimZombie + ensureConsumerGroup (idempotent BUSYGROUP)
                   blocks on XREADGROUP zombie:{id}:events BLOCK 5s

...later...

T=N    Operator  → zombied-api   POST .../steer                (chat)
                   zombied-api   SET zombie:{id}:steer <msg> EX 300
                   zombied-api   ← 202
T=N+≤5 zombie thread pollSteerAndInject → GETDEL → XADD zombie:{id}:events
                   XREADGROUP returns it
                   INSERT core.zombie_events status='received'
                   balance + approval gates
                   resolveSecretsFromVault
                   createExecution + setExecutionActive → Unix socket → zombied-executor
T=N+Δ  zombied-executor wakes, creates session
                   startStage → runner.execute → Agent.runSingle
                     tool: http_request GET fly...      (secrets substituted at tool-bridge)
                     tool: http_request GET upstash...
                     tool: http_request POST slack...
                   return StageResult → Unix socket → zombied-worker
T=N+Δ' zombied-worker UPDATE core.zombie_events status='processed', response_text, tokens
                   checkpointState, metering, XACK zombie:{id}:events
                   back to XREADGROUP
T=N+Δ" zombied-executor (session destroyed in defer) sleeps

...later...

T=M    Operator  → zombied-api   POST .../kill
                   zombied-api   UPDATE core.zombies status='killed'
                   zombied-api   XADD zombie:control type=zombie_status_changed status=killed
                   zombied-api   ← 200
T=M+ε  watcher unblocks → cancels[id].store(true)
                   if execution_id present in core.zombie_sessions:
                     executor.cancelExecution(execution_id) → Unix socket → zombied-executor
                     zombied-executor flips session.cancelled=true; in-flight runner.execute aborts
                   XACK control
T=M+Δ  zombie thread watchShutdown sees cancel_flag → running=false
                   event loop exits → thread returns
                   (reaped by watcher on next process shutdown OR tracked out-of-band)
```

---

## 8. Invariants (hard guardrails)

| # | Invariant | Why it matters | Enforced by |
|---|---|---|---|
| 1 | `core.zombies` row exists ⇒ `zombie:{id}:events` stream and `zombie_workers` group exist | No race on first webhook. No "event XADDs before thread arrives, then sits without a consumer." | `innerCreateZombie` does INSERT + XGROUP CREATE + XADD atomically before 201 (M33_001) |
| 2 | Raw credential bytes never appear in LLM context, logs, DB | Prompt injection, log exfil, replay | Substitution at tool-bridge (M35_001); grep-assert in M37_001 §2.4 |
| 3 | At-least-once delivery on `zombie:{id}:events` | Webhooks are real events from real customers | XACK only after successful UPDATE + checkpoint (processNext). Don't XACK on TransportLoss. |
| 4 | Per-event idempotency | Replay after crash must not double-post | `core.zombie_events UNIQUE(zombie_id, event_id)` |
| 5 | Kill is immediate for in-flight runs | Operators flipping status=killed mean NOW, not "within 5s" | Watcher calls `executor.cancelExecution` immediately on control msg; does not wait for the zombie thread's next XREADGROUP cycle |
| 6 | One executor session per event | Session holds policy + secrets_map scoped to this execution | `createExecution` + defer `destroyExecution` in every processNext |
| 7 | Worker restart is safe | Any failure mode recovers from pg + streams | No in-memory state is truth; watcher reconciles on startup; XAUTOCLAIM reclaims orphaned pending |
| 8 | Chat is the one operator-initiated channel | Don't grow a `/fire` `/trigger` `/invoke` API per synonym | CLI `zombie chat` + UI chat widget both hit `/steer` |

---

## 9. Debugging — where to grep, what to look at

| Question | Command / place |
|---|---|
| "Did the create-stream race happen?" | `XLEN zombie:control` (should include the create msg); `XINFO STREAM zombie:{id}:events` |
| "Is the watcher processing control messages?" | `XPENDING zombie:control zombie_workers_control` (should trend to 0) |
| "Is a specific zombie's thread alive and consuming?" | `XINFO CONSUMERS zombie:{id}:events zombie_workers` |
| "Did a message fail to ACK?" | `XPENDING zombie:{id}:events zombie_workers` (entries idle > threshold will be XAUTOCLAIM candidates) |
| "What did the zombie receive and return?" | `SELECT * FROM core.zombie_events WHERE zombie_id=<id> ORDER BY created_at DESC` |
| "Is the executor alive?" | Unix socket connect + `ping` RPC (future) / process health `ps` / healthz endpoint (future) |
| "Was a credential value leaked?" | Grep seeded test token across: `core.zombie_events.{request_json,response_text}`, `core.zombie_activities.detail`, zombied-api + zombied-worker logs. Expected 0 hits outside the executor process memory. |
| "Did the agent self-schedule?" | `SELECT * FROM core.zombie_events WHERE actor LIKE 'cron:%'`; also NullClaw cron state (`cron_list` tool call in an interactive chat) |

---

## 10. Per-step ownership (M33–M39)

| Step in §7 | Owner workstream |
|---|---|
| `innerCreateZombie` + XGROUP + XADD zombie:control | **M33_001** (control stream producer additions) |
| Watcher thread | **M33_001** |
| Per-zombie cancel flag | **M33_001** |
| WorkerState drain | **M33_001** (port from pre-M10) |
| `zombiectl zombie chat` interactive CLI | **M33_001** |
| UI chat widget | **M33_001** (or M36_001 if tightly coupled with SSE) |
| `core.zombie_events` schema + write path + `actor` field | **M34_001** |
| `GET /v1/.../zombies/{id}/events` + `zombiectl zombie events` + UI events tab | **M34_001** |
| `createExecution` per-session policy (network/tools/secrets) | **M35_001** |
| `http_request` credential templating at tool-bridge | **M35_001** |
| Config PATCH + `zombie_config_changed` control msg | **M35_001** |
| SSE `/activity:stream` + UI live watch + CLI `zombie watch` | **M36_001** |
| Docs polish + launch-post rewrite | **M36_001** |
| `samples/platform-ops/` three files | **M37_001** |
| `samples/homebox-audit/` new three files | **M38_001** |
| Lead-collector teardown (post-flagship cleanup) | **M39_001** |
| `zombiectl zombie install --from <path>` | **M19_003** (prerequisite for M33) |

---

## 11. Deferred / out of current scope

- Edge-worker-in-tenant-network (creds never in control plane). Rejected as scope for v2.0-alpha; revisit post-first-customer.
- Zig-side prose-allowlist parser + verb-policy dispatch gate. Not needed while platform-ops uses `http_request` only; revisit when kubectl/shell-enabled zombies ship.
- Per-tenant rate limiting (v1 had a `TenantRateLimiter`; port when multi-tenant load warrants).
- pg LISTEN/NOTIFY for control-plane change propagation (the Redis stream + 30s pg reconcile is belt-and-suspenders enough for MVP).
- Multiple zombied-worker replicas (single-worker suffices for pre-alpha; per-zombie Redis lease is a known v2 add).
- Agent-driven delegation (`delegate` tool, `subagent` runner — NullClaw primitives we haven't switched on yet).
