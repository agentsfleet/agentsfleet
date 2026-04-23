# M37_001: Platform-Ops Zombie — Flagship Executable Sample

**Prototype:** v2.0.0
**Milestone:** M37
**Workstream:** 001
**Date:** Apr 23, 2026
**Status:** IN_PROGRESS — rewrite replaces the prior "homelab-zombie / kubectl-first" scope (see Discovery #1). Sample files need full rewrite; spec sections land incrementally alongside M33_001..M36_001.
**Priority:** P1 — the flagship sample we dogfood as first customer. Without it the v2.0-alpha story has no end-to-end executable proof of the "MD + secrets + APIs + LLM reasoning" claim.
**Batch:** B1 — alpha gate, parallel with M33_001 (worker control stream + chat), M34_001 (event history), M35_001 (per-session policy + credential templating), M36_001 (live watch + docs). Blocks M39_001 (lead-collector teardown references this sample).
**Branch:** feat/m37-platform-ops (not yet created)
**Depends on:** M19_003 (`zombiectl zombie install --from <path>`), M33_001 (control stream, chat, `XGROUP CREATE` + XADD `zombie:control` inside `innerCreateZombie`, per-zombie cancel), M34_001 (core.zombie_events + actor), M35_001 (per-session `network_policy` + `tools` + `secrets_map` on `createExecution`, credential templating on `http_request`), M13_001 (structured `{host, api_token}` creds in vault). Independent of M20_001 (approval inbox) and M38_001 (homebox-audit).

**Canonical architecture:** `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` — the 11-step walk below is the concrete platform-ops instance of the pattern described there.

---

## Overview

**Goal (testable):** Under the author's actual `kishore@usezombie` signup (not a test fixture, not a mock), `zombiectl zombie install --from samples/platform-ops` creates a zombie. `zombiectl zombie chat {id}` opens an interactive session — reprints historical messages, accepts live input, streams `[claw]` responses as they land, Ctrl-C exits while the zombie keeps running. On first chat the agent polls fly.io + upstash via `http_request`, correlates, posts to Slack `#platform-ops`, and may optionally schedule a recurring poll via NullClaw's `cron_add`. Every tool call goes through the `http_request` NullClaw tool with `${secrets.x.y}` substituted at the tool-bridge. Every event lands in `core.zombie_events`. The full dogfood fits in the $10 starter credit.

**Problem:** v2.0-alpha needs ONE flagship sample that proves the product claim: _"describe the zombie in SKILL.md, declare APIs + secrets in TRIGGER.md, the LLM reasons."_ No Zig/JS connector. No vendor SDK. No webhook routing gymnastics. Just prose + credentials + `http_request`. Today no executable sample runs end-to-end: homelab-zombie as originally specced was kubectl-first and depended on an edge-worker binary that was rejected; its three sample files need rewrite, not incremental edit.

**Solution summary:** Rewrite `samples/platform-ops/` as a three-file bundle (SKILL.md + TRIGGER.md + README.md) that watches fly.io + upstash via `http_request`, correlates, posts to Slack, and optionally self-schedules via `cron_add`. Trigger model: chat (universal operator input via `/steer`) + NullClaw-managed cron when the agent elects it. Credentials: structured entries `{host, api_token}` in `vault.secrets`, loaded into the zombie's execution policy per `createExecution` (M35_001) and substituted into `http_request` calls at the tool bridge. Policy: prose allowlist inside SKILL.md (LLM reads its own prompt); the per-zombie tool list in TRIGGER.md is the only code-enforced gate. No separate Zig verb-policy parser (deferred until kubectl/shell-heavy zombies ship).

---

## Acceptance walkthrough (end-to-end, under author signup)

All 11 steps run against the author's real `kishore@usezombie` tenant. Numbers tie to Files Changed and Sections below.

### Terminology

| Process | Role |
|---|---|
| **zombied-api** (`zombied serve`) | HTTP routes. Writes `core.zombies`, `vault.secrets`, `zombie:control` (produces), `zombie:{id}:steer` (produces). Reads `core.zombie_events` for history. |
| **zombied-worker** (`zombied worker`) | Hosts one watcher thread (consumes `zombie:control`) + N zombie threads (consume `zombie:{id}:events`). Owns per-zombie cancel flags. Never runs LLM code. |
| **zombied-executor** (sidecar; `zombied executor`) | Unix-socket RPC server. Hosts NullClaw agent inside Landlock + cgroups + bwrap. Credential substitution lives here. |

### Stream + DB ownership

| Target | Producer | Consumer |
|---|---|---|
| `zombie:control` | zombied-api on `innerCreateZombie`, status change, config PATCH | zombied-worker watcher |
| `zombie:{id}:events` | zombied-api on webhook/slack/svix. zombied-worker on steer inject. (Optional future: NullClaw cron firing.) | zombied-worker's zombie thread |
| `zombie:{id}:steer` (Redis key) | zombied-api on chat `/steer` | zombied-worker zombie thread (polls + GETDEL at top of loop) |
| `core.zombie_events` | zombied-worker zombie thread (INSERT on receive, UPDATE on complete) | zombied-api `GET /events` |
| `core.zombies` | zombied-api only | zombied-worker at claim + watcher tick |
| `core.zombie_sessions` | zombied-worker (checkpoint + execution_id) | zombied-worker at claim + kill path |
| `vault.secrets` | zombied-api on `credential add` | zombied-worker resolves just-in-time before each `createExecution` |

### The 11 steps

| # | Action | zombied-api | `zombie:control` | `zombie:{id}:events` | zombied-worker | zombied-executor |
|---|---|---|---|---|---|---|
| 1 | Sign in via Clerk | OAuth callback → INSERT core.users/workspaces | — | — | idle | idle |
| 2–4 | `zombiectl credential add {fly,upstash,slack} --from op://...` | `PUT /v1/.../credentials/{name}` → crypto_store.store → UPSERT vault.secrets | — | — | idle | idle |
| 5 | `zombiectl zombie install --from samples/platform-ops` | `innerCreateZombie`: INSERT core.zombies (active). **Atomically + before 201**: `XGROUP CREATE zombie:{id}:events zombie_workers 0 MKSTREAM` + XADD `zombie:control` type=zombie_created. **No race**: stream + group exist before any producer/consumer can arrive. | +1 entry | stream+group created, empty | idle | idle |
| 6 | Watcher wakes | — | — | — | Watcher XREADGROUP on `zombie:control` unblocks. SELECT core.zombies row. Allocates cancel_flag, spawns zombie thread. XACK. Zombie thread claims (loads config + checkpoint), blocks on XREADGROUP `zombie:{id}:events` BLOCK 5s. | idle |
| 7 | `zombiectl zombie chat {id}` opens interactive session: prints historical messages (GET `/events` filtered) then prompts. User types line → CLI POSTs `/steer`. | `innerSteer`: SET `zombie:{id}:steer "<msg>" EX 300`, 202. | — | — | (within ≤5s) zombie thread's `pollSteerAndInject` GETDEL steer key → XADD `zombie:{id}:events` event_id type=chat source=steer actor=`steer:kishore` data=msg. Next XREADGROUP returns it. | idle |
| 8a | processEvent starts | — | — | +1 entry, consumed, in pending list | `processEvent`: INSERT `core.zombie_events` (status='received', actor='steer:kishore', request_json=msg). Balance gate + approval gate pass. Resolves credentials from vault just-in-time. `executor.createExecution(workspace_path, {network_policy, tools, secrets_map})` over Unix socket. `setExecutionActive`. `executor.startStage(execution_id, {agent_config, message, context})`. | `handleCreateExecution` creates session storing policy. `handleStartStage` invokes `runner.execute` → NullClaw `Agent.runSingle`. **Wakes.** |
| 8b | Agent runs inside executor | — | — | — | waiting on Unix socket | Tool calls (order agent decides): `http_request GET ${fly.host}/v1/apps` (tool-bridge substitutes `${secrets.fly.api_token}` after sandbox entry, agent never sees raw bytes). `http_request GET ${fly.host}/v1/apps/{app}/logs` per app. `http_request GET ${upstash.host}/v2/redis/stats/{db}`. `http_request POST ${slack.host}/api/chat.postMessage` with `${secrets.slack.bot_token}`. **Optional** if prose requests it: `cron_add "*/30 * * * *" "poll fly+upstash"` — NullClaw persists the schedule; future fires will land as synthetic events on `zombie:{id}:events` with actor=`cron:<schedule>` (via NullClaw cron runtime; exact wiring in M35_001). |
| 8c | Agent returns StageResult | — | — | — | Receives `{content, tokens, wall_s, exit_ok}` on Unix socket. updateSessionContext (in-memory). Defers destroyExecution + clearExecutionActive. | `runner.execute` returns; session destroyed on handler side; executor **sleeps** (no other work). |
| 8d | zombie thread finalizes | — | — | XACK | UPDATE core.zombie_events (status='processed', response_text, tokens, wall_ms, completed_at). checkpointState → UPSERT core.zombie_sessions. metering.recordZombieDelivery. XACK. CLI's chat session (polling GET `/events` or tailing activity stream) picks up the new row and prints `[claw] <response_text>`. Back to XREADGROUP BLOCK. | idle |
| 9 | `zombiectl zombie events {id}` OR Ctrl-C then re-open chat | `GET /v1/.../zombies/{id}/events` reads core.zombie_events | — | — | idle (still listening on XREADGROUP) | idle |
| 10 | `zombiectl zombie kill {id}` | UPDATE core.zombies SET status='killed'. XADD `zombie:control` type=zombie_status_changed status=killed. | +1 entry | — | Watcher reads control msg, sets `cancels[id].store(true)`, reads execution_id from zombie_sessions, calls `executor_client.cancelExecution(execution_id)`. Zombie thread's watchShutdown sees cancel_flag → running=false → exits loop → thread returns. | If mid-stage: `handleCancelExecution` flips session.cancelled=true; in-flight `runner.execute` breaks out with `.cancelled`. |
| 11 | Grep test-token-xyz across logs + DB | manual | — | — | token bytes held only transiently during `createExecution` RPC | token bytes held only in session memory + emitted inline into HTTPS TCP to upstream — never logged, never written to disk |

### Notable properties this walkthrough proves

- **No race on stream/group creation.** `innerCreateZombie` does INSERT + XGROUP CREATE + XADD `zombie:control` synchronously before returning 201. Any webhook arriving within microseconds of the 201 finds the stream already there.
- **Chat is the universal input.** CLI and UI both hit `/steer`. No separate `fire`/`trigger`/`invoke`. Latency ≤5s (steer-key + top-of-loop poll); interactive CLI bridges the gap by streaming responses from the activity tail.
- **NullClaw owns cron.** If the agent elects a schedule, `cron_add` persists it in NullClaw's runtime. Platform-ops doesn't declare a platform-cron in TRIGGER.md; all scheduling is agent-initiated. One-shot chat runs just complete.
- **Credentials never enter agent context.** Substitution happens at tool-bridge, inside the executor, after sandbox entry. Agent sees `${secrets.fly.api_token}`; HTTPS request headers get real bytes; responses never echo the token.
- **Kill is immediate for in-flight runs.** Control-stream XADD triggers `cancelExecution` RPC within milliseconds — not the 5s XREADGROUP cycle.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `samples/platform-ops/SKILL.md` | CREATE | Diagnostic prompt, names `http_request` as the one tool, prose budget note (≤$8/month), optional `cron_add` guidance. |
| `samples/platform-ops/TRIGGER.md` | CREATE | `trigger.type: chat`, `tools:[http_request, memory_recall, memory_store, cron_add, cron_list, cron_remove]`, `credentials:[fly, upstash, slack]`, `network.allow:[api.machines.dev, api.upstash.com, slack.com]`, `budget.monthly_dollars:8`. |
| `samples/platform-ops/README.md` | CREATE | Operator quickstart under signup: credential setup, install, chat UX, example, credential-hygiene + cron story. |
| `samples/homelab/` | DELETE | Old kubectl-first sample superseded. M39_001's orphan sweep catches stale refs. |

No `src/**` changes in this workstream — all worker/executor/CLI wiring lives in M33_001..M36_001. Integration tests that prove §2 dims live alongside those.

---

## Applicable Rules

**FIR** (TRIGGER.md `tools:` subset of NullClaw v2026.4.9 primitives). **ORP** (no `samples/homelab/` refs outside historical docs). **TST-NAM** (no milestone IDs in any test name). *(FLL does not apply — markdown is exempt per CLAUDE.md.)*

---

## Sections (implementation slices)

### §1 — Sample authoring

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `samples/platform-ops/SKILL.md` | skill parser | model `claude-sonnet-4-6`; names `http_request`; prose budget ≤$8/month; optional `cron_add` guidance | integration |
| 1.2 | PENDING | `samples/platform-ops/TRIGGER.md` | trigger loader | matches shape above; every tool name resolves in NullClaw v2026.4.9; budget ≤ 8 | integration |
| 1.3 | PENDING | `samples/platform-ops/README.md` | new-operator read-through | sections: Prereqs · Credential setup × 3 · Install · Chat (CLI interactive UX documented + UI path) · Example diagnosis · Cron self-scheduling story · Credential hygiene story | manual |

### §2 — Install + chat integration (dogfood-ready)

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | BLOCKED_ON M19_003, M33_001 | CLI | `zombiectl zombie install --from samples/platform-ops` | 201 + zombie_id; zombie row active; zombie:control entry; watcher spawns thread; stream+group exist | integration |
| 2.2 | BLOCKED_ON M33_001 | CLI | `zombiectl zombie chat {id}` → type "poll fly+upstash" | history replays; steer key written; synthetic event on zombie:{id}:events; ≥3 http_request tool calls in executor; Slack post lands; `[claw] <response>` prints in chat client | integration |
| 2.3 | BLOCKED_ON M33_001, M35_001 | missing `fly` credential | chat | one `UZ-GRANT-001`; no http_request fires; zombie stays active; clear error in chat output | integration |
| 2.4 | BLOCKED_ON M35_001 | cred non-leak | seed `fly.api_token="test-token-xyz"` | `grep test-token-xyz` across core.zombie_events, core.activity_events, zombied-api + zombied-worker logs = 0 hits. Appears only transiently in executor process memory. | integration (grep-assert) |
| 2.5 | OPTIONAL | agent self-schedules | chat prompts "poll every 30 min" | agent emits `cron_add`; future events arrive on zombie:{id}:events with actor=`cron:*/30 * * * *`. NullClaw cron runtime wires exact path (M35_001). | integration |

### §3 — Acceptance under author signup

| Dim | Status | Target | Input | Expected |
|-----|--------|--------|-------|----------|
| 3.1 | BLOCKED_ON §2 | kishore@usezombie CLI track | steps 1–11 of acceptance walkthrough | all steps green; Slack post visible; cost ≤ $2/run |
| 3.2 | BLOCKED_ON §2 + M36_001 | kishore@usezombie UI track | same steps via dashboard: sign-in → credentials page → install from sample catalog → chat widget on zombie detail → live activity tab → kill button | all steps green; status transitions visible live |

---

## Interfaces (consumed)

- **SKILL.md / TRIGGER.md** frontmatter schemas (existing).
- **`POST /v1/workspaces/{ws}/zombies`** (existing) + **XGROUP CREATE** + **XADD zombie:control** additions (M33_001).
- **`POST /v1/.../zombies/{id}/steer`** (existing M23_001).
- **`POST /v1/.../zombies/{id}/kill`** (new in M33_001 or repurposed from current `/stop`).
- **`GET /v1/.../zombies/{id}/events`** (new in M34_001).
- **`zombiectl zombie install --from <path>`** (M19_003).
- **`zombiectl zombie chat {id}`** (M33_001 — interactive streaming CLI).
- **`zombiectl credential add <name>`** (M13_001) with structured-credential bodies: `fly = {host, api_token}`, `upstash = {host, api_token}`, `slack = {host, bot_token}`.
- **NullClaw tools**: `http_request`, `memory_recall`, `memory_store`, `cron_add`, `cron_list`, `cron_remove`.
- **Executor RPC** (M35_001): `createExecution` grows `{network_policy, tools, secrets_map}`; tool-bridge does `${secrets.x.y}` substitution on `http_request` header/body fields.

---

## Failure Modes

| Failure | Trigger | Behavior | Observed |
|---|---|---|---|
| Credential missing | operator chats before `credential add fly` | one `UZ-GRANT-001`; zombie halts cleanly; agent reasons from error in response_text | clear event pointing at the add command |
| Fly.io 5xx mid-poll | transient upstream | `http_request` returns error; agent retries once, reports inconclusive | error + reasoning in activity stream |
| Slack post fails (bad bot token) | rotated slack without updating vault | final http_request 401; diagnosis still persisted in response_text | operator sees via `zombie events` |
| Prompt-injection asks for token | malicious chat input | agent can at worst print `${secrets.fly.api_token}` literal; substitution never hits agent context | harmless placeholder in output |
| Budget exhaustion mid-run | >$8 spent this month | balance gate blocks next event; in-flight completes | balance-exhausted activity event |
| zombied-worker SIGKILL mid-run | ops | on restart watcher re-reads core.zombies, respawns threads; XAUTOCLAIM reclaims pending events; zombie_events UNIQUE makes replay idempotent | one-turn delay possible; no duplicate Slack post if agent reasoning dedupes (agent sees prior context) |
| zombied-executor crash mid-run | bug | `startStage` returns TransportLoss; worker does NOT XACK; event replays on retry | activity stream shows transport_loss; next claim retries |

---

## Implementation Constraints (Enforceable)

| Constraint | Verification |
|---|---|
| Only NullClaw-registered tools in TRIGGER.md `tools:` | Eval E4 |
| SKILL.md names `http_request` as primary; no kubectl/docker primary refs | Eval E5 |
| Credential refs consistent SKILL/TRIGGER/README | Eval E6 |
| $10 starter credit covers ≥1 full acceptance run | manual billing check post-§3.1 |
| No `samples/homelab/` references | Eval E7 |

---

## Invariants (Hard Guardrails)

| # | Invariant | Enforcement |
|---|---|---|
| 1 | No raw credential bytes in any log, event, activity-stream row, or LLM context | §2.4 grep-assert + §3 spot-check |
| 2 | TRIGGER.md `tools:` ⊆ NullClaw v2026.4.9 tool set | Eval E4 |
| 3 | No `samples/homelab/` refs outside historical/brainstormed docs | Eval E7 + M39_001 orphan sweep |
| 4 | Chat is the one operator-initiated input channel (no webhook in MVP TRIGGER.md; cron is agent-elected) | grep: no `trigger.type: webhook` in TRIGGER.md |
| 5 | Budget cap ≤ 8 monthly_dollars in TRIGGER.md | Eval E8 |

---

## Test Specification

### Unit

| Test | Dim | Target | Expected |
|---|---|---|---|
| `platform-ops skill parses` | 1.1 | SKILL.md | frontmatter shape + prose assertions |
| `platform-ops trigger parses` | 1.2 | TRIGGER.md | full shape |
| `trigger tools are NullClaw primitives` | inv 2 | tools array | all names known |
| `budget under starter credit` | inv 5 | budget.monthly_dollars | ≤ 8 |
| `no webhook trigger configured` | inv 4 | trigger.type | == "chat" |

### Integration (lands in M34/M36 worktrees; referenced here)

| Test | Dim | Expected |
|---|---|---|
| `install from platform-ops samples` | 2.1 | 201 + stream+group created + control msg consumed + thread spawned |
| `chat end-to-end` | 2.2 | ≥3 http_request calls; Slack mock hit 1×; `[claw]` prints |
| `missing credential halts cleanly` | 2.3 | one `UZ-GRANT-001`; no http_request |
| `credential non-leak` | 2.4 | grep of seeded token in all logs+DB = 0 |
| `agent self-schedule` (optional) | 2.5 | `cron_add` call; subsequent synthetic event with actor=cron:* |

### Acceptance (manual dogfood)

| Test | Dim | Expected |
|---|---|---|
| `CLI dogfood under signup` | 3.1 | all 11 steps green, Slack post visible |
| `UI dogfood under signup` | 3.2 | same via dashboard |

---

## Execution Plan (Ordered)

| Step | Action | Verify |
|---|---|---|
| 1 | CHORE(open): spec already in active. Create worktree `../usezombie-m37-platform-ops` on `feat/m37-platform-ops`. | `git worktree list` |
| 2 | Write `samples/platform-ops/SKILL.md` | §1.1 |
| 3 | Write `samples/platform-ops/TRIGGER.md` | §1.2 |
| 4 | Write `samples/platform-ops/README.md` | §1.3 |
| 5 | `git rm -r samples/homelab/` | filesystem clean |
| 6 | §2 integration tests land in M34/M36 branches; verify here after those merge | §2.1–2.4 |
| 7 | CLI dogfood (§3.1) under kishore@usezombie signup; capture transcript in Ripley's Log | §3.1 |
| 8 | UI dogfood (§3.2) once M36_001 ships; capture screenshots | §3.2 |
| 9 | CHORE(close): spec → done/, Ripley log, release-doc `<Update>` block | spec in done/; changelog updated |

---

## Acceptance Criteria

- [ ] Three files at `samples/platform-ops/`; `samples/homelab/` deleted — E1
- [ ] Credential names consistent — E6
- [ ] Install succeeds + stream+group+control msg fire — §2.1
- [ ] Chat opens interactive session; history replays; `[claw]` prints response — §2.2
- [ ] Credential-missing path emits one `UZ-GRANT-001` — §2.3
- [ ] Seeded credential value never appears in any log / event / LLM context — §2.4
- [ ] CLI dogfood under `kishore@usezombie` signup green end-to-end — §3.1
- [ ] UI dogfood under same signup green end-to-end — §3.2
- [ ] Cost per dogfood run ≤ $2 — manual billing check
- [ ] No `samples/homelab/` refs outside historical docs — E7

---

## Eval Commands

```bash
# E1
test -f samples/platform-ops/SKILL.md    && echo "ok: SKILL.md"
test -f samples/platform-ops/TRIGGER.md  && echo "ok: TRIGGER.md"
test -f samples/platform-ops/README.md   && echo "ok: README.md"
test ! -d samples/homelab                && echo "ok: homelab removed" || echo "FAIL"

# E4 (tools are NullClaw primitives — lookup against v2026.4.9 manually once)
grep -A 10 '^tools:' samples/platform-ops/TRIGGER.md

# E5
grep -E 'http_request' samples/platform-ops/SKILL.md >/dev/null \
  && echo "ok: http_request named" || echo "FAIL"
grep -iE 'kubectl|docker' samples/platform-ops/SKILL.md \
  | grep -viE 'optional|future' \
  && echo "WARN: non-http_request tool mentioned as primary" || echo "ok"

# E6
for c in fly upstash slack; do
  count=$(grep -l "$c" samples/platform-ops/{SKILL.md,TRIGGER.md,README.md} | wc -l | tr -d ' ')
  echo "$c: $count/3"
done

# E7
grep -rn "samples/homelab" . \
  | grep -v -E '(\.git/|v1/done/|v2/done/|docs/brainstormed/|docs/nostromo/|docs/changelog|P1_API_CLI_UI_M39_001)' \
  && echo "FAIL: orphan homelab ref" || echo "ok"

# E8
grep -E 'monthly_dollars' samples/platform-ops/TRIGGER.md \
  | awk -F: '{ print $2 }' | tr -d ' ' \
  | awk '{ if ($1+0 > 8) print "FAIL: " $1 " > 8"; else print "ok: " $1 }'
```

---

## Dead Code Sweep

`samples/homelab/{SKILL.md,TRIGGER.md,README.md}` deleted. No `src/`, `zombiectl/`, `ui/` deletions here — M39_001 owns any downstream cleanup.

---

## Verification Evidence

**Status:** PENDING — fills during EXECUTE + §3 dogfood.

| Check | Eval | Result |
|---|---|---|
| Files present | E1 | ⏳ |
| Tools are NullClaw primitives | E4 | ⏳ |
| http_request primary | E5 | ⏳ |
| Credentials consistent | E6 | ⏳ |
| Orphan sweep | E7 | ⏳ |
| Budget ≤ $8 | E8 | ⏳ |
| §2 integration | — | BLOCKED_ON M34/M36 |
| §3 dogfood | — | BLOCKED_ON §2 + M37 |

---

## Discovery

1. **Scope reversal (Apr 23, 2026).** Original M33 was "homelab-zombie / kubectl-first / edge-worker binary." Conversation with owner surfaced that the edge-worker model (old M35_001, now deleted via git rm) was invalid and kubectl-first was not the right first-customer demo (author's infra runs on fly.io + upstash, not k3s). Full rewrite; `samples/homelab/` deleted.
2. **Decision: NullClaw owns cron.** TRIGGER.md has no platform cron. Agent elects a schedule via `cron_add`; NullClaw's cron runtime persists it; future fires land on `zombie:{id}:events` as synthetic events with `actor=cron:<schedule>`. Exact wiring lives in M35_001 (may require reading nullclaw/src/cron.zig + daemon.zig to finalize path).
3. **Decision: Chat is universal input.** CLI `zombiectl zombie chat` + UI chat widget both hit `/steer`. No separate `fire`/`trigger` commands. CLI is interactive: history replay + live prompt + streamed `[claw]` responses + Ctrl-C exits (zombie keeps running).
4. **Decision: Credential templating at tool-bridge.** Agent references `${secrets.x.y}`; executor substitutes at dispatch post-sandbox. Never in LLM context. Implementation in M35_001.
5. **Deferred: Zig verb-policy parser.** The prior M34_001 spec (now deleted via git rm) proposed a Zig parser for the prose allowlist. Not needed for MVP — platform-ops uses `http_request` only. Revisit when kubectl/shell zombies ship.
6. **Decision: Stream + group created at install time, no race.** `innerCreateZombie` does INSERT + `XGROUP CREATE MKSTREAM` + XADD `zombie:control` synchronously, all before 201. Any webhook/steer immediately after install finds the stream ready.

---

## Out of Scope

- `samples/homebox-audit/` — separate M38_001.
- kubectl/docker tool enablement — v1.1 zombie behind approval gates.
- Remediation writes — post-alpha, approval-gated.
- Slack interactive messages (buttons, modals) — plain text only.
- Multi-channel Slack routing — single channel per zombie.
- PagerDuty / email outputs — same pattern, future credentials.
- Prometheus / Loki / Datadog sources — fly.io + upstash suffices for first customer.
- Approval gates — M20_001 territory.
