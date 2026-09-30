# github-pr-reviewer: production readiness and user journey audit

```text
+--------------------------------------------------------------------------+
| TO:       Indy                                                           |
| REVIEW:   Tarzy CTO role, as requested                                    |
| PERSONA:  nkishore@megam.io                                               |
| SCOPE:    One fleet, then two fleets; browser, GitHub, daemon, runner, data |
| DATE:     2026-09-26                                                      |
| BASE:     dbd2f32f396c82e7fe0b236d337efba084b133de                        |
| METHOD:   Source and documentation audit; analysis only                   |
| VERDICT:  HOLD general production readiness sign-off                      |
+--------------------------------------------------------------------------+
```

## 1. Recommendation to Indy

Do not sign off on the complete promise "connect GitHub, get a reliable review,
then discuss that review in the UI" from the current evidence. The repository
contains substantial durability, isolation, and recovery machinery. Several
boundaries still undermine that promise, and some documentation describes
stronger behavior than the executable path provides.

My release blockers, in order:

| ID | Priority | Finding | Evidence classification |
|---|---|---|---|
| R1 | P0 | A received event can become undiscoverable after queue/group loss if the daemon dies before saving its runner lease. | Concrete source-derived failure sequence; not fault-injected in this audit. |
| R2 | P0 | Failed GitHub delivery and partially completed fan-out can leave fleets with no durable admission and no automatic GitHub retry. | Handler ordering plus current GitHub documentation. |
| R3 | P0 | The persisted conversation checkpoint is read but not sent in the lease. A follow-up cannot reliably refer to the preceding review. | Executable payload and prompt construction. |
| R4 | P1 | UI sends have no stable operation ID. The optional backend ID is globally deduplicated within the steer producer, without fleet/workspace scope. | Client body, admission key, and SQL uniqueness. |
| R5 | P1 | GitHub review POSTs occur before the terminal report and outside the durable outbound obligation mechanism. Retried execution can duplicate public comments. | Fixture skill, ingress envelope, report transaction. |
| R6 | P1 | Normal lease consumption does not clear readiness marks. Ever-active idle fleets continue entering candidate selection. | Production call-site search and empty-read branch. |
| R7 | P1 | Install succeeds after best-effort grant creation; App routing requires an approved grant. A failed grant can leave an active fleet that never receives App events. | Install ordering and subscriber query. |
| R8 | P1 | Recovery duration, real host failover, and the external GitHub/UI journey lack release evidence in the audited documents. | Explicit documentation caveats; test scope inspection. |

P0 means block the stated end-to-end production promise. P1 means resolve before
broad unattended use. These are recommendations to Indy, not an assertion that a
production incident has occurred. Findings and proposed proofs are expanded below.

The immediate next agent should correct documentation using this report, preserve
the unresolved behavior as unresolved, and prepare separately scoped product
fixes. Rewording a guarantee does not repair a lost event or restore model context.

### 1.1 Audit boundary

- The email is a hypothetical persona. No account, installation, fleet, pull
  request, production database, or live deployment was inspected or modified.
- This is static analysis of the checkout at the base above. No application test,
  benchmark, browser walk, external review POST, or failure exercise was run.
- Source facts, inferred failure sequences, and missing evidence are identified
  separately. A test file's existence is not a passing test result.
- The local `github-pr-reviewer` fixture was read. The currently onboarded bundle
  and the live external template repository were not verified. Bundle-specific
  behavior below therefore describes the fixture, not an observed installation.
- The architecture scenario links to published quickstart and connector pages.
  Attempts to retrieve those pages through the web tool failed. Their content
  was not audited; this is not evidence that the public site is down. The next
  agent must inspect the matching branch in `~/Projects/docs` independently.
- Only this report is authored. Product code, canonical documentation, and
  deployment state are unchanged by the audit.

## 2. Identities, assumptions, and the actual topology

### 2.1 Symbols used throughout

```text
U   = Clerk subject for nkishore@megam.io; email is a profile attribute
T   = tenant created for U
W   = default workspace belonging to T
I   = GitHub App installation connected to W
R   = a repository accessible to I, e.g. megam/example-service
P   = a pull request in R, e.g. #42, with head commit H
F1  = installed fleet UUID; display name github-pr-reviewer or generated suffix
F2  = a DIFFERENT installed fleet UUID, even if from the same template
E   = logical agentsfleet event ID from the Postgres admission
Q   = physical Dragonfly stream receipt returned by XADD
L   = one runner lease UUID for one attempt to execute E
N   = monotonically increasing fleet fencing token
```

An installed fleet is a durable configuration and execution boundary. It is not
one operating-system process. One user can own two fleets; one runner host can
execute both through separate worker slots. A second attempt can have a new L
and N while retaining the same `(F, E)`.

The GitHub installation ID, GitHub delivery header, raw-body digest, admission
row ID, logical E, physical Q, lease L, and fence N must remain distinguishable
in the docs and diagnostics. In particular, E is not generally Q after replay.

### 2.2 Deployment and service map

```text
 nkishore's browser             GitHub
      |                          |  user authorization / App installation
      | Clerk sign-up            |  signed pull_request webhook
      v                          v
 Next.js UI/proxy ----------> agentsfleetd (Rust)
      ^                       |     |       |          |
      | SSE + history         |     |       |          +--> GitHub token APIs
      |                       |     |       +------------> bundle object store
      |                       |     +--------------------> Dragonfly cluster
      |                       +--------------------------> Postgres
      |                                  ^
      +----------------------------------| activity hub
                                         |
                           authenticated HTTP runner plane
                                         |
                            agentsfleet-runner (Zig host)
                              | heartbeat/control thread
                              | fixed worker pool
                              +-- worker A --> sandbox child for (F1,E1,L1)
                              +-- worker B --> sandbox child for (F2,E2,L2)
                                                  |
                                           model + tool calls
                                                  |
                                             GitHub REST API
```

SSE means Server-Sent Events. The UI receives activity through authenticated
same-origin Next.js routes; browser code does not directly consume Dragonfly.
The runner also uses daemon APIs, not direct Postgres/Dragonfly access.

The target recorded in `docs/architecture/datastore_scaling.md:29` is four
Dragonfly processes, two primaries and two replicas, in one region. Its chosen
layout at line 90 is two cross-paired containers:

```text
failure domain A                         failure domain B
+---------------------------+            +---------------------------+
| primary A | replica of B  |            | primary B | replica of A  |
+---------------------------+            +---------------------------+
```

Separate containers on the same host would still share a host failure domain.
Actual placement, storage, restart policy, controller, and replication lag need
deployment evidence. The local four-node rig shares one container/network
namespace and does not demonstrate host or production DNS survival
(`docs/architecture/datastore_scaling.md:103`).

The transport is redis-rs `cluster_async` over RESP3, configured by
`DRAGONFLY_URL`; this does not make Redis a supported backend. Postgres holds
accepted work, results, leases, and money. Dragonfly provides delivery,
readiness, live activity, and temporary state. That intended division needs the
R1 repair before it justifies an unconditional "no work lost" claim.

### 2.3 Preconditions the UX must make visible

Before the first review can work:

1. The platform has configured the shared GitHub App and its secrets, webhook
   ingress, browser callback, repository permissions, and subscriptions.
2. The platform or tenant has onboarded the reviewer library and its immutable
   support archive. The user's install selects an existing library entry.
3. The App is already installed on R and U can access the installation. Current
   Rust connect flow does not continue to an App installation page when none
   exists. `app_slug` is not used for that continuation.
4. W has a usable GitHub connector handle; F1 has an approved GitHub integration
   grant; both inbound and outbound repository lists contain R.
5. A compatible registered runner has capacity. Model/provider configuration,
   wallet/budget gates, and network policy permit the run.

"Connected", "active", "runner available", "review posted", and "answer saved"
are five different facts. A green active tile alone proves none of the last three.

## 3. One fleet: browser registration and both GitHub callbacks

There are TWO callbacks with different authentication and consequences:

```text
Browser authorization return:
  GitHub -> GET UI /api/connectors/github/callback?code=...&state=...
         -> authenticated POST daemon /v1/connectors/github/callback?...
         -> save workspace connection

PR event delivery:
  GitHub -> signed POST daemon /v1/ingress/github
         -> route installation + repository + event to F1
         -> admit event and enqueue execution
```

The first callback does not enqueue a review. The second does not create a user
or establish a browser session.

### 3.1 Sign-up and workspace entry

| Order | Caller and API/action | Durable writes and observed implementation |
|---|---|---|
| A1 | Browser uses Clerk sign-up/sign-in UI. | Identity/session lives with the identity provider; no invented agentsfleet registration POST is needed. |
| A2 | Clerk sends `POST /v1/auth/identity-events/clerk`. | Handler verifies the signed identity event and handles `user.created`. |
| A3 | Signup service provisions U. | One Postgres transaction creates `core.tenants`, `core.users`, `core.memberships`, `core.workspaces`, and `billing.tenant_wallet`. |
| A4 | Daemon updates identity metadata for the tenant. | Separate provider operation; do not treat it as part of the Postgres transaction. |
| A5 | Signed-in UI resolves workspace with `GET /v1/tenants/me/workspaces?limit=1`, then normal workspace/list reads. | Reads membership/workspace state. Full workspace picker walks cursor pages. |

The signup implementation grants starter credit of 5 dollars in its integer
money unit. Signup replay repairs a missing wallet with conflict-safe insert;
it does not replenish an existing wallet. Email is not the authorization key:
the external subject and tenant/workspace membership are.

Evidence: `rustd/crates/afd_tenant/src/signup.rs:73`, `:168`, `:193`;
`rustd/crates/afd_api_ingress/src/handler/webhook/identity_route.rs:150`;
`ui/packages/app/lib/api/workspaces.ts:153`.

UX requirement for documentation: distinguish "Clerk session exists" from
"workspace provisioning and tenant claims are usable". A delayed identity
webhook or failed metadata update needs an actionable provisioning/retry state.
This audit did not measure that delay or execute metadata repair.

### 3.2 Connect GitHub once for W

| Order | API/action | Postgres / Dragonfly / external effect |
|---|---|---|
| B1 | UI reads `GET /v1/workspaces/W/connectors/github` or the connector list. | Reads connector status. |
| B2 | UI action calls `POST /v1/workspaces/W/connectors/github/connect`. | Reads platform App credentials. Creates signed state bound to W and U. |
| B3 | Daemon remembers state nonce, then returns authorization URL. | Dragonfly key `connect:gh:nonce:<nonce>`, 600-second lifetime. No fleet event. |
| B4 | Browser visits GitHub user authorization URL. | U authorizes the App. This requires an accessible existing App installation. |
| B5 | GitHub returns browser to `GET /api/connectors/github/callback` with code/state and optional installation ID. | Next.js obtains U's bearer token. |
| B6 | Next.js forwards `POST /v1/connectors/github/callback` with original query and bearer. | Daemon verifies signed state, caller binding, workspace access, and consumes nonce BEFORE provider exchange. |
| B7 | Daemon exchanges the one-time code with GitHub. | User access token proves U; it is not itself the fleet's installation token. |
| B8 | Daemon resolves installation ownership. | With no claimed ID: `GET https://api.github.com/user/installations?per_page=2`. With a claim: probe that installation's repositories to prove access. Zero/several/unreachable claimed installations refuse. |
| B9 | Daemon seals connection and routes I to W. | ONE transaction writes `vault.secrets` under workspace key `github` and claims `core.connector_installs`. Other installation routes for this provider/workspace are released in that transaction. |
| B10 | Daemon returns redirect; Next.js checks same origin and redirects browser. | UI can re-read connector status. |

The nonce prefix is specifically `connect:gh:nonce:`, not `connect:github:nonce:`.
Nonce consumption prevents callback replay; it also means a later exchange/DB
failure requires a fresh connect attempt. Losing Dragonfly during authorization
can invalidate that pending attempt. Completed connections survive in Postgres.

One GitHub installation cannot be claimed by two workspaces: the conditional
upsert refuses another owner. A second fleet in W reuses I. Connecting a
different installation in W replaces the workspace's provider connection and
can affect BOTH fleets; it is not a per-fleet connector switch.

Evidence: `ui/packages/app/lib/api/connectors.ts:47`;
`ui/packages/app/app/api/connectors/[provider]/callback/route.ts:26`;
`rustd/crates/afd_connector/src/connect.rs:154`;
`rustd/crates/afd_connector/src/state/nonce.rs:55`;
`rustd/crates/afd_connector/src/registry.rs:55`, `:309`;
`rustd/crates/afd_connector/src/complete.rs:90`, `:175`;
`rustd/crates/afd_connector/src/github/probe.rs:36`;
`rustd/crates/afd_connector/src/grant.rs:161`;
`rustd/crates/afd_connector/src/sql.rs:33`, `:61`.

The callback proxy currently forwards a non-302 upstream response body. The docs
should describe recovery from a refused callback, expired state, missing browser
session, and ambiguous installation. Do not promise an installation chooser or
an automatic install-page continuation that the inspected path does not provide.

### 3.3 Install github-pr-reviewer

| Order | API/action | Data effect |
|---|---|---|
| C1 | UI browses `GET /v1/workspaces/W/fleet-libraries`. | Reads platform/tenant library rows and requirements. |
| C2 | Install surface checks required credentials; creation sends `POST /v1/workspaces/W/fleets` with `{platform_library_id:"github-pr-reviewer"}`. | Current UI sends the library selector, not repository bindings or a stable install operation ID. |
| C3 | Daemon resolves stored library, validates documents, hashes/snapshot identity and credential declarations. | Reads library and workspace state; install does not fetch a new GitHub tarball. |
| C4 | Daemon inserts F1 as `installing`. | `core.fleets`: tenant/workspace IDs, name, installed SKILL/TRIGGER, bundle hash/snapshot, configuration/status. |
| C5 | Daemon ensures F1 stream consumer group, with bounded backoff. | `XGROUP CREATE` for `fleet:F1:events`, group `fleet_lease`; empty stream may be created here. |
| C6 | Daemon activates F1. | Updates `core.fleets.status` to `active`. |
| C7 | Daemon requests declared integration grants. | Best-effort creation of approved `core.integration_grants` for F1/github; failures are logged. |
| C8 | Daemon returns 201 with installed fleet information. | Current successful pipeline has already activated F1. |
| C9 | UI shows install ladder and reconciles status. | Opens fleet SSE and polls fleet list, 12 attempts, delays capped at 5 seconds; reads active status as completion. |
| C10 | U opens F1 and configures R in its TRIGGER document if the template does not already bind the intended repo. | `PATCH /v1/workspaces/W/fleets/F1` updates live configuration using its concurrency/version mechanism. |

There are two repository declarations. The nested webhook trigger repository
list controls which App events reach F1. The top-level repository list and
`repository_access: write` bound runtime GitHub tokens. Review comments need
write permission. Copying only the inbound list can produce a fleet that wakes
but cannot post. Copying only the outbound list can produce a fleet that never
wakes. The fixture is bound to `agentsfleet/linkwarden`; that must not silently
become the persona's expected repository.

Evidence: `rustd/crates/afd_fleet_lifecycle/src/install.rs:200`, `:238`, `:266`;
`rustd/crates/afd_fleet_lifecycle/src/install/grants.rs:38`;
`rustd/crates/afd_fleet_lifecycle/src/install/row.rs:31`;
`ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/InstallStates.tsx:43`;
`ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/InstallStreamSteps.tsx:25`;
`tests/fixtures/fleetbundle/github-pr-reviewer/TRIGGER.md:3`.

Two install hazards need explicit handling:

- R7: active status does not prove the approved grant was persisted. App subscriber
  selection requires that grant, so "the first event will ask for the grant"
  cannot heal an App event that is filtered out before admission.
- A hard daemon exit after C4 bypasses ordinary error rollback. An `installing`
  row can survive. No installing-row repair was found in the inspected sweeper
  wiring. An ambiguous C8 response followed by a fresh default-name install can
  create another fleet. A second install is not inherently a retry of F1.

The UI still has vocabulary for synthetic `install:*` frames that the current
server does not emit. Its status reconciliation is the fallback that makes the
normal path usable. It lists only the first 100 fleets rather than directly
reading F1; at larger workspace sizes a missing first-page result can become a
false error. Source: `ui/packages/app/lib/api/events-types.ts:49` and
`InstallStreamSteps.tsx:41` (full path above).

## 4. One fleet: PR admission, queueing, lease, execution, report

### 4.1 Signed PR delivery and durable acceptance

```text
GitHub        daemon ingress        Postgres                 Dragonfly
  | signed PR      |                   |                        |
  |--------------->| verify signature  |                        |
  |                | resolve I -> W    |                        |
  |                |------------------>| connector_installs     |
  |                | find F1           | fleets + grants        |
  |                |------------------>|                        |
  |                | normalize PR      |                        |
  |                | INSERT admission  |                        |
  |                |------------------>| COMMIT E               |
  |                | XADD normalized event + logical E -------->|
  |                |<----------------------------------------- Q|
  |                | save receipt Q    |                        |
  |                |------------------>|                        |
  |                | HSET ready partition field F1 ------------>|
  |<---------------| 202 matched/enqueued                        |
```

Ingress endpoint: `POST /v1/ingress/github`. Signature verification precedes
routing from payload fields. Installation maps to W; eligible subscribers are
active fleets with an approved GitHub grant and matching repository/event.
Unmapped installation or no subscriber can deliberately return ignored 200.
That is not evidence that a review was queued.

The event body is a normalized flat PR digest: `repo`, `number`, `action`, title,
URL, state, draft, author, head/base refs, head SHA, and receive time. It is not
the full raw GitHub webhook. Source:
`rustd/crates/afd_api_ingress/src/handler/webhook/github.rs:202`.

For App delivery the producer key is `F1:<sha256 of signed raw body>`. The
producer is `WebhookApp`; the header delivery ID is not trusted as that key.
`core.fleet_admissions` commits before XADD. It stores the source identity,
scope, normalized request, logical identity ingredients, receipt, delivered
marker, and replay counters. A repeated body for F1 returns the original E.
F2 has its own key and admission for the same GitHub delivery.

If XADD fails after durable acceptance, the admission can remain without a
receipt for replay. If XADD succeeds but receipt persistence fails, the caller
can see an error even though work is durable or already queued. Logical
deduplication is essential to recovering that ambiguity.

`core.fleet_events` is not inserted by webhook ingress. It is opened later on
lease delivery. Thus "accepted" can precede visible event history. Docs/UI need
to distinguish admission backlog from a running or completed event.

Evidence: `rustd/crates/afd_api_ingress/src/handler/webhook/app_route.rs:210`,
`:255`, `:281`; `rustd/crates/afd_ingress/src/app.rs:137`, `:177`;
`rustd/crates/afd_ingress/src/deliver.rs:104`;
`rustd/crates/afd_admission/src/admit.rs:126`;
`schema/910_fleet_admissions.sql:65`.

The optional manual hook is `POST /v1/webhooks/F1/github`, with a different
secret/routing path and producer identity. Configuring both manual and App hooks
for the same PR can legitimately create two admissions. Do not present the two
routes as interchangeable retries.

App ingress accepts PR actions beyond opened/reopened/synchronize/ready-for-review;
the four-action filter is specific to the manual route. Repair branches are
filtered. A reviewer must explicitly decide how drafts, labels, edits, closure,
and rapid successive heads should be handled. Current fixture prose is not a
verified action/coalescing policy.

### 4.2 Runner selection and leasing in detail

Runner API authentication uses an operator-provisioned runner token (`agt_r`
plane). Fleet installation does not register or spawn a dedicated runner host.

| Order | API/internal operation | State and consequence |
|---|---|---|
| D1 | Host sends `POST /v1/runners/me/heartbeats`. | Updates runner liveness/capabilities and receives assignment policy. Control thread runs independently of workers. |
| D2 | A free worker sends `POST /v1/runners/me/leases`. | Daemon rotates one of 16 readiness partitions and peeks a bounded set of up to 64 fleet IDs. |
| D3 | Daemon resolves eligible ready fleets. | Postgres filters active status and runner tags; sticky previous-runner preference participates in ordering. |
| D4 | Daemon claims F1 affinity. | Conditional upsert in `fleet.runner_affinity`; only expired/free ownership can be won; fencing sequence advances. |
| D5 | Daemon first checks for a previous active but lapsed lease. | Reclaim can reconstruct work from `fleet.runner_leases` joined to `core.fleet_events` even if the stream has disappeared. |
| D6 | Otherwise it reads own pending entries, then new entries. | Nonblocking `XREADGROUP`, group `fleet_lease`, count 1. Consumer is daemon-side `agentsfleetd-<pid>`, not the runner ID. |
| D7 | Daemon resolves installed source/session, records received event, stamps admission delivered. | Writes `core.fleet_events`, updates `core.fleet_admissions.delivered_at`, updates event counters through schema machinery. These precede lease issuance. |
| D8 | It evaluates gates, billing posture, provider/model, credentials and policy. | Reads/configures budgets and permissions; can refuse or park without a runner execution. Terminal redelivery is suppressed before these effects. |
| D9 | It issues the lease LAST. | Inserts `fleet.runner_leases`, runner audit event and lifetime counters; lease binds F1/E/Q/U's scope, runner, fence, expiry, model/posture. |
| D10 | Returns lease payload. | Includes L, N, expiry, current event, instructions, policy, secret-delivery information and optional bundle content hash. |

The intended concurrency unit is one active valid lease per fleet. Two daemon
replicas and multiple workers can contend, but Postgres affinity/fencing chooses
the owner. Sticky assignment is an optimization, not a durable session tied to
one machine. After expiry a higher fence supersedes the earlier attempt.

The lease lasts 30 seconds, renew window is 10 seconds, renewal tick is 5 seconds,
maximum run duration is 12 hours, heartbeat interval is 10 seconds, and runner
offline threshold is 90 seconds. These are code defaults/bounds, not measured
recovery times. Evidence: `rustd/crates/afd_core/src/timing.rs:27`.

The parent renews through `POST /v1/runners/me/leases/L/renew` while the child
runs. Renewal extends the valid lease and its affinity within the maximum
runtime. Heartbeat liveness and lease renewal are separate operations. An
old child can still exist physically during a network failure; fencing prevents
its stale durable settlement, not an already-issued external GitHub request.

An idle lease poll returns HTTP 200 with `lease:null` and a retry delay, not a
204 or an indefinitely blocked stream read. A ready response contains one lease
for that worker. The SQL claim tests `leased_until < now`; release is guarded
by the current fencing sequence so an older holder cannot release a newer
holder's slot. Lease issue records its audit/counter changes in the same SQL
statement. This atomic issue does not include the earlier received-event and
delivered-admission writes described in R1.

Evidence: `src/runner/daemon/loop.zig:115`, `:256`;
`src/runner/daemon/worker_pool.zig:1`;
`rustd/crates/afd_fleet/src/lease/assign.rs:104`, `:218`;
`rustd/crates/afd_fleet/src/lease/sql/lease.rs:41`, `:89`, `:274`;
`rustd/crates/afd_fleet/src/lease/pull.rs:176`;
`rustd/crates/afd_fleet/src/lease/event.rs:105`;
`rustd/crates/afd_fleet/src/lease/deliver.rs:133`.

Additional response evidence: `rustd/crates/afd_wire/src/lease.rs:71` and
`rustd/crates/afd_fleet/src/lease/answer.rs:55`.

### 4.3 Worker, child, credentials, and GitHub effects

```text
runner host
  control thread ---- heartbeat / assignment
  worker slot ------- obtains (F1,E,L,N)
       |
       +--> fetch support bundle via daemon proxy, when required
       +--> GET fleet memory via daemon
       +--> materialize isolated lease workspace
       +--> launch sandboxed child with current event + installed instructions
       |       |
       |       +--> model produces tool requests
       |       +--> token placeholder -> parent -> daemon credential broker
       |       +--> GET GitHub PR diff
       |       +--> POST GitHub review (external effect happens HERE)
       |       +--> output / activity / memory frames
       |
       +--> renew lease and forward activity while child executes
       +--> serialize terminal report; attempt local spool write
       +--> POST terminal report to daemon
       +--> release spool entry after successful report response
```

Runner calls for this execution:

| Call | Purpose | Persistent state / boundary |
|---|---|---|
| `GET /v1/runners/me/bundles/<content_hash>` | Fetch immutable support archive through daemon. | Object store content; not a new fleet install or direct runner R2 credential. |
| `GET /v1/runners/me/memory/F1` | Hydrate fleet memory. | Reads `memory.memory_entries` scoped to F1. Current failure handling continues with empty memory. |
| `POST /v1/runners/me/credentials/mint` | Obtain short-lived scoped GitHub installation token on demand. | Broker reads grant/handle and signs App request; private App key stays daemon-side. |
| `POST /v1/runners/me/leases/L/activity` | Forward tool/progress/output frames. | Best-effort activity publication; not a durable full transcript. |
| `POST /v1/runners/me/memory/F1` | Capture explicit memory entries with lease/fence validation. | Writes fleet-scoped memory. Capture is a separate operation, not the report transaction. |
| `POST /v1/runners/me/leases/L/renew` | Keep this attempt valid. | Lease/affinity and metering updates. |
| `POST /v1/runners/me/reports` | Settle final result. | Fenced Postgres transaction; ACK follows commit. |

The credential broker exchanges App authentication at GitHub's installation-token
endpoint (`POST /app/installations/I/access_tokens`). Repository binding and the
approved F1/github grant constrain that request. The child does not receive the
platform App private key. This audit did not exercise the provider's effective
permission response or a token revocation race.

The inspected fixture calls generic `http_request`:

```text
GET  https://api.github.com/repos/R/pulls/P
     Accept: application/vnd.github.diff

POST https://api.github.com/repos/R/pulls/P/reviews
     event: COMMENT
     comments: [{path, line, ...}, ...]
```

It does not instruct the fleet to merge, approve, push, or close the PR. It also
does not use the scenario document's `/pulls/P/files` example. The actual
onboarded SKILL must be captured in any live acceptance evidence.

Evidence: `tests/fixtures/fleetbundle/github-pr-reviewer/SKILL.md:31`;
`rustd/crates/afd_credential/src/credential/github.rs`;
`src/runner/daemon/lease_run.zig:125`;
`src/runner/daemon/control_plane_client.zig:125`;
`src/runner/engine/inrun_memory.zig:105`;
`src/runner/daemon/lease_run_report.zig:38`;
`src/runner/daemon/ReportSpool.zig:100`.

Process isolation needs a deployment-specific check. The parent establishes
the child control-group boundary before execution and can kill/reap the child;
the mandatory Linux hardening sequence includes no-new-privileges, Landlock and
seccomp. The inspected supervisor has a development `dev_none` tier and network
modes that differ materially: deny-all, shared-network allow-all, and a strict
allowlist mode that currently refuses because its per-lease kernel setup is
unimplemented. Do not describe the tool's declared host allowlist as proven
kernel network isolation. Verify the selected production tier and effective
tool/credential restrictions before claiming isolation for untrusted PR content.
The fixture's "comment only" instructions are also model instructions; a complete
audit of every permitted GitHub HTTP method was outside this journey review.
Evidence: `src/runner/child_supervisor.zig:93`, `:124`;
`src/runner/sandbox_hardening.zig:29`.

Memory capture is best effort in the child/parent path. A successful local
`memory_store` tool call must not be assumed to prove a remote Postgres commit.
Test dropped memory frames and failed capture POSTs before promising "remembered
for next time". The checkpoint described next is not a substitute for those
explicit memory entries.

### 4.4 Terminal report and what is actually durable

The report transaction fences/settles the lease and money, marks the event
terminal with final output, replaces the fleet session checkpoint, releases
affinity, and conditionally creates a reply obligation. After commit the daemon
ACKs physical Q, performs safe trim, and publishes completion/audit activity.

```text
BEGIN Postgres
  fence and settle L; reconcile billing.usage_ledger / tenant wallet
  UPDATE core.fleet_events(F1,E) -> terminal result + final response
  UPSERT core.fleet_sessions(F1) -> latest checkpoint
  release fleet.runner_affinity through fence N
  if explicit reply destination exists: INSERT core.fleet_obligations
COMMIT
  XACK fleet:F1:events fleet_lease Q
  safe trim acknowledged history
  SPUBLISH fleet:F1:activity event_complete
```

Both App webhook and UI steer admissions use `Reply::None`. Consequently the
ordinary reviewer/chat journey does NOT create `core.fleet_obligations` or send
the review through `connector:outbound`. That durable outbox serves other reply
surfaces, such as a connector thread destination. It does not protect the
generic tool's earlier GitHub POST.

A repeated accepted report is recognized as already settled; retry can re-ACK
without repeating the settlement. If Dragonfly dies after commit, the final
answer and bill remain in Postgres. Lost activity can be repaired from event
history, but individual vanished tool/progress frames are not reconstructed.

Session checkpoint shape is `{last_event_id,last_response}`; response is capped
at 2,048 bytes. It is one latest checkpoint per fleet, not a conversation table.
The full event response belongs in `core.fleet_events`.

Evidence: `rustd/crates/afd_fleet/src/lease/commit.rs:136`;
`rustd/crates/afd_fleet/src/lease/report.rs:116`;
`rustd/crates/afd_fleet/src/lease/finalize.rs:144`, `:182`;
`rustd/crates/afd_fleet/src/lease/sql/session.rs:17`;
`rustd/crates/afd_ingress/src/deliver.rs:118`;
`rustd/crates/afd_events/src/steer.rs:105`.

## 5. One fleet: UI steering and follow-up questions

### 5.1 What the browser does

U opens F1, reads the review and asks:

```text
Q1: "Explain the race condition you reported on PR #42."
Q2: "Remember that our review policy requires a regression test for races."
```

The main data APIs are:

| Action | Browser/server API path | Data source |
|---|---|---|
| Fleet detail/configuration | `GET /v1/workspaces/W/fleets/F1` | `core.fleets` and projections. |
| Thread history | `GET /v1/workspaces/W/fleets/F1/messages` | Keyset history of `core.fleet_events`; includes relevant non-chat events, not only steer messages. |
| Event list/backfill | `GET /v1/workspaces/W/fleets/F1/events` | Durable event rows; cursor/since filters. |
| Full event detail | `GET /v1/workspaces/W/fleets/F1/events/E` | Full request/response for the event. |
| Live fleet tail | `GET /live/v1/workspaces/W/fleets/F1/events/stream` in UI, proxied to `/v1/...` | Dragonfly sharded pub/sub through daemon subscription hub. |
| Send Q1 or Q2 | UI server action -> `POST /v1/workspaces/W/fleets/F1/messages` | New durable steer admission, then the same F1 stream. |
| Workspace wall | `/live/v1/workspaces/W/events/stream` | One multiplexed workspace stream; frames carry fleet identity. |

POST body from the current UI is `{message:"..."}`. The backend accepts optional
`operation_id`, validates message size (8,192 bytes), checks workspace/fleet
ownership and active status, and uses a `steer:<subject>` actor for the browser.
Paused/non-active fleet sends can be refused; a typed message is not proof of
acceptance. Successful admission returns the canonical E for reconciliation.

The browser inserts an optimistic local row and serializes sends within that
mounted delivery hook. On success it reconciles to E; on uncertain failure it
offers retry. This local Promise chain does not order GitHub webhooks, other
tabs, another user's messages, or work submitted by another client.

The steer goes into `fleet:F1:events`, behind other pending F1 work under the
same lease rules. It is a new execution. The inspected message path does not
inject text into an already-running child's model context, cancel that child,
or modify the currently executing review in place. Docs must define "steer"
accordingly and expose when the message is queued.

Evidence: `ui/packages/app/lib/api/fleets.ts:155`;
`ui/packages/app/components/domain/useFleetMessageDelivery.ts:40`;
`ui/packages/app/components/domain/FleetThread.tsx:85`;
`ui/packages/app/lib/api/events.ts:90`, `:127`, `:146`;
`ui/packages/app/lib/api/events-types.ts:67`;
`rustd/crates/afd_api_tenant/src/handler/fleet/message.rs:193`, `:267`, `:292`;
`rustd/crates/afd_events/src/history/statement.rs:134`.

### 5.2 R3: visible history is not reliable model context

```text
What U sees                              What the new child receives
------------------------------------     -----------------------------------
PR event and completed review            current message Q1
previous questions and responses         installed SKILL instructions
the new optimistic message               configured policy and explicit memory

Postgres session checkpoint -- read by daemon --X--> not in LeasePayload
```

`resolve_installed` reads `context_json`, but `answer.rs` does not serialize it
into the lease. The wire payload has no checkpoint/history field. The child
constructs input from the current event's `message` or event JSON and the
installed instructions. The prompt helper adds selected optional context fields
and memory, not the saved `last_response` or UI transcript.

Evidence chain:

- `rustd/crates/afd_fleet/src/lease/installed.rs:100`
- `rustd/crates/afd_fleet/src/lease/answer.rs:67`
- `rustd/crates/afd_wire/src/lease.rs:45`
- `src/runner/child_exec_input.zig:94`
- `src/runner/engine/runner_helpers.zig:286`

The fixture explicitly says a steer without a PR event is chat and should not
fetch a diff or post a review. Its memory instructions retain concise explicit
facts, not transcripts (`SKILL.md:48`, full fixture path in section 4). Therefore
Q1's reference to "the race condition you reported" is not reliably grounded.
Q2 may be remembered through explicit memory tools, subject to capture/hydration
failure, but that is a different guarantee.

Next product decision: bind follow-up to a selected `(F1,E,repo,PR,head SHA)` and
provide a bounded, documented context input. Decide whether "ask" is read-only
discussion and "review again" is a separate visible external action. Do not
silently infer the latest PR in a busy multi-PR fleet.

Required proof: complete two different PR reviews on F1, ask about the older one
after a runner handoff, and verify the response cites the correct prior finding
and commit without issuing an unintended GitHub POST.

### 5.3 R4: retry identity has two distinct defects

1. UI omits `operation_id`; `Key::Unrepeatable` creates a new admission identity.
   If the first POST committed but the response was lost, manual retry can run
   and charge the same intended question twice. The optimistic UI ID alone does
   not prevent server duplication. Do not claim every failed POST is automatically
   retried; transport retry classification and manual retry are distinct.
2. A client that DOES send an operation ID uses `Key::Repeated(operation)` without
   F/W scope. SQL uniqueness is only `(producer, producer_key)`. Sending `op-1`
   to F1 and then F2 can suppress F2 and return F1's original E. The same namespace
   extends across workspaces/tenants. Payload mismatch logs a warning; it does
   not turn reuse into a scoped conflict response.

Evidence: `rustd/crates/afd_events/src/steer.rs:79`;
`rustd/crates/afd_admission/src/sql.rs:53`;
`rustd/crates/afd_admission/src/admit.rs:126`;
`schema/910_fleet_admissions.sql:83`.

Recommended proof/fix scope: stable client ID retained across retry and reload;
server key bound to workspace/fleet and defined actor semantics; different
payload on the same operation refused clearly; accepted-operation lookup so an
ambiguous send is reconciled before another execution. Reproduce both same-fleet
duplicate retry and cross-fleet/cross-tenant ID collision. The code supports the
suppression finding; this audit does not claim disclosure of another fleet's
full response.

### 5.4 Live output, reload, and errors

Activity uses `SPUBLISH`/`SSUBSCRIBE`. It is ephemeral. Event list/history/detail
reads are durable. Browser reconnect/backfill can recover event status and final
response, not every lost intermediate tool frame. The hub and client queues are
bounded; completion reconciliation matters more than a perfect animation.

Report-spool replay improves recovery after a report transport failure. It is
runner-local, may be unavailable, and does not recover a host disk that is gone.
Spooling happens after child execution, so it cannot undo uncertainty about a
GitHub POST sent just before a child/host crash. Power-loss durability of the
file/rename sequence was not established by this review.

UX documentation should give distinct labels for accepted/queued, executing,
waiting on a gate, disconnected live stream, failed execution, ambiguous send,
and completed result with a GitHub review URL. A connected stream is not evidence
that a GitHub review was successfully posted.

## 6. The same user with TWO fleets

Assume F1 and F2 both come from `github-pr-reviewer` in W. A second default-name
install may receive a generated numeric suffix; an explicit name collision is a
conflict. UUIDs, not display names or email, scope execution and history.

### 6.1 What is shared and what is independent

| Resource | Shared by F1/F2? | Consequence |
|---|---|---|
| U, T, membership, W | Yes | No second signup transaction. |
| Tenant wallet | Yes | Both spend from the same pool and can contend on billing writes. |
| GitHub App installation I and workspace `github` handle | Yes | One connect; disconnect/reconnect affects both fleets. |
| Library content hash / immutable support archive | Can be identical | No need to create two template archives. |
| `core.fleets` configuration | No | Two IDs, names, live SKILL/TRIGGER documents, status and budgets. |
| Integration grants | No | F1/github and F2/github are separate approvals. Revoking one need not revoke the other. |
| Admissions, events, checkpoints, memory | No | Rows are fleet-scoped; F2 does not inherit F1's review conversation. |
| Event stream and consumer group | No | `fleet:F1:events` and `fleet:F2:events`, each with `fleet_lease`. |
| Readiness hash | Possibly same partition | Each remains a distinct field; sharing a partition is normal. |
| Affinity/active lease/fencing sequence | No | One valid execution per fleet; two fleets can run simultaneously. |
| Runner host/worker pool | Can be shared | Two slots can execute both; one slot makes them compete. |
| GitHub installation rate budget, daemon pools, model capacity | Shared bottlenecks | Fleet separation alone is not resource isolation. |

### 6.2 Case A: both subscribe to the SAME repository and PR event

```text
GitHub PR #42, head H, signed body B
                    |
             POST /v1/ingress/github
                    |
          resolve W; subscribers = [F1,F2]
                    |
         sequential fan-out, not one transaction
             /                          \
   admission key F1:sha256(B)    admission key F2:sha256(B)
          E1, Q1                       E2, Q2
             |                          |
      fleet:F1:events             fleet:F2:events
             |                          |
       affinity F1                 affinity F2
         L1 / N1                     L2 / N2
             |                          |
      worker A / child A          worker B / child B
             |                          |
         GH review A                 GH review B
             |                          |
       event(F1,E1)                 event(F2,E2)
```

Both reviews are expected under the current subscription model. Deduplication
does not turn two subscribed fleets into one reviewer. Each can bill, emit
comments, store memory, and complete independently. A display name or differing
persona does not prevent duplicate-looking reviews on the same PR.

If F1's admission succeeds and F2 fails BEFORE admission commit, handler returns
an error after partial success. F1 may run; F2 has no durable work. An explicit
redelivery safely revisits F1 and creates F2's missing admission, but GitHub does
not perform that redelivery automatically. A response deadline expiring also
does not prove that all already-started server work was cancelled. Operator
reconciliation must inspect per-fleet acceptance.

U must open the intended fleet's thread to ask it a question. Sending Q1 to F1
does not steer F2. A workspace event wall aggregates visibility, not model
context or a shared inbox. Reusing one explicit operation ID across F1 and F2
currently encounters R4.

### 6.3 Case B: separate repository bindings

F1 subscribes to `megam/service-a`; F2 subscribes to `megam/service-b`. Each also
has the corresponding outbound permission list. A service-a PR yields only F1's
admission, and a service-b PR yields only F2's. Both can reuse I only if I covers
both repositories. A repository permission change in GitHub can invalidate the
ability to read/post without changing the fleet's stored active status.

If the requirement is two independent GitHub installations/accounts, current W
has one handle per provider. Connecting a second installation replaces that
provider connection. Use separate workspaces or design explicit multi-install
support; do not document two independent per-fleet GitHub connections in W.

### 6.4 Row and queue accounting for a clean illustrative run

These counts assume no gates, retries, continuations, failures, additional PR
actions, or pre-existing rows. They count logical successful work, not every SQL
statement, counter write, or transient activity frame.

| Resource after completion | One fleet: 1 PR + 2 questions | Two fleets: same PR to both + 1 question each |
|---|---:|---:|
| New fleet rows | 1 | 2 |
| GitHub connector installation/handle per W | 1 / 1 | 1 / 1 |
| Approved fleet/github grants | 1 | 2 |
| Admission rows | 3 | 4 |
| Event rows `(fleet_id,event_id)` | 3 | 4 |
| Lease attempts in the no-retry case | 3 | 4 |
| Latest session rows after reports | 1 | 2 |
| Affinity rows after first use | 1 | 2 |
| Fleet stream keys / groups | 1 / 1 | 2 / 2 |
| Terminal unacknowledged entries, healthy ACK path | 0 | 0 |
| Retained stream entries in this small example | Up to 3 | Up to 2 per fleet |
| GitHub review POSTs if each PR has findings | 1 | 2 |
| Outbound obligations for ordinary PR/chat admissions | 0 | 0 |
| Credit-pool ledger rows when both receive and stage charges apply | 6 | 8 |
| Explicit memory entries | Depends on tools | Depends on tools, independently per fleet |

One PR plus TWO follow-up questions PER fleet in the two-fleet case gives six
admissions/events instead of four. Lease retries add attempts, not new logical
events. Billing joins must use `(fleet_id,event_id)`, and ledger identity also
includes `charge_type`. Memory count is never "one per message" by definition.

## 7. Postgres state inventory and ownership

| Table | Writes in this journey | Identity / recovery meaning |
|---|---|---|
| `core.tenants` | Signup | T owns the billing pool. |
| `core.users` | Signup/profile | External identity subject represents U. |
| `core.memberships` | Signup | U's tenant authorization membership. |
| `core.workspaces` | Signup or explicit workspace creation | W belongs to T. |
| `billing.tenant_wallet` | Signup; receive/run debits and settlement | Shared tenant money; preserve with financial ledgers. |
| `vault.secrets` | GitHub connect/reconnect | Encrypted workspace handle; also platform/provider secrets as configured. Restore requires encryption keys, not just rows. |
| `core.connector_installs` | GitHub callback transaction | Exclusive provider/external installation -> W route. |
| `core.fleet_library`, `core.tenant_fleet_library` | Onboarding; read on install | Stored bundle documents/hash; onboarding is not repeated for each F. |
| `core.fleets` | Install, activate, configuration/status changes | F identity, scope, live instructions, bindings, immutable bundle pointer. |
| `core.integration_grants` | Install grant provisioning/revocation | Per F/service runtime permission and App subscriber eligibility. |
| `core.fleet_admissions` | Every accepted PR or steer | Durable source identity, request and delivery bookkeeping; replay source. |
| `core.fleet_events` | Receive, final report/refusal | Request/final answer/status; key `(F,E)`; source for UI and expired-lease recovery. |
| `core.fleet_activity_counters` | Event lifecycle trigger/projection updates | Aggregate UI counters, not the detailed work ledger. |
| `fleet.runners` | Operator provisioning and heartbeat | Runner authentication, capability, status and assignment data. |
| `fleet.runner_affinity` | Claim, renew, release | One row per F; fencing sequence and current/sticky ownership. |
| `fleet.runner_leases` | Issue, renew, settle/reclaim | One row per attempt L; carries E and physical receipt Q separately. |
| `fleet.runner_events` | Issue/release/liveness audit paths | Runner operational audit. |
| `fleet.runner_lifetime_counters` | Runner lifecycle accounting | Aggregate runner metrics. |
| `billing.usage_ledger` | Receive/stage charge and stage metering reconciliation | Unique `(event_id,charge_type,fleet_id)`; do not join on E alone. |
| `core.fleet_sessions` | Terminal checkpoint upsert | One latest bounded checkpoint per F; currently missing from lease payload. |
| `memory.memory_entries` | Explicit memory capture | Unique `(key,fleet_id)`; distinct from session and UI history. |
| `core.fleet_obligations` | Only if admission has explicit reply destination | No ordinary GitHub-reviewer/chat obligation in this scenario. |

Relevant schema evidence: `schema/300_vault_secrets.sql:30`,
`schema/500_fleets.sql:35`, `schema/510_fleet_sessions.sql:22`,
`schema/540_integration_grants.sql`, `schema/550_connector_installs.sql`,
`schema/600_runners.sql:50`, `schema/610_runner_leases.sql:50`,
`schema/630_runner_affinity.sql:50`, `schema/640_runner_events.sql:20`,
`schema/650_runner_lifetime_counters.sql:31`, `schema/710_usage_ledger.sql`,
`schema/800_fleet_events.sql:30`, `schema/820_memory_entries.sql:34`,
`schema/910_fleet_admissions.sql:65`, `schema/911_runner_leases_receipt.sql`,
`schema/913_fleet_obligations.sql:55`, `schema/916_usage_ledger_fleet_scoped_key.sql:92`.

There is no current `core.fleet_execution_telemetry` write in the traced report
path. Runtime metering uses the billing ledger. Do not copy the obsolete table
from an older data-flow diagram into the corrected journey.

## 8. Dragonfly inventory: exact purpose and loss behavior

In the following table F is replaced by its UUID. The braces in readiness keys
are literal cluster hash tags; stream and activity keys have no literal braces
around the fleet UUID.

| Key/channel | Structure and operations | Owner / loss consequence |
|---|---|---|
| `fleet:F:events` | Stream; XADD, XGROUP, XREADGROUP, XAUTOCLAIM, XACK, safe trim | One per installed fleet. Group `fleet_lease`; daemon consumes for runners. Rebuilt from eligible admissions and/or active lease records, subject to R1. |
| `fleet:ready:{0}` ... `fleet:ready:{15}` | Hash; fleet ID field -> readiness token; bounded peeks | Shared readiness index. Partition uses CRC16-XMODEM of fleet ID modulo 16. Producers/recovery mark; consumers should retire stale hints safely. |
| `fleet:F:activity` | Sharded pub/sub channel, SPUBLISH/SSUBSCRIBE | No persistent history or consumer ACK. Losing it loses intermediate live output. Final rows survive in Postgres. |
| `connect:gh:nonce:<nonce>` | Temporary single-use key, 600-second lifetime | Pending GitHub connect only; loss means restart authorization. |
| `connector:outbound` | Stream, group `connector_workers` | Shared connector reply delivery queue. Rebuilt from obligations for applicable surfaces; not the review POST path here. |

Other temporary auth/approval keys exist elsewhere. A deployment-wide flush has
a larger effect than these five rows. Inventory them in the operations runbook;
the architecture already admits pending single-use approvals are lost and must
be re-requested. This audit did not trace all auth flows.

There is no separate `fleet:F:steer` queue. PRs, steers, and other admitted event
types share the fleet stream. There is no runner-per-fleet queue or user-email
queue. A second fleet adds another stream/group and readiness field, not another
Dragonfly cluster or mandatory host process.

Retention avoids XADD MAXLEN cutting off undelivered work. The documented/current
design trims only behind safety bounds (group delivery position, oldest pending
entry, retained tail of 1,000). Admission limits protect against an indefinitely
growing live backlog: 10,000 per fleet and a global estimate of 100,000 in the
documented policy. These are admission/backpressure limits, not proof that
Postgres history, billing, and memory storage are bounded over months.

Evidence: `rustd/crates/afd_dragonfly/src/streams.rs:74`, `:120`, `:141`;
`rustd/crates/afd_dragonfly/src/ready/partition.rs:31`, `:118`, `:145`;
`rustd/crates/afd_dragonfly/src/streams/tail.rs:15`;
`docs/architecture/datastore_scaling.md:211`.

### 8.1 R6: ready does not currently mean still has work

The empty pending/new-read branch releases affinity and returns without clearing
the readiness field (`rustd/crates/afd_fleet/src/lease/assign.rs:226`). A search of
Rust production call sites found no consumer use of `clear_if_unchanged`; force
clear appears for pause/purge. Admission marking uses the fleet ID as token;
reclaim marking uses the consumer name. `ReadyIndex::mark` stores the supplied
token rather than minting a fresh generation.

Consequences inferred from those operations:

- After draining F1, F1 can keep being offered for idle DB/stream probes.
- As many fleets have ever run, this index tends toward the active population,
  weakening the documented idle-cost argument.
- The 64-candidate bound still bounds an individual poll; this is not an
  assertion that each poll scans every fleet.
- A future conditional-clear call alone is insufficient if the same token is
  reused across new work. It needs race-safe generation semantics.

Evidence: `rustd/crates/afd_dragonfly/src/ready.rs:146`;
`rustd/crates/afd_admission/src/admit.rs:327`;
`rustd/crates/afd_runner/src/sweep/reclaim.rs:234`;
`docs/architecture/scaling.md:174`.

Required proof: drain F1, verify its hint disappears, race a new admission with
the empty-read clear, and prove the new work remains discoverable. Then measure
idle polling with many previously used fleets, not only freshly empty fleets.

## 9. Dragonfly disappearing: recovery and permanent gaps

### 9.1 Recovery mechanisms present in source

```text
Admission has receipt NULL
  -> replay scan (age >= 30s)
  -> XADD same logical E, save new Q, mark ready

Admission has receipt Q, delivered_at NULL, but Q missing from stream
  -> reconcile probes Q
  -> guarded clear of lost receipt
  -> replay scan above

Expired active lease exists, even if stream is empty
  -> lease-ledger recovery marks fleet ready
  -> normal poll claims higher fence
  -> reclaim_prior_active reads event payload from Postgres
  -> new lease attempt for same E

Completed E reappears from an old snapshot / physical duplicate
  -> fleet_events terminal check
  -> ACK without gates/provider execution/second settlement
```

This is a sound basis for an at-least-once execution system with fenced durable
settlement. It is not a guarantee of one external tool side effect.

Current sweep pacing:

| Mechanism | Bound and interval from source | Practical limit |
|---|---|---|
| Admission replay | 32 rows/pass; minimum age 30 s; ordinary interval 30 s; clean full batches can continue sooner | Recovery depends on database/queue health and scan progress; 30 s is not an end-to-end service-level guarantee. |
| Lost-receipt reconcile | 128 fleets/pass; 32 rows per repairing fleet; remembered repair capacity 128; 300 s ordinary, 30 s after repairs | First detection can wait minutes; large population and long per-fleet backlog require many passes. |
| Pending/reclaim readiness | 100 fleets/pass; up to 10 claim attempts per fleet; 30 s interval; XAUTOCLAIM minimum pending idle 300 s; separate expired-active-lease scan | The five-minute stream eligibility is distinct from 30-second lease expiry. Population, cursor position and runner availability also matter. |

Evidence: `rustd/crates/afd_runner/src/sweep/replay.rs:36`;
`rustd/crates/afd_runner/src/sweep/reconcile.rs:42`;
`rustd/crates/afd_runner/src/sweep/reclaim.rs:51`;
`rustd/crates/afd_runner/src/sql/sweep.rs:173`;
`rustd/crates/afd_admission/src/sql.rs:111`, `:230`, `:270`;
`rustd/crates/afd_dragonfly/src/streams.rs:82`.

### 9.2 Failure matrix

| Failure point | Surviving evidence | Current expected behavior / gap |
|---|---|---|
| GitHub request never reaches daemon | GitHub delivery record only | No admission replay can help; external redelivery/reconciliation required. |
| DB refuses before admission commit | No accepted row | Error to sender; GitHub does not automatically retry. |
| F1 admitted, F2 not admitted in sequential fan-out | F1 row only | Partial execution; F2 requires delivery repair. R2. |
| Admission committed, no XADD receipt | Admission with NULL receipt | Replay can enqueue same logical E. |
| XADD succeeds, receipt write/HTTP response lost | Admission and perhaps physical entry | Replay/redelivery may create physical duplicates; logical identity must suppress duplicate completed execution/settlement. |
| Receipted, never delivered; stream lost | Undelivered admission | Reconcile clears missing Q then replay appends. |
| Received marker saved, no lease yet; group/stream lost | Received event + delivered admission, no active lease | R1: normal recovery predicates can miss it permanently. |
| Active lease exists; Dragonfly lost; runner healthy | Lease/event in Postgres | Runner can finish and report; ACK may have no entry. Activity disappears. Full availability also depends on daemon/API health. |
| Active lease exists; runner dies; Dragonfly lost | Expired active lease + event | Ledger scan re-marks; higher-fence lease reconstructs event from PG. |
| GH accepted review; child/host dies before reliable report | GitHub side effect, incomplete local settlement | Execution can repeat the POST. Local spool may not yet exist. R5. |
| Report committed; ACK or response lost | Terminal event, session, bill, settled lease | Report retry/terminal duplicate handling avoids new settlement; re-ACK is safe. |
| Activity hub/node fails | Final event rows survive; intermediate frames may not | Reconnect/history backfill restores terminal result, not complete tool transcript. |
| Pending OAuth state key lost | No reusable nonce | Start a fresh connect attempt; existing completed connection remains. |
| Restore an older Dragonfly snapshot | PG is newer; queue may contain old/missing entries | Suppress terminal entries, reconcile missing undelivered ones, reclaim active leases; R1 still needs proof. |
| Runner report spool host disk lost | Whatever already committed in PG; perhaps GH side effects | Local report retry protection is gone; do not promise output recovery from Dragonfly. |
| Postgres or its backup history is lost | Queue does not contain all authoritative state | This architecture cannot reconstruct full ownership, money, grants, results, and admissions from Dragonfly alone. Separate PG disaster recovery required. |
| Bundle object store or vault decryption keys lost | Rows may survive but cannot run/decrypt | Recovery requires objects and key material as well as database backups. |

### 9.3 R1: precise unprotected handoff to reproduce

```text
1. E admitted; receipt Q saved.
2. Daemon wins affinity and XREADGROUP receives E.
3. INSERT core.fleet_events(F,E,status=received) commits.
4. UPDATE core.fleet_admissions SET delivered_at=... commits.
5. Daemon dies BEFORE INSERT fleet.runner_leases.
6. Dragonfly loses the stream or consumer group.

What each repair asks:
  replay:     receipt IS NULL                         -> false
  reconcile:  receipt exists AND delivered_at IS NULL -> false
  lease scan: expired ACTIVE runner lease exists      -> false
  group seed: latest receipt joined to fleet_events   -> at/after E

Result inferred: E exists in PG as received, but no inspected normal repair
path makes it runnable after the lost queue/group. An affinity row alone is
not the expired-active-lease record queried by that scan.
```

Statements 3 and 4 are separate autocommit statements. `issue_ready` explicitly
issues the lease last, after other asynchronous work. The persisted event's
existence is also used to seed a recreated group's delivered cursor, without a
terminal-status requirement.

Evidence: `rustd/crates/afd_fleet/src/lease/event.rs:105`, `:122`;
`rustd/crates/afd_fleet/src/lease/deliver.rs:133`;
`rustd/crates/afd_admission/src/sql.rs:111`, `:156`, `:230`;
`rustd/crates/afd_runner/src/sql/sweep.rs:173`.

Recommended engineering work: define a recoverable state for every committed
handoff, couple the delivered marker to a durable execution owner or add an
orphan-received reconciliation path, and test each write boundary. Include the
shorter crash between event insert and delivered stamp, group-only loss,
stream-only loss, total loss, and a crash during reclaim of a prior attempt.
Preserve terminal-event suppression and monetary idempotency while repairing
the hole. Do not just re-enqueue every historical event.

### 9.4 R5: external reviews need their own durable identity

A higher lease fence protects agentsfleet's database. GitHub has not been shown
to reject a POST because the agentsfleet lease became stale. Body-digest webhook
deduplication also does not deduplicate two attempts inside one admitted event.

Recommended design decision: record review intent and observed GitHub review ID
against fleet/repository/PR/head and action identity, reconcile ambiguous POST
outcomes with the provider, and define when a new head supersedes a queued
review. Bind inline comments to the intended head. A marker alone is not proof
of race-free exactly-once behavior; test crash-after-POST and two valid recovery
attempts explicitly.

For two intentional reviewers, F must remain in that identity so one fleet does
not suppress the other's review. Also show the reviewer identity in the posted
result so the user can tell the two runs apart.

### 9.5 Rehydration runbook Indy should require

This is a recommended runbook outline, not a claim that a production command
already implements it:

1. Declare incident scope: one node, one shard, lost groups, stale snapshot,
   complete cluster loss, or concurrent PG loss. Preserve logs and identity maps.
2. Restore cluster topology and reachable announced addresses; verify both
   shards, replicas, replication lag and sharded pub/sub. Recovering data is not
   the same operation as promoting a node or fixing topology.
3. Keep PG, vault keys and bundle objects authoritative. Do not restore PG
   backwards merely to match an older Dragonfly snapshot.
4. Reconcile admissions with missing receipts/entries; drain replay; recreate
   groups with a proven safe cursor; re-mark active-lease work and unresolved
   received work once R1 is fixed.
5. Rebuild connector reply queues only for durable obligations that exist.
   Separately reconcile uncertain GitHub review effects and missing webhook
   deliveries; the generic outbound queue cannot stand in for either.
6. Reconnect activity subscriptions. Tell users that intermediate activity may
   be missing and reload terminal history. Ask users with lost pending approval
   or connect state to restart those operations.
7. Reconcile accepted counts, nonterminal age, leases, terminal outcomes, bills,
   and external review IDs per fleet. Prove F2 as well as F1 has drained.
8. Resume normal admission only with defined capacity and backlog criteria;
   document which operations were unavailable or refused during the incident.

`rustd/crates/afd_runner/src/sweep/rebuild.rs:90` provides a composition helper,
but its bounded rounds/counters are not a proof that every unfinished event was
recovered. No production CLI/API invocation of that helper was found in the
inspected daemon/CLI source. The docs currently call it an operator tool and
describe looping to zero changed; the next agent must identify an executable
entry point and an invariant-based completion check before publishing commands.

Recovery objectives must be chosen explicitly: maximum acknowledged work loss,
time to first resumed run, time to drain the backlog, and acceptable loss of
cosmetic activity. This audit provides no measured recovery-time or recovery-
point objective. PG backups, point-in-time restore, vault-key recovery, and
object-store retention need separate exercised evidence.

## 10. Scalability, concurrency, and performance review

### 10.1 Capacity is not the number of installed fleets

Let S be mean execution time in seconds, K available compatible worker slots,
and A runnable fleets. Ignoring other bottlenecks, concurrency is at most
`min(K,A)`. One busy fleet processes roughly `1/S` events/second because its
execution is serialized. Giving that fleet ten more runner slots does not
increase its single-fleet throughput under the current rule.

For two fleets, two free slots can run both. One slot serializes them across the
host. Long PR reviews delay queued questions on the same fleet. GitHub actions
arriving faster than completion can make backlog grow even if many unrelated
fleets are idle. Define head supersession/coalescing and interactive latency
before promising fast steering on a busy repository.

Order is scoped and qualified: stream append order, admission order, HTTP send
order, and GitHub creation time need not agree. Replay can append older work
later; pending/reclaim can precede new entries; GitHub delivery ordering must not
be assumed. There is no global order across F1 and F2. One mounted UI send chain
does not change those properties.

### 10.2 Specific pressure points

| Pressure point | Source observation | Recommendation / measurement |
|---|---|---|
| GitHub deadline | Sequential subscriber fan-out shares one request deadline; GitHub treats >10 s as failure. | Durable ingress envelope plus durable per-fleet fan-out progress; acknowledge promptly after a defined durable boundary; delivery reconciliation. |
| Subscriber selection | App resolver fetches all active/granted candidates in W, parses and filters in process; cap 100 is applied to matches afterward. | Measure many eligible configurations with few matches; index routable subscription facts or page/bound resolution without losing matches. |
| Ready index | 16 partitions, 64 IDs per poll; idle marks are not retired on ordinary consume. | Repair R6; measure distribution, selection latency, stale hints and fairness with real population history. |
| Partition rotation | One shared per-process cursor advances on polls. | Measure sparse single-fleet latency; one idle poll is not a scan of all partitions. Do not equate a 1 s idle retry with a 1 s dispatch guarantee. |
| Recovery scans | Bounded per-pass population and per-fleet work. | Define catch-up throughput and maximum tail age at target population, including PG/queue round trips and multiple replicas. |
| Shared wallet | F1/F2 share T's money and metering path. | Test concurrent near-zero balance, renew/report races, and budget enforcement; inspect lock wait and overdraft semantics before promising a hard spend ceiling. |
| Shared GitHub installation | Both fleets consume the same external installation resources. | Rate-limit/backoff metrics and visible provider refusal; avoid synchronized replay storms after outage. |
| DB pool | Ingress, polling, renewals, report, UI reads and recovery share finite service capacity. | Load combined traffic, prioritize keeping leases/report viable, and instrument pool wait separately from database outage. |
| Billing/history growth | Durable rows accumulate with events and attempts. | Measure retention/index/query plans; queue tail trim does not trim PG automatically. |
| SSE | Workspace wall multiplexes; fleet views and many viewers still fan out through hubs. | Test slow clients, disconnect storms, queue overflow and terminal backfill under node loss. |
| Runner memory | Hydration failure proceeds with empty memory; capture is separate best effort. | Make degraded context visible and define which requests should fail rather than answer without necessary history. |
| Install status | UI searches first 100 fleets during reconciliation. | Use direct fleet read or complete pagination; test success when F is outside first page. |

Source: `rustd/crates/afd_ingress/src/app.rs:137`;
`rustd/crates/afd_ingress/src/sql.rs`;
`rustd/crates/afd_api_ingress/src/handler/webhook/app_route.rs:258`;
`rustd/crates/afd_fleet/src/lease/sql/lease.rs:274`;
`rustd/crates/afd_billing/src/sql.rs:33`;
`ui/packages/app/lib/api/events-types.ts:93`.

Illustrative arithmetic, NOT a benchmark: one recovery scanner visiting 100
fleets every 30 seconds needs about 3,000 seconds (50 minutes) to visit 10,000
fleets, excluding query time and retries. That applies to the scan population,
not every incident: the separate expired-lease scan visits fleets with expired
active leases. Reconcile's 128-fleet and 32-row limits produce different bounds.
Multiple daemon replicas may overlap scan work rather than divide it perfectly;
do not multiply throughput by replica count without evidence.

The practical question for Indy is the maximum supported active backlog and
recovery time at that backlog. Fleet count alone does not answer it.

### 10.3 Self-hosted Dragonfly operations

Current official Dragonfly documentation says the server does not supply the
cluster control plane for health monitoring, automatic failover, and rebalancing.
That is an operator/controller responsibility in the selected self-hosted target.
See [Dragonfly cluster mode](https://www.dragonflydb.io/docs/managing-dragonfly/cluster-mode).

Its current persistence page says Append Only File (AOF) is unsupported. Snapshot
and replication policy therefore cannot be described as a Redis AOF guarantee.
See [Dragonfly AOF status](https://www.dragonflydb.io/docs/managing-dragonfly/aof).

Choose and exercise topology reconciliation, promotion, split-owner prevention,
consistent node configuration, announced hostname changes, and client reconnect
behavior. Rehydrating stream data does not implement these controls. Check the
deployed version and configuration before applying the current vendor docs to a
specific incident.

## 11. Documentation audit: exact corrections for the next agent

The table distinguishes documentation drift from product work. The next agent
must not write recommended behavior in the present tense before it exists.

| Document / anchor | Current issue | Required correction and evidence |
|---|---|---|
| `docs/architecture/scenarios/github-pr-reviewer.md:7` | CLI-only John Doe journey; no complete browser follow-up path. | Add U/W/F identities, both callbacks, one/two-fleet cases, UI admission/history/context boundaries from sections 3-6. |
| Same, line 49 | Replay "slot" and queue notation obscure PG admission identity. | Describe admission row, body+fleet producer key, logical E vs physical Q and replay. |
| Same, line 51 | Runner directly fetches R2 in diagram. | Show authenticated runner bundle proxy endpoint and daemon/object-store leg. |
| Same, line 80 | Says exchange/probe then consumes state. | Actual flow consumes nonce before provider exchange; document fresh-connect recovery. |
| Same, line 84 | Install approval written as unconditional success. | Disclose current best-effort grant write and R7 until repaired. |
| Same, line 92 | Calls lease event raw PR payload. | It is a normalized flat digest with repo/number/head/action. |
| Same, line 94 | Diff example uses `/files`, unlike fixture. | Reconcile actual onboarded SKILL; fixture uses PR endpoint with diff Accept header. |
| Same, line 102 | Tool calls and response described together as durable event history. | Distinguish ephemeral tool activity from durable request/final response/status. |
| Same, lines 9 and 106 | Local/external proof status can be read as full readiness. | Retain explicit external proof gap, add dated run evidence and head/bundle IDs when executed. |
| `docs/architecture/data_flow.md:35` | Earlier event-ID explanation equates canonical identity with stream receipt. | Use `(F,E)` plus separate physical receipt; repair all examples and joins. |
| Same, write-path diagram near line 129 | Obsolete telemetry table and stale operation names remain. | Trace current receive/lease/meter/report SQL; use billing ledger, logical IDs, current transaction boundaries. |
| Same, line 314 | "One row per answer" for obligations is too broad. | Obligation exists only with reply destination; ordinary PR webhook and steer use none. |
| Same, lines 315 and 422 | Read session is described as resuming conversation. | State checkpoint is read but not transported; preserve R3 until end-to-end context proof exists. |
| Same, lines 392-427 | Examples suggest joining billing only on event_id. | Require `(fleet_id,event_id)`; ledger uniqueness includes charge_type. |
| Same, line 446 | PUBLISH/SUBSCRIBE labels reflect earlier topology. | SPUBLISH/SSUBSCRIBE, sharded hub behavior, durable backfill limits. |
| Same, line 453 | ConnectionManager/single dedicated socket drawing is stale. | Document cluster client and node connections; separate shared command path from hub/blocking delivery connections. |
| Same, install sequence near line 520 | Async install frames/retry timings can conflict with current pipeline. | 201 follows activation; status reconciliation covers absent frames; bounded current backoff differs from old fixed sleeps. |
| `docs/architecture/runner_fleet.md:136` | Single-threaded heartbeat/no midrun-heartbeat account is stale. | Control thread plus worker pool, separate lease renewal and report spool. |
| Same, recovery discussion near line 116 | Small fixed recovery-time wording ignores scan population. | Publish measured recovery distribution and configured bounds; include R1 gap. |
| `docs/architecture/scaling.md:174` | Idle ready slice presented as excluding drained fleets. | Record missing consumer clear and token issue; re-measure after repair. |
| `docs/architecture/datastore_scaling.md:41` | "Erases no work" exceeds currently traced recovery coverage. | Qualify with PG survival and R1; list pre-admission GH loss and external side-effect ambiguity separately. |
| Same, line 58 | rebuild helper described as operator entry point looping to completion. | Identify actual invocation and stopping invariant; bounded helper exists, deployable procedure not established here. |
| Same, lines 64 and 103 | Full rebuild and real host/DNS proof are explicitly absent/stale. | Preserve as open until linked evidence; distinguish per-fleet deletion tests from actual host loss. |
| Same, transport discussion near line 197 | Retired REDIS knob language conflicts with repo policy. | Use DRAGONFLY_URL and current Rust cluster-only boot behavior. |
| `docs/architecture/user_flow.md` and `docs/architecture/web_app.md` | Journey description needs cross-check against current UI paths. | Consolidate active/ready/reviewed distinctions, callback recovery, queued steering, retry identity, context and two-fleet behavior. |
| `docs/AUTH.md:233` and callback section | Provisioning/connection boundaries need consistent ordering. | Keep five-row signup transaction; distinguish provider metadata write; consume signed state before vendor call. |
| Public quickstart / fleets/connectors / related running-webhook docs | Published pages not retrieved in this audit. | Audit in matching `~/Projects/docs` branch, explicitly label unresolved UX and link release evidence; no assumed content claims. |
| Source comments: App route introduction, install UI, session SQL | Comments still mention GH retry loop, retired install frames, or session continuity. | Align comments with executable behavior while keeping future intent separate. |

GitHub's current documentation explicitly says failed webhooks are not
automatically redelivered; a response taking more than ten seconds can fail.
Use [GitHub failed deliveries](https://docs.github.com/en/webhooks/using-webhooks/handling-failed-webhook-deliveries)
as the external source. The corrected comment near
`rustd/crates/afd_api_ingress/src/handler/webhook/app_route.rs:270` already agrees;
the older introductory retry-loop wording should not survive the doc cleanup.

### 11.1 Suggested documentation deliverables

1. Replace the golden-path scenario with one canonical browser walkthrough for
   U, plus a CLI appendix. Keep an explicit source/proof date.
2. Add this report's API/write/queue sequence to data flow, with one identity
   legend and transaction boundaries maintained in one place.
3. Add a two-fleet page/section: shared installation/wallet, independent context,
   duplicate intended reviews, and separate-repository configuration.
4. Add a recovery matrix and runnable staging runbook, including admission,
   received-without-lease, external delivery, old snapshot, and lost nonce cases.
5. Update runner topology and model-context documentation from the serialized
   wire payload and child input, not comments about intended resume behavior.
6. Update public setup/help copy only after verifying it against the relevant
   implementation and live evidence. Work in the docs repository's own branch;
   do not edit it through this worktree.

## 12. Release evidence and implementation handoff

### 12.1 Existing tests are useful but not the requested proof

The checkout contains coverage for connector ownership, App ingress, message
admission, affinity/fairness, report fencing, terminal redelivery, queue recovery,
cluster rebuild, renewal, UI history and multiple fleet tiles. Examples:

- `rustd/crates/afd_api/tests/integration_connector_github.rs`
- `rustd/crates/afd_fleet/tests/integration_lease_affinity.rs`
- `rustd/crates/afd_fleet/tests/integration_report_commit.rs`
- `rustd/crates/afd_fleet/tests/integration_cluster_rebuild.rs`
- `ui/packages/app/tests/e2e/acceptance/multi-fleet.spec.ts`

The rebuild test seeds unreceipted, undelivered, active-live and active-dead
states, including more stranded fleets than one recovery page. Its comments
explicitly exclude outbound/session scope. The received-without-lease handoff
above is not among its described states. It deletes each test fleet's queue
state; it does not demonstrate the target host-failure topology.

The multi-fleet UI acceptance file seeds six active tiles. Its inspected
assertions establish tile presence/status, and comments explicitly qualify
stream animation coverage. It does not establish two real GitHub reviews,
correct follow-up context, concurrent wallet behavior, or no data loss.

No test results are asserted here. The scenario itself still calls the external
repository-bound PR proof unproven (`scenarios/github-pr-reviewer.md:9`, full
path in section 11).

### 12.2 Concrete verification work to commission

| Gate | Reproduction / workload | Pass evidence required |
|---|---|---|
| V1: one fleet browser journey | Dedicated test identity follows signup, connect, install, both repo bindings, real PR, Q1/Q2, reload and runner handoff. | IDs and timestamps across callback, admission, stream, lease, report, GitHub review URL; correct cited follow-up context after handoff. |
| V2: two fleets same repo | Same user/workspace installs F1/F2, one signed PR delivery, one question per fleet, concurrent slots. | Two intended admissions/reviews, independent output/memory/fences, correct scoped billing, no question crossing fleet boundaries. |
| V3: two fleets different repos | Swap only ingress or egress binding, then correct both; revoke F1 grant. | Explainable wake/mint behavior; F2 continues; no undeclared repo access. |
| V4: ambiguous UI send | Commit first message then drop response; retry same intent; repeat same explicit operation ID across F/W/T and with changed body. | One logical operation within intended scope; independent operations across scopes; mismatch explicitly refused. |
| V5: every handoff crash | Stop daemon after each admission/receive/delivered/lease/settlement write; remove group/stream in isolated rig. | Every accepted nonterminal E becomes runnable or explicitly terminal; no silent received orphan; no duplicate durable charges. |
| V6: uncertain external side effect | GH accepts review, then kill child/host before report; lose response to review POST. | Reconcile to one intended external effect or visibly diagnosed ambiguity under approved product semantics. |
| V7: partial fan-out | Fail admission at F2 after F1 succeeds; exceed handler latency budget; restart ingress. | Durable per-fleet fan-out recovery or proven external delivery repair, including work never admitted initially. |
| V8: readiness | Drain thousands of previously busy fleets; race clear vs append; overload one partition. | Idle hints retired safely, finite dispatch tail, measured fairness and pool cost. |
| V9: install interruption | Kill after installing-row commit, after activation, during grant creation, and after success before response. | No permanently misleading active/ready state or accidental second install; deterministic reconciliation. |
| V10: real cluster recovery | Lose each production-shaped failure domain; restore stale snapshot; change announced host; reconnect hub. | Topology restored, tail work recovered within chosen objective, per-fleet completion and billing reconciliation, ephemeral loss documented. |
| V11: capacity/near-zero money | Mixed PR+chat traffic, many installed-idle fleets, slow models, same tenant F1/F2, scarce wallet and GH limit. | p50/p95/p99 queue-to-lease and question latency, provider error handling, wallet semantics, resource saturation and recovery throughput. |
| V12: memory durability | Fail hydrate/capture, drop child memory frame, restart before next run. | No false persistent-memory success; degraded context visible or request refused according to product decision. |
| V13: deployed execution restrictions | Inspect runner tier and policies; exercise sandbox establishment failure and untrusted PR instructions attempting undeclared access. | Effective production filesystem, process, network and credential boundaries documented and demonstrated; unsupported strict mode is not advertised as enabled. |

Use repository-supported gates when implementation changes are made:

```text
orly gate work
make harness-verify                # CONFORM only
make lint-all
make test-unit-all                 # datastore-free repository unit gate
make test-integration-rustd         # isolated live Postgres + Dragonfly lane
make check-version
orly gate pr                       # lifecycle gate before PR creation
```

Follow the repository lifecycle and independent review requirements; package-only
test commands are inner-loop evidence, not repository verification. A fresh
worktree needs root, CLI and each UI package dependencies as AGENTS.md specifies.
The live external GitHub journey and production-shaped failure exercise are
additional evidence; they are not automatically proven by those make targets.
Do not flush a shared or production datastore to run these tests.

### 12.3 Observability needed to make the proof auditable

For each delivery and each target fleet, retain correlation across:

```text
GitHub installation / delivery reference / body digest
  -> workspace W + fleet F
  -> admission row + logical E
  -> physical stream receipt Q (can change)
  -> runner ID + lease L + fence N (can change)
  -> terminal outcome + billing entries
  -> external review ID / URL + reviewed head H
```

Alert on oldest accepted nonterminal age, received events with no valid execution
owner, partial fan-out, missing-grant active fleets, stale ready fields, report
spool backlog/unavailable writes, reconcile/replay progress, repeated provider
POST ambiguity, memory degradation, and pool wait. Metric names and alert
thresholds should come from the implementation and measured staging envelope;
this report does not invent existing dashboards.

### 12.4 Decisions requested from Indy

1. Define the production promise: queued questions versus interruption, selected
   review context, and whether follow-up may write to GitHub.
2. Prioritize R1-R3 as release blockers; R4-R7 before broad unattended traffic.
3. Require a recoverable durable record for every accepted work handoff and a
   deliberate policy for external review duplication/ambiguity.
4. Choose the supported workload, review latency, backlog limit and recovery
   objectives before claiming scalability.
5. Name the owner/controller for the chosen self-hosted cluster and require a
   real failure-domain exercise, PG/key/object recovery, and operational runbook.
6. Require a dated one-fleet and two-fleet live evidence bundle before changing
   the scenario's external-proof status to complete.

My recommendation is to preserve the useful durable admission and fenced-report
foundation, close the uncovered handoffs and context gap, then measure the
complete user promise. The current documentation should say precisely what is
implemented, what this audit inferred, and what remains to be demonstrated.

## 13. Model assignment and next-agent start prompt

Recorded at Indy's request on 2026-09-26. These are task-based recommendations,
not results from a model benchmark on this repository. They do not change the
finding priorities or count as implementation, test, or independent-review
evidence.

### 13.0 Indy's human review comes first

Status: AWAITING INDY'S HUMAN REVIEW. No implementation approval is recorded in
this audit. Indy explicitly requested that remediation start after his review.
Preparing this report, the model recommendation, and the handoff prompt does not
approve the product changes they describe.

Before the next agent implements fixes, record Indy's actual review decision,
date, approved finding IDs/scope, and any constraints in a dated review entry.
Do not infer approval from the existence of the report, an agent review, or this
prompt being copied. If his approval is already explicit in the receiving
session, record and use it without asking again. Otherwise, provide a concise
review brief and wait before implementation. Read-only review can continue.

Indy's review decides scope and product intent. The independent model review
below evaluates the resulting implementation and test evidence. Both records
must accurately say which review occurred.

### 13.1 Recommended model and reasoning effort

| Work | Recommended model | Reasoning effort |
|---|---|---|
| Documentation corrections in section 11 | GPT-6 Sol | `high` |
| R3 follow-up context, R4 message retry identity, R7 install/grant consistency | GPT-6 Sol | `high` |
| R6 readiness cleanup and race tests | GPT-6 Sol, then independent GPT-6 Astra review | `high` |
| R1 recovery gap, R2 durable GitHub fan-out, R5 duplicate external reviews | GPT-6 Astra for design; GPT-6 Sol can implement the resulting design | Start `high`; use `xhigh` for unresolved failure sequences |
| Reproduction cases and integration tests | GPT-6 Sol | `high` |
| R8 recovery objectives, operational evidence and capacity analysis | GPT-6 Astra for synthesis; GPT-6 Sol for test/runbook implementation | `high` |
| Final independent review of recovery, concurrency and billing invariants | GPT-6 Astra in a fresh review session | `high` |

For one model owning the whole workstream, choose GPT-6 Astra at `high`, raising
effort selectively to `xhigh` when a crash sequence, race, or recovery design
remains unresolved. Do not start every task at `max`. Higher effort does not
replace reproduction, repository gates, or the live evidence in section 12.

Official guidance describes [GPT-6 Sol](https://developers.openai.com/api/docs/models/gpt-6-sol)
as suited to complex coding and agentic workflows, and
[GPT-6 Astra](https://developers.openai.com/api/docs/models/gpt-6-astra) as the
most capable model for the hardest end-to-end work. Both support `high` and
`xhigh`. The [deployment guidance](https://developers.openai.com/api/docs/guides/deployment-checklist)
recommends evaluating the quality benefit of higher effort against its latency
and cost. Sources checked on 2026-09-26; the allocation above is this audit's
engineering judgment.

### 13.2 Execution order and review boundaries

Prerequisite: Indy's human review and explicit scope approval under section 13.0.

1. Revalidate findings against the current head. The original audit was static;
   reproduce each inferred failure before treating it as a confirmed defect.
2. Start with R1 as a complete, separately reviewable slice. Use Astra `high`
   to define recoverable states and testable invariants across the handoff.
3. Sol `high` can implement an explicit design and its tests. Keep architecture
   updates in the same change as the implementation.
4. Review the resulting diff and test evidence independently with Astra `high`.
   Record the actual reviewer and outcome. A recommended reviewer is not a
   completed review; unavailable independent review remains outstanding.
5. Reassess R2/R3 next, then the remaining findings by risk and dependencies.
   Preserve a status/evidence entry for every finding, including disproved or
   still-unverified findings. Keep real GitHub and host-failure proof distinct
   from local integration coverage.

Model assignment does not require simultaneous agents. The same work can move
between sequential implementation and review sessions. Do not have two agents
edit the same recovery path concurrently.

### 13.3 Copyable handoff prompt: start with R1

Run this prompt in GPT-6 Astra at `high`. Implementation begins only after the
human review prerequisite in section 13.0 is satisfied. The present audit session
remains analysis and handoff only.

```text
Work in /Users/kishore/Projects/agentsfleet.

Read AGENTS.md, AGENTS.orly.md, the applicable lifecycle/dispatch rules, and:
docs/v2/reviews/github-pr-reviewer-production-readiness-audit-2026-09-26.md

First verify that Indy has personally reviewed this audit and explicitly
approved the remediation scope. Record the actual decision/date/scope; do not
invent approval. If approval is absent, prepare a concise review brief and wait
before implementation. If it is already explicit, proceed without asking again.

After that approval, start production-readiness remediation. Complete R1, the possible
received-event-without-lease recovery gap, as the first reviewable slice.
Use sections 9.3, 12.2/V5, and 13 as the starting evidence and model guidance.
Treat the audit as hypotheses to verify against the current checkout.
Read section 14's Jev proposal as a separate design question for Indy. Do not
make Jev a dependency of the R1 repair or introduce it into the recovery path.
Section 15 gives the concrete github-pr-* runtime design using Jev primitives;
include it in Indy's review brief, with its own implementation scope.

1. Inspect git state and establish the repository-required isolated workstream.
   Preserve existing changes and the audit, including if it is still untracked.
   Read the Rust error standard before changing fallible Rust signatures.
2. Trace admission, event-received writes, delivered marking, affinity, lease
   issuance, group recreation, and all relevant recovery scans. Reproduce the
   claimed gap in an isolated integration test before changing production code.
   If the finding is disproved, record the counterevidence and update its status.
3. Define the invariant: every accepted nonterminal event remains discoverable
   and recoverable across every committed handoff. Preserve fleet/workspace
   isolation, fencing, terminal-event suppression, and monetary idempotency.
4. Implement the smallest complete repair. Test crashes before/after the
   received marker, delivered marker and lease issue; group-only loss,
   stream loss, total queue loss, and concurrent recovery. Include two fleets
   so recovery of one cannot hide starvation or contamination of the other.
5. Update the affected architecture documentation with the implementation.
   Follow the separate docs-repository branch rule if public behavior changes.
6. Run the required repository make targets and lifecycle gates. Package tests
   are iteration evidence only. Use isolated datastores for failure injection.
7. Add a dated resolution entry to the audit: finding status, changed files,
   reproduction evidence, exact verification commands/results, review outcome,
   and remaining limitations. Preserve the original audit and its base revision.

Once Indy approves that scope, carry R1 through implementation and verification.
Keep the slice focused. Finish with a reviewable diff, an evidence-backed R1
status, outstanding independent-review work, and the next recommended slice.
Do not mark overall production readiness complete from this one fix.
```

## 14. Proposal J1: TypeSafe Jev as a System One decision model

Status: PROPOSED FOR INDY'S REVIEW; not an implemented component or a new
production guarantee. Indy supplied TypeSafe's "Jev with coding agents" page;
the documentation index and relevant official pages were then fetched on
2026-09-26. This section supersedes the earlier provisional assumption that the
identity and interface of Jev were unknown.

Jev takes state and typed questions and returns structured decisions. It does
not write code, generate review prose or conduct the conversation. Sol/Astra
remain coding agents for this remediation; Jev would be a product dependency
they integrate. Source: [Jev with coding agents](https://docs.typesafe.ai/introduction/coding-agents).

Unverified: Jev's suitability and measured behavior on this repository's review
traffic, available account capacity, deployment/data-handling requirements and
end-to-end repeatability. No Jev inference request was made and no integration
was installed. Do not assign OpenAI's `high`/`xhigh` settings to Jev; section 13's
effort recommendation applies to Sol/Astra.

### 14.0 Concrete interface and candidate questions

The documented endpoint is `POST https://api.typesafe.ai/v1/systemone`, using
bearer authentication and a JSON body containing `state`, `model`, and
`questions`. Named answers return under `answers`, with the actual model version
and usage. Source: [TypeSafe API](https://docs.typesafe.ai/api).

| Primitive | Documented result | Proposed use in this fleet journey |
|---|---|---|
| `choice` | Selected option, per-option probabilities, confidence. | Select `explain_review`, `remember_preference`, `request_new_review`, `general_question`, or `unclear`. Separately select a candidate review ID or `none`. |
| `score` | Probability-weighted value on an ordered rubric, legend, probabilities and confidence. | Rate whether a request needs simple retrieval, contextual explanation or deeper code investigation. |
| `noul` | Probability from 0 to 1 for a yes/no question; no separate confidence field. | Assess a narrowly defined condition such as whether a message explicitly asks to remember a preference. |

These are proposed task questions, not existing application fields. Each question
needs complete instructions and criteria; its identifier alone does not instruct
the model. Batched questions see the same state and are evaluated independently,
so a dependent question must use a later request or code-supplied inputs. Source:
[TypeSafe primitives](https://docs.typesafe.ai/primitives).

Construct bounded `state` from the current message, explicit UI selection and
short authorized candidate review excerpts. Include repository/PR/head references
needed for selection; keep secrets and unrelated tenant data out. Jev chooses
among references prepared by code; it does not retrieve missing history or invent
new identifiers. Scope all candidate retrieval to the requesting workspace and
fleet before the call.

For reproducible evaluation, pin an explicit supported version and record the
returned version. The fetched model page lists `jev-1.13.0`; aliases can move.
Recheck the available version before implementation rather than treating this
audit date as a permanent default. Source: [TypeSafe models](https://docs.typesafe.ai/models).

### 14.1 Recommended placement

Use Jev first as an optional interpreter of operator intent and selector among
authorized context candidates AFTER durable admission and lease issuance, during
runner execution preparation before the expensive review/chat reasoning call.
This keeps the decision step under an existing fenced execution owner. Keep
signature checks, tenant routing and the GitHub acknowledgement path free of a
new model call. This placement is this audit's proposed adaptation of
[TypeSafe intent routing](https://docs.typesafe.ai/patterns/intent-routing).

```text
GitHub webhook / UI message
          |
          v
existing deterministic authentication, scope and admission rules
          |
          v
durably accepted event E for fleet F
          |
          v
existing lease L / fence N, with parent renewal continuing
          |
          v
build bounded input + authorized context candidates + policy version
          |
          v
Jev proposes one typed decision, or abstains
  e.g. explain selected review / save explicit preference / clarify target
          |
          v
deterministic validator and current-state checks
          |
          v
persist accepted decision + input identity + selected context references
          |
          v
review/chat execution with validated context and permitted actions
          |
          v
existing fenced result/billing settlement + external-action reconciliation
```

This describes a candidate extension, not a prescribed new queue, table or
synchronous lease-handler model call. The runner would persist accepted decision
state through the daemon, before acting, and retain the same logical operation
on reclaim. Specify interruption recovery and context transport before
implementation so it does not add another R1 handoff. Existing reference points
are execution preparation in `src/runner/daemon/lease_run.zig:125`,
the admission-before-execution sequence
in `docs/architecture/data_flow.md:285`, the current message envelope in
`rustd/crates/afd_events/src/steer.rs:79`, and the missing context transport in
`rustd/crates/afd_fleet/src/lease/answer.rs:67`.

The best first use is R3's ambiguous follow-up: "Explain the race you found."
Given authorized candidate reviews, Jev can propose which review the user means
or request clarification. An explicit user-selected review should resolve by
ordinary code without an unnecessary model call. A follow-up about findings
must not implicitly authorize another GitHub review POST.

### 14.2 Where Jev helps, and which decisions stay in code

| Decision | Candidate Jev role | Deterministic authority / review boundary |
|---|---|---|
| Operator intent | Classify explanation, explicit memory request, new review request, or ambiguity. | Typed allowed actions; user action and policy decide whether an external write is permitted. |
| Follow-up context | Rank a bounded set of reviews already scoped to F/W. | Code verifies scope, event existence, selected PR/head and input freshness; ambiguous target asks U. |
| Review work routing | Recommend configured execution tier or abstain when reasoning is complex. | Versioned routing policy, cost limits and permitted models constrain the choice. Benchmark before automatic routing. |
| PR action/head handling | Optional advice for cases requiring semantics. | Known action filters, current-head checks and explicit supersession rules execute in code. |
| Output quality | Flag missing evidence, uncertain claims or mismatch with selected review. | Supplemental signal; it cannot certify absence of bugs or authorize publication by itself. |
| Remediation-agent assistance | Optionally classify a narrow, pre-extracted evidence item; it cannot write the summary or code review. | Sol/Astra write and review code; Indy supplies human approval; actual tests supply verification evidence. |
| Authentication, signatures, installation ownership, repository permissions | No learned decision required. | Existing deterministic checks and least-privilege credential boundaries. |
| Admission identity, retry deduplication, affinity, fencing, ACK/trim and recovery | No Jev authority. | Database uniqueness, atomic state changes and tested recovery algorithms. |
| Billing or wallet settlement | No Jev authority. | Exact arithmetic, transactional writes and scoped idempotency. |
| Production readiness or release acceptance | No decision authority; typed scores do not certify readiness. | Indy's human decision and required verified gates. |

Jev does not repair R1, R2, R4, R5, R6 or R7 merely by choosing a decision label.
Those findings require concrete persistence, identity, concurrency or lifecycle
changes. It could help interpret R3 once context references can actually reach
the runner. Do not expand the initial R1 slice into a new decision-model project.

### 14.3 What "determinism" must mean here

Specify and test three separate properties:

1. Decision repeatability: whether the same complete input, model/rules version
   and configuration produce the same output. This is unverified for Jev.
2. Deterministic enforcement: code accepts only well-formed decisions that pass
   scope, permission, state-version and action checks. Unknown/malformed output
   cannot broaden an action or invent a review target.
3. Replay consistency: after a decision is accepted durably, the same logical
   operation reuses it and its pinned context instead of silently asking again
   and accepting a different action during recovery.

The supplied reference identifies Jev as a model, not an exact rules engine.
Typed answers make branching explicit; they do not establish correct semantic
decisions or atomic recovery. TypeSafe documents limitations in numeric/date
reasoning, sensitivity to adversarial state, and lack of guaranteed identities
between separately phrased questions. Keep arithmetic, expiration comparisons,
permission checks and cross-question consistency rules in code. Source:
[Jev 1.13 limitations](https://docs.typesafe.ai/model-jaggedness/jev-1.13).

Choice/Score confidence is derived from the probability distribution. It is not
a measured probability that the entire downstream action will be correct. Tune
thresholds per question, model version and action risk using labelled examples;
do not copy a threshold from a different primitive. A Noul near 0.5 is uncertain,
not "half a permission". Source: [TypeSafe confidence](https://docs.typesafe.ai/confidence).

Store question definitions and the raw typed answers, including distributions
where supplied, with the accepted decision. Compare proposed action, policy
acceptance and human-labelled correctness separately during evaluation. Test
concurrent/repeated calls, restarts, upgrades and dependency failures; measure
repeatability rather than claiming it from the API's output shape.

An accepted decision should be auditable through the existing F/E identity and
include an operation/revision identity, schema version, policy version, Jev
model or rules version, canonical input digest, selected context references,
repository/PR/head where applicable, chosen action, validation outcome, and
decision timestamp. Decide its storage home during design; do not claim a new
table already exists. Retain reproducible input references or snapshots under
the data-retention policy; a digest alone cannot reconstruct lost input.

Persist decision state in Postgres if execution depends on it. Dragonfly may
cache or deliver it, but losing that cache must not cause an accepted decision
to disappear or a new external action to be chosen. Concurrent deciders must
converge on one accepted decision through a scoped database guard. Replay must
still recheck live permissions and fencing; a stored decision cannot revive a
revoked grant. Changed user intent or changed review head requires a deliberate
new revision, with its relation to the earlier decision recorded.

For F1 and F2, decision identity and candidate context are separate even when
the same user sends the same words or both fleets reviewed the same PR. Test
that no decision cache or operation ID suppresses the second fleet or exposes
the first fleet's private context.

### 14.4 Failure handling, performance and evaluation

Begin with shadow evaluation after the critical recovery fixes: Jev proposes
decisions for a controlled corpus, and those proposals cause no external action.
Compare them with human-labelled outcomes and a simple deterministic baseline.
Measure ambiguous-target handling, unauthorized-action proposals, false deferral,
latency distribution, cost per decision, and added queue-to-execution delay.
There are no Jev measurements or acceptance thresholds in this audit.

On timeout, malformed output, unknown action, missing context or unavailable Jev,
use an explicitly documented safe fallback where one exists. Otherwise retain
the accepted work in a recoverable deferred state and expose the reason. Never
silently mark it complete, discard it, or broaden a tool permission. Put a bound
on retries and latency, with a visible escalation/clarification outcome.

The API also documents 429 rate limits and 529 overload responses. Budget retry
backoff inside the execution deadline and retain lease renewal; avoid an
unbounded retry chain. Keep a failed decision attempt observable without losing
the admitted event. Source: [TypeSafe API errors](https://docs.typesafe.ai/api#errors).

Required evaluation cases:

- Same input/version under repeat, concurrent call, restart and replay.
- Two fleets with identical message text and different review histories.
- Explicit selection of an older review after a newer PR head arrives.
- Ambiguous pronouns, missing candidate, unauthorized candidate and stale grant.
- PR text attempting to instruct the decision model to change permissions.
- Crash before and after decision persistence; lost Dragonfly delivery/cache.
- Model/policy version change while an event or decision is queued.
- Jev outage without loss of accepted work or blockage of unrelated fleets.
- An explain-only question that must produce zero GitHub write actions.

Do not promote shadow results to production authority until Indy reviews the
measured benefit, failure behavior and enforcement design. No existing audit
finding becomes resolved just because a Jev evaluation is successful.

### 14.5 Questions for Indy's design review

1. Which supported Jev version and deployment/data-handling arrangement should
   the evaluation use? Who owns its question definitions, thresholds and rollout?
2. Which bounded decisions should it make first? Recommendation: follow-up
   intent and authorized context selection, with abstention.
3. What repeatability, accuracy, latency and cost bounds justify it over direct
   UI selection plus ordinary rules?
4. What is the durable decision representation and recovery procedure, and how
   are revoked permissions or changed PR heads checked on replay?
5. What measured evidence would permit moving from shadow evaluation to use in
   execution, and which decisions must always remain human or code-controlled?

Next-agent deliverable for J1, after Indy approves its scope: use the supplied
reference and [documentation index](https://docs.typesafe.ai/llms.txt) to prepare
a short design comparison with placement, input/question/answer schemas, scope
guards, persistence/replay rules, failure behavior and a proposed evaluation
corpus. Measure results only in an authorized evaluation environment; until then,
label the evaluation unrun. Keep this work separate from the initial approved
R1 implementation. TypeSafe's agent skill is optional implementation tooling;
this audit has not installed it or changed the coding agent's model.

## 15. Worked proposal: Jev decisions inside github-pr-* execution

This section answers Indy's clarified intent: use TypeSafe Jev inside the
`agentsfleet` product to make bounded decisions in GitHub PR workflows. The
generative reviewer still reads code, reasons about defects and writes responses.
Sol/Astra in section 13 are the agents implementing this integration; the runtime
reviewer's model remains a separate product configuration choice.

Everything below is a proposed extension awaiting Indy's review. The questions,
policy names and records shown are illustrative, not existing APIs or observed
Jev responses. No product code or provider request was executed for this design.

### 15.1 Three decision points in one PR journey

```text
Signed GitHub event
        |
        v
CODE: authentication, repository subscription, durable admission, lease/fence
        |
        v
CODE: read diff for intended head; build bounded, scoped input
        |
        v
JEV A: choice of review category + score of investigation depth
        |
        v
CODE: apply versioned routing rules; persist selected route
        |
        v
GENERATION MODEL: inspect code; return candidate findings + evidence
        |          (analysis has no direct GitHub publish permission)
        v
JEV B: noul for one evidence relationship + score of stated impact
        |
        v
CODE: validate finding locations; select publish / investigate / withhold
        |
        v
CODE: durable action intent, current permission/head checks, controlled POST
        |
        v
GitHub review ID + terminal event + UI history
        |
  user asks "Why is this a race?"
        v
JEV C: choice of message intent + choice among authorized review references
        |
        v
CODE: bind selected evidence; GENERATION MODEL: explain in read-only mode
```

Jev A and B execute within an owned run and its renewal budget, not in the
public webhook request. Jev C executes on the separately admitted UI message.
Persist each accepted decision before its dependent action. If new durable
decision stages are introduced, define how a new runner resumes them after
lease expiry; do not add a new untracked gap between admission and ownership.

| Point | Narrow question for Jev | Primitive | Deterministic code response |
|---|---|---|---|
| A: review category | Which declared review concern best describes these changed hunks? | `choice`, including `unclear` | Select only a configured review path; uncertainty uses full review. |
| A: investigation depth | Does assessing this change require local inspection, related-file inspection, or wider investigation? | `score` with those described levels | Map score/confidence to configured resources within budget; arithmetic and budget limits stay in code. |
| B: finding evidence | Does the supplied failing test exhibit the symptom alleged by this candidate finding? | `noul` | Require sufficient support for this signal, or send the candidate for further investigation. This is not a proof that the defect is real. |
| B: impact | What impact does the candidate allege, assuming its evidence is valid? | `score` | Rank supported findings or escalate serious uncertainty. Validate evidence separately. |
| C: message intent | Is U asking for explanation, a saved preference, a new review, or something unclear? | `choice` | Enter a typed handler; explanation handler has no GitHub write capability. |
| C: target review | Which authorized candidate review does this message reference, or none? | `choice` | Check the returned identifier against the supplied candidate set; clarify ambiguity. Explicit UI selection bypasses this inference. |

Initially keep A's optimization and B's publication decisions in shadow mode.
A mistaken "irrelevant" decision could hide a defect. Measure recall before
allowing automatic skip/withhold. Unknown or incomplete diff coverage must not
silently turn into "no review needed". Known event actions, generated-file rules
and exact repository policy matches should be handled directly in code.

### 15.2 Example using all three primitives on one candidate

Application code prepares one candidate finding, a small relevant source/diff
window and associated reproduction evidence. It sends a bounded state to the
documented endpoint. The following is an illustrative request shape; placeholders
must be replaced by evidence from the current scoped execution.

```json
{
  "model": "jev-1.13.0",
  "state": {
    "candidate": "<one proposed finding>",
    "changed_code": "<relevant code and diff excerpt>",
    "reproduction": "<test steps and observed failure, or explicitly missing>"
  },
  "questions": {
    "category": {
      "type": "choice",
      "instructions": "Which category describes the candidate finding?",
      "criteria": {
        "correctness": "Alleges incorrect observable program behavior",
        "security": "Alleges an unauthorized access or security-boundary failure",
        "test_gap": "Alleges missing verification without a demonstrated defect",
        "maintainability": "Alleges readability or maintenance cost",
        "unclear": "The candidate does not support a clear category"
      }
    },
    "symptom_supported": {
      "type": "noul",
      "instructions": "Does reproduction show the same observable symptom alleged by candidate?",
      "criteria": {
        "true": "Provided observed output directly exhibits the alleged symptom",
        "false": "Observed output is missing, unrelated, or does not exhibit the alleged symptom"
      }
    },
    "stated_impact": {
      "type": "score",
      "instructions": "What impact does candidate explicitly allege, conditional on its evidence being valid?",
      "criteria": [
        "Readability or cosmetic impact without changed user-visible behavior",
        "Degraded behavior with an available workaround",
        "Required behavior fails with no stated workaround",
        "Irrecoverable data loss or unauthorized access is alleged"
      ]
    }
  }
}
```

Each question evaluates the supplied state independently. `stated_impact` must
not assume that the concurrently asked Noul has already confirmed evidence.
For production, use a pinned version confirmed at implementation time and
question definitions versioned with the code. See [Choice](https://docs.typesafe.ai/primitives/choice),
[Score](https://docs.typesafe.ai/primitives/score), and
[Noul](https://docs.typesafe.ai/primitives/noul).

For a static-analysis-only finding with no reproducer, this example's Noul cannot
establish support. Route it to an explicit alternative evidence rubric or further
investigation. Do not use "no failing test supplied" as a universal rule that a
finding is false. A model that labels a symptom supported also does not prove
root cause, concurrency safety or full test coverage.

### 15.3 Turn typed answers into deterministic decisions

The policy function should be ordinary code, with explicit ordering and named,
versioned thresholds. This is design pseudocode, not existing implementation:

```text
decide_publish(accepted_answers, finding, policy, current_state):
    if not current_state.permission_valid:
        return BLOCK_PERMISSION
    if not current_state.lease_fence_valid:
        return STOP_STALE_ATTEMPT
    if not current_state.review_head_matches:
        return REPLAN_FOR_CURRENT_HEAD
    if not finding.location_valid_for_diff:
        return INVESTIGATE_LOCATION
    if not valid_typed_answers(accepted_answers):
        return DEFER_DECISION

    if accepted_answers.category.choice == unclear:
        return INVESTIGATE_CATEGORY
    if accepted_answers.category.confidence < policy.category_floor:
        return INVESTIGATE_CATEGORY
    if accepted_answers.symptom_supported.noul < policy.support_floor:
        return INVESTIGATE_EVIDENCE
    if accepted_answers.stated_impact.confidence < policy.impact_floor:
        return INVESTIGATE_IMPACT
    if accepted_answers.stated_impact.score >= policy.human_review_level:
        return REQUEST_HUMAN_REVIEW
    if not policy.permits_category(accepted_answers.category.choice):
        return WITHHOLD_UNDER_POLICY

    return ELIGIBLE_FOR_COMMENT
```

`ELIGIBLE_FOR_COMMENT` is input to the controlled publisher, not proof that the
comment was posted. The publisher still handles durable intent, current action
ownership, provider response ambiguity and external review reconciliation (R5).
The policy ordering ensures that high model confidence cannot override an
expired fence or revoked grant. Which impacts need human review is Indy's
product-policy choice; this is a proposed configurable rule, not current behavior.

No threshold values are asserted as safe here. Tune them on labelled PR/finding
examples, including difficult false negatives and adversarial inputs. Scores
are positions on a declared rubric, not defect probabilities. Do not multiply
several Noul probabilities as if the questions were statistically independent;
an independently evaluated question is not proof of independent error. Combine
decisions with explicit code rules and evaluate the combined behavior.

The guarantee to aim for is precise: identical accepted answers, policy version
and relevant state snapshot produce the same planned action. Live permission,
fence or PR-head changes can intentionally stop that action. Replaying a saved
decision is different from re-invoking Jev and assuming the same answer.

### 15.4 Enforce the decision before any GitHub write

The current fixture instructs the generative agent to call `http_request` and
post the review directly (`tests/fixtures/fleetbundle/github-pr-reviewer/SKILL.md:39`).
Adding "ask Jev first" to that prose would make Jev an optional convention.
A meaningful publishing decision needs a mandatory execution boundary.

Recommended design: separate candidate generation from publication. During
analysis, permit necessary reads and prohibit direct review writes. A controlled
publisher consumes only persisted, validated candidates and decisions. Author
the posting payload in code and bind its repository, PR, head, comment locations
and `event: COMMENT` to the accepted intent. Do not grant merge, approve or push
authority through Jev's classification.

There is an existing provider-neutral HTTP method/path/body validation boundary
in `src/runner/engine/runtime/http_request_policy.zig:11`, invoked before dispatch
by `src/runner/engine/runtime/policy_http_request.zig:127`. Use it as a source
reference when designing enforcement; this audit does not claim it already
validates Jev decisions or reviewer publication identities. A dedicated
publication operation is another design option. Indy should review which
boundary keeps generic HTTP tools from bypassing the selected policy.

Persisted action identity must distinguish automatic review of a given head
from an explicit requested rerun. Keep event/decision lineage and fleet scope;
choose the external action's deduplication identity deliberately rather than
assuming a new event ID always means a new desired public comment. No Jev answer
can supply exactly-once external delivery on its own.

### 15.5 Example: nkishore@megam.io with one and two fleets

For one fleet:

1. F1 receives PR #42 at head H. Code validates and admits it, then issues L/N.
2. Jev A suggests a review category/depth; policy selects the configured reviewer.
3. The reviewer proposes findings without posting. Jev B evaluates each bounded
   candidate/evidence pair. Code routes candidates for comment or investigation.
4. The publisher posts eligible comments using the recorded action identity.
5. U asks "Why is the first finding a race?" Jev C chooses explanation and the
   supplied review reference. Code verifies F1/E/H and provides that evidence to
   the answering model. This route has no posting permission.

For two fleets, suppose F1 focuses on security and F2 on correctness:

- Both may read the same immutable PR diff, but their review criteria, candidate
  findings, Jev answers, accepted decisions and publish intents are scoped
  separately. Shared diff caching must not share conversational authority.
- A correctness finding can be out of policy for F1 and eligible for F2. Each
  decision is reproducible from that fleet's own recorded policy and answers.
- Asking F1 about a finding from F2 does not automatically import F2's context.
  The UI must explicitly select the permitted target or ask for clarification.
- If Dragonfly disappears, pending work and accepted decisions recover from
  Postgres under the repaired lifecycle. Do not rerun a Jev decision solely
  because a cache entry vanished; resume the recorded decision revision.

### 15.6 First Jev slice to propose after Indy's review

Recommendation: start with C, follow-up intent/context routing, after R3's
context transport exists. Use `choice` with a bounded candidate set and an
explicit unclear/none outcome. Measure correct reference selection and prove
that explanation never reaches a GitHub write. This directly improves the
journey Indy asked to audit and has a narrow, observable boundary.

Next evaluate A in shadow mode for resource routing. Add B as advisory evidence
scoring before considering any authority to withhold or publish. A policy that
automatically posts or suppresses findings requires separate measured evidence
and Indy's decision. Record J1 as a proposed feature/evaluation, not a fix already
delivered for R1-R8.
