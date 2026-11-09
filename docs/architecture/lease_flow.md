# How a lease works, from "start a fleet" to a sandbox being nuked

Read this when you want one lease's whole path on one page. The path starts with a fleet installed and a pull request (PR) opened. It continues through the event leased and its turn run, and ends with the report settled and the sandbox held or destroyed.

The page walks the path and links each step to the page that owns it.

[`data_flow.md`](./data_flow.md) owns ingress, admission and settlement. [`runner_protocol.md`](./runner_protocol.md) owns the connection, renewal and recovery rules. [`runner_execution.md`](./runner_execution.md) owns the runner's side.

Where a step's facts live on one of those pages, this page links rather than restates them.
Crate names refer to crates under `rustd/crates/`.
Expanded acronyms: Hash-based Message Authentication Code (HMAC), Server-Sent Events (SSE), Time To Live (TTL).

This fleet-runtime example uses `github-pr-reviewer`, saved as "agent-bob-01" on `agentsfleet/linkwarden`.
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

 GATEWAY CLAIM PASS      native session has advertised worker capacity
                         a Postgres: oldest unsettled head per eligible fleet
                         b prefer held fleets, wake notifications and previous placement; ≤64
                         c lock slot and exact head with SKIP LOCKED
                         d advance slot + admission fences; decode before commit
                         e INSERT fleet.events status=received (+ stamp admission delivered)
                         f SPUBLISH event_received bracket on fleet:{id}:activity
                         g gates: payer → balance → budget → approval (nothing charged)
                         h vault: static secrets + mintable "github"; build ExecutionPolicy
                             (tools, network.allow, origin rules for api.github.com)
                         i INSERT runner.leases        status=active, fencing_token, expires  ← THE LEASE ROW
                         j push offer{lease:{lease_id,fencing_token,lease_expires_at,
                                              event{digest},policy,
                                              instructions=instructions_md}}
```

### Save writes the fleet and its setup, then activates once ready

Save (`POST /v1/workspaces/{ws}/fleets`, C1 in [`api_resources.md`](./api_resources.md)) commits the `fleet.fleets` row as `installing`. The reviewed Definition becomes its first `fleet.revisions` row (`afd_authoring`'s creation reserve). Nothing touches a queue or an approval before that commit.

Setup then advances in bounded steps (`afd_authoring`'s creation activate).
A missing grant raises a pending approval card (`afd_approval`'s setup request).
After approval, run `agentsfleet resume <FLEET_ID>` to retry setup.

[Setup recovery](data_flow.md#fleet-creation-and-setup-recovery) covers bounded retries.
Once setup checks pass, a locked recheck activates the fleet.
A requested initial admission commits in that same transaction.

With start `none` (`afd_authoring`'s creation types), Save writes no admission, event, lease or affinity row.

The fleet's display name ("agent-bob-01") is `fleet.fleets.name`. The lease reads it into `Installed.name` (`afd_fleet`'s lease install); nothing in routing reads it.

### Ingress: how a PR on `agentsfleet/linkwarden` finds the fleet

The App webhook route (`afd_api_ingress`) serves only `github`. It checks the body cap before anything is hashed and requires the `X-GitHub-Event` header. It then verifies the body against the platform App's `webhook_secret` under the vault key `github-app` (`APP_IDENTITY_GITHUB`).

A `ping` is answered only after verification.

The route reads the body once with `github::Delivery::read`. It takes `installation.id` and `repository.full_name`, resolves the installation to a workspace with `SELECT_INSTALL_WORKSPACE` (`afd_ingress`), and classifies under `Policy::AppIngress`. It then asks `subscribers`.

`subscribers` (`afd_ingress`) runs `SELECT_APP_SUBSCRIBERS`: every `fleet.fleets` row in that workspace with `status = active`, joined to `connector.integration_grants` where `service = 'github'` and `status = approved`. The document half is in Rust. `Binding::read_for_source` picks the webhook trigger whose `source` is `github`, then `serves_repository(repository)` and `admits(event)` decide.

`serves_repository` is fail-closed: a trigger with no `repositories` list subscribes to nothing, and the match is case-insensitive. `admits` is the opposite: no `events` list means every event. More than `MAX_FANOUT = 100` matches refuses the delivery.

The fleet slug `agent-bob-01` receives the request through linkwarden's installation, which maps to Indy's workspace.
Each active fleet there with an approved `github` grant receives one admission when its reviewed setup includes the following webhook trigger.

The trigger uses `source: github`, `events: [pull_request]`, and `repositories: [agentsfleet/linkwarden]`.
The test fixture's native setup says exactly that (`tests/fixtures/fleets/github-pr-reviewer/definition.json`).
Two fleets so configured both run (`docs/architecture/connectors.md`).

The digest the fleet reasons over is twelve fields (`afd_api_wire`'s `PullRequestDigest`). The fields are action, repo, number, title, url, state, draft, author, head_ref, base_ref, head_sha, received_at.

Under `Policy::AppIngress` every `pull_request` action wakes the fleet except one on a repair branch (`is_repair_branch`). The manual per-fleet route narrows to opened, reopened, synchronize and ready_for_review.

### Admission commits the queue row

Each producer calls `Admissions::admit`. The row carries producer identity, payload, original event time and any reply destination.
An accepted message costs one write (`admit_one.sql`) and holds no fleet-row lock across round trips; a steer adds one read.

Repeated producer identities return the existing admission.
After commit, a bounded wake publish starts a gateway claim pass for hosts with available capacity.

### The gateway claims an admission and its fleet slot

1. Read the oldest unsettled admission for each eligible fleet through `idx_admissions_unsettled`.
2. Prefer held fleets and recent wake notifications, then try a bounded candidate set.
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

`agentsfleet-runner run` reads its API address, native stream origin and runner token before opening its storage home.
The home holds sandboxes, repository mirrors and the toolbox.
Host checks and the engine's cleanup sweep run before the runner takes work (`afr_supervisor::boot`, `afr_sandbox`'s engine).

The native session, heartbeat policy and worker pool run together (`afr_supervisor/src/lib.rs`).
Idle workers wait for pushed offers and advertise their free capacity.
The [runner guide](runner_protocol.md) owns connection setup, heartbeat timing, cancellation, renewal and reconnect behavior.

Each fleet lease follows these steps:

1. Admit its tools, provider and policy; claim the fleet's local turn.
2. Load the lease's memory window, or read memory through the retained HTTP route when the window is absent.
3. Prepare or thaw a sandbox when the admitted tools need one, then run the agent loop.
4. Request folded inputs before model calls and send bounded activity over the native session.
5. Freeze and hold a completed sandbox, or kill it; post full tool records through the retained HTTP route.
6. Flush outstanding usage through renewals, then send the report with memory, timing segments and folded-input progress.
7. Retain the report in the worker's memory until acknowledgement or a terminal refusal, then finish sandbox cleanup and drain activity.

The daemon captures reported memory under the lease fence before the settlement transaction.
Settlement records usage, event outcome, session checkpoint, admission completion, slot release and any outbound obligation.
Only a successful report settles folded messages through `folded_through_sequence`.

Connection loss preserves pending reports in memory while the runner reconnects.
Process loss can discard results or usage the daemon has not accepted.
[Results and billing](runner_protocol.md#results-and-billing) covers retries, shutdown and saved acknowledgements.

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
   │                      gateway sees available capacity on the native session
   │                      claim slot, fleet.events received, gates, vault, policy:
   │                        tools: [http_request, memory_store, memory_recall]
   │                        network.allow: [api.github.com]
   │                        mintable: github; binding: linkwarden, write, base main
   │                        origin api.github.com rules: GET/HEAD /repos/agentsfleet/linkwarden/*
   │                           POST /git/blobs|trees|commits, POST /git/refs{ref locked},
   │                           POST /pulls{head,base locked, draft:true}
   │                           PATCH own event ref{force:false, sha only}
   │                      ├──── offer{lease:{instructions,event=digest}} ─────────────────►
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
   │                      │◄──── HTTP tool records; native report with memory ───────────┤
   │                      settle: fleet.events processed, response_text = the answer
   │                      admission settled, event_complete frame → dashboard thread
```

**What the lease carries for this fleet.** The setup's configuration names three tools, one credential and one network host.

The fleet's write binding is on `agentsfleet/linkwarden` with base `main`. Its daily budget is `2000000000` nanos, two dollars (`tests/fixtures/fleets/github-pr-reviewer/definition.json`).

All three tools are `Runtime::Supervisor` (`afr_tools`' catalog), so `needs_sandbox` is false. The lease therefore starts no sandbox at all ([`runner_execution.md`](./runner_execution.md) §"A lease's sandbox today"). No repository is cloned either: only a lease offered a sandbox tool gets its bound repositories checked out (`afr_supervisor`'s checkout).

**The prompt.** The system prompt is `## Installed instructions` plus the lease's `instructions` (`afr_agent`'s prompt).

Because the binding is write, a `## Trusted repair context` block follows, naming the repository, the locked repair branch and the base. The first user turn is the whole digest JSON, because the `PullRequestDigest` has no `message` field.

The instructions' first step reads `repo` and `number` from exactly that digest (`instructions_md` in `definition.json`).

**Step 2, the diff read.** `http_request` drafts `GET https://api.github.com/repos/agentsfleet/linkwarden/pulls/{n}` with `Authorization: Bearer ${secrets.github.token}` (`afr_tools`' `http_request`).

`Admission::admit` (`af_egress`) checks: method listed, HTTPS and no placeholder in the URL, placeholder only in `Authorization`, host in `network_policy.allow`. The credential is bound to this host because the `api.github.com` origin names `github` in `credential_names` (`afd_gate`'s egress policy).

A read rule matches: `GET` with prefix `/repos/agentsfleet/linkwarden/`.

The vault then mints. `LeaseMint` calls `POST /v1/runners/me/credentials/mint`. `Plane::mint` resolves the lease scope, checks the approved grant and opens the workspace's `github` handle (`afd_fleet`'s lease mint; `docs/architecture/connectors.md`).

It exchanges an installation token narrowed to the binding. The token is kept per lease until expiry and masked out of responses (`af_egress`; `afr_tools`' egress).

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
The compiler is `afd_gate`'s egress write rules.
The runner matches these rules in `af_egress`'s origin module.

The PATCH supports bounded corrections within the same event's branch and existing draft.
GitHub rejects non-fast-forward updates when `force:false`; this is not an expected-old-head comparison.

Fleet instructions require fresh ref/draft reads and the observed branch head as the correction commit's sole parent.
A later admission cannot select an earlier event's draft through these rules.

Compatible runners must precede leases carrying the new `patch` method; older readers reject it.

The reviews path remains denied by `origin_admits` in `af_egress`'s admission module.
The tool returns `ToolErrorCode::RequestPolicyNotAllowed`, with `[request_policy_not_allowed]` visible to the model.
The lease loop can continue after a refusal; the CI fleet instructions require stopping.

The error advises against trying another path, API or credential because one development run tried sixteen alternatives after a denied post.
The same guidance accompanies `secret_not_found` (`af_egress`'s error module).
The platform has no refusal-count ceiling; that remains open scope.

The daemon's `integration_rust_runner_reviews` test verifies zero review POSTs reach GitHub and preserves the final denial code.
The [reviewer scenario](scenarios/github-pr-reviewer.md) records the same boundary.

**What the operator sees.** The live tail carries `event_received`, `tool_call_started`, `tool_call_completed`, streamed text chunks and `event_complete` ([Runner Fleet](./runner_fleet.md) §"Live activity (the SSE tail)"). The completed frame has `status: failed`, and its output head carries the code.

Durably, the `fleet.events` row is `processed`. Its `response_text` is the model's final answer and its `tool_calls` the trace (`afd_fleet`'s lease finalize). `fleet.tool_calls` serves "show all" (`afd_api_runner`).

On GitHub: nothing. Memory: whatever `memory_store` wrote, pushed before the report.

---

## 4. How AGENT BOB 01 can reply

**Instructions are prose and cannot widen anything.** The lease marks `instructions` as soft reasoning input (`af_wire`'s lease payload).

Hard tool and secret policy stays in `policy`. The daemon compiles the egress rules from the binding alone (`afd_gate`'s policy build and egress modules). The runner evaluates them as written (`af_egress`'s admission).

So writing "post the review" in the instructions (which the fixture already does) changes the model's intent, not the admission.

**A chat message uses the same admission path.** `POST /v1/workspaces/{ws}/fleets/{id}/messages` creates its own `chat` admission (`afd_events`'s steer module).

The 202 names a `disposition`: `started`, `folded` or `queued`. With no run live, the message waits for the single affinity slot (`queue::CLAIM_HEAD`, `afd_fleet`'s lease queue). It then starts a run of its own.

With a run live, the message folds into it, and the 202 also carries `folded_into_event_id`.
The lease claims it through the native `folds` request before its next model call; the run's answer becomes its reply.
A parked or task-bound message ahead of it is a barrier, so the new message queues instead.

The web app shows a folded message as "Added to the current run" and waits on the message's own event. The CLI `steer` command waits on the message's own event id the same way.

The message runs with the same policy. The instructions tell the fleet to treat a message with no PR as chat. They also require saving operator facts with `memory_store` (`definition.json`).

A chat message cannot add an egress rule.

**What the code admits today for this fleet (one-repository write binding):** reads under `/repos/agentsfleet/linkwarden/` (the read rules), and the five writes above. In principle the fleet could create blobs, trees and commits, create one ref `refs/heads/agentsfleet-repair/<event>`, and open one draft PR from it against `main`. What is refused: a review (`/pulls/{n}/reviews`), an issue comment, a PR comment, approve, request changes, any other ref, a non-draft PR.

The write rules admit a fix on the repair branch and one draft PR, and no comment (`afd_gate`'s egress write rules).

**The deferral, verbatim** (the agent-loop workstream's spec, under Discovery, its Deferrals bullet):

```text
**Deferrals** — Dimension 6.3's review post: no `afd_gate` rule admits `POST …/pulls/{number}/reviews`, so the post is refused today. > Indy (2026-10-04 11:49): "6.3 The PR review gets posted from the runner? SKILL.md? I want to experience the test and decide, so make me record that and add this as parked." — context: the runner posts it, the `github-pr-reviewer/definition.json` instructions' step 4 sending `POST …/pulls/{number}/reviews` through `http_request`; a write binding admits only `/git/blobs`, `/git/trees`, `/git/commits`, the locked ref and the locked draft (`rustd/crates/daemon/afd_gate/src/policy/egress/write.rs`), so the runner refuses the post before it leaves and the test asserts that; Dimension 6.3 stays open until Indy runs it, and this supersedes the Oct 03 "move it to done" for 6.3. > Indy (2026-10-03 15:32): "you just tell me crap, increase the scope, so the refusal of review must be ignored for now. If that blocks the spec to move to done, then record Indys wording and move it to done."
```

**The other reply path, also not for GitHub.** The report owes a `fleet.obligations` delivery only when the admission recorded a reply destination. That is `afd_fleet`'s lease commit and `schema/20261108125100_fleet_admissions.sql`.

The App webhook admits with `Reply::None` (`afd_ingress`'s deliver). Only Slack posters exist (`afd_outbound`'s posters), so there is no daemon-side "answer back to the PR" either.

**The sandbox does not change this.** The sandbox `git` tool refuses `push`, `fetch`, `pull`, `remote` and `clone`. A change is meant to leave through `propose_change`, which is not written (`afr_tools`' sandbox git; [`runner_execution.md`](./runner_execution.md) §"Repository writes").

Unblocking the review is a one-rule change in `afd_gate`'s egress write rules: an exact `POST /repos/{repo}/pulls/{n}/reviews` with `event` locked to `COMMENT`. Indy parked it until he runs the test.

---

## 5. The PR is fixed and pushed again

```text
 push ──► pull_request "synchronize" ──► new signed body ──► new sha256 ──► new admission row
                                                                           new unsettled queue row
 fleet.admissions    [opened: running]  [synchronize: waiting]  [labeled]  [edited] ...
                       one active lease per fleet (affinity slot) ⇒ strictly one at a time
 next gateway claim after the report frees the slot:
   claim fence+1 (sticky: last_runner_id sorts first, random tie-break)
   new lease row, new lease_id, new fencing_token
   load: fleet memory window (≤256 KiB, core pinned) ⇒ what memory_store saved last run
   NOT carried: the previous answer, and for this fleet no sandbox and no clone
```

**Same fleet, new event, new lease.** The body digest is the dedupe key (`afd_ingress`'s App routing). A `synchronize` body differs and creates a new admission.

Every pull-request action wakes the fleet under `Policy::AppIngress`. An action that arrives while no run is live, such as `labeled` or `edited`, queues a run of its own.

An action that arrives while a run that is not a durable task is live folds into it. That run reads it at its next model call, and its answer settles the action (§2).

The affinity slot allows one active lease per fleet. On the runner, `FleetTurns` allows one run per fleet ([Runner loop](./runner_execution.md) §"One lease, end to end"). An event the live run did not fold waits until that run reports and the settle transaction frees the slot.

**Same runner?** The candidate scan prefers the last runner, and the claim records the hint (`afd_fleet`'s `select_ledger_candidates.sql`). Any eligible runner can win.

A runner with a held sandbox checks that fleet first. Other runners skip the fleet while the hold lasts ([Runner fleet holds](./runner_fleet.md#cold-and-warm-execution)).

**What carries over.** Fleet memory: the memory window is `HYDRATE_WINDOW_BYTES`, 256 KiB, with every `core` entry pinned first.

A recall reads up to `RECALL_LIMIT_MAX = 50` entries past the window (`af_wire`'s memory module; [Runner Fleet](./runner_fleet.md) §"Memory continuity"). Memory belongs to the fleet, not the PR.

The second review knows the first only if its instructions have it read the existing reviews or recall what it stored.

A chat lease also carries the thread's last eight finished turns. A webhook lease like this one carries none (`afd_fleet`'s lease answer).

A fleet with a sandbox tool continues in its held sandbox when the hold is still live and its key matches. This fleet has no sandbox, so it never holds one.

**What does not carry over.** The session checkpoint is written to `fleet.sessions` and loaded with the fleet.

No lease carries it ([`runner_execution.md`](./runner_execution.md) §"Workspace between leases"). No workspace restore exists yet.

---

## 6. Sandbox lifecycle

Layout: [runner_fleet.md](./runner_fleet.md) §"The sandbox filesystem rules". Build and teardown: [runner_execution.md](./runner_execution.md) §"A lease's sandbox today". Hold: [runner_execution.md](./runner_execution.md) §"Workspace between leases".

---

## 7. Where the image lives, and what bounds a bare-metal host

The toolbox image, its page cache and what bounds a host's memory and disk: [runner_execution.md](./runner_execution.md) §"Toolbox" and §"A lease's sandbox today".

**The numbers the code declares.**

- `Limits::default()`: 2 GiB of memory, two cores, 512 processes and a 4 GiB disk (`afr_sandbox`'s engine). Disk input and output: 200 MiB/s each way, swap off (`afr_sandbox`'s cgroup).

- Workers per host: default 1, between 1 and 64 (`af_core`'s limits).

- Activity: 64 KiB batches, 4 held (`afr_supervisor`'s activity). Memory: a 256 KiB memory window (`af_wire`'s memory module).

- Timers in `af_core`: the lease TTL is 30 seconds and renewals run every 5 seconds.
  A run lasts at most 12 hours, and a runner counts as offline after 90 seconds.
  Heartbeats run every 10 seconds; a sandbox hold lasts 10 minutes.

- Claim and admission: at most 64 candidate fleets per claim; no per-fleet admission cap.
  [Queue limits](./datastore_scaling.md#the-admissions-ledger-is-the-queue) describe the sampled deployment budget.

---

## 8. Gaps still open

Each gap names what would close it.

1. **The review post is refused.** No write rule admits `/pulls/{n}/reviews`, and Indy parked it until he runs the test. A one-rule change in `afd_gate`'s egress write rules, an exact path with `event` locked to `COMMENT`, closes it.
2. **No daemon path answers a PR**. The App webhook admits with `Reply::None` (`afd_ingress`'s deliver), and only Slack posters exist (`afd_outbound`'s posters). A GitHub poster in the outbound worker closes it.
3. **No workspace restore or save.** Snapshots in R2 are design only ([`runner_execution.md`](./runner_execution.md) §"Workspace between leases"). The snapshot workstream closes it.
4. **The session checkpoint reaches no lease**. It is written and loaded, and `render` never puts it on the lease (`afd_fleet`'s lease answer). A lease field the agent loop reads closes it.
5. **`propose_change` and the supervisor push are not written.** The sandbox `git` tool refuses `push` (`afr_tools`' sandbox git). The push path in [`runner_execution.md`](./runner_execution.md) §"Repository writes" closes it.
6. **Kill and pause never reach a running lease**: renewal ignores the fleet's status but still enforces its stored budget ceiling. The heartbeat names no lease ([Runner Fleet](./runner_fleet.md) §"Steer, kill, pause"). A revocation on the heartbeat or a refused renewal closes it.
7. **Browser tools are refused** under the bubblewrap engine ([`runner_execution.md`](./runner_execution.md) §"Sandbox engines"). A Firecracker engine closes it.
8. **Warm slots are not wired into the supervisor.** Wiring `WarmSlots` around the host's engine closes it.
9. **Limits are not per fleet.** The daemon sends `limits: null` (`afd_fleet`'s lease answer). A size on the fleet's config closes it.
10. **Nothing checks memory or disk against the host.** 2 GiB × workers and 4 GiB × live and held leases go unchecked. A reserve the worker checks before it leases closes it.
11. **An operator's extra binds are never bound**. The assigned policy carries them and no runner reads them ([Runner Fleet](./runner_fleet.md) §"The sandbox filesystem rules"). Binding them in the sandbox's layout, or dropping the field, closes it.
12. **A rotating-CDN host can fail mid-lease under `allow_list_egress`.** The set is pinned at lease bind. The DNS-answer name layer in [Runner Fleet](./runner_fleet.md) §"Egress model" closes it.
13. **GitHub never auto-redelivers and gives ten seconds;** a leg that fails before its admission row commits is lost outright ([`connectors.md`](./connectors.md)). Answering 202 before admission, with a durable inbox, closes it.
14. **`deployment_status` and repair-branch deliveries are acknowledged and dropped,** because repair evidence has a reader and no writer (`afd_api_ingress`'s App route). A writer for the repair sweeper's evidence closes it.


## Original Pull Request repair

The [durable repair scenario](scenarios/durable-repair.md) owns the combined flow: an authorized externally opened PR, later-CI continuation, worker recovery and configured goal decisions.
Native task bindings and publication receipts retain the authorized PR and expected head across requests. Combined live acceptance remains pending.

Task continuations use the existing admission/lease path.
A task wake requests fresh exact-head evidence; it does not grant authority or certify CI success.
See the scenario for target binding, publication races, cumulative limits and combined acceptance; see [sandbox evaluation](sandbox_evaluation.md#coding-sessions-and-worktrees-across-leases) for workspace recovery mechanics.

M224 proves the reduced manual-first path separately.
A dedicated reviewed implementation spec and development autonomy proof remain outstanding.

## Proposed Durable Tasks across leases

A fleet receives many leases over time and retains at most one active lease.
Several [Durable Tasks](durable_tasks.md) can remain unfinished while only one runs.
Waiting releases the runtime slot without abandoning the accepted objective.

```text
DT A: lease A1 -> waiting for checks ----------> lease A2
DT B:                 lease B1 -> done
slot: [A1]            [B1]       [idle]         [A2]
```

The worker is a slot in `agentsfleet-runner`'s pool, not the conversational model or a subagent.
A replacement worker receives a fresh lease and compatible DT state under current authority.
DT recovery must use the joined sandbox build and existing workspace readiness boundary.
