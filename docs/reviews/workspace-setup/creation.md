---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Save, setup, retry, and cancellation

## What it is

The proposed creation barrier uses the existing fleet resource's `installing` state. All setup steps run in bounded requests. There is no background creation worker or separate creation resource. Exact request shapes are in [the resource reference](../../architecture/api_resources.md).

## Why it exists

`rustd/crates/afd_fleet_lifecycle/src/install.rs:223` creates a new fleet identity and `:232–250` rolls back failure. `install/grants.rs:39–69` makes grant seeding best effort. `install.rs:263–271` activates without checking affected rows. The target must retain one identity across retries, wait for actual grants, and prevent a concurrent Kill from being overwritten by late activation.

## How it behaves

### Review and first Save

A draft has no queued/running message when Save begins. GET draft returns normalized effective setup, immutable requirements, live readiness, and a digest. The person chooses Save or Save and run now; trigger content is identical. Library installation of an unchanged revision bypasses a draft and reads its review through Inspect. Changing its name/model/setup opens a draft so every effective input has been reviewed.

POST fleets carries a UUIDv7 `Idempotency-Key`, immutable candidate or Library revision, reviewed name, review digest, and start choice. Authorize first, then record key/body digest and one fleet identity in a short transaction. A different body for the accepted key returns409. A same-key retry resumes that fleet; a progress response may advance, but accepted inputs and identity never change. Keep the content-free key receipt while the fleet exists and a 24-hour tombstone after purge; tombstone replay returns 410.

The draft enters `saving` with the linked fleet identity. A previously published candidate's Library receipt remains a source pin; do not publish it again or change an existing revision. Save itself creates no Library entry. Publishing before Save and Save without publication both work.

### Bounded setup checks

1. Confirm source bytes, schema, manifest, review digest, and policy marker.
2. Resolve the workspace owner's runtime model and wallet; recheck current membership and fleet scope.
3. Verify declared connections and credentials by name without putting secret values in the candidate or response.
4. Create or locate the required approval requests. A pending/rejected grant produces a blocked check, not ready access. Repeated setup does not duplicate approval requests.
5. Create runtime stream and other required setup resources under the retained identity, with duplicate-safe steps and no arbitrary external calls while holding database locks.
6. Atomically compare installing state, setup revision, policy marker, and approved access. Check affected rows. Only this guarded update may activate the fleet.
7. In the same activation transaction record the explicit initial admission and stable identity, or the honest blocked admission outcome. Its response becomes accepted only after the existing inbox append/receipt boundary succeeds; until then start is pending and replay resumes delivery of that same event. Existing admission recovery may finish this append. Schedule registration runs only after activation and this initial admission attempt; it never creates another event.

Each external setup call has a declared timeout. A request has a 15-second setup budget and returns `201 installing` if the fleet exists but required checks remain. Five-second external-call limits fit this budget; after it is spent, retain pending checks for explicit retry. These are proposed limits to measure in implementation, not observed timings.

Malformed source or mismatched review fails before creating a runnable fleet. A database outage may leave the client uncertain whether the accepted identity committed: same-key retry resolves that uncertainty. A platform outage cannot become a guessed success. A stored partial result exposes checks and next actions through GET fleet.

### Initial run and triggers

`none` creates no initial admission. `run_now` records exactly one admission tied to this creation when activation succeeds. If runtime admission is refused, the fleet may still be active and `start.state` is `blocked` with its error; an uncertain inbox append keeps it pending; retrying setup never sends another initial run. A person may send a fresh fleet message explicitly later. Acceptance means queued work, not a finished answer.

Cron is registered after activation, removing the proposed pre-activation callback queue. GitHub and Slack can observe activation immediately; no claim is made that the initial run completes before other work arrives. Existing affinity and admission ordering apply. A schedule push failure leaves an active fleet with `sync:"failed"` and Retry sync. GET fleet and CLI expose that independently from required activation checks.

### Explicit retry and Kill

Same-key POST or `PATCH fleet {status:"active"}` with the current resource `If-Match` resumes missing setup checks. PATCH remains status/name only. A name edit during installing must invalidate the pending review and refuse activation until a newly reviewed revision is applied; it cannot silently change accepted creation inputs. Concurrent setup requests share the same guarded stages and initial admission identity.

Kill uses existing fleet:write authority. The transaction changes fleet state and setup cancellation generation before a late activation can commit. The linked draft returns to `editing`. Pending approval requests are cancelled under the fleet lifecycle guard. Setup resources remain owned by the killed fleet until ordinary fleet:admin purge; no new cleanup or creation worker is assumed. Killed state prevents any new lease or grant use. If activation already won and advanced the resource tag, the stale setup Kill receives412; the person must refetch before deciding to Kill the active fleet. Another Save has a fresh key and fleet ID. The cancelled key keeps referring to the killed fleet.

Fleet DELETE still requires fleet:admin and killed state. Deleting private draft text is not permission to delete the shared fleet. Draft removal cancels private work and hides its prefix; shared installing checks remain recoverable through the fleet by authorized members.

### Applying an improvement

Improve copies an immutable fleet setup revision into a creator-private draft. POST fleet revisions carries `If-Match: setup_etag`, one header replay key, candidate ID, and review digest. It atomically checks target/source revisions and updates the setup pointer, policy marker, and affected inherited schedules. A stale base or digest refuses the whole update. The response is the new setup tag and schedule states; publication remains separate.

Already leased runs retain captured setup except live revocation and renewed budget ceilings. New leases use the reviewed new revision. A policy marker drift blocks new admission until a human reviews and applies a revision. A name/status resource change does not falsely make setup content change; inherited schedule tags include instance setup and recipe/override revision.

### CLI partial success

The CLI displays `Fleet created: installing`, each missing check and its action, the fleet ID, and a copyable resume command that reads the fresh tag then PATCHes active. It exits nonzero, preserving the creation replay key for explicit retry. An active fleet with a blocked initial run or failed schedule sync prints the partial outcome and actionable retry, rather than claiming all requested work succeeded.

## Limits

L8 provider content-revision identity and its sweep remain a follow-up; the independent schedule fix owns stable IDs and workspace checks. Late stable-ID provider writes still have the limitation explained in the [API gaps](api.md). An image rollback after the schema cutover cannot restore removed columns or rows; release recovery is a tested roll-forward or an explicitly chosen test-data reset.

## Related pages

- [Runner and private deletion](../../architecture/runner_fleet.md#authoring-leases)
- [ASCII walkthrough](../../architecture/data_flow.md#fleet-setup-and-runtime-sequence)
- [Runtime preservation](runtime-baseline.md)
