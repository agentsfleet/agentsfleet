# Scenario — Incident responder in a Slack thread

> Parent: [`README.md`](./README.md) · References: [`slack-channel-resident.md`](./slack-channel-resident.md) (the channel's resident), [`github-pr-reviewer.md`](./github-pr-reviewer.md) (the repository subscription this mirrors), [`production-deploy-repair.md`](./production-deploy-repair.md) (the repair crew), [`../connectors.md`](../connectors.md), [`../data_flow.md`](../data_flow.md), [`../memory.md`](../memory.md).
>
> **Built; the drill is pending.** M206_001–003 live in `docs/v2/done/`; M206_004 is in `docs/v2/done/` with its live drills (its own §3) parked until 001–003 reach `api-dev`. Line citations were rechecked against source on 2026-10-14; the scenario's original "today" claims were read at `b1bc6f0c4`.

Legend: ✅ in the Rust daemon · 🟡 in the Rust daemon, broken.

**Outcome under test:** a failed `agentsfleet/linkwarden` GitHub Actions run is announced in `#ci-dev`. Someone replies in that announcement's thread with `@agentsfleet-dev why did this fail?`. One fleet answers in the same thread with a GitHub Actions job-log line and a Grafana Loki log line it read itself, plus a proposed fix. Asked to, the channel's repairer opens one draft fix Pull Request (PR) against Linkwarden `dev`: attaching it to the channel was the authorisation, as in Claude Tag. Merging and deploying stay with people and the pipeline.

## 1. The flow

```text
GitHub Actions --(GitHub's own Slack app, external)--> #ci-dev  "PR run 123 failed against dev"
                                                            |
a person, in that thread:  "@agentsfleet-dev why did this fail?"
                                                            |  signed app_mention
                                                            v
agentsfleetd:  check signature -> Slack team -> workspace -> who listens in this channel?
                                                            |
        +----------------------+----------------------------+----------------------------+
        | nobody               | one fleet                  | several fleets             |
        v                      v                            v
  the channel's resident   that fleet             "@agentsfleet-dev <name> ..." -> that fleet
  (read-only, no tools)                           no name -> one notice listing the names
        |                      |
        +----------+-----------+
                   v
  read the thread: the announcement + the latest replies, capped
                   v
  admit ONE event, keyed by Slack's own event id  (a Slack retry changes nothing)
                   v
  runner: GitHub reads + Grafana reads + the fleet's memory, all read-only
                   v
  answer is owed to that thread -> daemon posts it with chat.postMessage(thread_ts)

later, in the same thread:  "@agentsfleet-dev ci-dev-repairer open the fix"
  -> the repairer attached to this channel runs; no per-request approval (as in Claude Tag)
  -> one draft PR on one daemon-issued branch -> its link lands in the thread
  -> a person merges in GitHub -> the pipeline deploys -> verdicts belong in #release-*
```

## 2. Three things that get confused

| | Slack connector | Channel resident | Channel subscription |
|---|---|---|---|
| What it is | a workspace's Slack credential and team routing | the channel's memory namespace and front desk | a fleet that answers mentions in one channel |
| Where it lives | vault handle `slack` + `connector.installs (slack, team_id)` | `connector.channels`, `kind` resident, one row per channel | the fleet's reviewed setup: a trigger with `type: mention`, `source: slack`, `channels: [<channel identifier>]` |
| Created by | Connect Slack (Open Authorization (OAuth)) | the first mention in the channel | a workspace member who asks for the channel in chat, reviews the setup and Saves it, or applies a reviewed improvement |
| Proves | the daemon may post into that team | nothing about which fleet answers | this fleet answers mentions here |

A connected card proves only the first column. The catalogue's "Connected" is vault-key membership (`held`, `afd_connector/src/grant/holding.rs:141-156`); the per-provider status reads a JSON handle with an `integration` field (`connection`, `holding.rs:107-127`). Neither checks a token is alive, an event subscription exists, or a channel is bound. The journey is proven by the drill in M206_004, never by a card.

## 3. The same shape GitHub already has

| Question | GitHub App ✅ | Slack mention ✅ |
|---|---|---|
| Ceiling | the repositories the App installation covers | the channels the bot user is a member of |
| Workspace routing | `installation.id` → `connector.installs` (`SELECT_INSTALL_WORKSPACE`, `afd_ingress/src/sql.rs:43-46`) | `team_id` → the same table and statement |
| Subscription | `webhook` trigger: `source: github`, `repositories`, `events` | `mention` trigger: `source: slack`, `channels` |
| Subscription identity | repository full name, compared case-insensitively (`serves_repository`, `afd_ingress/src/binding/mod.rs:250-256`); a rename stops routing | channel identifier (ID); a rename changes nothing |
| Who receives it | every matching fleet (fan-out) | exactly one fleet, or one notice |
| Replay key | producer `webhook_app`, `<fleet>:<body digest>` (`afd_ingress/src/deliver.rs:104-106`) | producer `slack_mention`, `<team_id>:<event_id>` |
| Credential | the fleet mints an installation token behind an approved `github` grant | the fleet holds no Slack credential; the daemon posts |

The GitHub subscription keys on a repository's name, so renaming a repository silently drops its events (`UZ-WH-022`). That weakness is recorded here and not fixed by M206.

**Against Claude Tag**, Claude's own Slack app, read through its documentation (`claude.com/docs/claude-tag/concepts/how-it-works.md`, `agent-identity.md`, `admins/add-connections.md`, `users/memory.md`, `users/use-cases/fix-bugs.md`):

| Claude Tag | here |
|---|---|
| one agent; an admin attaches access bundles (credentials, domains, plugins) to a channel or the workspace | a fleet (instructions, credentials, allowlist, budget) attached to one channel |
| a sandbox session per thread | one run per mention |
| memory per channel; public channels share it workspace-wide | memory per fleet; the resident's never leaves its channel |
| a service account in channels, no per-user linking | the workspace bot token and the GitHub App: the same |
| opens draft PRs with no approval step | the same: an attached write fleet opens one draft PR per request (§7) |
| live progress checklist in the thread | the final answer and notices; progress comes later |

Fleets are attached, never created per message: a fleet per message would start with empty memory ([`../memory.md`](../memory.md) §3).

## 4. Routing a mention

One mention produces one event or one notice, never both and never two.

1. **Filter.** Only `event_callback` with `event.type` `app_mention`. A message carrying `bot_id` or `subtype`, or sent by the bot user itself (`bot_user_id` is in the Slack handle, `afd_connector/src/grant/parse/mod.rs:146-149`; compared at `afd_api_ingress/src/handler/mention/mod.rs:252`), is acknowledged and dropped.
2. **Name.** Strip the leading bot mention. If the first word, without a trailing `:` or `,`, equals one subscribed fleet's name ignoring case, the mention is **addressed** to that fleet. Names are unique per workspace but case-sensitive (`schema/1500_fleet_fleets.sql:56`), so two subscribed fleets whose names differ only in case make that word ambiguous.
3. **Choose.**

| Subscribed fleets in the channel | Addressed | Unaddressed |
|---|---|---|
| none | no name can match, so it is unaddressed | the resident answers |
| one read-only | that fleet | that fleet |
| one write-bound | that fleet: at most one draft PR (§7) | notice naming it; it takes addressed requests only |
| several | the named one | notice listing the names; no model runs |
| any paused, named | notice: paused, and who can resume it | not eligible |

4. **Admit.** One `fleet.admissions` row, producer `slack_mention`, key `<team_id>:<event_id>`. The unique `(producer, producer_key)` (`schema/1720_fleet_admissions.sql`) turns every Slack retry into the first answer. Notices are keyed the same way, so a retried ambiguous mention is not answered twice.

The resident is materialised on the first mention in any channel, whatever the routing outcome, because it owns the channel's notices. It answers with a model only when no fleet subscribes, and when asked for something it cannot reach it names the attach command with this channel's identifier filled in.

## 5. What the fleet is told

Slack's mention event carries the mention and nothing else. The daemon re-reads the thread before admitting the event: `conversations.replies` for the thread root, parent first, then the latest replies, at most 20 messages, each capped, under a 1.5 second deadline, with the bot token loaded and the pool connection released before the call (`afd_connector/src/slack/replies/mod.rs`). The parent is the GitHub announcement, so it carries the run link. Attachment and block text are flattened into the message so the link survives.

The runner reads the request's `message` string and falls back to the whole body (`afd_wire/src/event/message.rs:14-24`). The daemon therefore composes one `message`: the mention text, then the thread under a fixed heading that labels it untrusted data. A failed or forbidden re-read (a private channel needs `groups:history`, which the app does not request) degrades to the mention alone, and the message says so.

## 6. How the answer comes back

The producer that owns a reply surface records the reply destination on the admission: provider `slack` and the address `{team_id, channel_id, thread_ts}` (the poster requires `channel_id` and `thread_ts`), where `thread_ts` is the mention's `thread_ts`, or its `ts` when it started the thread. The destination travels with the event, and an approval continuation inherits it from the event it resumes. The report transaction owes a delivery only when the event carries a destination, addressed by it. The Slack poster reads the address from the delivery obligation.

The report parses the recorded connector once, so the lease's **model** provider never reaches the delivery ledger (`afd_fleet/src/lease/obligation.rs`). A job its destination refuses for good, or one out of delivery cycles, is stamped abandoned, and no scan offers it again (`afd_outbound/src/abandon.rs`).

A run may say up to eight interim lines before its answer, through `POST /v1/runners/me/leases/{lease_id}/messages` (`MESSAGES_PER_RUN_MAX`, `afd_wire/src/message_verb/mod.rs`). The lease plane fences each line and counts it in `runner.leases.posted_message_count` (`schema/1810_runner_leases.sql`), so a reclaimed lease starts a new count. The daemon posts every Slack text, interim line and answer alike, as literal text, with `&`, `<` and `>` sent as Slack entities (`afd_outbound/src/slack/escape/mod.rs`, called from `SlackPoster::post`). Each line is stamped `agentsfleet_interim` with `{lease_id, line}` (`afd_connector/src/slack/answered/mod.rs`), so a repeat finds only its own line and never silences the answer.

## 7. Four stages, four authorities

| Stage | Who acts | Credential | Approval |
|---|---|---|---|
| Diagnosis | the responder fleet, on a mention | GitHub token with `contents`, `actions` and `checks` read; Grafana Viewer token; network read-only | none needed; it cannot write |
| Draft fix PR | the repairer fleet, addressed by name | GitHub token for `agentsfleet/linkwarden`, writing one daemon-issued branch and one draft PR against `dev` (`afd_gate/src/policy/egress/write.rs`) | none per request: attaching the repairer to the channel is the authorisation, as in Claude Tag |
| Merge | a person in GitHub | their own | the repository's branch protection |
| Deploy | Linkwarden's pipeline | the pipeline's | the release process; the setup checks that `agentsfleet-repair/*` branches cannot reach deploy secrets |

A Slack-requested run can do exactly what the attached fleet's own policy allows. For the repairer that is one daemon-issued branch in `agentsfleet/linkwarden` and one draft PR against trusted base `dev` (`rules` in `afd_gate/src/policy/egress/write.rs`): no rule admits a merge, a ref update, a deletion or GraphQL, the runner denies any request no rule matches on a ruled host (`origin_admits` in `afr_egress/src/admission/mod.rs`), and the token carries no workflow permission. A write-bound fleet never receives an unaddressed mention. A branch pushed into Linkwarden runs that repository's workflows, so the drill audits their secret paths for `agentsfleet-repair/*` branches. An installed grant covers the write, exactly as [`../connectors.md`](../connectors.md) §"Trust anchors" item 6 says for every other origin.

## 8. Evidence sources

| Source | How a fleet reads it | Known gaps |
|---|---|---|
| GitHub run | the fleet reads the linked run, failed jobs, steps and annotations through `http_request` under the repository's read prefix, as its instructions direct (M206_003 §2); it also reads commits and compare there (`rules`, `afd_gate/src/policy/egress/read.rs:21-30`) | A mint reads the installation's permissions first and asks for `contents` read plus whichever of `actions` and `checks` read it holds (`for_binding`, `afd_credential/src/credential/github/request.rs:85-96`). The App registration lists Checks: read-only and each installation accepts it before the drill (`playbooks/operations/github_app_registration/001_playbook.md:38-43`); until one does, the mint succeeds without it and the fleet's annotation read answers 403. Job logs answer with a redirect to storage outside the exact-host allowlist; the runner's client follows no redirect (`guarded`, `afr_egress/src/network/mod.rs:171-181`), so the fleet names a job-log gap. The storage hop is owner-held. |
| Grafana | workspace secret `grafana = {host, token}`; `Bearer ${secrets.grafana.token}`; Loki and alert reads over GET | No connector, by design ([connector archetypes](../connectors.md#archetypes)). The host must resolve publicly, because the tool rejects private addresses. Read-only is enforced by `network.read_only`, which is otherwise off by default (`afd_fleet_runtime/src/config/policy/mod.rs:202`). |
| Memory | the fleet's own namespace, hydrated only under a live lease for that fleet that no reclaim has moved past (`Plane::hydrate` in `afd_fleet/src/lease/memory.rs`) | none for this journey |

## 9. Integration map

| Integration | Connector (auth) | Events in | Actions out | In the first drill |
|---|---|---|---|---|
| GitHub | App install; per-lease installation token, repository-scoped, 1 hour | App ingress: `pull_request`; `workflow_run` completed with `failure` only; `deployment_status` dropped (`afd_api_ingress/src/handler/webhook/github/mod.rs:155-181`, `app_route.rs:22-29`) | fleet `http_request` inside egress rules | read token only |
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
| Channel → fleet | ✅ subscriptions: a `mention` trigger in the reviewed setup names the channel, matched by `is_attached_to` (`afd_fleet_runtime/src/config/attach.rs:17`) and read per mention (`mention_subscribers`, `afd_ingress/src/slack/mod.rs:73`); the resident: written as native system setup on the first mention in a channel and bound once (`resident`, `afd_api_ingress/src/handler/mention/resident.rs:110`; `Creations::materialise`, `afd_authoring/src/creation/system.rs:46`) | — | M206_002 |
| Thread context | ✅ `conversations.replies` under 1.5 s, parent plus latest replies, capped, under a fixed untrusted-data heading (`afd_connector/src/slack/replies/mod.rs:48-51,183`, `afd_ingress/src/slack/message.rs:20,52`) | — | M206_002 |
| Answer delivery | ✅ owed only to a recorded destination; abandoned when refused or out of cycles (slot 1740) | — | M206_001 |
| Slack poster | ✅ `chat.postMessage` from the job's own address; reads no event row (`afd_outbound/src/slack/mod.rs`) | — | M206_001 |
| Write reach from Slack | ✅ one ref and one draft PR, proven for a Slack-requested lease with no approval asked (`afd_gate/src/policy/egress/tests/slack.rs`, `afd_fleet/tests/integration_lease_gates/slack.rs`) | — | M206_003 |
| One run per approval | unverified: the parked delivery passes once approved (`afd_gate/src/gate/pass.rs:164-165`) while approval also admits a continuation (`afd_approval/src/inbox/resolve/mod.rs:149-152`) | exactly one run | its own spec; off this path, since Slack requests do not park |
| Continuous Integration (CI) evidence reach | 🟡 the read mint asks for `actions` and `checks` read where the installation holds them, and the read rule covers the CI paths (M206_003 §1–§2); logs unreachable | every installation accepts the App's Checks permission; the job-log storage hop is owner-held | M206_003 |
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
- The resident's memory is the channel's; a subscribed fleet never reads another fleet's memory.
- A Slack-requested write run reaches one daemon-issued branch and one draft PR, and never merges or deploys.
- A write-bound fleet never receives an unaddressed mention.
- A fleet reads CI evidence only inside its bound repository.
- A fleet cites a job log line only if its own `http_request` returned that text; a redirect it cannot follow becomes a named gap.
- No fleet holds the Slack bot token; the daemon posts.

## 13. Proof status

Nothing here is proven live. The Slack registration playbook's live check, a threaded reply to a mention (`playbooks/operations/slack_app_registration/001_playbook.md:87-99`), has not been recorded against the Rust daemon. M206_004 records the development and production drills; this section changes to one line per proven stage when they land.
