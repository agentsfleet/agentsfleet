# Scenario — Chief of staff: reach any fleet by name, and let one fleet run the others

> Parent: [`README.md`](./README.md) · Spec: `docs/v2/done/M252_001_P1_API_CLI_DOCS_INFRA_UI_REACH_FLEETS_BY_NAME_AND_STEER.md`

> M252_001 owns the proofs; this page owns the shape.

**Outcome under test:** a person reaches any fleet by its name, and a fleet hands work to any fleet in its workspace and hears back. Nobody copies a UUID, and nobody relays between fleets.

## The cast

Two fleets, both saved from fixtures under `rustd/crates/protocol/af_fleet_runtime/tests/fixtures/fleets/`.

| Fleet | What it does | Tools |
|---|---|---|
| `runner-link-watch` | Reads Grafana, read-only, and writes one report on the runner link. | `http_request`, memory, `fleet_steer` to report back |
| `chief-of-staff` | The one fleet the owner talks to. It reads every fleet and hands work on. | `fleet_activity`, `fleet_steer` |

## 1. A person reaches a fleet by name

```text
command line   agentsfleet fleet steer @runner-link-watch "Hourly runner link check"
dashboard      in any fleet's chat:   @runner-link-watch Hourly runner link check
                                      the browser opens runner-link-watch's chat
Slack          @agentsfleet-dev runner-link-watch: Hourly runner link check
```

The name is the fleet's saved name, the one the sidebar shows. The command line and the dashboard look it up with `GET …/fleets?name=`, then send an ordinary message. Slack has one bot for every fleet, so there the first word after the bot names the fleet.

## 2. A fleet hands work to a fleet, and hears back

```text
owner, in the chief's chat:  "have runner-link-watch run its check and tell me the result"

chief-of-staff      fleet_activity()                       find runner-link-watch's id
                    fleet_steer(<watch id>, "run the hourly check")
                    answers at once: "asked runner-link-watch"          it does not wait

runner-link-watch   runs under its own grants and budget
                    its message begins: [from fleet chief-of-staff, fleet_id <id>; …]
                    writes its report, then
                    fleet_steer(<chief id>, "✅ No action needed · lost 0.2 % · …")

chief-of-staff      wakes for a second run:  "runner-link-watch: no action needed"
```

A digest is the same fleet reading without steering. `fleet_activity()` lists every fleet with its status, and one more read per fleet gives its latest result.

## 3. The rules that bound it

- A name grants nothing. A person's message passes the message route's own guard, or Slack's channel subscription.
- An unknown `@name` is refused. It never falls back to the fleet whose chat was open.
- A fleet's target is another fleet in its own workspace, and active.
- The fleet's `tools` list is the only consent. Nothing lists targets, limits the chain or caps the count.
- Authority never widens. The target runs under its own revision, grants, gates and budget, and a steer carries text, never policy.
- A fleet's steer runs as its own run. A resent steer admits once. A runner that lost its run writes none.
- Prompt injection is accepted, and stated. A fleet that reads untrusted text and holds `fleet_steer` can be told to steer any fleet in its workspace. Each target's own gates and budget bound the damage.

## 4. Not this feature

- A wait primitive. The receiver steers back, or the sender reads with `fleet_activity`.
- A picker for `@` in the composer, and a Slack mention per fleet.
- Reaching a fleet in another workspace.
