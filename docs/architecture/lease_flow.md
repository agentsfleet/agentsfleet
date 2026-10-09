# How a lease works, from "start a fleet" to a sandbox being nuked

Read this when you want one lease's whole path on one page: a fleet installed, a pull request (PR) opened, the event leased, its turn run, its report settled, and its sandbox held or destroyed. It walks the path and links each step to the page that owns it. [`data_flow.md`](./data_flow.md) owns ingress, admission and settlement, [`runner_fleet.md`](./runner_fleet.md) the protocol, renewal and fencing, and [`runner_execution.md`](./runner_execution.md) the runner's side. Where a step's facts live on one of those pages, this page links rather than restates them.

Paths are under `rustd/crates/` unless they start with `docs/`, `schema/`, `deploy/`, `scripts/` or `tests/`. Expanded acronyms: Enhanced Read-Only File System (EROFS), Random Access Memory (RAM), Hash-based Message Authentication Code (HMAC), Server-Sent Events (SSE), Time To Live (TTL), Out Of Memory (OOM), Chrome DevTools Protocol (CDP), Kernel-based Virtual Machine (KVM).

The example is the `github-pr-reviewer` bundle installed as "AGENT BOB 01" on `agentsfleet/linkwarden`. Section 1 is the daemon's side, section 2 the runner's, section 3 the whole example end to end, and sections 6 and 7 the sandbox and the host it sits on.

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

`afd_fleet_lifecycle/src/install.rs` writes the `core.fleets` row and then `ensure_stream` creates the per-fleet stream and its consumer group. Installing a bundle that declares a credential writes the approved `core.integration_grants` row at install, with no card (`docs/architecture/scenarios/github-pr-reviewer.md`). Install writes no admission, event, lease or affinity row (`afd_fleet_lifecycle/src/install.rs`). The fleet's display name ("AGENT BOB 01") is `core.fleets.name`, read at lease time into `Installed.name` (`afd_fleet/src/lease/installed.rs`); nothing in routing reads it (next paragraph).

### Ingress: how a PR on `agentsfleet/linkwarden` finds the fleet

`afd_api_ingress/src/handler/webhook/app_route.rs`: only `github` is served, the body cap is checked before anything is hashed, the `X-GitHub-Event` header is required, then the body is verified against the platform App's `webhook_secret` under the vault key `github-app` (`APP_IDENTITY_GITHUB`, `app_route.rs`). A `ping` is answered only after verification.

`route` parses the body once with octocrab, reads `installation.id` and `repository.full_name`, resolves the installation to a workspace with `SELECT_INSTALL_WORKSPACE` (`afd_ingress/src/sql.rs`, `afd_ingress/src/app.rs`), classifies under `Policy::AppIngress`, then asks `subscribers`.

`subscribers` (`afd_ingress/src/app.rs`) runs `SELECT_APP_SUBSCRIBERS` (`afd_ingress/src/sql.rs`): every `core.fleets` row in that workspace with `status = active` joined to `core.integration_grants` where `service = 'github'` and `status = approved`. The document half is in Rust: `Binding::read_for_source` picks the webhook trigger whose `source` is `github` (`afd_ingress/src/binding.rs`), then `serves_repository(repository)` and `admits(event)` (`app.rs`). `serves_repository` is fail-closed: a trigger with no `repositories` list subscribes to nothing, and the match is case-insensitive (`binding.rs`). `admits` is the opposite: no `events` list means every event. More than `MAX_FANOUT = 100` matches refuses the delivery (`app.rs`; `app_route.rs`).

So "the fleet slug AGENT BOB 01 receives the request" is really: the installation that linkwarden lives under maps to Indy's workspace, and every active fleet there holding an approved `github` grant whose `TRIGGER.md` says `source: github`, `events: [pull_request]`, `repositories: [agentsfleet/linkwarden]` gets one admission. The fixture bundle says exactly that (`tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md`). Two fleets so configured both run (`docs/architecture/connectors.md`).

The digest the fleet reasons over is twelve fields (`afd_api_ingress/src/handler/webhook/github.rs`: action, repo, number, title, url, state, draft, author, head_ref, base_ref, head_sha, received_at). Under `Policy::AppIngress` every `pull_request` action wakes the fleet except one on a repair branch (`is_repair_branch`, `github.rs`); the manual per-fleet route narrows to opened/reopened/synchronize/ready_for_review.

### Admission: the row is the acceptance, the stream entry is a receipt

`fan_out` (`app_route.rs`) calls `deliver` once per fleet, sequentially. `afd_ingress/src/deliver.rs` keys the admission `"{fleet}:{replay_id}"` where `replay_id = sha256(body)` (`afd_ingress/src/app.rs`; the unsigned `X-GitHub-Delivery` header is deliberately not used), producer `webhook_app`, actor `github-app` (`app_route.rs`), and `Reply::None` (`deliver.rs`), which matters in section 4.

`Admissions::admit` (`afd_admission/src/admit.rs`): fleet backlog budget first, then `INSERT_ADMISSION` (`afd_admission/src/sql.rs`) into `core.fleet_admissions` (`schema/910_fleet_admissions.sql`) with `ON CONFLICT (producer, producer_key) DO UPDATE ... RETURNING (xmax = 0) AS inserted, created_at, seq, receipt`. The logical event id is `<created_at>-<seq>` (`afd_admission/src/lib.rs`). A fresh row then goes to `queue_entry` (`afd_admission/src/admit_receipt.rs`): `XADD fleet:{id}:events` with the five envelope fields plus `event_id`, `RECORD_RECEIPT` writes the stream entry id back (`sql.rs`), and `mark_ready` sets the fleet's readiness mark through `ReadyIndex::mark` in one of 16 partitions (`afd_dragonfly/src/ready/partition.rs`). A replay (same key) answers the first row with `replayed = true` and re-marks (`admit.rs`). No `core.fleet_events` row is written at ingress (`deliver.rs`).

GitHub gives this whole path ten seconds and never auto-redelivers (`docs/architecture/connectors.md`).

### The poll: ready index → claim → event → row

`POST /v1/runners/me/leases` (`afd_api_runner/src/handler/runner/lease.rs`, body not read) calls `Plane::lease` (`afd_fleet/src/lease/pull.rs`). A degraded runner gets no work. Then `Leases::select` (`afd_fleet/src/lease/assign.rs`):

1. advance a process-wide cursor to the next of 16 partitions and `peek` at most `MAX_READY_CANDIDATES_PER_POLL = 64` fleets; an empty peek answers no-work with zero Postgres reads;
2. `SELECT_READY_CANDIDATES` (`afd_fleet/src/lease/sql/lease.rs`): `status = active`, id in the peeked set, `leased_until IS NULL OR < now`, `required_tags <@ runner labels`, ordered `last_runner_id = this runner DESC, random()` (sticky is a hint);
3. per candidate, `try_candidate`: `claim` → `CLAIM_AFFINITY_SLOT` (`sql/lease.rs`), a conditional upsert on `fleet.runner_affinity` (`schema/630_runner_affinity.sql`) that wins only if `leased_until < now`, bumps `fencing_seq + 1`, sets `leased_until = now + LEASE_TTL_MS` (30 s, `afd_core/src/timing.rs`) and returns the new `fencing_seq`. That returned number is the `Fence` (`afd_fleet/src/lease/affinity.rs`), the only source of a fencing token in the system;
4. `take_claimed` (`assign.rs`): if the fleet still has an `active` lease row (a holder that stopped renewing), `RECLAIM_PRIOR_ACTIVE` (`sql/lease.rs`) flips it to `expired` and re-reads the body from `core.fleet_events` in one statement (`afd_fleet/src/lease/reclaim.rs`); otherwise `acquire_fresh` takes the group's oldest pending entry then a new one (`docs/architecture/data_flow.md`), and an empty fleet releases the claim and clears its mark. `from_fresh` refuses an entry missing any of the six fields (`afd_fleet/src/lease/envelope.rs`).

`run_claimed` (`afd_fleet/src/lease/pull/held.rs`) then runs `admit_claimed` (`pull.rs`): read the installed fleet (`afd_fleet/src/lease/installed.rs`: config, `instructions = afd_fleet_runtime::instructions(source_markdown)`, the SKILL.md body after the frontmatter per `afd_fleet_runtime/src/instructions.rs`, the bundle hash and the session row); `record_received` inserts `core.fleet_events` with `status = received` (`afd_fleet/src/lease/event.rs`; table `schema/800_fleet_events.sql`) and stamps `core.fleet_admissions.delivered_at` on the same connection (`afd_admission::sql::MARK_DELIVERED`, `event.rs`); a first delivery publishes the `event_received` bracket (`pull.rs`); then `billed` parses the event type, reads the payer once, resolves the provider, runs the money gates and the approval gate. Every refusal frees the claim (`held.rs`) and writes a `gate_blocked` terminal row (`pull/refuse.rs`).

`deliver` (`afd_fleet/src/lease/deliver.rs`): open the vault for the declared credentials, derive the repair branch for a write binding (→ `agentsfleet-repair/<base64url(event_id)>`, `afd_gate/src/policy/repair.rs`), read approved grants, and `build::assemble` (`afd_gate/src/policy/build.rs`): `tools` straight from the config, `secrets_map`, `mintable`, provider and api key, `repository_binding`, and `http_origin_policies` from `egress::build`. A mintable credential with no grant parks the lease (`deliver.rs`).

`issue_ready` (`deliver.rs`) → `Leases::issue` (`afd_fleet/src/lease/issue.rs`) writes the `fleet.runner_leases` row (`schema/610_runner_leases.sql`): `status = active`, `fencing_token`, `lease_expires_at = leased_until`, the stream `receipt`, and resets the meter cursor on a fresh lease. Zero rows affected means the claim lapsed under it and no lease is answered. `render` (`afd_fleet/src/lease/answer.rs`) builds the `LeasePayload` (`afd_wire/src/lease.rs`): `lease_id`, `fencing_token`, `lease_expires_at`, `event` (the digest as `request_json`), `policy`, `instructions`, optional `bundle`.

**Who creates what, in order:** `core.fleets` + stream group (install) → `core.fleet_admissions` (ingress) → stream entry + receipt + ready mark (ingress) → `fleet.runner_affinity` claim, fence minted (poll) → `core.fleet_events` received + `admissions.delivered_at` (poll) → `fleet.runner_leases` active (poll). Reclaim re-uses the admission and event rows and only writes a new lease row under a higher fence.

---

## 2. The runner's side of one lease

```text
 agentsfleet-runner run                        agentsfleetd
 ───────────────────────                       ───────────
 boot: AGENTSFLEET_API_URL, _RUNNER_TOKEN,
       RUNNER_STORAGE_HOME (/var/lib/agentsfleet-runner)
 delegated cgroup from /proc/self/cgroup; probe the kernel; admit the staged toolbox
 engine boot sweep: leftover <home>/sandboxes/*, afegress* tables, afv* links
 tokio::join!( heartbeat , spool drainer , worker pool )

 HEARTBEAT every 10 s ──► POST /v1/runners/me/heartbeats {capability_report, holds}
                      ◄── {status: ok, assigned_policy{worker_count, tier,
                           network_policy, registry_allowlist}, interval}
                           worker_count clamped 1..=64; workers spawn to that count

 WORKER n ────────────► POST /v1/runners/me/leases {holds}   (sleep retry_after_ms ≥250 ms if null)
                      ◄── { lease }
 ┌─ lease task ────────────────────────────────────────────────────────────────────────┐
 │ 1 admit(policy)        unhosted tool or provider ⇒ refuse at startup                │
 │                        needs_sandbox = any tool with Runtime::Sandbox               │
 │ 2 turns.claim(fleet)   one run per fleet per runner                                 │
 │ 3 bundle fetch ──────► GET /v1/runners/me/bundles/{content_hash}  (404 = skill-only)│
 │ 4 hydrate ───────────► GET /v1/runners/me/memory/{fleet_id}                         │
 │ 5 [sandbox]            held sandbox for this fleet and key, else engine.prepare:    │
 │                        dir, workspace.img, cgroup, bwrap on the assigned network,   │
 │                        executor; check_out the bound repositories; land bundle files│
 │ 6 agent loop           turns ↔ provider; each call: start frame → router → end frame│
 │     tool calls ──────► POST /v1/runners/me/credentials/mint  (first ${secrets.x})   │
 │     mid-run memory ──► POST /v1/runners/me/memory/{fleet_id} (every N calls)        │
 │     activity pump ───► POST /v1/runners/me/leases/{id}/activity (≤64 KiB, 250 ms)   │
 │   ║ renewal tick 5 s ─► POST /v1/runners/me/leases/{id}/renew {cumulative tokens}   │
 │   ║                  ◄── {lease_expires_at}   4xx ⇒ interrupt run (5 s grace)       │
 │ 7 [sandbox] keep       processed ⇒ freeze and hold; else destroy                    │
 │ 8 settle               POST .../tool-calls (full records, ≤256 KiB per post)        │
 │                        POST .../memory/{fleet_id} {lease_id, fence, deltas}         │
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

**Boot and composition.** `afr_supervisor::boot` reads three variables and opens `<home>/{sandboxes,spool,bundles}` (`afr_supervisor/src/config.rs`, `afr_supervisor/src/storage_home.rs`). The binary then builds the host's engine, refusing to start on a host that lacks a mechanism every sandbox needs, and runs `afr_supervisor::run` with it (`agentsfleet_runner/src/main.rs`). Building the engine sweeps what a crashed run left ([Runner execution](./runner_execution.md) §"A lease's sandbox today").

**Heartbeat → assignment.** The heartbeat posts the capability report and the fleets the runner holds, and reads back the assigned policy; the worker count is clamped to `1..=64` (`afr_supervisor/src/heartbeat.rs`, `afd_core/src/limits.rs`). The daemon serves the 10 s cadence and reconciles the report against the assignment into a `degraded` verdict ([Runner Fleet](./runner_fleet.md) §"Assigned policy and reconciliation (M148)"). The reply's `status` is always `ok` from this daemon, so a running lease learns nothing of a kill from the heartbeat ([Runner Fleet](./runner_fleet.md) §"Steer, kill, pause").

**Worker pool.** Workers grow to the assigned count and are never killed. A worker polls while the assignment says to take work, sleeps the daemon's `retry_after_ms` (floored at 250 ms) on no work, and runs a lease to its report before polling again (`afr_supervisor/src/worker_pool.rs`).

**One lease.** The steps and their refusals are in [Runner execution](./runner_execution.md) §"One lease, end to end". The renewal ticks every 5 s with the run's cumulative tokens, moves the runner's own deadline to the reply's `lease_expires_at` less 2 s, keeps the lease on a 5xx and ends it on a 4xx ([Runner Fleet](./runner_fleet.md) §"Per-lease renewal — how a long fleet keeps its lease"). An interrupted engine gets 5 s to hand back its tokens and memory (`afr_supervisor/src/lease_loop/drive.rs`).

**The turn.** The agent loop builds the prompt from the SKILL.md body and, on a write binding, a trusted repair context; a chat lease also carries the thread's recent turns (`afr_agent/src/prompt.rs`). Each call is numbered, announced, routed to the supervisor or the sandbox's executor, and closed exactly once (`afr_agent/src/ledger.rs`). At the context cap the loop stops offering tools and asks for the answer (`afr_agent/src/context.rs`).

**Settle and report.** Full tool records post first, then the fenced memory capture, then the report is spooled and posted; the daemon settles it in one transaction ([Runner Fleet](./runner_fleet.md) §"Running one event"). A stale fence, a lease not found or a lease lost settles the spooled entry for good; 5xx, 401, 403, 408 and 413 keep it for the drainer (`afr_supervisor/src/report_spool.rs`, `afr_supervisor/src/drainer.rs`).

---

## 3. `github-pr-reviewer` end to end

```text
 GitHub                  agentsfleetd                       agentsfleet-runner
 ──────                  ────────────                       ──────────────────
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

**What the lease carries for this bundle.** `TRIGGER.md` names three tools, one credential, one network host, a write binding on `agentsfleet/linkwarden` with base `main`, and a 2 dollar daily budget (`tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md`). All three tools are `Runtime::Supervisor` (`afr_tools/src/catalog.rs`), so `needs_sandbox` is false and the lease starts no sandbox at all ([Runner execution](./runner_execution.md) §"A lease's sandbox today"). No repository is cloned either: only a lease offered a sandbox tool gets its bound repositories checked out (`afr_supervisor/src/lease_loop/checkout.rs`).

**The prompt.** System prompt = `## Installed instructions` + the SKILL.md body (`afr_agent/src/prompt.rs`), then, because the binding is write, a `## Trusted repair context` block naming the repository, the locked repair branch and the base. The first user turn is the whole digest JSON, because the `PullRequestDigest` has no `message` field (fields at `afd_api_ingress/src/handler/webhook/github.rs`). SKILL.md step 1 reads `repo` and `number` from exactly that digest (`SKILL.md`).

**Step 2, the diff read.** `http_request` drafts `GET https://api.github.com/repos/agentsfleet/linkwarden/pulls/{n}` with `Authorization: Bearer ${secrets.github.token}` (`afr_tools/src/http_request.rs`). `Admission::admit` (`afr_egress/src/admission.rs`): method listed, HTTPS and no placeholder in the URL, placeholder only in `Authorization`, host in `network_policy.allow`, credential bound to this host because the `api.github.com` origin names `github` in `credential_names` (`afd_gate/src/policy/egress/mod.rs`), and a read rule matches: `GET` with prefix `/repos/agentsfleet/linkwarden/` (`afd_gate/src/policy/egress/read.rs`; `admission.rs`). The vault then mints: `LeaseMint` → `POST /v1/runners/me/credentials/mint` → `Plane::mint` resolves the lease scope, checks the approved grant, opens the workspace's `github` handle and exchanges an installation token narrowed to the binding (`afd_fleet/src/lease/mint.rs`; `docs/architecture/connectors.md`). The token is kept per lease until expiry and masked out of responses (`afr_egress/src/egress.rs`; `afr_tools/src/egress.rs`).

**Step 4, the review post.** `POST .../pulls/{n}/reviews`: the origin `api.github.com` has rules, and none matches. The write set is exactly `POST /repos/{repo}/git/blobs`, `/git/trees`, `/git/commits` (open), `/git/refs` with `ref` locked to the repair branch, and `/pulls` (exact path) with `head`, `base` and `draft: true` locked (`afd_gate/src/policy/egress/write.rs`, where the paths are `HttpPathMatch::Exact`). `/pulls/{n}/reviews` is neither the exact `/pulls` nor a prefix rule, so `origin_admits` answers `request_policy_not_allowed` (`afr_egress/src/admission.rs`) → `ToolErrorCode::RequestPolicyNotAllowed` (`afr_tools/src/egress.rs`) → the model reads `[request_policy_not_allowed] ...` and the run continues (`afr_tools/src/runtime.rs`). The integration test asserts zero POSTs reach the fake GitHub and the last tool result starts with that code (`agentsfleetd/tests/integration_rust_runner_reviews.rs`). The scenario page records the same (`docs/architecture/scenarios/github-pr-reviewer.md`).

**What the operator sees.** The live tail: `event_received`, `tool_call_started` / `tool_call_completed` (the second with `status: failed` and the output head carrying the code), streamed text chunks, `event_complete` ([Runner Fleet](./runner_fleet.md) §"Live activity (the SSE tail)"). Durably: `core.fleet_events` row `processed` with `response_text` = the model's final answer and `tool_calls` = the trace (`afd_fleet/src/lease/finalize.rs`), and `core.fleet_tool_call_details` for "show all" (`afd_api_runner/src/handler/runner/tool_call.rs`). On GitHub: nothing. Memory: whatever `memory_store` wrote, pushed before the report.

---

## 4. How AGENT BOB 01 can reply

**SKILL.md is prose and cannot widen anything.** The lease marks `instructions` as "soft reasoning input, hard tool and secret policy stays in `policy`" (`afd_wire/src/lease.rs`; `afd_fleet_runtime/src/instructions.rs`). The egress rules are compiled by the daemon from the binding alone (`afd_gate/src/policy/build.rs`; `afd_gate/src/policy/egress/mod.rs`) and evaluated by the runner as written (`afr_egress/src/admission.rs`). So writing "post the review" in SKILL.md (which the fixture already does, `SKILL.md`) changes the model's intent, not the admission.

**A chat steer is the same.** `POST /v1/workspaces/{ws}/fleets/{id}/messages` is admitted as its own `chat` event through the same ledger (`afd_events/src/steer.rs`), waits behind the running lease for the fleet's one affinity slot (`CLAIM_AFFINITY_SLOT`, `afd_fleet/src/lease/sql/lease.rs`), and runs with the same policy. SKILL.md tells the fleet to treat a steer with no PR as chat and to save operator facts with `memory_store` (`SKILL.md`). A steer cannot add an egress rule.

**What the code admits today for this fleet (write binding on one repository):** reads under `/repos/agentsfleet/linkwarden/` (`read.rs`), and the five writes above. In principle the fleet could create blobs, trees and commits, create one ref `refs/heads/agentsfleet-repair/<event>`, and open one draft PR from it against `main`. What is refused: a review (`/pulls/{n}/reviews`), an issue comment, a PR comment, approve, request changes, any other ref, a non-draft PR. The write rules admit a fix on the repair branch and one draft PR, and no comment (`afd_gate/src/policy/egress/write.rs`).

**The deferral, verbatim** (the agent-loop workstream's spec, under Discovery, its Deferrals bullet):

> **Deferrals** — Dimension 6.3's review post: no `afd_gate` rule admits `POST …/pulls/{number}/reviews`, so the post is refused today. > Indy (2026-10-04 11:49): "6.3 The PR review gets posted from the runner? SKILL.md? I want to experience the test and decide, so make me record that and add this as parked." — context: the runner posts it, `github-pr-reviewer/SKILL.md` step 4 sending `POST …/pulls/{number}/reviews` through `http_request`; a write binding admits only `/git/blobs`, `/git/trees`, `/git/commits`, the locked ref and the locked draft (`rustd/crates/afd_gate/src/policy/egress/write.rs`), so the runner refuses the post before it leaves and the test asserts that; Dimension 6.3 stays open until Indy runs it, and this supersedes the Oct 03 "move it to done" for 6.3. > Indy (2026-10-03 15:32): "you just tell me crap, increase the scope, so the refusal of review must be ignored for now. If that blocks the spec to move to done, then record Indys wording and move it to done."

**The other reply path, also not for GitHub.** The report owes a `core.fleet_obligations` delivery only when the admission recorded a reply destination (`afd_fleet/src/lease/commit.rs`; `schema/918_fleet_admissions_reply_destination.sql`). The App webhook admits with `Reply::None` (`afd_ingress/src/deliver.rs`), and only Slack posters exist (`afd_outbound/src/poster.rs`). So there is no daemon-side "answer back to the PR" either.

**The sandbox does not change this.** The sandbox `git` tool refuses `push`, `fetch`, `pull`, `remote` and `clone`, and a change is meant to leave through `propose_change`, which is not written (`afr_tools/src/sandbox/git.rs`; [Runner execution](./runner_execution.md) §"Repository writes"). Unblocking the review is a one-rule change in `afd_gate/src/policy/egress/write.rs` (an exact `POST /repos/{repo}/pulls/{n}/reviews` with `event` locked to `COMMENT`), which Indy parked until he runs the test.

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
   NOT carried: the previous answer, and for this fleet no sandbox and no clone
```

**Same fleet, new event, new lease.** The dedupe key is the body digest (`afd_ingress/src/app.rs`), and a `synchronize` body differs, so it is a new admission and a new stream entry. Every pull-request action wakes the fleet under `Policy::AppIngress`, so `labeled`, `edited` and friends each queue a run too; nothing coalesces or supersedes per PR. The affinity slot allows one active lease per fleet, and on the runner `FleetTurns` allows one run per fleet ([Runner execution](./runner_execution.md) §"One lease, end to end"). So the second event waits until the first reports and the settle transaction frees the slot.

**Same runner?** Preferred, not promised: the candidate scan sorts the last runner first, and the claim records the hint (`afd_fleet/src/lease/sql/lease.rs`). Any eligible runner can win. A runner that holds a sandbox for the fleet looks at it first, and others skip it while the hold lasts ([Runner Fleet](./runner_fleet.md) §"Cold and warm execution").

**What carries over.** Fleet memory: the hydrate window is `HYDRATE_WINDOW_BYTES = 256 KiB` with every `core` entry pinned first, plus up to `RECALL_LIMIT_MAX = 50` per recall past the window (`afd_wire/src/memory.rs`; [Runner Fleet](./runner_fleet.md) §"Memory continuity — durable fleet memory rides the trusted plane"). Memory belongs to the fleet, not the PR, so the second review knows the first only if SKILL.md has it read the existing reviews or recall what it stored. A chat lease also carries the thread's last eight finished turns; a webhook lease like this one carries none (`afd_fleet/src/lease/answer.rs`). A fleet with a sandbox tool continues in its held sandbox when the hold is still live and its key matches; this fleet has no sandbox, so it never holds one.

**What does not carry over.** The session checkpoint is written to `core.fleet_sessions` and loaded with the fleet, and no lease carries it ([Runner execution](./runner_execution.md) §"Workspace between leases"). No workspace restore exists yet.

---

## 6. Sandbox lifecycle

Layout: [runner_fleet.md](./runner_fleet.md) §"The sandbox filesystem contract". Build and teardown: [runner_execution.md](./runner_execution.md) §"A lease's sandbox today". Hold: [runner_execution.md](./runner_execution.md) §"Workspace between leases".

---

## 7. Where the image lives, and what bounds a bare-metal host

The toolbox image, its page cache and what bounds a host's memory and disk: [runner_execution.md](./runner_execution.md) §"Toolbox" and §"A lease's sandbox today".

**The numbers the code declares.**
- `Limits::default()`: 2 GiB of memory, two cores, 512 processes and a 4 GiB disk (`afr_sandbox/src/engine.rs`); 200 MiB/s of disk input and output each way; swap off (`afr_sandbox/src/cgroup.rs`).
- Workers per host: default 1, between 1 and 64 (`afd_core/src/limits.rs`).
- Activity: 64 KiB batches, 4 held (`afr_supervisor/src/activity.rs`). Memory: a 256 KiB hydrate window (`afd_wire/src/memory.rs`).
- Timers: lease TTL 30 s, renew every 5 s, max runtime 12 h, runner offline after 90 s, heartbeat 10 s, empty-poll hint 1 s, sandbox hold 10 min (`afd_core/src/timing.rs`).
- Poll and admission: `MAX_READY_CANDIDATES_PER_POLL` = 64 fleets peeked per poll (`afd_fleet/src/lease/assign.rs`); `FLEET_BACKLOG_BUDGET` = 10,000 outstanding stream entries per fleet before its producers are refused (`afd_admission/src/budget.rs`).

---

## 8. Gaps still open

Each gap names what would close it.

1. **The review post is refused.** No write rule admits `/pulls/{n}/reviews`, and Indy parked it until he runs the test. A one-rule change in `afd_gate/src/policy/egress/write.rs`, an exact path with `event` locked to `COMMENT`, closes it.
2. **No daemon path answers a PR.** The App webhook admits with `Reply::None` (`afd_ingress/src/deliver.rs`), and only Slack posters exist (`afd_outbound/src/poster.rs`). A GitHub poster in the outbound worker closes it.
3. **No workspace restore or save.** Snapshots in R2 are design only ([Runner execution](./runner_execution.md) §"Workspace between leases"). The snapshot workstream closes it.
4. **The session checkpoint reaches no lease.** It is written and loaded, and `render` never puts it on the lease (`afd_fleet/src/lease/answer.rs`). A lease field the agent loop reads closes it.
5. **`propose_change` and the supervisor push are not written.** The sandbox `git` tool refuses `push` (`afr_tools/src/sandbox/git.rs`). The push path in [Runner execution](./runner_execution.md) §"Repository writes" closes it.
6. **Kill and pause never reach a running lease.** Renewal admits a fleet that is no longer active, and the heartbeat names no lease ([Runner Fleet](./runner_fleet.md) §"Steer, kill, pause"). A revocation on the heartbeat or a refused renewal closes it.
7. **Browser tools are refused** under the bubblewrap engine ([Runner execution](./runner_execution.md) §"Sandbox engines"). A Firecracker engine closes it.
8. **Warm slots are not wired into the supervisor.** Wiring `WarmSlots` around the host's engine closes it.
9. **Limits are not per fleet.** The daemon sends `limits: null` (`afd_fleet/src/lease/answer.rs`). A size on the fleet's config closes it.
10. **Nothing checks memory or disk against the host.** 2 GiB × workers and 4 GiB × live and held leases go unchecked. A reserve the worker checks before it leases closes it.
11. **An operator's extra binds are never bound.** The assigned policy carries them and no runner reads them ([Runner Fleet](./runner_fleet.md) §"The sandbox filesystem contract"). Binding them in the sandbox's layout, or dropping the field, closes it.
12. **A rotating-CDN host can fail mid-lease under `allow_list_egress`.** The set is pinned at lease bind. The DNS-answer name layer in [Runner Fleet](./runner_fleet.md) §"Egress model — outbound is the only network surface" closes it.
13. **GitHub never auto-redelivers and gives ten seconds;** a leg that fails before its admission row commits is lost outright ([`connectors.md`](./connectors.md)). Answering 202 before admission, with a durable inbox, closes it.
14. **`deployment_status` and repair-branch deliveries are acknowledged and dropped,** because repair evidence has a reader and no writer (`afd_api_ingress/src/handler/webhook/app_route.rs`). A writer for the repair sweeper's evidence closes it.
