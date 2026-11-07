# Scenario — Incident responder in a Slack thread

> Parent: [`README.md`](./README.md) · References: [`github-pr-reviewer.md`](./github-pr-reviewer.md) (the repository subscription this mirrors), [`production-deploy-repair.md`](./production-deploy-repair.md) (the repair crew), [`../connectors.md`](../connectors.md), [`../data_flow.md`](../data_flow.md), [`../memory.md`](../memory.md).
>
> **Built; the drill is pending.** M206_001–003 live in `docs/v2/done/`; M206_004 is in `docs/v2/done/` with its live drills (its own §3) parked until 001–003 reach `api-dev`. Line citations were rechecked against source on 2026-10-14; the scenario's original "today" claims were read at `b1bc6f0c4`.

Legend: ✅ in the Rust daemon · 🟡 in the Rust daemon, broken.

**Outcome under test:** a failed `agentsfleet/linkwarden` GitHub Actions run is announced in `#ci-dev`. Someone replies in that announcement's thread with `@agentsfleet-dev why did this fail?`. One fleet answers in the same thread with a GitHub Actions job-log line and a Grafana Loki log line it read itself, plus a proposed fix. Asked to, the channel's repairer opens one draft fix Pull Request (PR) against Linkwarden `dev`: attaching it to the channel was the authorisation, as in Claude Tag. Merging and deploying stay with people and the pipeline.

## 1. The flow

```text
GitHub Actions -> #ci-dev: PR run failed
Person -> Slack thread: @agentsfleet-dev why did this fail?
Slack -> agentsfleetd: signed app_mention
agentsfleetd -> connector install: resolve team and current workspace
agentsfleetd -> reviewed subscriptions: choose configured fleet
    no subscriber -> channel_not_configured; no fleet or model call
    ambiguous / paused / needs name -> fixed notice through existing outbound ledger
    addressed or sole eligible fleet -> one slack_mention admission
Configured fleet -> GitHub + Grafana: read permitted evidence
Fleet report -> recorded Slack thread: answer through existing outbound poster

Person -> same thread: @agentsfleet-dev ci-dev-repairer open the fix
Configured repairer -> approved branch/draft PR scope -> reply with PR link
Human -> review/merge/deployment; no additional authority comes from the mention
```

## 2. Three things that get confused

| | Slack connector | Channel notice owner | Channel subscription |
|---|---|---|---|
| What it is | Workspace Slack credential and team routing. | Stable owner for fixed routing notices only. | A configured fleet whose reviewed setup names the channel. |
| Where it lives | Vault handle and `connector.installs`. | Existing `connector.channels` binding; no new schema. | A `mention` trigger with `source: slack` and channel identifiers. |
| Created by | Workspace administrator connects Slack. | First eligible notice binds an existing subscriber when no valid owner exists. | A workspace member reviews and saves the fleet setup. |
| Proves | The daemon can use the workspace's connector. | Notice ownership only; never model or admission authority. | This fleet is a candidate for mention routing. |

A connected card proves only the first column. The catalogue's "Connected" is vault-key membership (`held`, `afd_connector/src/grant/holding.rs:141-156`); the per-provider status reads a JSON handle with an `integration` field (`connection`, `holding.rs:107-127`). Neither checks a token is alive, an event subscription exists, or a channel is bound. The journey is proven by the drill in M206_004, never by a card.

## 3. The same shape GitHub already has

| Question | GitHub App ✅ | Slack mention ✅ |
|---|---|---|
| Ceiling | the repositories the App installation covers | the channels the bot user is a member of |
| Workspace routing | `installation.id` → `connector.installs` (`SELECT_INSTALL_WORKSPACE`, `afd_ingress/src/sql.rs:43-46`) | `team_id` → the same table and statement |
| Subscription | `webhook` trigger: `source: github`, `repositories`, `events` | `mention` trigger: `source: slack`, `channels` |
| Subscription identity | repository full name, compared case-insensitively (`serves_repository`, `afd_ingress/src/binding/mod.rs:250-256`); a rename stops routing | channel identifier (ID); a rename changes nothing |
| Who receives it | every matching fleet (fan-out) | one configured fleet, a fixed notice, or an unconfigured drop |
| Replay key | producer `webhook_app`, `<fleet>:<body digest>` (`afd_ingress/src/deliver.rs:104-106`) | producer `slack_mention`, `<team_id>:<event_id>` |
| Credential | the fleet mints an installation token behind an approved `github` grant | the fleet holds no Slack credential; the daemon posts |

The GitHub subscription keys on a repository's name, so renaming a repository silently drops its events (`UZ-WH-022`). That weakness is recorded here and not fixed by M206.

**Against Claude Tag**, Claude's own Slack app, read through its documentation (`claude.com/docs/claude-tag/concepts/how-it-works.md`, `agent-identity.md`, `admins/add-connections.md`, `users/memory.md`, `users/use-cases/fix-bugs.md`):

| Claude Tag | here |
|---|---|
| one agent; an admin attaches access bundles (credentials, domains, plugins) to a channel or the workspace | a fleet (instructions, credentials, allowlist, budget) attached to one channel |
| a sandbox session per thread | one run per mention |
| memory per channel; public channels share it workspace-wide | memory per configured fleet, subject to its saved grants |
| a service account in channels, no per-user linking | the workspace bot token and the GitHub App: the same |
| opens draft PRs with no approval step | the same: an attached write fleet opens one draft PR per request (§7) |
| live progress checklist in the thread | the final answer and notices; progress comes later |

Fleets are attached, never created per message: a fleet per message would start with empty memory ([`../memory.md`](../memory.md) §3).

## 4. Routing a mention

The handler admits only a configured `Addressed` or `Sole` fleet.
An unconfigured channel returns `channel_not_configured` without creating a fleet, scheduling a lease or calling a model.
The route does not use historical channel fleets as fallback responders.

1. Validate the signed event and resolve its Slack team to the current workspace.
2. Drop bot messages, unsupported event types and replayed deliveries through the existing ingress rules.
3. Read the channel's reviewed fleet subscriptions.
4. Match an explicit leading fleet name or choose the sole eligible unaddressed subscriber.
5. Admit the selected fleet or record the fixed routing notice described below.

| Subscription state | Native outcome |
|---|---|
| No subscribers | `Unconfigured`; return `channel_not_configured`, without a model or fleet creation. |
| One matching name, runnable | `Addressed`; admit that configured fleet. |
| Matching name paused | `Paused` notice; no model call. |
| Several names match ignoring case | `Ambiguous` notice; no arbitrary selection. |
| No matching name, one eligible unaddressed subscriber | `Sole`; admit that configured fleet. |
| No matching name, no eligible unaddressed subscriber | `AddressIt` notice naming configured candidates. |
| No matching name, several eligible unaddressed subscribers | `Choose` notice; ask which fleet should receive the request. |

Addressed-only repairers require their configured name even when they are the channel's only subscriber.
The [native route](../../../rustd/crates/daemon/afd_ingress/src/slack/route.rs) owns eligibility and name matching.
The [mention handler](../../../rustd/crates/daemon/afd_api_ingress/src/handler/mention/mod.rs) owns admission and the unconfigured outcome.

Fixed ambiguity, paused and addressing notices use the existing outbound ledger and Slack poster.
The existing `connector.channels` row supplies a stable notice-only owner.
If no valid binding exists, notice handling binds an existing configured candidate rather than creating a fleet.

The binding must match the current workspace and installed team.
Deleted owners and moved-team bindings cannot supply stale authority; a killed owner cannot acquire another outbound obligation.
Concurrent notices converge on the channel's existing unique binding.

[Notice ownership](../../../rustd/crates/daemon/afd_api_ingress/src/handler/mention/notice.rs) owns those checks and the recorded thread destination.
Slack retries reuse the existing deterministic notice identity.
No second outbound path or new schema is required.

Historical stored fleets and setup revisions remain unchanged.
Their channel bindings can carry notices only when current ownership checks permit them.
The new route cannot run them as an unconfigured-channel fallback.

A configured fleet can use its permitted memory tools across threads.
The thread remains a reply destination; [memory](../memory.md#4-slack-threads-and-fleet-memory) owns the memory scope.
Connecting Slack or saving a notice binding proves no memory capture or recall.

## 5. What the fleet is told

Slack's mention event carries the mention and nothing else. The daemon re-reads the thread before admitting the event: `conversations.replies` for the thread root, parent first, then the latest replies, at most 20 messages, each capped, under a 1.5 second deadline, with the bot token loaded and the pool connection released before the call (`afd_connector/src/slack/replies/mod.rs`). The parent is the GitHub announcement, so it carries the run link. Attachment and block text are flattened into the message so the link survives.

The runner reads the request's `message` string and falls back to the whole body (`af_wire/src/event/message.rs:14-24`). The daemon therefore composes one `message`: the mention text, then the thread under a fixed heading that labels it untrusted data. A failed or forbidden re-read (a private channel needs `groups:history`, which the app does not request) degrades to the mention alone, and the message says so.

## 6. How the answer comes back

The producer that owns a reply surface records the reply destination on the admission: provider `slack` and the address `{team_id, channel_id, thread_ts}` (the poster requires `channel_id` and `thread_ts`), where `thread_ts` is the mention's `thread_ts`, or its `ts` when it started the thread. The destination travels with the event, and an approval continuation inherits it from the event it resumes. The report transaction owes a delivery only when the event carries a destination, addressed by it. The Slack poster reads the address from the delivery obligation.

The report parses the recorded connector once, so the lease's **model** provider never reaches the delivery ledger (`afd_fleet/src/lease/obligation.rs`). A job its destination refuses for good, or one out of delivery cycles, is stamped abandoned, and no scan offers it again (`afd_outbound/src/abandon.rs`).

A run may say up to eight interim lines before its answer, through `POST /v1/runners/me/leases/{lease_id}/messages` (`MESSAGES_PER_RUN_MAX`, `af_wire/src/message_verb/mod.rs`). The lease plane fences each line and counts it in `runner.leases.posted_message_count` (`schema/1810_runner_leases.sql`), so a reclaimed lease starts a new count. The daemon posts every Slack text, interim line and answer alike, as literal text, with `&`, `<` and `>` sent as Slack entities (`afd_outbound/src/slack/escape/mod.rs`, called from `SlackPoster::post`). Each line is stamped `agentsfleet_interim` with `{lease_id, line}` (`afd_connector/src/slack/answered/mod.rs`), so a repeat finds only its own line and never silences the answer.

## 7. Four stages, four authorities

| Stage | Who acts | Credential | Approval |
|---|---|---|---|
| Diagnosis | the responder fleet, on a mention | GitHub token with `contents`, `actions` and `checks` read; Grafana Viewer token; network read-only | none needed; it cannot write |
| Draft fix PR | the repairer fleet, addressed by name | GitHub token for `agentsfleet/linkwarden`, writing one daemon-issued branch and one draft PR against `dev` (`afd_gate/src/policy/egress/write.rs`) | none per request: attaching the repairer to the channel is the authorisation, as in Claude Tag |
| Merge | a person in GitHub | their own | the repository's branch protection |
| Deploy | Linkwarden's pipeline | the pipeline's | the release process; the setup checks that `agentsfleet-repair/*` branches cannot reach deploy secrets |

A Slack-requested run stays within its attached fleet's policy.
The repairer targets one daemon-issued branch in `agentsfleet/linkwarden` and one draft against `dev` (`afd_gate/src/policy/egress/write.rs`).
Its event-owned ref permits bounded corrections through PATCH with `force:false`.

Other ref updates, merge, deletion and GraphQL remain refused.
The runner rejects unmatched requests on ruled hosts (`origin_admits` in `af_egress/src/admission/mod.rs`), and its token has no workflow permission.
Write-bound fleets never receive unaddressed mentions.

A branch push runs repository workflows, so the drill audits secret paths for `agentsfleet-repair/*` branches.
The installed grant bounds writes as described in [trust anchors](../connectors.md#trust-anchors).

## 8. Evidence sources

| Source | How a fleet reads it | Known gaps |
|---|---|---|
| GitHub run | the fleet reads the linked run, failed jobs, steps and annotations through `http_request` under the repository's read prefix, as its instructions direct (M206_003 §2); it also reads commits and compare there (`rules`, `afd_gate/src/policy/egress/read.rs:21-30`) | A mint reads the installation's permissions first and asks for `contents` read plus whichever of `actions` and `checks` read it holds (`for_binding`, `afd_credential/src/credential/github/request.rs:85-96`). The App registration lists Checks: read-only and each installation accepts it before the drill (`playbooks/operations/github_app_registration/001_playbook.md:38-43`); until one does, the mint succeeds without it and the fleet's annotation read answers 403. Job logs can redirect to storage. The tool follows at most three guarded bodyless GET redirects, readmitting each destination and stripping modeled headers on an origin change. The reviewed HTTP destination policy must admit the storage host; a denied or truncated required log still produces a named source gap. See [guarded downloads](../runner_fleet.md#guarded-tool-downloads). |
| Grafana | workspace secret `grafana = {host, token}`; `Bearer ${secrets.grafana.token}`; Loki and alert reads over GET | No connector, by design ([connector archetypes](../connectors.md#archetypes)). The host must resolve publicly, because the tool rejects private addresses. Read-only is enforced by `network.read_only`, which is otherwise off by default (`af_fleet_runtime/src/config/policy/mod.rs:202`). |
| Memory | the fleet's own namespace, hydrated only under a live lease for that fleet that no reclaim has moved past (`Plane::hydrate` in `afd_fleet/src/lease/memory.rs`) | none for this journey |

## 9. Integration map

| Integration | Connector (auth) | Events in | Actions out | In the first drill |
|---|---|---|---|---|
| GitHub | App install; per-lease installation token, repository-scoped, 1 hour | App ingress: `pull_request`; `workflow_run` completed with `failure` only; `deployment_status` dropped (`afd_api_ingress/src/handler/webhook/github/mod.rs:106-154`, `app_route.rs:22-29`) | fleet `http_request` inside egress rules | read token only |
| Slack | OAuth bot token: `app_mentions:read`, `chat:write`, `channels:history` | `app_mention` (M206_002) | daemon `chat.postMessage` | yes |
| Grafana | none; workspace secret | none | fleet reads | yes, read-only |
| Zoho Desk | OAuth refresh, multi-data-center, `Desk.*.READ` | none | none | no |
| Zoho Recruit fleet | static secret `zoho_recruit`, never refreshed | none | fleet `http_request` | no |
| Zoho Sprints fleet | rides the Desk grant, whose Desk scopes cannot read Sprints | none | fleet `http_request` | no |
| Linear | OAuth refresh: `read`, `comments:create` | no connector ingress; a per-fleet signed webhook exists | none from the daemon | no |
| Jira | OAuth refresh plus site lookup | none | none | no |

## 10. Current versus required

| Capability | Rust daemon today | Required | Workstream |
|---|---|---|---|
| Signed Slack delivery | ✅ signature, 5-minute window, `url_verification` echo (`afd_api_ingress/src/handler/events/mod.rs:156,197`, `afd_connector/src/registry/mod.rs:271-277`) | unchanged | — |
| Mention becomes an event | ✅ one `slack_mention` admission per Slack event, keyed `<team_id>:<event_id>` (`afd_ingress/src/slack/admit.rs:59-64`, `afd_admission/src/admission.rs:33`) | — | M206_002 |
| Channel → fleet | Reviewed subscriptions route only configured Addressed/Sole fleets. Unconfigured channels return `channel_not_configured`; existing bindings carry fixed notices only. | Combined live drill remains pending. | M236 |
| Thread context | ✅ `conversations.replies` under 1.5 s, parent plus latest replies, capped, under a fixed untrusted-data heading (`afd_connector/src/slack/replies/mod.rs:48-51,183`, `afd_ingress/src/slack/message.rs:20,52`) | — | M206_002 |
| Answer delivery | ✅ owed only to a recorded destination; abandoned when refused or out of cycles (slot 1740) | — | M206_001 |
| Slack poster | ✅ `chat.postMessage` from the job's own address; reads no event row (`afd_outbound/src/slack/mod.rs`) | — | M206_001 |
| Write reach from Slack | ✅ one ref and one draft PR, proven for a Slack-requested lease with no approval asked (`afd_gate/src/policy/egress/tests/slack.rs`, `afd_fleet/tests/integration_lease_gates/slack.rs`) | — | M206_003 |
| One run per approval | unverified: the parked delivery passes once approved (`afd_gate/src/gate/pass.rs:164-165`) while approval also admits a continuation (`afd_approval/src/inbox/resolve/mod.rs:149-152`) | exactly one run | its own spec; off this path, since Slack requests do not park |
| Continuous Integration (CI) evidence reach | 🟡 the read mint asks for `actions` and `checks` read where the installation holds them, and the read rule covers the CI paths (M206_003 §1–§2); guarded GET log redirects implemented, with live access dependent on the reviewed destination policy | every installation accepts required App permissions and the reviewed HTTP policy admits required log destinations; retain fleet-owned complete-text evidence | M206_003 |
| Grafana read-only | ✅ when the fleet declares `network.read_only` | declared by the drill fleet's setup | M206_004 |
| External setup | not in code | playbook: apps, channels, subscriptions, App permissions, Grafana token | M206_004 |

## 11. Channels and environments

| Channel | Slack app | Deployment | Bound fleets |
|---|---|---|---|
| `#ci-dev` | `agentsfleet-dev` | `api-dev.agentsfleet.net` | `ci-dev-responder` (read), `ci-dev-repairer` (write, addressed only) |
| `#ci-prod` | `agentsfleet` | `api.agentsfleet.net` | `ci-prod-responder`, `ci-prod-repairer` |
| `#release-dev`, `#release-prod` | not invited in M206 | — | none; rollout verdicts and release discussion stay human until a later milestone posts verifier results there |

Development runs first. The production app and `#ci-prod` follow only after the development drill passes, the order `playbooks/operations/slack_app_registration/001_playbook.md` already sets.
Both channels subscribe to `agentsfleet/linkwarden`: development selects pull requests targeting `dev`, and production selects `main` pushes. The repairer in either workspace opens a draft against Linkwarden `dev`.

## 12. Invariants

- One Slack event admits at most one event or one notice, across retries.
- A fleet answers in a channel only when its own reviewed setup names that channel's ID.
- A delivery is owed only to a destination its producer recorded; a model provider never reaches the delivery ledger.
- An obligation the destination permanently refuses is abandoned and never re-offered.
- Memory is private by default. This scenario grants no shared-memory access; explicit read grants can expose workspace-visible memory from other fleets. See [memory grants](../memory.md).
- A Slack-requested write run reaches one daemon-issued branch and one draft PR, and never merges or deploys.
- A write-bound fleet never receives an unaddressed mention.
- A fleet reads CI evidence only inside its bound repository.
- A fleet cites a job log line only if its own `http_request` returned that text; a redirect it cannot follow becomes a named gap.
- No fleet holds the Slack bot token; the daemon posts.

## 13. Proof status

Nothing here is proven live. The Slack registration playbook's live check, a threaded reply to a mention (`playbooks/operations/slack_app_registration/001_playbook.md:87-99`), has not been recorded against the Rust daemon. M206_004 records the development and production drills; this section changes to one line per proven stage when they land.

## Durable repair follow-on

The [durable repair scenario](durable-repair.md) extends the addressed manual path with later-CI continuation, existing-PR authority, worker recovery and Jev goal evaluation. M236 contains source for these capabilities; combined live acceptance remains unfinished. Saving setup alone does not demonstrate them.

A scheduled Grafana diagnosis needs a configured notification destination and explicit repair authority.
The fleet receives no implicit cross-fleet handoff or arbitrary channel routing.
