# Scenario — Fleet to fleet: one fleet steers another and reads what it did

> Parent: [`README.md`](./README.md) · Spec: `docs/v2/pending/M239_001_P1_API_DOCS_INFRA_UI_FLEET_TO_FLEET_STEER_AND_ACTIVITY.md`
>
> References: [`../data_flow.md`](../data_flow.md) §"B. TRIGGER" · [`../lease_flow.md`](../lease_flow.md) §"4. How AGENT BOB 01 can reply" · [`../runner_execution.md`](../runner_execution.md) §"Tool catalog" · [`../memory.md`](../memory.md) §1

> **Proposed.** Only §0 runs today. The spec owns the proofs; this page owns the shape.

**Outcome under test:** a fleet holding the `fleet_steer` tool puts work on another fleet in its workspace by fleet id. The work takes the path a person's message already takes.

A fleet holding the `fleet_activity` tool reads which fleets exist, their status and their recent answers, with no API key. The person reads the result in the sending fleet's chat.

## 0. Today: a person is the wire

```text
#ci-dev thread
  responder: diagnosis ──► a person reads it ──► "@agentsfleet-dev ci-dev-repairer open the fix" ──► repairer runs
```

The relay is a human ([`slack-incident-responder.md`](./slack-incident-responder.md) §1). "Fleet 1 never wakes Fleet 2 automatically" ([`production-deploy-repair.md`](./production-deploy-repair.md)).

| What exists | Where |
|---|---|
| A person steers a fleet by id. It is the only place work from a person enters a fleet's stream. | `rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs`, `rustd/crates/afd_events/src/steer/mod.rs` |
| A fleet posts one line to the thread it was asked from. The daemon holds the channel credential. | `rustd/crates/afr_tools/src/verbs/message/mod.rs`, `POST /v1/runners/me/leases/{lease_id}/messages` |
| A fleet sends a push notification. | `rustd/crates/afr_tools/src/pushover/mod.rs` |
| Fleets share facts through memory a workspace admin lets them publish and read. | [`../memory.md`](../memory.md) §1 |
| A run starts child loops inside itself: same lease, same grants, gone when the run ends. | `rustd/crates/afr_tools/src/nested/mod.rs` |
| A fleet edits only its own schedules. No verb names another fleet. | `rustd/crates/afr_tools/src/verbs/mod.rs` |

## 1. Chief of staff

One fleet the owner talks to. It reads every fleet and may steer any of them.

```text
08:00  schedule fires chief-of-staff
         fleet_activity()            ──► responder: active, 3 runs, last 02:14
                                          repairer: active, last answer "draft PR #412"
                                          docs-bot: stopped, monthly budget spent 03:40
         answer in its chat          ──► "Night: one incident, repaired to draft #412; docs-bot is out of budget."

10:12  owner, in the chief's chat: "have the repairer retry run #991 against dev"
         fleet_steer(<repairer id>, "retry run #991 against dev")
                                     ──► repairer runs under its own grants and budget
         next turn: fleet_activity(<repairer id>) ──► "opened draft PR #413"

10:13  owner: "is docs-bot awake?"
         fleet_activity(<docs-bot id>) ──► status, last run.  docs-bot is not woken.
```

"Awake" is a daemon fact: status and last run (`rustd/crates/afd_fleet_lifecycle/src/lib.rs`, `FleetStatus`). Reading that fact costs no model call on the fleet asked. A digest agent is this fleet on a schedule.

## 2. Incident crew in `#ci-dev`

```text
#ci-dev
  CI bot:     run #991 failed
  dev:        @agentsfleet-dev why did this fail?
  responder:  job log line · Loki line · "null config in the migrate step"
              ↳ asked ci-dev-repairer to fix it                 ← fleet_steer; the thread is inherited
  repairer:   draft PR #412 → link                               ← answers in this thread
```

The repairer answers where the responder was asked because the steer copies the responder's reply destination onto the repairer's admission. An approved gate's continuation already copies a destination this way, within one fleet (`rustd/crates/afd_approval/src/inbox/resolve/mod.rs`, `Reply::Inherit`). The spec widens the copy to one workspace.

## 3. What is new, what it reuses

| Piece | Reuses | New |
|---|---|---|
| `fleet_steer` tool | `message` verb shape: `LeaseVerbs`, a lease-proven runner route, refusal codes the model reads (`rustd/crates/afr_tools/src/verbs/`) | the tool, `POST /v1/runners/me/leases/{lease_id}/steers` |
| Admission | `Producer::Steer`, key `<fleet_id>:<operation_id>` | `Steer::append_from_fleet`, actor `fleet:<sender_fleet_id>`, hop and sender name in the stored body |
| Reply in the sender's thread | `Reply::Inherit` | `insert_admission.sql` joins the inherited row by workspace, not by fleet |
| `fleet_activity` tool | lease proof by fencing token, as the schedules `GET` does | two statements (fleets with last event; events with an answer excerpt), `GET /v1/runners/me/leases/{lease_id}/fleets` and `.../fleets/{fleet_id}/events` |
| Chat label | the actor-to-summary map (`ui/packages/app/lib/events/event-summary.ts`) | a `fleet:` arm reading `sender_fleet_name` from the stored body |
| Owner consent | the fleet's `tools` list in its reviewed revision | none: no allowlist, no subscription, no rule |

## 4. The rules that bound it

- The target is in the sender's workspace and active. Anything else is the refusal a person would get.
- Authority never widens. The target runs under its own pinned revision, grants, gates and budget. A steer carries text, never policy.
- A chain stops at hop 2: A→B→C runs, and the daemon refuses C's steer to D.
- A lease sends at most 8 steers, counted apart from its 8 `message` posts.
- A resent steer admits once: the key is the sender's event and a hash of the words.
- The actor is `fleet:<fleet_id>`. Readers of `steer:%`, onboarding's "first steer" and the history filter, never count a fleet's steer as a person's.
- The spec accepts the prompt-injection risk and states it. A fleet that reads other fleets' answers and may steer them is the widest target in the workspace. Its blast radius is each receiver's own gates.

## 5. Current versus required

| Stage | Current | Required | Proof |
|---|---|---|---|
| Steer by fleet id | a person, through the tenant route | a fleet, through a runner route | M239_001 §2 |
| Read another fleet's status and answers | the dashboard and CLI, with a session or API key | a tool over the lease plane, no key | M239_001 §1 |
| Reply lands in the sender's thread | same fleet only | any fleet in the workspace | M239_001 §2 |
| The chat labels a fleet-sent event | — | "asked by <fleet name>" | M239_001 §4 |

## 6. Not this feature

- Named outcomes, subscriptions or a new trigger type. A steer reaches every fleet whatever it declares (`rustd/crates/afd_fleet_runtime/src/config/trigger/mod.rs`).
- Waking the sender when the receiver answers. The sender reads the answer with `fleet_activity`; a durable wait is M236's.
- A fleet posting to a channel other than the thread it was asked from.
- Steering across workspaces.
