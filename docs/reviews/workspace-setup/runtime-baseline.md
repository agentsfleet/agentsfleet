---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Runtime behavior the cutover preserves

## What it is

Source-inspected behavior at `ad3ed8f92c739b59df71863dbeec81d621dd7d0a`, on `feat/website-completion`. The branch includes merged Pull Request (PR) #750, M225's readiness wake hints. New chat behavior is proposed separately; this page makes no fresh runtime/test claim.

## Why it exists

The supplied two-fleet story and [checked walkthrough](scenarios/two-fleets-github-slack.md) are useful review inputs, but source decides ordering, acceptance, billing, and replies. Authoring must reach the existing runtime through reviewed setup rather than replace its event processor.

## How it behaves

```text
reviewed candidate --> retained installing fleet --> grant/stream barrier
                                                      |
                                               checked activation
                                                      |
          GitHub App / Slack / cron / manual message --+
                                                      |
                                   durable admission + inbox receipt
                                                      |
                                 fleet:{id}:events + readiness mark
                                                      |
                                   wake hint + normal partition rotation
                                                      |
                                      affinity-owned fenced lease
                                                      |
                                   model + permitted tools + child work
                                                      |
                                   report transaction + optional obligation
                                      |                           |
                                 app/CLI history          outbound Slack thread
```

| Behavior | Source evidence | Required preservation / correction |
|---|---|---|
| Install | `afd_fleet_lifecycle/src/install.rs:227–245,260–272` | Inspected install activates before best-effort grant seeding. Target introduces a real grant barrier and checked activation; do not call that already implemented. |
| Credential classification | `afd_fleet_lifecycle/src/install/grants.rs:84–91` | Grant seeding reads declared secrets locally. Authoring model must never inherit the secret-reading capability. |
| Installed setup | `afd_fleet/src/lease/installed.rs:110–125`, `lease/answer.rs:91–109` | Existing runner receives policy, instructions, and optional bundle manifest. New typed definition supplies the same validated meaning; all money readers move to nanos. |
| Producers | `afd_admission/src/admission.rs:15–50`, `admit_receipt.rs:43–61` | Seven producers including SlackMention. Ordinary accepted messages have durable admission; append can be deferred and receipt can be absent. Target start.accepted additionally requires append and saved receipt. |
| GitHub replies | `afd_ingress/src/deliver.rs:104–120` | App delivery binds Reply::None. Chat cutover does not add a GitHub poster or write permission. |
| Slack replies | `afd_ingress/src/slack/admit.rs:58–72`, `afd_fleet/src/lease/obligation.rs`, `afd_outbound/src/slack.rs` | Preserve thread-addressed reply obligations. Installing fleets are invisible to routing. Paused/stopped fleets receive an honest lifecycle notice. |
| Signature validation | `afd_api_ingress/src/handler/webhook/app_route.rs` | Receive raw bytes, verify their signature, then trust parsed fields. Never claim verification happens without reading bytes. |
| Wake hints | `afd_dragonfly/src/ready/partition.rs:35–49`, `ready/doorbell.rs` | Sixteen partitions; bounded in-process hints accelerate ordinary rotation. Lost hints do not lose durable work. |
| Candidate scan | `afd_fleet/src/lease/assign/wake.rs:109–196`, `lease/assign.rs:57` | Rotation still runs beside a distinct rung partition; bounded64-ID sampling and failed-peek ring clearing remain intact. |
| Fleet exclusion | `afd_fleet/src/lease/sql/lease.rs`, `lease/sql/report.rs:145–147` | Affinity owns exclusion, not simply a process-local mutex. Different fleets can run concurrently; stale reports lose the fence. |
| Queue wait | `afd_fleet/src/lease/deliver/queue_wait.rs:22–34` | Admission/lease stamps use daemon wall-clock time, with monotonic poll duration added. Negative skew clamps to zero. Reclaimed/redelivered work does not claim fresh wait. |
| Children | `afr_agent/src/nested/start.rs:20–40`, `nested/wait.rs:15–30` | Tools are optionally narrowed; omitted tools inherit the parent's selection. Child waits are bounded separately. No new child-role schema or child display API is introduced. |
| Renewed budget | `afd_fleet/src/lease/coverage.rs:108–110` | Renewal rereads stored budget. Current work does not unconditionally ignore setup edits: live ceilings/revocations remain effective. |
| Renewal expiry | `afd_fleet/src/lease/sql/renew.rs:78–102` | Current guard lacks original-expiry rejection. Target serializes both renewal kinds with claims under the runner guard, using fresh database time, and never revives an expired slot. |
| Scoped model delivery | `afd_fleet/src/lease/answer.rs:76–86`, `afr_providers/src/connect.rs:90–94` | Existing runtime uses a raw provider key; scoped/proxy delivery is unimplemented. The target adds a daemon-mediated authoring boundary with no runtime credential authority. |
| Report atomicity | `afd_fleet/src/lease/commit.rs`, `lease/report.rs:24–32` | Charge, result, session, affinity release, and owed delivery commit together; activity/queue append happens afterward. |
| Outbound completion | `afd_outbound/src/obligation.rs:18–28` | Queued reply is not delivered reply. Separate completed investigation, pending delivery, delivered, and abandoned states. |
| Steer recovery | `afd_api_tenant/src/handler/fleet/message_steer.rs:230–240`, `afd_admission/src/admit_receipt.rs:43–61`, `afd_events/src/history/statement/queued.rs:10–14` | Append deferral can succeed without a receipt; post-append receipt failure can leave an uncertain response. Existing queued reads omit these rows. Target message reads include durable pending admissions by logical event ID, preserving one visible message across sweep/lease/retry. |
| Slack copy | `afd_ingress/src/slack/notice.rs:51–56` | Existing paused wording also covers stopped. Correct text in this cutover; preserve refusal to admit non-active work. |

Crate paths in the table are relative to `rustd/crates/`. The checked runtime's stream/readiness topology remains authoritative; pending M226's proposed queue move is not assumed merged. New authoring uses its own private durable messages and scoped progress hints.

## Limits

No new authoring API, migration, installer, or runner capability is implemented by source inspection. Repository unit/integration runs and fresh coverage remain deferred by Indy. The one-PR spec carries failure-injected tests for every preserved behavior affected by the cutover.

## Related pages

- [Two fleets through GitHub and Slack](scenarios/two-fleets-github-slack.md)
- [API gaps](api.md)
- [ASCII flow](../../architecture/data_flow.md#fleet-setup-and-runtime-sequence)
