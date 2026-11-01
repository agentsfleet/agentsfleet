# How a lease works, from "start a fleet" to a sandbox being nuked

Read this when you want one lease's whole path on one page: a fleet installed, a pull request (PR) opened, the event leased, its turn run, its report settled, and its sandbox held or destroyed. It walks the path and links each step to the page that owns it. [`data_flow.md`](./data_flow.md) owns ingress, admission and settlement, [`runner_fleet.md`](./runner_fleet.md) the protocol, renewal and fencing, and [`runner_execution.md`](./runner_execution.md) the runner's side. Where a step's facts live on one of those pages, this page links rather than restates them.

Paths are under `rustd/crates/` unless they start with `docs/`, `schema/`, `deploy/`, `scripts/` or `tests/`. Expanded acronyms: Enhanced Read-Only File System (EROFS), Random Access Memory (RAM), Hash-based Message Authentication Code (HMAC), Server-Sent Events (SSE), Time To Live (TTL), Out Of Memory (OOM), Chrome DevTools Protocol (CDP), Kernel-based Virtual Machine (KVM).

This fleet-runtime example uses `github-pr-reviewer`, saved as “agent-bob-01” on `agentsfleet/linkwarden`.
Private setup turns use [authoring leases](runner_fleet.md#authoring-leases), without runtime tools or a sandbox.

Section 1 covers the daemon, section 2 the runner, and section 3 the complete example.
Sections 6 and 7 cover the sandbox and host.

---

## 1. From "start a fleet" to a lease

```text
 SAVE (once)             fleet.fleets row (name = "agent-bob-01", installing) Postgres
                         fleet.revisions r1: description,
                           instructions_md, config (selected)                 Postgres
                         connector.integration_grants (fleet, "github") pending,
                           then approved by a person                          Postgres
                         agentsfleet resume <FLEET_ID> retries setup
                         locked recheck, all checks ready ⇒ active            Postgres
                         start none: no admission, no event, no lease row

 PR OPENED               POST /v1/ingress/github
   GitHub ──signed──►    1 verify HMAC vs vault "github-app".webhook_secret
                         2 installation.id ─► connector.installs ─► workspace
                         3 body ─► 12-field PullRequestDigest (not GitHub's payload)
                         4 fleets in that workspace: active ∩ approved github grant
                                                   ∩ setup repositories ∋ repo
                                                   ∩ setup events ∋ pull_request
                         5 per matched fleet:
                             INSERT fleet.admissions (producer=webhook_app,
                                 key = "{fleet}:{sha256(body)}")           Postgres  ← the acceptance
                             SPUBLISH fleet:wake:{shard} (best effort)    Dragonfly
                         6 202 {matched, enqueued}

 RUNNER POLL             POST /v1/runners/me/leases  (every ≤1 s per worker)
                         a Postgres: oldest unsettled head per eligible fleet
                         b prefer held fleets, wake hints and previous placement; ≤64
                         c lock slot and exact head with SKIP LOCKED
                         d advance slot + admission fences; decode before commit
                         e INSERT fleet.events status=received (+ stamp admission delivered)
                         f SPUBLISH event_received bracket on fleet:{id}:activity
                         g gates: payer → balance → budget → approval (nothing charged)
                         h vault: static secrets + mintable "github"; build ExecutionPolicy
                             (tools, network.allow, origin rules for api.github.com)
                         i INSERT runner.leases        status=active, fencing_token, expires  ← THE LEASE ROW
                         j 200 { lease: { lease_id, fencing_token, lease_expires_at,
                                          event{digest}, policy,
                                          instructions = instructions_md } }
```

### Save writes the fleet and its setup, then activates once ready

Save (`POST /v1/workspaces/{ws}/fleets`, C1 in [`api_resources.md`](./api_resources.md)) commits the `fleet.fleets` row as `installing` with the reviewed Definition as its first `fleet.revisions` row (`afd_authoring/src/creation/reserve.rs`). Nothing touches a queue or an approval before that commit. Setup then advances in bounded steps (`afd_authoring/src/creation/activate.rs`).

A missing grant raises a pending approval card (`afd_approval/src/request/setup.rs`).
After approval, run `agentsfleet resume <FLEET_ID>` to retry setup.
[Setup recovery](data_flow.md#fleet-creation-and-setup-recovery) covers bounded retries.

Once setup checks pass, a locked recheck activates the fleet.
A requested initial admission commits in that same transaction.

With start `none` (`afd_authoring/src/types/creation.rs`), Save writes no admission, event, lease or affinity row.

The fleet's display name ("agent-bob-01") is `fleet.fleets.name`, read at lease time into `Installed.name` (`afd_fleet/src/lease/installed.rs`); nothing in routing reads it (next paragraph).

### Ingress: how a PR on `agentsfleet/linkwarden` finds the fleet

`afd_api_ingress/src/handler/webhook/app_route.rs`: only `github` is served, the body cap is checked before anything is hashed, the `X-GitHub-Event` header is required, then the body is verified against the platform App's `webhook_secret` under the vault key `github-app` (`APP_IDENTITY_GITHUB`, `app_route.rs`). A `ping` is answered only after verification.

`route` reads the body once with `github::Delivery::read` (`afd_api_ingress/src/handler/webhook/github/payload/mod.rs`), takes `installation.id` and `repository.full_name`, resolves the installation to a workspace with `SELECT_INSTALL_WORKSPACE` (`afd_ingress/src/sql/mod.rs`, `afd_ingress/src/app/mod.rs`), classifies under `Policy::AppIngress`, then asks `subscribers`.

`subscribers` (`afd_ingress/src/app/mod.rs`) runs `SELECT_APP_SUBSCRIBERS` (`afd_ingress/src/sql/mod.rs`): every `fleet.fleets` row in that workspace with `status = active` joined to `connector.integration_grants` where `service = 'github'` and `status = approved`. The document half is in Rust: `Binding::read_for_source` picks the webhook trigger whose `source` is `github` (`afd_ingress/src/binding/mod.rs`), then `serves_repository(repository)` and `admits(event)` (`app.rs`). `serves_repository` is fail-closed: a trigger with no `repositories` list subscribes to nothing, and the match is case-insensitive (`binding.rs`). `admits` is the opposite: no `events` list means every event. More than `MAX_FANOUT = 100` matches refuses the delivery (`app.rs`; `app_route.rs`).

The fleet slug `agent-bob-01` receives the request through linkwarden's installation, which maps to Indy's workspace.
Each active fleet there with an approved `github` grant receives one admission when its reviewed setup includes the following webhook trigger.

The trigger uses `source: github`, `events: [pull_request]`, and `repositories: [agentsfleet/linkwarden]`.
The test fixture's native setup says exactly that (`tests/fixtures/fleets/github-pr-reviewer/definition.json`).
Two fleets so configured both run (`docs/architecture/connectors.md`).

The digest the fleet reasons over is twelve fields (`afd_api_ingress/src/handler/webhook/github/mod.rs`: action, repo, number, title, url, state, draft, author, head_ref, base_ref, head_sha, received_at). Under `Policy::AppIngress` every `pull_request` action wakes the fleet except one on a repair branch (`is_repair_branch`, `github/mod.rs`); the manual per-fleet route narrows to opened/reopened/synchronize/ready_for_review.

### Admission commits the queue row

Each producer calls `Admissions::admit`. The row carries producer identity, payload, original event time and any reply destination.

Repeated producer identities return the existing admission. After commit, a bounded wake publish can prioritize its fleet on the next poll.

### The poll claims an admission and its fleet slot

1. Read the oldest unsettled admission for each eligible fleet through `idx_admissions_unsettled`.
2. Prefer held fleets and recent hints, then try a bounded candidate set.
3. Lock a candidate's slot and exact oldest admission with `SKIP LOCKED`. A locked or parked head blocks later rows.
4. Advance the slot and admission fences together. Decode the envelope before committing, so malformed stored data rolls back both fences.
5. Open runtime history and stamp `delivered_at` under the same current fences.
6. Read installed configuration, resolve billing and check approval gates. Parked work frees the slot and retains its place in the queue.
7. Assemble credentials and policy, then issue `runner.leases` under the claimed fence.

An expired slot exposes the same unsettled row. Redelivery preserves its event identity, original time and prior billing identity.

A report settles the admission in the transaction that records usage, terminal state, checkpoint, slot release and any outbound obligation.
See [the queue design](./datastore_scaling.md#the-admissions-ledger-is-the-queue) for budgets, notifications and recovery limits.

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
 │ 3 hydrate ───────────► GET /v1/runners/me/memory/{fleet_id}                         │
 │ 4 [sandbox]            held sandbox for this fleet and key, else engine.prepare:    │
 │                        dir, workspace.img, cgroup, bwrap on the assigned network,   │
 │                        executor; check_out the bound repositories                   │
 │ 5 agent loop           turns ↔ provider; each call: start frame → router → end frame│
 │     tool calls ──────► POST /v1/runners/me/credentials/mint  (first ${secrets.x})   │
 │     mid-run memory ──► POST /v1/runners/me/memory/{fleet_id} (every N calls)        │
 │     activity pump ───► POST /v1/runners/me/leases/{id}/activity (≤64 KiB, 250 ms)   │
 │   ║ renewal tick 5 s ─► POST /v1/runners/me/leases/{id}/renew {request lines}      │
 │   ║                  ◄── {lease_expires_at}   4xx ⇒ interrupt run (5 s grace)       │
 │ 6 [sandbox] keep       processed ⇒ freeze and hold; else destroy                    │
 │ 7 settle               POST .../tool-calls (full records, ≤256 KiB per post)        │
 │                        POST .../memory/{fleet_id} {lease_id, fence, deltas}         │
 │                        spool <home>/spool/<lease>.json (fsync + rename)             │
 │                        POST /v1/runners/me/reports  {lease_id, fence, outcome, ...} │
 │                        kept on 5xx/401/403/408/413 → drainer retries forever        │
 │ 8 wait ≤5 s for activity to drain                                                   │
 └─────────────────────────────────────────────────────────────────────────────────────┘
                                               daemon report: ONE transaction
                                               METER_LEASE (current fences, active→reported, measured usage)
                                               → fleet.events processed/fleet_error + tool_calls
                                               → fleet.sessions checkpoint → admission settled → affinity released
                                               → obligation if a reply destination; COMMIT
                                               → SPUBLISH event_complete
```

**Boot and composition.** `afr_supervisor::boot` reads three variables and opens `<home>/{sandboxes,spool,git,toolbox}` (`afr_supervisor/src/config/mod.rs`, `afr_supervisor/src/storage_home/mod.rs`). The binary then builds the host's engine, refusing to start on a host that lacks a mechanism every sandbox needs, and runs `afr_supervisor::run` with it (`agentsfleet_runner/src/main.rs`). Building the engine sweeps what a crashed run left ([Runner execution](./runner_execution.md) §"A lease's sandbox today").

**Heartbeat → assignment.** The heartbeat posts the capability report and the fleets the runner holds, and reads back the assigned policy; the worker count is clamped to `1..=64` (`afr_supervisor/src/heartbeat/mod.rs`, `afd_core/src/limits.rs`). The daemon serves the 10 s cadence and reconciles the report against the assignment into a `degraded` verdict ([Runner Fleet](./runner_fleet.md) §"Assigned policy and reconciliation (M148)"). The reply's `status` is always `ok` from this daemon, so a running lease learns nothing of a kill from the heartbeat ([Runner Fleet](./runner_fleet.md) §"Steer, kill, pause").

**Worker pool.** Workers grow to the assigned count and are never killed. A worker polls while the assignment says to take work, sleeps the daemon's `retry_after_ms` (floored at 250 ms) on no work, and runs a lease to its report before polling again (`afr_supervisor/src/worker_pool/mod.rs`).

**One lease.** The steps and their refusals are in [Runner execution](./runner_execution.md) §"One lease, end to end". The renewal ticks every 5 s with the provider requests the daemon has not recorded, one line each, moves the runner's own deadline to the reply's `lease_expires_at` less 2 s, keeps the lease on a 5xx and ends it on a 4xx ([Runner Fleet](./runner_fleet.md) §"Per-lease renewal — how a long fleet keeps its lease"). An interrupted engine gets 5 s to hand back its tokens and memory (`afr_supervisor/src/lease_loop/drive.rs`).

**The turn.** The agent loop builds the prompt from the lease's `instructions` and, on a write binding, a trusted repair context; a chat lease also carries the thread's recent turns (`afr_agent/src/prompt/mod.rs`). Each call is numbered, announced, routed to the supervisor or the sandbox's executor, and closed exactly once (`afr_agent/src/ledger/mod.rs`). At the context cap the loop stops offering tools and asks for the answer (`afr_agent/src/context/mod.rs`).

**Settle and report.** Full tool records post first, then the fenced memory capture, then the report is spooled and posted; the daemon settles it in one transaction ([Runner Fleet](./runner_fleet.md) §"Running one event"). A stale fence, a lease not found or a lease lost settles the spooled entry for good; 5xx, 401, 403, 408 and 413 keep it for the drainer (`afr_supervisor/src/report_spool/mod.rs`, `afr_supervisor/src/drainer/mod.rs`).

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
   │                        ∩ setup repositories ∋ agentsfleet/linkwarden
   │                        ∩ events ∋ pull_request          ⇒ agent-bob-01 (and any twin)
   │                      commit admission + best-effort wake
   │◄──── 202 ────────────┤
   │                      │◄──── POST /v1/runners/me/leases ──────────────────────────────┤
   │                      claim slot, fleet.events received, gates, vault, policy:
   │                        tools: [http_request, memory_store, memory_recall]
   │                        network.allow: [api.github.com]
   │                        mintable: github; binding: linkwarden, write, base main
   │                        origin api.github.com rules: GET/HEAD /repos/agentsfleet/linkwarden/*
   │                           POST /git/blobs|trees|commits, POST /git/refs{ref locked},
   │                           POST /pulls{head,base locked, draft:true}
   │                           PATCH own event ref{force:false, sha only}
   │                      ├──── 200 lease{instructions, event=digest} ────────────────────►
   │                      │                                  admit: all three tools Supervisor
   │                      │                                  ⇒ needs_sandbox = false ⇒ NO SANDBOX
   │                      │◄──── GET memory/{fleet} ─────────────────────────────────────┤
   │                      │                                  prompt: instructions + repair context;
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
   │                      settle: fleet.events processed, response_text = the answer
   │                      admission settled, event_complete frame → dashboard thread
```

**What the lease carries for this fleet.** The setup's configuration names three tools, one credential, one network host, a write binding on `agentsfleet/linkwarden` with base `main`, and a daily budget of `2000000000` nanos, two dollars (`tests/fixtures/fleets/github-pr-reviewer/definition.json`). All three tools are `Runtime::Supervisor` (`afr_tools/src/catalog/mod.rs`), so `needs_sandbox` is false and the lease starts no sandbox at all ([Runner execution](./runner_execution.md) §"A lease's sandbox today"). No repository is cloned either: only a lease offered a sandbox tool gets its bound repositories checked out (`afr_supervisor/src/lease_loop/checkout.rs`).

**The prompt.** System prompt = `## Installed instructions` + the lease's `instructions` (`afr_agent/src/prompt/mod.rs`), then, because the binding is write, a `## Trusted repair context` block naming the repository, the locked repair branch and the base. The first user turn is the whole digest JSON, because the `PullRequestDigest` has no `message` field (fields at `afd_api_ingress/src/handler/webhook/github/mod.rs`). The instructions' first step reads `repo` and `number` from exactly that digest (`instructions_md` in `definition.json`).

**Step 2, the diff read.** `http_request` drafts `GET https://api.github.com/repos/agentsfleet/linkwarden/pulls/{n}` with `Authorization: Bearer ${secrets.github.token}` (`afr_tools/src/http_request/mod.rs`). `Admission::admit` (`afr_egress/src/admission/mod.rs`): method listed, HTTPS and no placeholder in the URL, placeholder only in `Authorization`, host in `network_policy.allow`, credential bound to this host because the `api.github.com` origin names `github` in `credential_names` (`afd_gate/src/policy/egress/mod.rs`), and a read rule matches: `GET` with prefix `/repos/agentsfleet/linkwarden/` (`afd_gate/src/policy/egress/read.rs`; `admission.rs`). The vault then mints: `LeaseMint` → `POST /v1/runners/me/credentials/mint` → `Plane::mint` resolves the lease scope, checks the approved grant, opens the workspace's `github` handle and exchanges an installation token narrowed to the binding (`afd_fleet/src/lease/mint/mod.rs`; `docs/architecture/connectors.md`). The token is kept per lease until expiry and masked out of responses (`afr_egress/src/egress/mod.rs`; `afr_tools/src/egress/mod.rs`).

**Step 4, the review post.** `POST .../pulls/{n}/reviews` matches none of the rules for `api.github.com`.
The write binding permits the exact paths below, prefixed by `/repos/{repo}`.

| Method | Path | Body boundary |
| --- | --- | --- |
| POST | `/git/blobs`, `/git/trees` | Unreferenced objects; open bodies |
| POST | `/git/commits` | Only `message`, `tree` and `parents` |
| POST | `/git/refs` | Exact event-derived `ref`; only `sha` beside it |
| POST | `/pulls` | Locked `head`, `base`, `draft:true`; only `title`, `body`, `maintainer_can_modify` beside them |
| PATCH | `/git/refs/heads/{event_branch}` | Required boolean `force:false`; only `sha` beside it |

Closed rules reject other fields, duplicate fields and query strings.
The compiler lives in `rustd/crates/afd_gate/src/policy/egress/write.rs`.
The runner matches these rules in `rustd/crates/afr_egress/src/origin/mod.rs`.

The PATCH supports bounded corrections within the same event's branch and existing draft.
GitHub rejects non-fast-forward updates when `force:false`; this is not an expected-old-head comparison.
Fleet instructions require fresh ref/draft reads and the observed branch head as the correction commit's sole parent.
A later admission cannot select an earlier event's draft through these rules.
Compatible runners must precede leases carrying the new `patch` method; older readers reject it.

The reviews path remains denied by `origin_admits` in `rustd/crates/afr_egress/src/admission/mod.rs`.
The tool returns `ToolErrorCode::RequestPolicyNotAllowed`, with `[request_policy_not_allowed]` visible to the model.
The lease loop can continue after a refusal; the CI fleet instructions require stopping.

The error advises against trying another path, API or credential because one development run tried sixteen alternatives after a denied post.
The same guidance accompanies `secret_not_found`; see `rustd/crates/afr_egress/src/error/mod.rs`.
The platform has no refusal-count ceiling; that remains open scope.

`rustd/crates/agentsfleetd/tests/integration_rust_runner_reviews.rs` verifies zero review POSTs reach GitHub and preserves the final denial code.
The [reviewer scenario](scenarios/github-pr-reviewer.md) records the same boundary.

**What the operator sees.** The live tail: `event_received`, `tool_call_started` / `tool_call_completed` (the second with `status: failed` and the output head carrying the code), streamed text chunks, `event_complete` ([Runner Fleet](./runner_fleet.md) §"Live activity (the SSE tail)"). Durably: `fleet.events` row `processed` with `response_text` = the model's final answer and `tool_calls` = the trace (`afd_fleet/src/lease/finalize.rs`), and `fleet.tool_calls` for "show all" (`afd_api_runner/src/handler/runner/tool_call.rs`). On GitHub: nothing. Memory: whatever `memory_store` wrote, pushed before the report.

---

## 4. How AGENT BOB 01 can reply

**Instructions are prose and cannot widen anything.** The lease marks `instructions` as "soft reasoning input, hard tool and secret policy stays in `policy`" (`afd_wire/src/lease/payload.rs`). The egress rules are compiled by the daemon from the binding alone (`afd_gate/src/policy/build/mod.rs`; `afd_gate/src/policy/egress/mod.rs`) and evaluated by the runner as written (`afr_egress/src/admission/mod.rs`). So writing "post the review" in the instructions (which the fixture already does) changes the model's intent, not the admission.

**A chat steer uses the same admission path.** `POST /v1/workspaces/{ws}/fleets/{id}/messages` creates its own `chat` admission (`afd_events/src/steer/mod.rs`). It waits behind the fleet's running lease for the single affinity slot (`queue::CLAIM_HEAD`, `afd_fleet/src/lease/sql/queue.rs`).

The steer runs with the same policy. The instructions tell the fleet to treat a steer with no PR as chat. They also require saving operator facts with `memory_store` (`definition.json`).

A steer cannot add an egress rule.

**What the code admits today for this fleet (write binding on one repository):** reads under `/repos/agentsfleet/linkwarden/` (`read.rs`), and the five writes above. In principle the fleet could create blobs, trees and commits, create one ref `refs/heads/agentsfleet-repair/<event>`, and open one draft PR from it against `main`. What is refused: a review (`/pulls/{n}/reviews`), an issue comment, a PR comment, approve, request changes, any other ref, a non-draft PR. The write rules admit a fix on the repair branch and one draft PR, and no comment (`afd_gate/src/policy/egress/write.rs`).

**The deferral, verbatim** (the agent-loop workstream's spec, under Discovery, its Deferrals bullet):

> **Deferrals** — Dimension 6.3's review post: no `afd_gate` rule admits `POST …/pulls/{number}/reviews`, so the post is refused today. > Indy (2026-10-04 11:49): "6.3 The PR review gets posted from the runner? SKILL.md? I want to experience the test and decide, so make me record that and add this as parked." — context: the runner posts it, the `github-pr-reviewer/definition.json` instructions' step 4 sending `POST …/pulls/{number}/reviews` through `http_request`; a write binding admits only `/git/blobs`, `/git/trees`, `/git/commits`, the locked ref and the locked draft (`rustd/crates/afd_gate/src/policy/egress/write.rs`), so the runner refuses the post before it leaves and the test asserts that; Dimension 6.3 stays open until Indy runs it, and this supersedes the Oct 03 "move it to done" for 6.3. > Indy (2026-10-03 15:32): "you just tell me crap, increase the scope, so the refusal of review must be ignored for now. If that blocks the spec to move to done, then record Indys wording and move it to done."

**The other reply path, also not for GitHub.** The report owes a `fleet.obligations` delivery only when the admission recorded a reply destination (`afd_fleet/src/lease/commit.rs`; `schema/1720_fleet_admissions.sql`). The App webhook admits with `Reply::None` (`afd_ingress/src/deliver.rs`), and only Slack posters exist (`afd_outbound/src/poster/mod.rs`). So there is no daemon-side "answer back to the PR" either.

**The sandbox does not change this.** The sandbox `git` tool refuses `push`, `fetch`, `pull`, `remote` and `clone`, and a change is meant to leave through `propose_change`, which is not written (`afr_tools/src/sandbox/git/mod.rs`; [Runner execution](./runner_execution.md) §"Repository writes"). Unblocking the review is a one-rule change in `afd_gate/src/policy/egress/write.rs` (an exact `POST /repos/{repo}/pulls/{n}/reviews` with `event` locked to `COMMENT`), which Indy parked until he runs the test.

---

## 5. The PR is fixed and pushed again

```text
 push ──► pull_request "synchronize" ──► new signed body ──► new sha256 ──► new admission row
                                                                           new unsettled queue row
 fleet.admissions    [opened: running]  [synchronize: waiting]  [labeled]  [edited] ...
                       one active lease per fleet (affinity slot) ⇒ strictly one at a time
 next poll after the report frees the slot:
   claim fence+1 (sticky: last_runner_id sorts first, random tie-break)
   new lease row, new lease_id, new fencing_token
   hydrate: fleet memory window (≤256 KiB, core pinned) ⇒ what memory_store saved last run
   NOT carried: the previous answer, and for this fleet no sandbox and no clone
```

**Same fleet, new event, new lease.** The body digest is the dedupe key (`afd_ingress/src/app/mod.rs`). A `synchronize` body differs and creates a new admission.

Every pull-request action wakes the fleet under `Policy::AppIngress`. Actions such as `labeled` and `edited` each queue a run. Actions neither coalesce nor supersede other events for the same PR.

The affinity slot allows one active lease per fleet. On the runner, `FleetTurns` allows one run per fleet ([Runner loop](./runner_execution.md) §"One lease, end to end"). The second event waits until the first reports and the settle transaction frees the slot.

**Same runner?** The candidate scan prefers the last runner, and the claim records the hint (`afd_fleet/src/lease/sql/lease/select_ledger_candidates.sql`). Any eligible runner can win.

A runner with a held sandbox checks that fleet first. Other runners skip the fleet while the hold lasts ([Runner fleet holds](./runner_fleet.md#cold-and-warm-execution)).

**What carries over.** Fleet memory: the hydrate window is `HYDRATE_WINDOW_BYTES = 256 KiB` with every `core` entry pinned first, plus up to `RECALL_LIMIT_MAX = 50` per recall past the window (`afd_wire/src/memory.rs`; [Runner Fleet](./runner_fleet.md) §"Memory continuity — durable fleet memory rides the trusted plane"). Memory belongs to the fleet, not the PR, so the second review knows the first only if its instructions have it read the existing reviews or recall what it stored. A chat lease also carries the thread's last eight finished turns; a webhook lease like this one carries none (`afd_fleet/src/lease/answer/mod.rs`). A fleet with a sandbox tool continues in its held sandbox when the hold is still live and its key matches; this fleet has no sandbox, so it never holds one.

**What does not carry over.** The session checkpoint is written to `fleet.sessions` and loaded with the fleet, and no lease carries it ([Runner execution](./runner_execution.md) §"Workspace between leases"). No workspace restore exists yet.

---

## 6. Sandbox lifecycle

Layout: [runner_fleet.md](./runner_fleet.md) §"The sandbox filesystem contract". Build and teardown: [runner_execution.md](./runner_execution.md) §"A lease's sandbox today". Hold: [runner_execution.md](./runner_execution.md) §"Workspace between leases".

---

## 7. Where the image lives, and what bounds a bare-metal host

The toolbox image, its page cache and what bounds a host's memory and disk: [runner_execution.md](./runner_execution.md) §"Toolbox" and §"A lease's sandbox today".

**The numbers the code declares.**
- `Limits::default()`: 2 GiB of memory, two cores, 512 processes and a 4 GiB disk (`afr_sandbox/src/engine/mod.rs`); 200 MiB/s of disk input and output each way; swap off (`afr_sandbox/src/cgroup/mod.rs`).
- Workers per host: default 1, between 1 and 64 (`afd_core/src/limits.rs`).
- Activity: 64 KiB batches, 4 held (`afr_supervisor/src/activity/mod.rs`). Memory: a 256 KiB hydrate window (`afd_wire/src/memory.rs`).
- Timers: lease TTL 30 s, renew every 5 s, max runtime 12 h, runner offline after 90 s, heartbeat 10 s, empty-poll hint 1 s, sandbox hold 10 min (`afd_core/src/timing.rs`).
- Poll and admission: at most 64 candidate fleets per poll; 10,000 unsettled admissions per fleet. [Queue limits](./datastore_scaling.md#the-admissions-ledger-is-the-queue) also describe the sampled deployment budget.

---

## 8. Gaps still open

Each gap names what would close it.

1. **The review post is refused.** No write rule admits `/pulls/{n}/reviews`, and Indy parked it until he runs the test. A one-rule change in `afd_gate/src/policy/egress/write.rs`, an exact path with `event` locked to `COMMENT`, closes it.
2. **No daemon path answers a PR.** The App webhook admits with `Reply::None` (`afd_ingress/src/deliver.rs`), and only Slack posters exist (`afd_outbound/src/poster/mod.rs`). A GitHub poster in the outbound worker closes it.
3. **No workspace restore or save.** Snapshots in R2 are design only ([Runner execution](./runner_execution.md) §"Workspace between leases"). The snapshot workstream closes it.
4. **The session checkpoint reaches no lease.** It is written and loaded, and `render` never puts it on the lease (`afd_fleet/src/lease/answer/mod.rs`). A lease field the agent loop reads closes it.
5. **`propose_change` and the supervisor push are not written.** The sandbox `git` tool refuses `push` (`afr_tools/src/sandbox/git/mod.rs`). The push path in [Runner execution](./runner_execution.md) §"Repository writes" closes it.
6. **Kill and pause never reach a running lease.** Renewal ignores the fleet's status but still enforces its stored budget ceiling, and the heartbeat names no lease ([Runner Fleet](./runner_fleet.md) §"Steer, kill, pause"). A revocation on the heartbeat or a refused renewal closes it.
7. **Browser tools are refused** under the bubblewrap engine ([Runner execution](./runner_execution.md) §"Sandbox engines"). A Firecracker engine closes it.
8. **Warm slots are not wired into the supervisor.** Wiring `WarmSlots` around the host's engine closes it.
9. **Limits are not per fleet.** The daemon sends `limits: null` (`afd_fleet/src/lease/answer/mod.rs`). A size on the fleet's config closes it.
10. **Nothing checks memory or disk against the host.** 2 GiB × workers and 4 GiB × live and held leases go unchecked. A reserve the worker checks before it leases closes it.
11. **An operator's extra binds are never bound.** The assigned policy carries them and no runner reads them ([Runner Fleet](./runner_fleet.md) §"The sandbox filesystem contract"). Binding them in the sandbox's layout, or dropping the field, closes it.
12. **A rotating-CDN host can fail mid-lease under `allow_list_egress`.** The set is pinned at lease bind. The DNS-answer name layer in [Runner Fleet](./runner_fleet.md) §"Egress model — outbound is the only network surface" closes it.
13. **GitHub never auto-redelivers and gives ten seconds;** a leg that fails before its admission row commits is lost outright ([`connectors.md`](./connectors.md)). Answering 202 before admission, with a durable inbox, closes it.
14. **`deployment_status` and repair-branch deliveries are acknowledged and dropped,** because repair evidence has a reader and no writer (`afd_api_ingress/src/handler/webhook/app_route.rs`). A writer for the repair sweeper's evidence closes it.
