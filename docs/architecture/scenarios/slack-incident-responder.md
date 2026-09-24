# Scenario — Incident responder in a Slack thread

> Parent: [`README.md`](./README.md) · References: [`slack-channel-resident.md`](./slack-channel-resident.md) (the channel's resident), [`github-pr-reviewer.md`](./github-pr-reviewer.md) (the repository subscription this mirrors), [`production-deploy-repair.md`](./production-deploy-repair.md) (the repair crew), [`../connectors.md`](../connectors.md), [`../data_flow.md`](../data_flow.md), [`../memory.md`](../memory.md).
>
> **Specified, not built.** The four workstreams live in `docs/v2/pending/M206_001…M206_004`. Every "today" claim below was read from source at `b1bc6f0c4`; the retired Zig daemon was read at `1ad07eb2` in `~/Projects/oss/zig/agentsfleet_zig`.

Legend: ✅ in the Rust daemon · 🟡 in the Rust daemon, broken · 🔨 specified, not built · ⛔ only in the retired Zig daemon.

**Outcome under test:** a failed GitHub Actions run is announced in `#ci-dev`. Someone replies in that announcement's thread with `@agentsfleet why did this fail?`. One fleet answers in the same thread with evidence it read from GitHub and Grafana and a proposed fix. Asked to, the channel's repairer opens one draft fix Pull Request (PR): attaching it to the channel was the authorisation, as in Claude Tag. Merging and deploying stay with people and the pipeline.

## 1. The flow

```text
GitHub Actions --(GitHub's own Slack app, external)--> #ci-dev  "run 123 failed on main"
                                                            |
a person, in that thread:  "@agentsfleet why did this fail?"
                                                            |  signed app_mention
                                                            v
agentsfleetd:  check signature -> Slack team -> workspace -> who listens in this channel?
                                                            |
        +----------------------+----------------------------+----------------------------+
        | nobody               | one fleet                  | several fleets             |
        v                      v                            v
  the channel's resident   that fleet             "@agentsfleet <name> ..." -> that fleet
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

later, in the same thread:  "@agentsfleet ci-dev-repairer open the fix"
  -> the repairer attached to this channel runs; no per-request approval (as in Claude Tag)
  -> one draft PR on one daemon-issued branch -> its link lands in the thread
  -> a person merges in GitHub -> the pipeline deploys -> verdicts belong in #release-*
```

## 2. Three things that get confused

| | Slack connector | Channel resident | Channel subscription |
|---|---|---|---|
| What it is | a workspace's Slack credential and team routing | the channel's memory namespace and front desk | a fleet that answers mentions in one channel |
| Where it lives | vault handle `slack` + `core.connector_installs (slack, team_id)` | `core.connector_channels`, `kind` resident, one row per channel | the fleet's `TRIGGER.md`: `type: mention`, `source: slack`, `channels: [<channel identifier>]` |
| Created by | Connect Slack (Open Authorization (OAuth)) | the first mention in the channel | a workspace member: `agentsfleet install --library <id> --slack-channel <channel ID>`, or a `TRIGGER.md` edit |
| Proves | the daemon may post into that team | nothing about which fleet answers | this fleet answers mentions here |

A connected card proves only the first column. The catalogue's "Connected" is vault-key membership (`afd_connector/src/grant/holding.rs:129-142`); the per-provider status reads a JSON handle with an `integration` field (`holding.rs:95-115`). Neither checks a token is alive, an event subscription exists, or a channel is bound. The journey is proven by the drill in M206_004, never by a card.

## 3. The same shape GitHub already has

| Question | GitHub App ✅ | Slack mention 🔨 |
|---|---|---|
| Ceiling | the repositories the App installation covers | the channels the bot user is a member of |
| Workspace routing | `installation.id` → `core.connector_installs` (`afd_ingress/src/sql.rs:49-52`) | `team_id` → the same table and statement |
| Subscription | `webhook` trigger: `source: github`, `repositories`, `events` | `mention` trigger: `source: slack`, `channels` |
| Subscription identity | repository full name, compared case-insensitively (`afd_ingress/src/binding.rs:259-265`); a rename stops routing | channel identifier (ID); a rename changes nothing |
| Who receives it | every matching fleet (fan-out) | exactly one fleet, or one notice |
| Replay key | producer `webhook_app`, `<fleet>:<body digest>` (`afd_ingress/src/deliver.rs:104-106`) | producer `slack_mention`, `<team_id>:<event_id>` |
| Credential | the fleet mints an installation token behind an approved `github` grant | the fleet holds no Slack credential; the daemon posts |

The GitHub subscription keys on a repository's name, so renaming a repository silently drops its events (`UZ-WH-022`). That weakness is recorded here and not fixed by M206.

**Against Claude Tag**, Claude's own Slack app, read through its documentation (`claude.com/docs/claude-tag/concepts/how-it-works.md`, `agent-identity.md`, `admins/add-connections.md`, `users/memory.md`, `users/use-cases/fix-bugs.md`):

| Claude Tag | here |
|---|---|
| one agent; an admin attaches access bundles (credentials, domains, plugins) to a channel or the workspace | a fleet (skill, credentials, allowlist, budget) attached to one channel |
| a sandbox session per thread | one run per mention |
| memory per channel; public channels share it workspace-wide | memory per fleet; the resident's never leaves its channel |
| a service account in channels, no per-user linking | the workspace bot token and the GitHub App: the same |
| opens draft PRs with no approval step | the same: an attached write fleet opens one draft PR per request (§7) |
| live progress checklist in the thread | the final answer and notices; progress comes later |

Fleets are attached, never created per message: a fleet per message would start with empty memory ([`../memory.md`](../memory.md) §3).

## 4. Routing a mention

One mention produces one event or one notice, never both and never two.

1. **Filter.** Only `event_callback` with `event.type` `app_mention`. A message carrying `bot_id` or `subtype`, or sent by the bot user itself (`bot_user_id` is in the Slack handle, `afd_connector/src/grant/parse.rs:149-152`), is acknowledged and dropped.
2. **Name.** Strip the leading bot mention. If the first word, without a trailing `:` or `,`, equals one subscribed fleet's name ignoring case, the mention is **addressed** to that fleet. Names are unique per workspace but case-sensitive (`schema/500_fleets.sql:57`), so two subscribed fleets whose names differ only in case make that word ambiguous.
3. **Choose.**

| Subscribed fleets in the channel | Addressed | Unaddressed |
|---|---|---|
| none | no name can match, so it is unaddressed | the resident answers |
| one read-only | that fleet | that fleet |
| one write-bound | that fleet: at most one draft PR (§7) | notice naming it; it takes addressed requests only |
| several | the named one | notice listing the names; no model runs |
| any paused, named | notice: paused, and who can resume it | not eligible |

4. **Admit.** One `core.fleet_admissions` row, producer `slack_mention`, key `<team_id>:<event_id>`. The unique `(producer, producer_key)` (`schema/910_fleet_admissions.sql`) turns every Slack retry into the first answer. Notices are keyed the same way, so a retried ambiguous mention is not answered twice.

The resident is materialised on the first mention in any channel, whatever the routing outcome, because it owns the channel's notices. It answers with a model only when no fleet subscribes, and when asked for something it cannot reach it names the attach command with this channel's identifier filled in.

## 5. What the fleet is told

Slack's mention event carries the mention and nothing else. The daemon re-reads the thread before admitting the event: `conversations.replies` for the thread root, parent first, then the latest replies, at most 20 messages, each capped, under a 1.5 second deadline, with the bot token loaded and the pool connection released before the call. The retired Zig daemon did the same (`src/agentsfleetd/http/handlers/connectors/slack/events.zig:219-226` at `1ad07eb2`). The parent is the GitHub announcement, so it carries the run link. Attachment and block text are flattened into the message so the link survives.

The runner reads the request's `message` string and falls back to the whole body (`src/runner/child_exec_input.zig:99-105`). The daemon therefore composes one `message`: the mention text, then the thread under a fixed heading that labels it untrusted data. A failed or forbidden re-read (a private channel needs `groups:history`, which the app does not request) degrades to the mention alone, and the message says so.

## 6. How the answer comes back

The producer that owns a reply surface records the reply destination on the admission: provider `slack` and the address `{team_id, channel_id, thread_ts}` (the poster requires `channel_id` and `thread_ts`), where `thread_ts` is the mention's `thread_ts`, or its `ts` when it started the thread. The destination travels with the event, and an approval continuation inherits it from the event it resumes. The report transaction owes a delivery only when the event carries a destination, addressed by it. The Slack poster reads the address from the delivery obligation.

Today every non-empty answer is owed to the lease's **model** provider: `commit.rs:198-210` passes `lease.provider`, which is the provider resolved at billing (`schema/610_runner_leases.sql:35-39`). The dispatcher cannot parse `anthropic` as a connector and drops the job as permanent (`afd_outbound/src/poster.rs:77-92`). Nothing stamps the row, so the recovery scan re-appends it every `LOST_AFTER` of 300 seconds without end (`afd_outbound/src/obligation/sql.rs:111-116`, `producer.rs:59-66`). The Zig daemon took the provider from the channel binding and owed nothing for an unbound fleet (`src/agentsfleetd/fleet/service_report_outbound.zig:25-46` at `1ad07eb2`).

## 7. Four stages, four authorities

| Stage | Who acts | Credential | Approval |
|---|---|---|---|
| Diagnosis | the responder fleet, on a mention | GitHub token with `contents`, `actions` and `checks` read; Grafana Viewer token; network read-only | none needed; it cannot write |
| Draft fix PR | the repairer fleet, addressed by name | GitHub token for one repository, writing one daemon-issued branch and one draft PR (`afd_gate/src/policy/egress/write.rs`) | none per request: attaching the repairer to the channel is the authorisation, as in Claude Tag |
| Merge | a person in GitHub | their own | the repository's branch protection |
| Deploy | the repository's pipeline | the pipeline's | the release process; rollout verdicts belong in `#release-dev` / `#release-prod`; the setup keeps deploy secrets off `agentsfleet-repair/*` branches |

A Slack-requested run can do exactly what the attached fleet's own policy allows. For the repairer that is one daemon-issued branch and one draft PR against the trusted base (`afd_gate/src/policy/egress/write.rs:54-91`): no rule admits a merge, a ref update, a deletion or GraphQL, the runner denies any request no rule matches on a ruled host (`src/runner/engine/runtime/http_request_policy.zig:22-30`), and the token carries no workflow permission. A write-bound fleet never receives an unaddressed mention. A branch pushed into the same repository runs its workflows, so the drill's setup keeps deploy secrets off `agentsfleet-repair/*` branches. An installed grant covers the write, exactly as `connectors.md` trust anchor 6 says for every other origin.

## 8. Evidence sources

| Source | How a fleet reads it | Known gaps |
|---|---|---|
| GitHub run | the fleet reads the linked run, its jobs and their annotations over `http_request` under the read prefix, as its SKILL.md directs (M206_003 §2); the fleet reads commits and compare itself under the read egress prefix (`afd_gate/src/policy/egress/read.rs:14-27`) | A mint reads the installation's permissions first and asks for `contents` read plus whichever of `actions` and `checks` read it holds (`afd_credential/src/credential/github/request.rs:85`); the platform App grants no Checks permission (`playbooks/operations/github_app_registration/001_playbook.md:33-38`), so today the mint succeeds without it and the fleet's annotation read answers 403. Job logs answer with a redirect to storage the exact-host allowlist cannot name (`src/runner/network/AllowList.zig:154`), and the tool never follows redirects; that hop is owner-held. |
| Grafana | workspace secret `grafana = {host, token}`; `Bearer ${secrets.grafana.token}`; Loki and alert reads over GET | No connector, by design ([`connectors.md` §Archetypes](../connectors.md)). The host must resolve publicly, because the tool rejects private addresses. Read-only is enforced by `network.read_only`, which is otherwise off by default (`afd_fleet_runtime/src/config/policy.rs:154-157`). |
| Memory | the fleet's own namespace, hydrated only under a live lease for that fleet (`afd_fleet/src/lease/memory.rs:45-50`) | none for this journey |

## 9. Integration map

| Integration | Connector (auth) | Events in | Actions out | In the first drill |
|---|---|---|---|---|
| GitHub | App install; per-lease installation token, repository-scoped, 1 hour | App ingress: `pull_request`; `workflow_run` completed with `failure` only; `deployment_status` dropped (`afd_api_ingress/src/handler/webhook/github.rs:159-186`, `app_route.rs:22-32`) | fleet `http_request` inside egress rules | read token only |
| Slack | OAuth bot token: `app_mentions:read`, `chat:write`, `channels:history` | handshake only today; `app_mention` in M206_002 | daemon `chat.postMessage` | yes |
| Grafana | none; workspace secret | none | fleet reads | yes, read-only |
| Zoho Desk | OAuth refresh, multi-data-center, `Desk.*.READ` | none | none | no |
| Zoho Recruit bundle | static secret `zoho_recruit`, never refreshed | none | fleet `http_request` | no |
| Zoho Sprints bundle | rides the Desk grant, whose Desk scopes cannot read Sprints | none | fleet `http_request` | no |
| Linear | OAuth refresh: `read`, `comments:create` | no connector ingress; a per-fleet signed webhook exists | none from the daemon | no |
| Jira | OAuth refresh plus site lookup | none | none | no |

## 10. Current versus required

| Capability | Rust daemon today | Required | Workstream |
|---|---|---|---|
| Signed Slack delivery | ✅ signature, 5-minute window, `url_verification` echo (`afd_api_ingress/src/handler/events.rs:192-217`) | unchanged | — |
| Mention becomes an event | ✅ one `slack_mention` admission per Slack event, keyed `<team_id>:<event_id>` (`afd_api_ingress/src/handler/mention.rs:195`, `afd_admission/src/lib.rs:110`) | — | M206_002 |
| Channel → fleet | ✅ subscriptions: a `mention` trigger names the channel, written by `install --slack-channel` (`afd_fleet_runtime/src/config/attach.rs:39`) and read per mention (`afd_ingress/src/slack/mod.rs:67`); the resident: installed on the first unattached mention and bound once (`afd_api_ingress/src/handler/mention/resident.rs:42`) | — | M206_002 |
| Thread context | ✅ `conversations.replies` under 1.5 s, parent plus latest replies, capped, under a fixed untrusted-data heading (`afd_connector/src/slack/replies.rs:179`, `afd_ingress/src/slack/message.rs:52`) | — | M206_002 |
| Answer delivery | ✅ owed only to a recorded destination; abandoned when refused or out of cycles (slots 918–920) | — | M206_001 |
| Slack poster | ✅ `chat.postMessage` from the job's own address; reads no event row (`afd_outbound/src/slack.rs`) | — | M206_001 |
| Write reach from Slack | ✅ one ref and one draft PR, proven for a Slack-requested lease with no approval asked (`afd_gate/src/policy/egress/tests/slack.rs`, `afd_fleet/tests/integration_lease_gates/slack.rs`) | — | M206_003 |
| One run per approval | unverified: the parked delivery passes once approved (`afd_gate/src/gate/pass.rs:162-163`) while approval also admits a continuation (`afd_approval/src/inbox/resolve.rs:154-155`) | exactly one run | its own spec; off this path, since Slack requests do not park |
| Continuous Integration (CI) evidence reach | 🟡 the read mint asks for `actions` and `checks` read where the installation holds them, and the read rule covers the CI paths (M206_003 §1–§2); the App registration lacks Checks: read, so annotations answer 403 | the App grants Checks: read and installations accept; the log's storage hop is owner-held | M206_003 |
| Grafana read-only | ✅ when the fleet declares `network.read_only` | declared by the drill bundle | M206_004 |
| External setup | not in code | playbook: apps, channels, subscriptions, App permissions, Grafana token | M206_004 |

## 11. Channels and environments

| Channel | Slack app | Deployment | Bound fleets |
|---|---|---|---|
| `#ci-dev` | `agentsfleet-dev` | `api-dev.agentsfleet.net` | `ci-dev-responder` (read), `ci-dev-repairer` (write, addressed only) |
| `#ci-prod` | `agentsfleet` | `api.agentsfleet.net` | `ci-prod-responder`, `ci-prod-repairer` |
| `#release-dev`, `#release-prod` | not invited in M206 | — | none; rollout verdicts and release discussion stay human until a later milestone posts verifier results there |

Development runs first. The production app and `#ci-prod` follow only after the development drill passes, the order `playbooks/operations/slack_app_registration/001_playbook.md` already sets.

## 12. Invariants

- One Slack event admits at most one event or one notice, across retries.
- A fleet answers in a channel only when its own `TRIGGER.md` names that channel's ID.
- A delivery is owed only to a destination its producer recorded; a model provider never reaches the delivery ledger.
- An obligation the destination permanently refuses is abandoned and never re-offered.
- The resident's memory is the channel's; a subscribed fleet never reads another fleet's memory.
- A Slack-requested write run reaches one daemon-issued branch and one draft PR, and never merges or deploys.
- A write-bound fleet never receives an unaddressed mention.
- A fleet reads CI evidence only inside its bound repository.
- No fleet holds the Slack bot token; the daemon posts.

## 13. Proof status

Nothing here is proven live. The Slack registration playbook's live check, a threaded reply to a mention (`playbooks/operations/slack_app_registration/001_playbook.md:87-99`), cannot pass against the Rust daemon today. M206_004 records the development and production drills; this section changes to one line per proven stage when they land.
