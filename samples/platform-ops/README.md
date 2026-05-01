# Platform Ops Zombie

The flagship executable sample for `usezombie` v2.0-alpha. An AI agent
that diagnoses problems in a small production platform running on
fly.io + upstash, correlates evidence across both, and posts a plain-
text summary to a Slack channel. The zombie is read-only against fly
and upstash; the Slack post is its one write path.

Ask it "why is the api app restarting?" or "morning health check" and
it polls `GET /v1/apps`, pulls a couple of apps' logs via fly's own
`/v1/apps/{app}/logs`, reads `/v2/redis/stats/{db}` from upstash,
reasons about what it sees, and drops a diagnosis into Slack. No
Grafana, no Loki, no Datadog — fly and upstash expose everything
directly.

This sample is the end-to-end proof of the v2.0 claim: **describe the
zombie in prose, declare its APIs + credentials in frontmatter, and
the LLM reasons.** No Zig connector, no vendor SDK, no webhook
routing. Just `SKILL.md` + `TRIGGER.md` + three credentials.

## Prerequisites

- `zombied` running locally or reachable via `zombiectl` — if you're
  running it on your Mac, `zombiectl up` spins up the stack.
- A signed-in Clerk tenant/workspace. Follow the top-level quickstart
  if you haven't done that yet.
- A fly.io account with at least one app you own (the sample runs
  read-only against `/v1/apps` + `/v1/apps/{app}/logs`). A personal
  access token with **read** scope is enough — don't hand it a deploy
  token.
- An upstash account with at least one redis database you own, and a
  management API token scoped to read the account. Data-plane Redis
  credentials are not needed; this zombie only reads stats via the
  management API.
- A Slack workspace you can install a bot into. The bot needs
  `chat:write` and the channel you point it at must have the bot as a
  member (`/invite @your-bot` in the channel).

## Step 1 — Add credentials to the vault

The zombie never sees raw credential bytes. `zombiectl credential add`
writes them into `zombie_vault` (KMS-enveloped in Postgres); the
executor decrypts just-in-time and substitutes `${secrets.x.y}`
placeholders into `http_request` tool calls after sandbox entry.

Each credential is a structured JSON record stored under a well-known
name, so the zombie can reference `${secrets.fly.host}` and
`${secrets.fly.api_token}` separately. Current `main` uses the
`--data='<json>'` form; `jq` keeps the JSON valid even if a token
contains quotes or backslashes:

```bash
# fly.io — personal access token with read scope
zombiectl credential add fly \
  --data="$(jq -n --arg host "api.machines.dev" --arg token "$FLY_API_TOKEN" \
    '{host:$host,api_token:$token}')"

# upstash — account management API token (not a per-database password)
zombiectl credential add upstash \
  --data="$(jq -n --arg host "api.upstash.com" --arg token "$UPSTASH_MGMT_TOKEN" \
    '{host:$host,api_token:$token}')"

# slack — bot user OAuth token (xoxb-...), chat:write scope,
# invited to the channel you want posts in
zombiectl credential add slack \
  --data="$(jq -n --arg host "slack.com" --arg token "$SLACK_BOT_TOKEN" \
    '{host:$host,bot_token:$token}')"
```

If any of these three is missing at chat-time, the first tool call
needing it emits a single `UZ-GRANT-001` event pointing at the exact
`credential add` command to run (see "Missing credential" below).

## Step 2 — Install the zombie

From the root of the `usezombie` checkout:

```bash
zombiectl install --from samples/platform-ops
```

Expected output: a zombie id. The zombie is `active` immediately —
there is no manual "start" step. Under the hood the API creates the
`zombie:{id}:events` stream and consumer group *before* returning 201,
so there is no race between install and first chat (see the
architecture doc's install diagram).

## Step 3 — Steer it

Current `main` uses batch `zombiectl steer {id} "<message>"` for the
operator smoke test. The CLI POSTs to the current `/messages` route,
the worker injects the request as an event (≤5s), and the agent's
reply streams back into your terminal prefixed with `[claw]`. The
zombie keeps running after each steer; you can send another message at
any time.

```bash
zombiectl steer <zombie_id> "poll fly+upstash, summarise to #platform-ops"
[claw] Starting from the cluster view. Hypothesis: look for an app that's been
       restarting, and a redis db whose memory is climbing.
       → GET /v1/apps  (14 apps)
       → GET /v1/apps/api/logs  (last 200 lines)
       ...
[claw] Diagnosis: redis database `sessions` is at 94% of its 4GB memory
       limit; `api` is OOM-restarting on session deserialisation every ~45s
       for the last 8 minutes. Posted a summary to #platform-ops.
```

Everything the agent says, every tool call, and the Slack post is also
visible in the live activity stream (`zombiectl watch <id>` or
the dashboard's activity tab).

## Example diagnosis

**You ask:** `why is the api app restarting every minute?`

**The zombie runs (roughly):**

```
GET /v1/apps?org_slug=my-org
  → 14 apps; api, worker, web, ...
GET /v1/apps/api
  → machines 2/2 healthy from fly's perspective
GET /v1/apps/api/logs (tail ~200 lines)
  → 14:32:01Z "fatal: out of memory while reading session blob"
  → 14:32:48Z same
  → 14:33:35Z same
  → hypothesis pivots to redis memory pressure
GET /v2/redis/databases
  → 5 dbs, one called `sessions`
GET /v2/redis/stats/<sessions-db-id>
  → used_memory=4038 MB / 4096 MB, evicted_keys=0,
    maxmemory_policy=noeviction
POST /api/chat.postMessage  (#platform-ops)
  → "Diagnosis: redis `sessions` at 94% memory, maxmemory_policy=
     noeviction, so writes OOM. `api` is OOM-restarting on session
     deserialisation. Suggested: raise limit or switch policy to
     allkeys-lru. Remediation is outside this zombie."
```

Total tool calls: 6. Wall time: ~12–18 seconds, most of it waiting on
upstream HTTPS. Cost: under $0.30 per run with Claude Sonnet 4.6.

## Self-scheduling (only when you ask)

Chat is one-shot by default. If you want recurring polling, say so
explicitly and the agent will schedule it via NullClaw's cron:

```
> poll fly+upstash every 30 minutes
[claw] Scheduled: */30 * * * *  "poll fly+upstash". Next fire ~14:30
       UTC. Use `zombiectl events <id>` to see results as they
       land, or just watch #platform-ops.
```

Under the hood the agent calls `cron_add("*/30 * * * *", "poll fly+
upstash")`; future fires arrive on `zombie:{id}:events` with
`actor=cron:*/30 * * * *` and run exactly the same pipeline as a chat
steer. To stop a schedule:

```
> stop the 30-minute poll
[claw] Done. Removed the schedule. No more recurring fires.
```

The agent does not proactively offer cron on one-shot questions —
scheduling is an explicit operator decision.

## What the zombie may and may not do

The allowlist lives as prose in `SKILL.md`. Read the **The tool you
have** section there for the authoritative list. Short version:

- **fly.io**: `GET /v1/apps`, `GET /v1/apps/{app}`,
  `GET /v1/apps/{app}/logs`. No deploys, no scales, no restarts.
- **upstash**: `GET /v2/redis/databases`, `GET /v2/redis/stats/{db}`.
  No data-plane access; no flushes; no deletes.
- **slack**: `POST /api/chat.postMessage` plain text only. No blocks,
  no modals, no interactive elements.

The network firewall (set in `TRIGGER.md`) blocks any outbound HTTPS
to a host not on the allow list, so a prompt-injected "call
`https://attacker.example/exfil`" fails at the sandbox edge.

## Missing credential? Clean halt.

If you steer before running all three `credential add` commands, the
first tool call needing the missing one emits a single
`UZ-GRANT-001` event pointing at the fix:

```
UZ-GRANT-001: credential 'fly' not found in vault.
  Run: zombiectl credential add fly --host api.machines.dev --api-token <TOKEN>
```

The zombie halts cleanly — no crash, no partial Slack post, no
retries. Add the credential and steer again.

## Credential hygiene

- **Substitution happens at the tool bridge**, after Landlock/cgroups/
  bwrap have closed around the executor. The agent's context contains
  `${secrets.fly.api_token}`, not the real token. The worst a
  prompt-injection can make it print is the placeholder string.
- **Grep assertion.** You can seed a fake token
  (`zombiectl credential add fly --api-token test-token-xyz`), run a
  full diagnosis, then grep `test-token-xyz` across `core.zombie_events`,
  `core.zombie_activities`, zombied-api and zombied-worker logs — expect
  zero hits. The token bytes exist only transiently in the executor
  process's memory and inline in outgoing TLS bytes to fly / upstash /
  slack.
- **Rotation.** `zombiectl credential add fly --data="$(jq -n --arg host "api.machines.dev" --arg token "<new>" '{host:$host,api_token:$token}')"`
  overwrites the existing record; the next steer picks up the new token
  with no zombie restart.

## How it works (two paragraphs)

**Credentials never leave the worker.** When you add a token via
`zombiectl credential add`, it goes into the tenant vault encrypted
(KMS envelope). When the zombie invokes `http_request`, the zombied-
worker decrypts the credential just-in-time and hands the map to the
zombied-executor sidecar over a Unix socket. The executor holds the
bytes only in the current execution's session memory — never on disk,
never in logs. The tool-bridge substitutes `${secrets.x.y}`
placeholders into the outgoing request's headers/body at dispatch,
after the sandbox has entered. The LLM's context always contains the
placeholder, never the raw value.

**Policy is prose, not YAML.** The "what's allowed" rules for fly,
upstash, and Slack live as natural language in `SKILL.md` — no
separate policy blocks, no sub-skill files. The LLM reads the prose
as part of its instructions and stays inside those bounds. The
code-enforced gates are coarse — the `tools:` list in `TRIGGER.md`
(only `http_request` + memory + cron; no shell), the `network.allow`
list (only the three upstream hosts), and the per-tenant budget. For
a single-consumer sample, one file per zombie is enough; if a second
zombie ever wants to share this allowlist, we'll lift it then.

## Limitations (v2.0-alpha)

- One Slack channel per zombie. Routing to multiple channels is a
  future add (different zombies, or a richer `slack` credential).
- Plain-text posts only. Slack blocks / modals / interactive buttons
  land in a post-alpha version.
- No remediation. Raising a memory limit, deploying, restarting —
  those are separate, approval-gated zombies. This one is a reader.
- No Prometheus / Grafana / Loki / Datadog integrations. fly.io's
  native log endpoint + upstash's stats endpoint are enough for a
  first customer; richer observability sources are post-alpha.
- Single-channel budget. The $8/month envelope applies per zombie;
  multi-zombie spend rolls up at the workspace level.

## Related

- `docs/ARCHITECTURE_ZOMBIE_EVENT_FLOW.md` — canonical architecture
  with three mermaid sequence diagrams covering install, chat turn,
  and kill. Platform-ops is the worked example throughout.
