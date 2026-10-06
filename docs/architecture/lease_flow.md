# How a lease works, from "start a fleet" to a sandbox being nuked

> **Read this first.** A source trace from Oct 06, 2026, saved as read. It was
> written against `main` at `c5f7680f2` and the runner-sandbox worktree at
> `eeacb87bc`, so its `file:line` cites drift as code moves. It overlaps
> [`data_flow.md`](./data_flow.md) (the trigger, execute and one-lease-per-fleet
> sections), [`runner_fleet.md`](./runner_fleet.md) (running one event, and the
> renewal section) and [`runner_execution.md`](./runner_execution.md), and
> where the two disagree neither has been reconciled yet. A review that decides
> which page owns each claim is due before any of it is treated as canonical;
> until then this page is a trace, not a rule, and the pages above win.

Read against `main` at `c5f7680f2` and the sibling worktree `feat/m211-sandbox-tools-and-nested-loops` (head `eeacb87bc`, its five active specs). Every behavioural claim cites `file:line`; paths are under `rustd/crates/` unless they start with `docs/`, `schema/`, `deploy/`, `scripts/` or `tests/`. Where the the sandbox-tools milestone worktree changes the answer I say "main:" and "the sandbox-tools milestone:". Expanded acronyms: Enhanced Read-Only File System (EROFS), Random Access Memory (RAM), Hash-based Message Authentication Code (HMAC), Server-Sent Events (SSE), Time To Live (TTL), Out Of Memory (OOM), Chrome DevTools Protocol (CDP), Kernel-based Virtual Machine (KVM).

**The one fact that frames everything:** on `main` the production binary refuses every lease. `agentsfleet_runner/src/main.rs:114-137` boots, probes the host, then logs `NO_AGENT_ENGINE` (`:27`) and exits with status 2 (`:37`). `docs/architecture/runner_execution.md:160` and `docs/architecture/data_flow.md:1130` both record that the Zig runner serves every lease today. So sections 2, 6 and 7 describe what the Rust library crates do when driven by tests and the kernel lane, and what the binary will do once the runner cutover workstream composes `afr_supervisor::run` (Indy, Oct 04: "Defer to the runner cutover (Recommended)", its spec, line 320). Section 1 (the daemon side) is live production code regardless of runner.

---

## 1. From "start a fleet" to a lease

```text
 INSTALL (once)          core.fleets row (name = "AGENT BOB 01", config_json from TRIGGER.md,
                          source_markdown = SKILL.md)                         Postgres
                         stream fleet:{id}:events + consumer group            Dragonfly
                         core.integration_grants (fleet, "github") approved    Postgres
                         nothing else: no admission, no event, no lease row

 PR OPENED               POST /v1/ingress/github
   GitHub ──signed──►    1 verify HMAC vs vault "github-app".webhook_secret
                         2 installation.id ─► core.connector_installs ─► workspace
                         3 body ─► 12-field PullRequestDigest (not GitHub's payload)
                         4 fleets in that workspace: active ∩ approved github grant
                                                   ∩ TRIGGER repositories ∋ repo
                                                   ∩ TRIGGER events ∋ pull_request
                         5 per matched fleet:
                             INSERT core.fleet_admissions (producer=webhook_app,
                                 key = "{fleet}:{sha256(body)}")           Postgres  ← the acceptance
                             XADD fleet:{id}:events  (receipt)             Dragonfly ← a receipt
                             UPDATE admissions SET receipt                 Postgres
                             HSET fleet:ready:{p}  (mark, 16 partitions)   Dragonfly
                         6 202 {matched, enqueued}

 RUNNER POLL             POST /v1/runners/me/leases  (every ≤1 s per worker)
   (every ~1 s)          a peek ONE ready partition, ≤64 fleets, zero Postgres if empty
                         b SELECT_READY_CANDIDATES: active, slot free, labels, sticky first
                         c CLAIM_AFFINITY_SLOT  fleet.runner_affinity: fencing_seq+1,
                             leased_until = now+30 s            ← THE FENCE IS MINTED HERE
                         d reclaim a dead holder's active lease? else XAUTOCLAIM/XREADGROUP 1
                         e INSERT core.fleet_events status=received (+ stamp admission delivered)
                         f PUBLISH event_received bracket on fleet:{id}:activity
                         g gates: payer → balance → budget → receive debit → approval
                         h vault: static secrets + mintable "github"; build ExecutionPolicy
                             (tools, network.allow, origin rules for api.github.com)
                         i INSERT fleet.runner_leases  status=active, fencing_token, expires  ← THE LEASE ROW
                         j 200 { lease: { lease_id, fencing_token, lease_expires_at,
                                          event{digest}, policy, instructions=SKILL.md body,
                                          bundle{content_hash}? } }
```

### Install writes the fleet, the stream, the grant, and stops

`afd_fleet_lifecycle/src/install.rs:192-281` writes the `core.fleets` row and then `ensure_stream` creates the per-fleet stream and its consumer group (`:260-281`). Installing a bundle that declares a credential writes the approved `core.integration_grants` row at install, with no card (`docs/architecture/scenarios/github-pr-reviewer.md:84`). `docs/architecture/data_flow.md:1135-1137` is explicit: "no admission, event, lease or affinity row yet". The fleet's display name ("AGENT BOB 01") is `core.fleets.name`, read at lease time into `Installed.name` (`afd_fleet/src/lease/installed.rs:45,99`); nothing in routing reads it (next paragraph).

### Ingress: how a PR on `agentsfleet/linkwarden` finds the fleet

`afd_api_ingress/src/handler/webhook/app_route.rs:135-180`: only `github` is served (`:141`), the body cap is checked before anything is hashed (`:151`), the `X-GitHub-Event` header is required (`:153`), then the body is verified against the platform App's `webhook_secret` under the vault key `github-app` (`:158-165`, constant at `:70`). A `ping` is answered only after verification (`:169-177`).

`route` (`:187-256`) parses the body once with octocrab (`:194`), reads `installation.id` and `repository.full_name` (`:200-208`), resolves the installation to a workspace with `SELECT_INSTALL_WORKSPACE` (`afd_ingress/src/sql.rs:49-52`, `afd_ingress/src/app.rs:95-114`), classifies under `Policy::AppIngress` (`:222-228`), then asks `subscribers` (`:230-234`).

`subscribers` (`afd_ingress/src/app.rs:130-174`) runs `SELECT_APP_SUBSCRIBERS` (`afd_ingress/src/sql.rs:80-88`): every `core.fleets` row in that workspace with `status = active` joined to `core.integration_grants` where `service = 'github'` and `status = approved`. The document half is in Rust: `Binding::read_for_source` picks the webhook trigger whose `source` is `github` (`afd_ingress/src/binding.rs:92-100,271-282`), then `serves_repository(repository)` and `admits(event)` (`app.rs:165-168`). `serves_repository` is fail-closed: a trigger with no `repositories` list subscribes to nothing, and the match is case-insensitive (`binding.rs:258-264`). `admits` is the opposite: no `events` list means every event (`:230-235`). More than `MAX_FANOUT = 100` matches refuses the delivery (`app.rs:59,203-209`; `app_route.rs:247-252`).

So "the fleet slug AGENT BOB 01 receives the request" is really: the installation that linkwarden lives under maps to Indy's workspace, and every active fleet there holding an approved `github` grant whose `TRIGGER.md` says `source: github`, `events: [pull_request]`, `repositories: [agentsfleet/linkwarden]` gets one admission. The fixture bundle says exactly that (`tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md:4-15`). Two fleets so configured both run (`docs/architecture/connectors.md:200-204`).

The digest the fleet reasons over is twelve fields (`afd_api_ingress/src/handler/webhook/github.rs:235-252`: action, repo, number, title, url, state, draft, author, head_ref, base_ref, head_sha, received_at). Under `Policy::AppIngress` every `pull_request` action wakes the fleet except one on a repair branch (`:87`, `:212-213`); the manual per-fleet route narrows to opened/reopened/synchronize/ready_for_review (`:259-267`).

### Admission: the row is the acceptance, the stream entry is a receipt

`fan_out` (`app_route.rs:281-318`) calls `deliver` once per fleet, sequentially. `afd_ingress/src/deliver.rs:98-137` keys the admission `"{fleet}:{replay_id}"` (`:106`) where `replay_id = sha256(body)` (`afd_ingress/src/app.rs:198-200`; the unsigned `X-GitHub-Delivery` header is deliberately not used, `:177-196`), producer `webhook_app` (`:56-63`), actor `github-app` (`app_route.rs:93`), and `Reply::None` (`deliver.rs:117-118`), which matters in section 4.

`Admissions::admit` (`afd_admission/src/admit.rs:79-118`): fleet backlog budget first (`:161`), then `INSERT_ADMISSION` (`afd_admission/src/sql.rs:58-76`) into `core.fleet_admissions` (`schema/910_fleet_admissions.sql:65-84`) with `ON CONFLICT (producer, producer_key) DO UPDATE ... RETURNING (xmax = 0) AS inserted, created_at, seq, receipt`. The logical event id is `<created_at>-<seq>` (`afd_admission/src/lib.rs:163-165`). A fresh row then goes to `queue_entry` (`afd_admission/src/admit_receipt.rs:26-85`): `XADD fleet:{id}:events` with the five envelope fields plus `event_id` (`:36-45`), `RECORD_RECEIPT` writes the stream entry id back (`:55-61`, `sql.rs:101-104`), and `mark_ready` sets the fleet's readiness mark (`:83`, `:93-105`) in one of 16 partitions (`afd_dragonfly/src/ready/partition.rs:31`). A replay (same key) answers the first row with `replayed = true` and re-marks (`admit.rs:131-148`). No `core.fleet_events` row is written at ingress (`deliver.rs:12-15`).

GitHub gives this whole path ten seconds and never auto-redelivers (`docs/architecture/connectors.md:274-281`).

### The poll: ready index → claim → event → row

`POST /v1/runners/me/leases` (`afd_api_runner/src/handler/runner/lease.rs:69-89`, body not read `:9-23`) calls `Plane::lease` (`afd_fleet/src/lease/pull.rs:152-165`). A degraded runner gets no work (`:158-160`). Then `Leases::select` (`afd_fleet/src/lease/assign.rs:79-140`):

1. advance a process-wide cursor to the next of 16 partitions (`:113`) and `peek` at most `MAX_READY_CANDIDATES_PER_POLL = 64` fleets (`:114-118`, `:56`); an empty peek answers no-work with zero Postgres reads (`:126-128`);
2. `SELECT_READY_CANDIDATES` (`afd_fleet/src/lease/sql/lease.rs:145-161`): `status = active`, id in the peeked set, `leased_until IS NULL OR < now`, `required_tags <@ runner labels`, ordered `last_runner_id = this runner DESC, random()` (sticky is a hint, `:160`);
3. per candidate, `try_candidate` (`:174-219`): `claim` → `CLAIM_AFFINITY_SLOT` (`sql/lease.rs:38-50`), a conditional upsert on `fleet.runner_affinity` (`schema/630_runner_affinity.sql:50-61`) that wins only if `leased_until < now`, bumps `fencing_seq + 1`, sets `leased_until = now + LEASE_TTL_MS` (30 s, `afd_core/src/timing.rs:27`) and returns the new `fencing_seq`. That returned number is the `Fence` (`afd_fleet/src/lease/affinity.rs:49,107-135`), the only source of a fencing token in the system;
4. `take_claimed` (`assign.rs:222-250`): if the fleet still has an `active` lease row (a holder that stopped renewing), `RECLAIM_PRIOR_ACTIVE` (`sql/lease.rs:93-120`) flips it to `expired` and re-reads the body from `core.fleet_events` in one statement (`afd_fleet/src/lease/reclaim.rs:80-117`); otherwise `acquire_fresh` (`:258-289`) takes the group's oldest pending entry then a new one (`docs/architecture/data_flow.md:809-812`), and an empty fleet releases the claim and clears its mark (`:273-276`). `from_fresh` refuses an entry missing any of the six fields (`afd_fleet/src/lease/envelope.rs:147-182`).

`run_claimed` (`afd_fleet/src/lease/pull/held.rs:33-69`) then runs `admit_claimed` (`pull.rs:176-206`): read the installed fleet (`afd_fleet/src/lease/installed.rs:78-111`: config, `instructions = afd_fleet_runtime::instructions(source_markdown)` at `:107`, the SKILL.md body after the frontmatter per `afd_fleet_runtime/src/instructions.rs:34-36`, the bundle hash and the session row); `record_received` inserts `core.fleet_events` with `status = received` (`afd_fleet/src/lease/event.rs:109-163`; table `schema/800_fleet_events.sql:30-57`) and stamps `core.fleet_admissions.delivered_at` on the same connection (`:142`, `:221-239`); a first delivery publishes the `event_received` bracket (`pull.rs:199-203`); then `billed` (`:210-247`) parses the event type, reads the payer once, resolves the provider, runs the money gates and the approval gate (`:291-341`). Every refusal frees the claim (`held.rs:53-68`) and writes a `gate_blocked` terminal row (`pull/refuse.rs:87-123`).

`deliver` (`afd_fleet/src/lease/deliver.rs:34-89`): open the vault for the declared credentials (`:40-47`), derive the repair branch for a write binding (`:48`, `:215-226` → `agentsfleet-repair/<base64url(event_id)>`, `afd_gate/src/policy/repair.rs:45,66-68`), read approved grants (`:49-52`), and `build::assemble` (`afd_gate/src/policy/build.rs:81-140`): `tools` straight from the config (`:96-101`), `secrets_map`, `mintable` (`:109-117`), provider and api key (`:118-121`), `repository_binding`, and `http_origin_policies` from `egress::build` (`:127-130`). A mintable credential with no grant parks the lease (`deliver.rs:64-71,104-133`).

`issue_ready` (`deliver.rs:135-186`) → `Leases::issue` (`afd_fleet/src/lease/issue.rs:76-144`) writes the `fleet.runner_leases` row (`schema/610_runner_leases.sql:50-79`): `status = active`, `fencing_token`, `lease_expires_at = leased_until`, the stream `receipt`, and resets the meter cursor on a fresh lease (`:105`). Zero rows affected means the claim lapsed under it and no lease is answered (`:113-115`). `render` (`afd_fleet/src/lease/answer.rs:67-101`) builds the `LeasePayload` (`afd_wire/src/lease.rs:43-67`): `lease_id`, `fencing_token`, `lease_expires_at`, `event` (the digest as `request_json`), `policy`, `instructions`, optional `bundle`.

**Who creates what, in order:** `core.fleets` + stream group (install) → `core.fleet_admissions` (ingress) → stream entry + receipt + ready mark (ingress) → `fleet.runner_affinity` claim, fence minted (poll) → `core.fleet_events` received + `admissions.delivered_at` (poll) → `fleet.runner_leases` active (poll). Reclaim re-uses the admission and event rows and only writes a new lease row under a higher fence.

---

## 2. The runner's side of one lease

```text
 agentsfleet-runner run                        agentsfleetd
 ───────────────────────                       ───────────
 boot: AGENTSFLEET_API_URL, _RUNNER_TOKEN,
       RUNNER_STORAGE_HOME (/var/lib/agentsfleet-runner)
 engine boot sweep: kill/umount/rm leftover <home>/sandboxes/*
 tokio::join!( heartbeat , spool drainer , worker pool )

 HEARTBEAT every 10 s ──► POST /v1/runners/me/heartbeats {capability_report}
                      ◄── {status: ok, assigned_policy{worker_count, tier, net}, interval}
                           worker_count clamped 1..=64; workers spawn to that count

 WORKER n ────────────► POST /v1/runners/me/leases            (sleep retry_after_ms ≥250 ms if null)
                      ◄── { lease }
 ┌─ lease task ────────────────────────────────────────────────────────────────────────┐
 │ 1 admit(policy)        catalog.select: unhosted tool ⇒ refuse at startup            │
 │                        needs_sandbox = any tool with Runtime::Sandbox               │
 │ 2 turns.claim(fleet)   one run per fleet per runner                                 │
 │ 3 bundle fetch ──────► GET /v1/runners/me/bundles/{content_hash}  (404 = skill-only)│
 │ 4 hydrate ───────────► GET /v1/runners/me/memory/{fleet_id}                         │
 │ 5 [sandbox]            engine.prepare: dir, workspace.img, cgroup, bwrap, executor  │
 │   [the sandbox-tools milestone]               check_out: mint github token, gix fetch mirror, copy clone   │
 │                        materialize bundle support files via executor write_file     │
 │ 6 agent loop           turns ↔ provider; each call: start frame → router → end frame│
 │     tool calls ──────► POST /v1/runners/me/credentials/mint  (first ${secrets.x})   │
 │     mid-run memory ──► POST /v1/runners/me/memory/{fleet_id} (every N calls)        │
 │     activity pump ───► POST /v1/runners/me/leases/{id}/activity (≤64 KiB, 250 ms)   │
 │   ║ renewal tick 5 s ─► POST /v1/runners/me/leases/{id}/renew {cumulative tokens}   │
 │   ║                  ◄── {lease_expires_at}   4xx ⇒ interrupt run (5 s grace)       │
 │ 7 [sandbox] destroy    cgroup.kill → kill bwrap → rmdir cgroup → umount → rm img    │
 │ 8 settle               POST .../tool-calls (full records, ≤256 KiB per post)        │
 │                        POST /v1/runners/me/memory/{fleet_id} {lease_id, fence, deltas}
 │                        spool <home>/spool/<lease>.json (fsync + rename)             │
 │                        POST /v1/runners/me/reports  {lease_id, fence, outcome, ...} │
 │                        kept on 5xx/401/403/408/413 → drainer retries forever        │
 │ 9 wait ≤5 s for activity to drain                                                   │
 └─────────────────────────────────────────────────────────────────────────────────────┘
                                               daemon report: ONE transaction
                                               CLAIM_AND_SETTLE (fence ≥ seq, active→reported, charge)
                                               → fleet_events processed/fleet_error + tool_calls
                                               → fleet_sessions checkpoint → affinity released
                                               → obligation if a reply destination; COMMIT
                                               → XACK receipt → PUBLISH event_complete
```

**Boot and composition.** `afr_supervisor::boot` reads three variables (`afr_supervisor/src/config.rs:17-24`; storage home defaults to `/var/lib/agentsfleet-runner`, `:24`) and opens `<home>/{sandboxes,spool,bundles}` (`afr_supervisor/src/storage_home.rs:3-7,34-40`). `run` builds the control-plane client with `Limits::default()` for every lease (`afr_supervisor/src/lib.rs:114-132`, `:128`) and `serve` joins the heartbeat, the drainer and the worker pool (`:135-171`, `:166`). Building a `BubblewrapEngine` first sweeps every leftover lease directory, killing its cgroup and unmounting its disk (`afr_sandbox/src/bubblewrap_engine.rs:103-139`, `:137`; `afr_sandbox/src/bubblewrap_engine/sweep.rs:25-61`).

**Heartbeat → assignment.** `Heartbeat::beat` posts the capability report and reads back `assigned_policy` (`afr_supervisor/src/heartbeat.rs:78-99`); `worker_count` is clamped through `WorkerCount::clamping` (`:94`; `afd_core/src/limits.rs:58-66`, `MIN 1`, `MAX 64`, `DEFAULT 1` at `:14-20`). The daemon serves the cadence, `HEARTBEAT_INTERVAL_MS = 10 s` (`afd_core/src/timing.rs:62`; `afd_api_runner/src/handler/runner/heartbeat.rs:137`), and reconciles the host's report against the assignment into a `degraded` verdict (`afd_runner/src/heartbeat.rs:89-119`). The Rust daemon's heartbeat reply is unconditionally `status: ok` (`afd_api_runner/src/handler/runner/heartbeat.rs:3-10`, `:125`), so the "kill arrives on the next heartbeat" story in `docs/architecture/runner_fleet.md:583-587` is not what this daemon does; a running lease learns it is dead from a 4xx on its next renew (below).

**Worker pool.** `worker_pool::serve` spawns workers up to the assigned count and never kills one (`afr_supervisor/src/worker_pool.rs:39-67`). `Worker::next` waits until `takes_work` (status ok and `worker < workers`, `afr_supervisor/src/heartbeat.rs:47-49`), polls once (`:130`), sleeps `retry_after_ms` (daemon hint 1 s, `afd_core/src/timing.rs:68`) floored at 250 ms on no work (`:30`, `:143-144`), and runs a lease to its report before polling again (`:146-149`).

**One lease (`afr_supervisor/src/lease_loop.rs`).** The documented shape is at `:3-7`. `lease` (`:169-239`) opens the activity channel (`:172`), starts `Renewal` (`:173-179`), and selects over the work, the renewal and the halt token (`:189-204`); a renewal that ends first cancels the run and the ending is recorded as `RenewalTerminate` or `BudgetBreach` (`:193-197`, `:207-210`). Settle runs with the renewal still ticking (`:211-222`); then the live tail gets at most `ACTIVITY_DRAIN_WAIT = 5 s` (`:52`, `:223-237`).

`work` (`:245-291`): `agent.admit(policy)` (`:247`) is `Loop::admit` (`afr_agent/src/loop.rs:60-64`): the provider registry admits the model, `Catalog::select` refuses a lease naming a tool with no handler (`afr_tools/src/catalog.rs:199-215`), and `needs_sandbox` is true only if an offered tool has `Runtime::Sandbox` (`:237-241`). `turns.claim` serialises one run per fleet on this runner (`:251-259`; `afr_supervisor/src/turns.rs:1-8,63-69`). The bundle is fetched by content hash through `GET /v1/runners/me/bundles/{hash}`, verified against the digest and cached under `<home>/bundles/<hash>.tar`; a 404 means skill-only (`afr_supervisor/src/bundles.rs:59-92`; daemon `afd_api_runner/src/handler/runner/bundle.rs:72-93`). Memory hydrates through `GET /v1/runners/me/memory/{fleet_id}` with retries (`afr_supervisor/src/memory.rs:20-22`); the daemon answers only a runner holding a live lease on that fleet (`afd_fleet/src/lease/memory.rs:39-54`; `afd_fleet/src/lease/fence.rs:36-43`). Then `sandboxed` or `drive` (`:286-290`).

**Sandbox around the turn (`afr_supervisor/src/lease_loop/workspace.rs`).** `engine.prepare(SandboxRequest { lease_id, limits })` (`:33-40`); the sandbox-tools milestone inserts `check_out` here (diff: `workspace.rs:42-48`, `afr_supervisor/src/lease_loop/checkout.rs:33-58`), see section 6; `in_sandbox` writes the bundle's support files through the executor (`:62-79`, `:84-89`) and drives the turn; `sandbox.destroy()` runs unconditionally before settle (`:44-54`).

**The turn (`afr_supervisor/src/lease_loop/drive.rs:32-87`).** A `LeaseMint` (mints via `POST /v1/runners/me/credentials/mint`, `afr_supervisor/src/credentials.rs:24-43,62-79`) and a `LeaseCheckpoint` (mid-run memory push, `afr_supervisor/src/memory.rs:56-85`) go into `AgentRun`; the engine runs under `catch_unwind` (`:40-50`) and gets `ENGINE_STOP_GRACE = 5 s` after an interrupt to hand back its tokens and memory (`:24`, `:51-59`).

**Agent loop (`afr_agent/src/loop.rs`).** `Harness::new` (`:107-133`) builds the prompt (`afr_agent/src/prompt.rs:35-58`: system prompt = `## Installed instructions` + SKILL.md body, plus a trusted repair context on a write-bound lease `:78-113`; first user turn = `request_json.message` or the whole `request_json`), a `Lease` holding the hydrated memory and the egress guard (`:121-124`), the ledger and the budget. `drive` (`:135-173`) loops: one provider turn (`:201-246`), then each call through `Ledger::call` (`afr_agent/src/ledger.rs:67-75`), which emits `tool_call_started` (`:79-105`), runs `Router::dispatch` (`afr_agent/src/router.rs:43-63`: Supervisor in-process, Sandbox through the executor, Provider refused), and emits `tool_call_completed` (`:145-186`); a dropped call ends `interrupted` (`:189-195`). Memory is pushed every `memory_checkpoint_every` calls (`:155-157`; `afr_agent/src/context.rs:73-94`). The budget evicts old tool outputs and asks for a final answer at the context cap (`:165-170`; `context.rs:29-62`). `finish` returns `RunOutput { result, memory, trace, records }` (`afr_agent/src/loop/finish.rs:14-53`).

**Activity.** Frames go into an unbounded channel; the pump batches to `MAX_BATCH_BYTES = 64 KiB`, flushes every 250 ms, holds at most 4 batches, and posts `POST /v1/runners/me/leases/{id}/activity` best-effort (`afr_supervisor/src/activity.rs:34-38,104-159,179-191`). The daemon answers 202 and publishes on the fleet's SSE channel (`afd_api_runner/src/handler/runner/activity.rs:67-99`).

**Renewal.** Every `RENEWAL_TICK_MS = 5 s` (`afd_core/src/timing.rs:39`; `afr_supervisor/src/renew.rs:20,70-94`) the runner posts cumulative tokens to `POST /v1/runners/me/leases/{id}/renew` (`:98-105`) and moves its own deadline to the reply's `lease_expires_at` minus `EXPIRY_MARGIN = 2 s` (`:24`, `:107-112`). A 5xx keeps the lease (`:90`); a 4xx ends it, `BudgetBreach` on `RUN_BUDGET_EXCEEDED` (`:126-141`); a missed deadline ends it `RenewalTerminate` (`:143-154`). The daemon (`afd_fleet/src/lease/renew.rs:228-271`) refuses a non-active lease (`:238-240`), gates credits, and runs `RENEW_AND_METER` (`afd_fleet/src/lease/sql/renew.rs:69-163`): both `lease_expires_at` and `affinity.leased_until` move to `LEAST(now + 30 s, created_at + MAX_RUNTIME_MS)` under `fencing_token >= fencing_seq`, and the slice is charged to the wallet. `MAX_RUNTIME_MS = 12 h` (`afd_core/src/timing.rs:47`).

**Settle (`afr_supervisor/src/lease_loop/settle.rs:28-66`).** Order: full tool records to `POST /v1/runners/me/leases/{id}/tool-calls` in bodies of at most 256 KiB (`:71-100`; cap `afd_api_runner/src/handler/runner/tool_call.rs:34,76`); memory capture to `POST /v1/runners/me/memory/{fleet_id}` with `lease_id` and `fencing_token` (`:103-119`; `afr_supervisor/src/memory.rs:32-52`; fenced server-side `afd_fleet/src/lease/memory.rs:64-106`); build the `ReportRequest` (`afr_supervisor/src/report.rs:63-115`: outcome, failure class, tokens, time to first token, wall ms, checkpoint, tool trace); write it to `<home>/spool/<lease>.json` with fsync and rename (`afr_supervisor/src/report_spool.rs:85-112`); post `POST /v1/runners/me/reports` (`:135-173`). A stale fence, lease not found or lease lost settles the entry for good (`:36`); 5xx, 401, 403, 408, 413 keep it for the drainer (`:40-45`, `afr_supervisor/src/drainer.rs:34-78`).

**Daemon report.** `Plane::report` (`afd_fleet/src/lease/report.rs:117-140`) → `commit_report` (`afd_fleet/src/lease/commit.rs:136-224`): one transaction with `CLAIM_AND_SETTLE` (`afd_fleet/src/lease/sql/report.rs:119-158`: `FOR UPDATE OF l, a`, `fencing_token >= fencing_seq`, `active → reported`, charge the final slice), `mark_terminal` (`afd_fleet/src/lease/finalize.rs:91-134`, writes `processed` or `fleet_error`, `response_text` and `tool_calls`), `drop_other_fences`, `checkpoint` into `core.fleet_sessions` (`:146-173`), `release_through` on the affinity row (`afd_fleet/src/lease/affinity.rs:202-217`), and an obligation only when the admission recorded a reply destination (`commit.rs:207-216`). After commit: `XACK` the receipt (`finalize.rs:184-198`) and the `event_complete` frame. A repeated report answers success and charges nothing (`afd_api_runner/src/handler/runner/report.rs:119`).

**Sandbox destroy.** `Parts::teardown` (`afr_sandbox/src/bubblewrap_engine/parts.rs:161-175`) → `release` (`:194-224`): `cgroup.kill` (writes `1` to `cgroup.kill`, `afr_sandbox/src/cgroup.rs:130-132`), kill the bwrap child if still up, `cgroup.remove` polling `EBUSY` every 5 ms for up to 5 s (`:43-45,140-155`), unmount the workspace disk and delete its image (`afr_sandbox/src/workspace_disk.rs:121-123`), remove the lease directory. A disk that will not unmount keeps its directory for the boot sweep (`parts.rs:189-193`).

---

## 3. `github-pr-reviewer` end to end

```text
 GitHub                  agentsfleetd                       runner (library path; Zig today)
 ──────                  ────────────                       ─────────────────────────────
 PR opened on
 agentsfleet/linkwarden
   │ POST /v1/ingress/github
   ├────────────────────► verify HMAC (github-app.webhook_secret)
   │                      installation → workspace W
   │                      digest{action:"opened", repo:"agentsfleet/linkwarden", number, ...}
   │                      fleets in W: active ∩ github grant approved
   │                        ∩ TRIGGER repositories ∋ agentsfleet/linkwarden
   │                        ∩ events ∋ pull_request          ⇒ AGENT BOB 01 (and any twin)
   │                      admission + XADD + ready mark
   │◄──── 202 ────────────┤
   │                      │◄──── POST /v1/runners/me/leases ──────────────────────────────┤
   │                      claim slot, fleet_events received, gates, vault, policy:
   │                        tools: [http_request, memory_store, memory_recall]
   │                        network.allow: [api.github.com]
   │                        mintable: github; binding: linkwarden, write, base main
   │                        origin api.github.com rules: GET/HEAD /repos/agentsfleet/linkwarden/*
   │                           POST /git/blobs|trees|commits, POST /git/refs{ref locked},
   │                           POST /pulls{head,base locked, draft:true}
   │                      ├──── 200 lease{instructions=SKILL.md, event=digest} ───────────►
   │                      │                                  admit: all three tools Supervisor
   │                      │                                  ⇒ needs_sandbox = false ⇒ NO SANDBOX
   │                      │◄──── GET memory/{fleet} ─────────────────────────────────────┤
   │                      │                                  prompt: SKILL.md + repair context;
   │                      │                                  user turn = the 12-field digest JSON
   │                      │                                  model turn 1 → http_request GET .../pulls/{n}
   │                      │◄──── POST credentials/mint {lease_id, "github"} ─────────────┤
   │                      ├──── installation token, scoped to linkwarden ──────────────►
   │◄── GET /repos/agentsfleet/linkwarden/pulls/{n}  Accept: vnd.github.diff ───────────┤  ADMITTED
   ├── 200 diff ──────────────────────────────────────────────────────────────────────►
   │                      │◄──── activity: tool_call_started/completed(succeeded) ───────┤
   │                      │                                  model turn 2 → http_request POST .../pulls/{n}/reviews
   │                      │                                  egress: origin has rules, none match
   │      ✗ never sent    │                                  ⇒ [request_policy_not_allowed] to the model
   │                      │◄──── activity: tool_call_completed(failed) ──────────────────┤
   │                      │                                  model turn 3 → final answer text
   │                      │◄──── tool-calls, memory push, POST reports ──────────────────┤
   │                      settle: fleet_events processed, response_text = the answer
   │                      XACK, event_complete frame → dashboard thread
```

**What the lease carries for this bundle.** `TRIGGER.md` names three tools, one credential, one network host, a write binding on `agentsfleet/linkwarden` with base `main`, and a 2 dollar daily budget (`tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md:16-36`). All three tools are `Runtime::Supervisor` (`afr_tools/src/catalog.rs:50,58,60`), so `needs_sandbox` is false and the lease starts no sandbox at all (`afr_agent/src/loop.rs:62`; `afr_supervisor/src/lease_loop.rs:286-290`; `docs/architecture/runner_execution.md:162`). On the sandbox-tools milestone the same holds, and no repository is cloned either, because the clone fires only for `shell`, `exec_command` or `git` (`afr_tools/src/sandbox/repositories.rs:15-17,42-53` on the the sandbox-tools milestone branch).

**The prompt.** System prompt = `## Installed instructions` + the SKILL.md body (`afr_agent/src/prompt.rs:46-47`), then, because the binding is write, a `## Trusted repair context` block naming the repository, the locked repair branch and the base (`:78-113`). The first user turn is the whole digest JSON, because the `PullRequestDigest` has no `message` field (`:37-45`; fields at `afd_api_ingress/src/handler/webhook/github.rs:235-252`). SKILL.md step 1 reads `repo` and `number` from exactly that digest (`SKILL.md:14-33`).

**Step 2, the diff read.** `http_request` drafts `GET https://api.github.com/repos/agentsfleet/linkwarden/pulls/{n}` with `Authorization: Bearer ${secrets.github.token}` (`afr_tools/src/http_request.rs:63-77`). `Admission::admit` (`afr_egress/src/admission.rs:122-140`): method listed (`:255-260`), HTTPS and no placeholder in the URL (`:143-174`), placeholder only in `Authorization` (`:263-289`), host in `network_policy.allow` (`:176-183`), credential bound to this host because the `api.github.com` origin names `github` in `credential_names` (`:187-210`; `afd_gate/src/policy/egress/mod.rs:85-89`), and a read rule matches: `GET` with prefix `/repos/agentsfleet/linkwarden/` (`afd_gate/src/policy/egress/read.rs:22-30`; `admission.rs:220-231`). The vault then mints: `LeaseMint` → `POST /v1/runners/me/credentials/mint` → `Plane::mint` resolves the lease scope, checks the approved grant, opens the workspace's `github` handle and exchanges an installation token narrowed to the binding (`afd_fleet/src/lease/mint.rs:65-103,115-137`; `docs/architecture/connectors.md:285-287`). The token is kept per lease until expiry and masked out of responses (`afr_egress/src/egress.rs:47-95`; `afr_tools/src/egress.rs:48-75`).

**Step 4, the review post.** `POST .../pulls/{n}/reviews`: the origin `api.github.com` has rules, and none matches. The write set is exactly `POST /repos/{repo}/git/blobs`, `/git/trees`, `/git/commits` (open), `/git/refs` with `ref` locked to the repair branch, and `/pulls` (exact path) with `head`, `base` and `draft: true` locked (`afd_gate/src/policy/egress/write.rs:32,57-74`, paths are `HttpPathMatch::Exact` at `:83-94`). `/pulls/{n}/reviews` is neither the exact `/pulls` nor a prefix rule, so `origin_admits` answers `request_policy_not_allowed` (`afr_egress/src/admission.rs:225-229`) → `ToolErrorCode::RequestPolicyNotAllowed` (`afr_tools/src/egress.rs:38`) → the model reads `[request_policy_not_allowed] ...` and the run continues (`afr_tools/src/runtime.rs:141-147`). The integration test asserts zero POSTs reach the fake GitHub and the last tool result starts with that code (`agentsfleetd/tests/integration_rust_runner_reviews.rs:87-104`). The scenario page records the same (`docs/architecture/scenarios/github-pr-reviewer.md:95,101,113-114`).

**What the operator sees.** The live tail: `event_received`, `tool_call_started` / `tool_call_completed` (the second with `status: failed` and the output head carrying the code), streamed text chunks, `event_complete` (`docs/architecture/runner_fleet.md:552-568`). Durably: `core.fleet_events` row `processed` with `response_text` = the model's final answer and `tool_calls` = the trace (`afd_fleet/src/lease/finalize.rs:106-121`), and `core.fleet_tool_call_details` for "show all" (`afd_api_runner/src/handler/runner/tool_call.rs:43-52`). On GitHub: nothing. Memory: whatever `memory_store` wrote, pushed before the report.

---

## 4. How AGENT BOB 01 can reply

**SKILL.md is prose and cannot widen anything.** The lease marks `instructions` as "soft reasoning input, hard tool and secret policy stays in `policy`" (`afd_wire/src/lease.rs:59-61`; `afd_fleet_runtime/src/instructions.rs:18-20`). The egress rules are compiled by the daemon from the binding alone (`afd_gate/src/policy/build.rs:127-130`; `afd_gate/src/policy/egress/mod.rs:71-90`) and evaluated by the runner as written (`afr_egress/src/admission.rs:220-231`). So writing "post the review" in SKILL.md (which the fixture already does, `SKILL.md:39-42`) changes the model's intent, not the admission.

**A chat steer is the same.** `POST /v1/workspaces/{ws}/fleets/{id}/messages` is admitted as its own `chat` event through the same ledger (`afd_events/src/steer.rs:6-13`; `docs/architecture/data_flow.md:649-656`), waits behind the running lease (`data_flow.md:1203`), and runs with the same policy. SKILL.md tells the fleet to treat a steer with no PR as chat and to save operator facts with `memory_store` (`SKILL.md:48-61`). A steer cannot add an egress rule.

**What the code admits today for this fleet (write binding on one repository):** reads under `/repos/agentsfleet/linkwarden/` (`read.rs:22-30`), and the five writes above. In principle the fleet could create blobs, trees and commits, create one ref `refs/heads/agentsfleet-repair/<event>`, and open one draft PR from it against `main`. What is refused: a review (`/pulls/{n}/reviews`), an issue comment, a PR comment, approve, request changes, any other ref, a non-draft PR. `docs/architecture/data_flow.md:1201`: "A fleet can push a fix to its repair branch and open a draft PR; it cannot comment."

**The deferral, verbatim** (the agent-loop workstream's spec, under Discovery, its Deferrals bullet):

> **Deferrals** — Dimension 6.3's review post: no `afd_gate` rule admits `POST …/pulls/{number}/reviews`, so the post is refused today. > Indy (2026-10-04 11:49): "6.3 The PR review gets posted from the runner? SKILL.md? I want to experience the test and decide, so make me record that and add this as parked." — context: the runner posts it, `github-pr-reviewer/SKILL.md` step 4 sending `POST …/pulls/{number}/reviews` through `http_request`; a write binding admits only `/git/blobs`, `/git/trees`, `/git/commits`, the locked ref and the locked draft (`rustd/crates/afd_gate/src/policy/egress/write.rs:32-72`), so the runner refuses the post before it leaves and the test asserts that; Dimension 6.3 stays open until Indy runs it, and this supersedes the Oct 03 "move it to done" for 6.3. > Indy (2026-10-03 15:32): "you just tell me crap, increase the scope, so the refusal of review must be ignored for now. If that blocks the spec to move to done, then record Indys wording and move it to done."

**The other reply path, also not for GitHub.** The report owes a `core.fleet_obligations` delivery only when the admission recorded a reply destination (`afd_fleet/src/lease/commit.rs:207-216`; `schema/918_fleet_admissions_reply_destination.sql`). The App webhook admits with `Reply::None` (`afd_ingress/src/deliver.rs:117-118`), and only Slack posters exist (`data_flow.md:937-939`). So there is no daemon-side "answer back to the PR" either.

**the sandbox-tools milestone does not change this.** The diff stat touches no `afd_gate` file; the sandbox-tools workstream §2 says a push leaves through `propose_change` (the outage toolkit spec, not written), and the sandbox `git` tool refuses `push`, `fetch`, `pull`, `remote`, `clone` (`afr_tools/src/sandbox/git.rs:21,35-36` on the branch). Unblocking the review is a one-rule change in `afd_gate/src/policy/egress/write.rs` (an exact `POST /repos/{repo}/pulls/{n}/reviews` with `event` locked to `COMMENT`), which Indy parked until he runs the test.

---

## 5. The PR is fixed and pushed again

```text
 push ──► pull_request "synchronize" ──► new signed body ──► new sha256 ──► new admission row
                                                                           new XADD, same stream
 fleet:{id}:events   [opened: running]  [synchronize: waiting]  [labeled]  [edited] ...
                       one active lease per fleet (affinity slot) ⇒ strictly one at a time
 next poll after the report frees the slot:
   claim fence+1 (sticky: last_runner_id sorts first, random tie-break)
   new lease row, new lease_id, new fencing_token
   hydrate: fleet memory window (≤256 KiB, core pinned) ⇒ what memory_store saved last run
   NOT carried: the previous conversation, the previous answer, any sandbox, any clone
```

**Same fleet, new event, new lease.** The dedupe key is the body digest (`afd_ingress/src/app.rs:198-200`), and a `synchronize` body differs, so it is a new admission and a new stream entry (`docs/architecture/data_flow.md:1207-1217`). Under `Policy::AppIngress` every action wakes the fleet (`github.rs:87`), so `labeled`, `edited` and friends each queue a run too; nothing coalesces or supersedes per PR (`data_flow.md:1217`). The affinity slot allows one active lease per fleet (`afd_fleet/src/lease/sql/lease.rs:38-50`; `data_flow.md:1118-1126`), and on the runner `FleetTurns` allows one run per fleet per host (`afr_supervisor/src/turns.rs:1-8`), so the second event waits until the first reports and `release_through` frees the slot inside the settle transaction (`afd_fleet/src/lease/commit.rs:195-196`).

**Same runner?** Preferred, not promised: `SELECT_READY_CANDIDATES` sorts `last_runner_id = this runner` first (`sql/lease.rs:160`), and the claim records the hint (`:45`). Any eligible runner can win.

**What carries over.** Fleet memory: the hydrate window is `HYDRATE_WINDOW_BYTES = 256 KiB` (`afd_wire/src/memory.rs:43`) with every `core` entry pinned first (`docs/architecture/runner_fleet.md:511`), plus up to `RECALL_LIMIT_MAX = 50` per recall past the window (`memory.rs:52`; `afr_memory/src/hydrated.rs:1-7`). Memory belongs to the fleet, not the PR, so the second review knows the first only if SKILL.md has it read the existing reviews or recall what it stored (`data_flow.md:1217`).

**What does not carry over (main).** The session checkpoint is written to `core.fleet_sessions` and loaded into `Installed.context_json` (`afd_fleet/src/lease/installed.rs:50-51,100,108`), but `render` never puts it on the `LeasePayload` (`afd_fleet/src/lease/answer.rs:74-100`; `docs/architecture/runner_execution.md:216` "nothing hands it to a lease"). The prompt holds the instructions and the current message only (`afr_agent/src/prompt.rs:35-58`). No sandbox (this fleet has none), no clone, no workspace restore (`runner_execution.md:201`).

**the sandbox-tools milestone changes, all still `IN_PROGRESS` and not in the branch's code yet.** the chat-history workstream adds the last eight finished turns to a `chat` lease only; webhook leases stay self-contained (its spec, line 100,113). the sandbox-hold workstream holds a processed lease's sandbox frozen for ten minutes and lets the holder claim that fleet's next event first (the sandbox-hold workstream spec, line 36-38,100-132); it depends on the runner cutover and on the fleet having a sandbox, so a PR reviewer built only from supervisor tools never holds one.

---

## 6. Sandbox lifecycle

```text
 engine.prepare(lease_id, Limits::default())           afr_sandbox/src/bubblewrap_engine.rs:141-213
   mkdir <home>/sandboxes/<lease_id>        0711                                   :143-147
   workspace.img  sparse set_len(4 GiB) 0600                 workspace_disk.rs:62-67
     mke2fs -q -F -t ext4 -m 0 -O ^has_journal -E root_owner=uid:gid   host.rs:125-145
     mount -t ext4 -o loop,nosuid,nodev  img  <dir>/workspace          workspace_disk.rs:22,70-72
   cgroup <delegated>/<lease_id>                             cgroup.rs:62-105
     memory.max 2 GiB · cpu.max "200000 100000" · pids.max 512 · memory.swap.max 0
     io.max <loop major:minor> rbps=wbps=200 MiB/s           cgroup.rs:48,112-118
   run/  0700, chowned to sandbox user                       bubblewrap_engine.rs:195-203
   bwrap --unshare-{user,pid,ipc,uts,net,cgroup}             bubblewrap.rs:60-67
         --disable-userns --cap-drop ALL --clearenv --die-with-parent --new-session   :69-76
         --ro-bind <toolbox mount> /                         :127   ← the shared EROFS root
         --proc /proc --dev /dev --perms 1777 --tmpfs /dev/shm --tmpfs /tmp --tmpfs /run  :128-139
         --bind <dir>/workspace /workspace --bind <dir>/run /run/agentsfleet   :140-149
         --ro-bind /usr/local/bin/agentsfleet-runner /opt/agentsfleet/agentsfleet-runner  :150-154
         --uid 1000 --gid 1000 --chdir /workspace -- /opt/agentsfleet/agentsfleet-runner sandbox
     pre_exec: write "0" to cgroup.procs ⇒ born inside the cgroup   parts.rs:92-135
   inside: bind executor.sock → no_new_privs → Landlock (write only /workspace,/tmp,/dev/pts,/dev/shm)
           → seccomp EPERM on 12 syscalls → serve executor    serve.rs:30-50, harden.rs:27-49, harden/linux.rs:25-38
   supervisor: Client::connect_within(socket, ready_timeout)  bubblewrap_engine.rs:207-213
   ... the turn runs; tool calls cross the Unix socket ...
 sandbox.destroy()                                            parts.rs:194-224
   cgroup.kill=1 → kill bwrap → rmdir cgroup (poll EBUSY 5 ms × ≤1000) → umount → rm img → rm dir
```

**One sandbox per lease, destroyed before the report.** `afr_supervisor/src/lease_loop/workspace.rs:26-56`; `docs/architecture/runner_execution.md:164-181` ("There is no idle timeout and no reuse"). Per message in a fleet means per lease means per event: yes, a fresh sandbox for every event that needs one, and none at all for a lease whose tools are all supervisor-side (`afr_agent/src/engine.rs:136-142`).

**Warm slots.** `WarmSlots::start(inner, slots, limits)` keeps `slots` sandboxes already built, each named `warm-<uuid7>` with its own cgroup and empty workspace disk (`afr_sandbox/src/warm_slots.rs:71-80,192-233`). A lease claims one through a channel (`:99-103`) only if its limits equal the pool's (`:110-114`); the keeper refills on every claim (`:146,165-187`); a slot serves one lease and is destroyed with it; a slot whose sandbox died while waiting is retired (`:173-186`). How many: whatever the caller passes; only the kernel lane constructs `WarmSlots` today, and `afr_supervisor::run` takes a bare `Box<dyn Engine>` (`afr_supervisor/src/lib.rs:117`), so nothing in the supervisor wires warm slots in (`runner_execution.md:181`).

**Cold start, measured.** Lease accept to executor ready, debug build, p50 of five in the kernel lane: cold 23.6 ms, warm 0.66 ms (its spec, line 316; `runner_execution.md:183`). `bwrap → true` is 1.89 to 3.35 ms (`docs/v2/reviews/m211-toolbox-spikes.md:178`). The first exec of each binary from the image costs about 6 ms more cold than a host bind, which is LZ4 decompression on first read; after that every sandbox shares the decompressed pages (`:181-185`). The waits that matter are elsewhere: the poll interval (up to 1 s), the hydrate round trip, and the first model call (`runner_execution.md:183`). The toolbox download is never on the lease path (`:136`). So there is no cold-start problem from the sandbox itself; the per-event cost is dominated by the network round trips and the model.

**Boot sweep.** A crashed runner's leftovers are removed when the engine is built (`afr_sandbox/src/bubblewrap_engine/sweep.rs:25-61`): kill the leftover cgroup, unmount the disk, remove the directory. S6 found that each kernel-lane worker building its own engine swept its siblings' leases (`spikes.md:156`), which is why a host builds one engine per state directory (`runner_execution.md:181`).

**What the sandbox-tools milestone adds (the branch's code, as of `eeacb87bc`).**
- Sandbox-side handlers in `afr_tools/src/sandbox/`: `shell`, `exec_command`, `write_stdin` (`sh -c`, `COMMAND_ENV` with a git identity, `afr_tools/src/sandbox.rs:48-78`; at most 64 sessions per lease, `sessions.rs:17`; yields 250 ms to 30 s, `exec_session.rs:27-34`), `git` (local subcommands only, `git.rs:21`), the seven `file_*` tools and `apply_patch`, `image`; the three browser tools answer `browser_unavailable` (`runtime.rs` diff; spike S1, `spikes.md:16-46`). `Catalog::hosted` now registers them (`catalog.rs` diff), so a policy naming `shell` is admitted and `needs_sandbox` is true.
- A host-side clone before the turn: `check_out` mints a `github` token, fetches a bare mirror under `<home>/mirrors/<workspace>/<owner>/<name>.git` over HTTPS with the token in an in-memory `http.extraHeader` (`afr_supervisor/src/workspace_clone/fetch.rs:49-74,95-97`), copies the objects into `<workspace>/<name>/.git`, checks out `repository_base`, sets a plain `origin`, and chowns everything to the sandbox user (`workspace_clone/checkout.rs:45-74,184-192`). The token never enters the sandbox (`afr_supervisor/src/lease_loop/checkout.rs:1-3`). The prompt gains a `## Workspace` block naming the checkout (`afr_agent/src/prompt.rs` diff). Every open session is killed at run end (`afr_agent/src/loop/finish.rs` diff, `close_sessions`).
- Not yet in the branch's code: §6 toolbox manifest proof, §7 code-running bundle, §8 admission by descriptor (`LOOP_CONFIGURE`), and all of the sandbox-limits workstream (`/tmp` on the workspace disk, a `sandbox`/`tenant` cgroup split with a 64 MiB reserve so the OOM killer picks a tenant process and never `bwrap`, direct I/O on the loop device, a 2 GiB host disk reserve, the sandbox-limits workstream spec, line 37-38,92-122) and the sandbox-hold workstream (freeze/thaw hold). The the sandbox-tools workstream Dimensions 6.1, 7.1 and 8.1 to 8.4 carry no `DONE` marker (the sandbox-tools workstream spec, line 144,150,156-159), and the diff stat shows no `afr_sandbox/src/toolbox/` directory.

---

## 7. Where the image lives, and what bounds a bare-metal host

```text
 HOST DISK (state dir)                                   HOST RAM
 ┌────────────────────────────────────────────┐           ┌──────────────────────────────────────┐
 │ toolbox-<sha256>.erofs   ~871 MB, lz4hc    │  loop,ro  │ kernel page cache                     │
 │   mounted once at <state>/<digest>  ──────────────────►│   decompressed toolbox pages, SHARED  │
 │                                            │           │   by every sandbox, evictable,        │
 │ sandboxes/<lease_A>/workspace.img  4 GiB   │  loop,rw  │   charged to NO cgroup                │
 │ sandboxes/<lease_B>/workspace.img  sparse  │           ├──────────────────────────────────────┤
 │ spool/<lease>.json   bundles/<hash>.tar    │           │ cgroup lease_A  memory.max 2 GiB      │
 │ [the sandbox-tools milestone] mirrors/<ws>/<owner>/<repo>.git     │           │   processes + /tmp + /run + /dev/shm  │
 └────────────────────────────────────────────┘           │   (tmpfs = RAM, charged here)         │
                                                          │ cgroup lease_B  memory.max 2 GiB      │
   bound RO into each sandbox as "/"                      │ ...  × worker_count (1..=64)          │
                                                          └──────────────────────────────────────┘
 ceiling: Σ memory.max = 2 GiB × workers ≤ 128 GiB (nothing checks it against the host)
          disk  = 4 GiB × live leases, sparse, no reserve on main (the sandbox-limits workstream adds 2 GiB)
```

**The image is a file, mounted once per host, read-only, via a loop device.** The build produces `toolbox-<sha256>.erofs` with `mkfs.erofs -zlz4hc,12 -b4096` from a pinned Debian trixie snapshot (`scripts/toolbox/manifest.txt`; `scripts/toolbox/build.sh:160-230`; the names at `afr_sandbox/src/toolbox.rs:25-27`). The spike image is 871,460,864 bytes and 243 packages (`docs/v2/reviews/m211-toolbox-spikes.md:14`). `Toolbox::mount` hashes the file, runs `mount -t erofs -o loop,ro,nosuid,nodev <image> <state>/<digest>` (`toolbox.rs:29,114-153`; `host.rs:148-159`), then hashes the loop device itself so what is mounted is what was verified (`:164-182`). Every sandbox gets that one mount as its root through `--ro-bind <root> /` (`afr_sandbox/src/bubblewrap.rs:127`); unmounting is lazy so running sandboxes keep theirs (`afr_sandbox/src/mounts.rs:30-40`; `toolbox.rs:225-234`).

**"Where does the cache live?" In the kernel page cache, in RAM, shared, reclaimable.** There is no per-sandbox copy and no runner-level cache. EROFS decompresses blocks on read into the page cache of the one mount; every sandbox's reads hit the same pages (`runner_execution.md:190`: "the kernel page cache in RAM, shared and evictable; none of its own; memory pressure evicts it"; `spikes.md:185`). Those pages are not charged to any lease cgroup and are not pinned: under memory pressure the kernel drops them and the next read decompresses again (about 6 ms per cold binary, `spikes.md:181-185`). Hashing the image at admission warms that cache (`runner_execution.md:156`). A Firecracker guest would not share this (`:152`).

**What each lease owns.** A sparse 4 GiB `workspace.img` on the state disk, formatted ext4 without a journal and loop-mounted read-write (`afr_sandbox/src/workspace_disk.rs:18-24,41-73`; `DEFAULT_DISK_BYTES` at `afr_sandbox/src/engine.rs:17`); its blocks only take disk as they are written. `/tmp`, `/run` and `/dev/shm` are tmpfs, so they are RAM charged to the lease's cgroup (`bubblewrap.rs:46-49,136-139`; `runner_execution.md:192`). Spike S6 showed that tmpfs at the kernel's default size (about 4 GB there) exceeds the 2 GiB memory limit, so filling `/tmp` ends in an OOM kill that takes `bwrap` and the whole sandbox, and one lease was killed writing its workspace before the disk filled, likely the loop device's second page cache (`spikes.md:154-169`). the sandbox-limits workstream §1 to §3 are the fix.

**The numbers the code declares.**
- `Limits::default()`: `memory_bytes = 2 GiB`, `cpu_millis = 2000` (two cores), `pids = 512`, `disk_bytes = 4 GiB` (`afr_sandbox/src/engine.rs:11-17,32-41`); `DEFAULT_IO_BYTES_PER_SECOND = 200 MiB/s` each way (`afr_sandbox/src/cgroup.rs:48`); swap off (`:22-23,99-103`). Every lease gets the defaults, never `disk_write_limit_mb` (`afr_supervisor/src/lib.rs:128`; `runner_execution.md:200`).
- Workers per host: `DEFAULT_WORKERS = 1`, `MIN_WORKERS = 1`, `MAX_WORKERS = 64` (`afd_core/src/limits.rs:14,17,20`), clamped on the wire (`:58-66,81-86`).
- Activity: 64 KiB batches, 4 held (`afr_supervisor/src/activity.rs:34,36`). Memory: 256 KiB hydrate window, 256 KiB push, 1000 entries per fleet, 16 KiB per entry (`afd_wire/src/memory.rs:16,31,37,43`).
- Timers: lease TTL 30 s, renew every 5 s, max runtime 12 h, runner offline after 90 s, heartbeat 10 s, empty-poll hint 1 s (`afd_core/src/timing.rs:27,39,47,55,62,68`).

**So what bounds concurrency on an OVHcloud bare-metal box.** RAM: `2 GiB × worker_count`, up to 128 GiB at the cap, plus whatever page cache you want the toolbox to keep warm (up to the image's decompressed size), plus the supervisor itself; nothing checks the sum against the host (`runner_execution.md:196`). Disk: `4 GiB × live leases` worst case, sparse, with no reserve on main (S6 FAIL, `spikes.md:169`; the sandbox-limits workstream §4 adds a 2 GiB reserve and refuses a lease the state disk cannot hold). CPU: 2 cores per lease by quota, so 64 workers would oversubscribe anything short of 128 cores. The image's own RAM footprint is not a bound: it is evictable.

**Two deployment facts to fix before the Rust runner lands on that host.** The storage home defaults to `/var/lib/agentsfleet-runner` (`afr_supervisor/src/config.rs:24`), but `deploy/baremetal/agentsfleet-runner.service` was written for the Zig runner: `ReadWritePaths` allows only `/run/agentsfleet` and `/tmp` (`:76`) and `Delegate=cpu memory pids` (`:71`) omits `io`, which the probe requires (`afr_sandbox/src/probe.rs:37`), so as written workspace images would land on tmpfs and every sandbox would be refused (`runner_execution.md:203`). OVHcloud is named nowhere in the deploy tree; the only mentions are its spec, line 379 ("production worker is ... via systemd on OVHCloud bare-metal") and the model-library pricing fixture (the model-library pricing workstream spec, line 62,115). `deploy/baremetal/deploy.sh:235-244` calls the hosts "live bare-metal boxes" provisioned as `zombie-runner`.

---

## 8. Gaps and "not built yet"

1. **The Rust runner takes no leases.** `agentsfleet_runner/src/main.rs:114-137` refuses at boot; the runner cutover workstream switches the engine on. Everything in sections 2, 6 and 7 is library behaviour until then; the Zig runner serves production (`data_flow.md:1130`).
2. **The review post is refused.** No write rule admits `/pulls/{n}/reviews` (`afd_gate/src/policy/egress/write.rs:32,57-74`); parked by Indy (the agent-loop workstream spec, line 320). No obligation path answers GitHub either (`afd_ingress/src/deliver.rs:117-118`).
3. **No workspace restore or save.** R2 snapshots are design only (`runner_execution.md:201,205-216`).
4. **The session checkpoint is written and loaded but never handed to a lease** (`afd_fleet/src/lease/answer.rs:74-100`; `runner_execution.md:216`).
5. **Chat history is not in the prompt** on main; the chat-history workstream is `IN_PROGRESS` and chat-only.
6. **No sandbox reuse**; the sandbox-hold workstream's hold is `IN_PROGRESS`, depends on the runner cutover, and never applies to a supervisor-only fleet.
7. **Exhaustion kills the sandbox.** tmpfs `/tmp` larger than the cgroup's memory.max file, OOM kills `bwrap`, no host disk reserve (`spikes.md:154-169`); the sandbox-limits workstream is `IN_PROGRESS`.
8. **Toolbox admission by descriptor (the sandbox-tools workstream §8) is not on the branch**; today the image is mounted by path and the loop device re-hashed after (`toolbox.rs:7-11`).
9. **Browser tools refused** until a Firecracker engine (`spikes.md:16-46`; the sandbox-tools workstream spec, line 134-138). No Firecracker engine exists; `/dev/kvm` is only probed (`afr_sandbox/src/probe.rs:18-19,42-52`).
10. **`propose_change` and the supervisor push are not written**; `git push` is refused in the sandbox (`afr_tools/src/sandbox/git.rs:21`).
11. **No per-lease network allowlist inside the sandbox**; the sandbox has loopback only and every outbound call leaves through the supervisor's `afr_egress` (`runner_execution.md:202`).
12. **Kill and pause never reach a running lease through the heartbeat**; the Rust daemon always answers `status: ok` (`afd_api_runner/src/handler/runner/heartbeat.rs:125`). What a runner observes on a killed fleet is the next renew's 4xx (`afr_supervisor/src/renew.rs:91,126-141`). unverified: what the kill route writes to the lease row; `afd_fleet/src/lease/renew.rs:238-240` would refuse any non-active status.
13. **Warm slots are not wired into the supervisor** (`afr_supervisor/src/lib.rs:117`; `runner_execution.md:181`).
14. **Limits are not per fleet** (`afr_supervisor/src/lib.rs:128`).
15. **The bare-metal systemd unit cannot host the Rust runner as written** (`deploy/baremetal/agentsfleet-runner.service:71,76`; `runner_execution.md:203`).
16. **GitHub never auto-redelivers and gives ten seconds**; a leg that fails before its admission row commits is lost outright (`connectors.md:274-281`).
17. **`deployment_status` and repair-branch deliveries are acknowledged and dropped** (`app_route.rs:23-32`).
18. **Runner spans and metrics are not exported** (`runner_execution.md:92`). Built since this trace was taken, in the runner-telemetry workstream (PR #730).
19. **Nothing checks `2 GiB × worker_count` or `4 GiB × leases` against the host** (`runner_execution.md:196`).

**Next action for Indy:** the two decisions that gate the morning's question are (a) whether to add the one `POST .../pulls/{n}/reviews` write rule with `event` locked to `COMMENT` (parked at the agent-loop workstream spec, line 320), and (b) the runner cutover workstream, which turns `main.rs:114-137` from a refusal into `afr_supervisor::run`, after which the bare-metal unit's `ReadWritePaths` and `Delegate=... io` need fixing before a single sandbox can build there.