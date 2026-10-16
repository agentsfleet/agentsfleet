---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Workspace chat API and implementation gaps

## What it is

The [resource reference](../../architecture/api_resources.md) defines the one route set:17 new person routes,7 changed,2 removed, one changed machine claim and 4 new machine routes. This page explains what is missing in the inspected code and where existing behavior can be reused. Target behavior is proposed, not implemented by this rewrite.

## Why it exists

Source-backed gaps let an implementing agent distinguish existing handlers from new resources, policy work, and client cutover. New APIs have no fabricated source-file citation. Existing source evidence is pinned to `ad3ed8f92c739b59df71863dbeec81d621dd7d0a`.

## How it behaves

### Source-backed gap ledger

| Gap | What exists / evidence | What is missing in this milestone |
|---|---|---|
| G1 Private authoring | Fleet message handlers at `rustd/crates/afd_api_tenant/src/handler/fleet/message.rs:84` | Creator-owned drafts/messages/storage, message read state/usage, private stream, bounded model edits. |
| G2 Stop | Runtime lifecycle and existing fleet controls | Accepted-message sequence watermark, cancellation/report linearization, terminal replay, late-send test. |
| G3 Private deletion | Existing fleet purge is shared fleet authority | Draft text boundary, deletion generation, private-copy inventory, logical spool expiry and restart cleanup, content-free accounting. |
| G4 Improve setup | Existing raw setup edits in `afd_fleet_lifecycle/src/edit/rewrite.rs` | Sole fleet revision writer, setup tag, immutable source/target pins, review digest; remove raw PATCH writers. |
| G5 Private scope | Workspace owner resolution at `afd_http/src/auth/ownership.rs:63–74` | Creator predicate on every draft/message/stream/cursor; revocation and joined-member cases. |
| G6 Runtime controls | Existing Stop/Resume/Kill | Preserve their semantics. Future-only pause is outside scope; do not rename Stop into that behavior. |
| G7 Durable Save identity | Install generates identity at `afd_fleet_lifecycle/src/install.rs:223` | Key/body receipt, retained installing row, grant barrier, checked activation, exact initial-run admission, explicit resume. |
| G8 Schedule ownership | Main schedule read/write handlers were identified in the review | Independent fix branch must land workspace-qualified lookups and stable prefixed provider IDs; no claim of completion here. |
| G9 Activation ordering | Active-only App/Slack routing at `afd_ingress/src/app.rs:139`, `slack/mod.rs:41–51` | Move schedule registration after activation; remove invented early-callback queue. |
| G10 Initial run | Existing fleet admissions and steer replay | One creation-derived admission identity and visible accepted/blocked outcome; same-key retry cannot submit it twice. |
| G11 Library versions | Gallery/card types at `afd_api_wire/src/workspace_library.rs:41` | Inspect, revisions, owned literal path, explicit draft publication receipt, no `/library-entries` routes. |
| G12 Model/account resolution | Caller-account model read at `afd_api_tenant/src/handler/tenant/model_entry.rs:201` | Workspace-owner models, pinned authoring prices/tokenizer, idle model replacement, durable failure actions, joined-member wallet tests; no fallback. |
| G13 Shared worker capacity | Existing runner lease routes at `afd_http/src/route/runner.rs:36–42` | Shared host guard for claims/renewals, expiry rejection, tagged claim, four machine routes including daemon-enforced scoped model calls. |
| G14 Chat shell recovery | Live onboarding-refresh import at `ui/packages/app/components/domain/useFleetMessageDelivery.ts:13` | Subject-keyed private/shared adapters, shell stream, pending-send recovery, no cross-workspace caches; delete only dead widget. |
| G15 Wire proof | Existing shared refusal at `afd_http/src/envelope.rs:192–213` and paging primitives | Proposed code registry rows, required keys/preconditions, decimal strings, exact route inventory, timed_out, stateful reads. |
| G16 Provider revision identity | Generation changes on claim at `afd_cron/src/sql.rs:123,148`; signed destination check at `verifier.rs:125` | L8 follow-up: distinct content counter, revision-bearing signed destination, owned-prefix sweep and provider purge cleanup. |
| G17 Budget cutover | Dollar input at `afd_fleet_runtime/src/config/raw/policy.rs:37–48` | Nanos across every live parser/reader/writer/fixture/client/export; new migration removes trigger Markdown. |
| G18 Authoring spend | Runtime estimate at `afd_fleet/src/lease/admit/gates.rs:42–55`; report wallet lock at `lease/sql/report.rs:130–135` | Wallet-locked maximum bounded-turn admission, unknown usage and replay accounting; no fixed draft cap or reservation ledger. |
| G19 Policy drift | Existing typed runtime config | Hand-bumped marker, review digest inclusion, blocking new admission until re-review. |
| G20 UI cleanup | Approved workspace entrance and existing navigation | Account models/keys links, Approvals, Retry on list failure, Stopped badge, removal of placeholder settings. |

| G21 Pending-message read | `afd_events/src/history/statement/queued.rs:10–14` excludes null receipts; append deferral may succeed | Repair existing queued reads without changing their wire shape; before/after append recovery proves one visible message/event. |
| G22 Candidate version handoff | Proposed lease/ack lacked the daemon-derived version | Lease includes candidate; proposal acknowledgement returns immutable ID/revision; guard/replay small edits. |
| G23 Build-bound proof | `make/acceptance.mk:13–16` runs tests; Playwright only starts the app | Verify actual four-component builds and environment before fixture mutation; record release success and failed-release recovery. |

`afd_*` paths in this table are relative to `rustd/crates/`; frontend paths are repository-relative. Detailed agreements and qualifications: [five source findings](source-findings.md).

### Schedule prerequisite and limitation

Claude's separate fix branch owns the stable schedule ID and workspace-ownership bugs. Verify its merge and tests before integration acceptance. Stable IDs make normal retry registration idempotent but do not fence a late provider write from an old edit; a read-back cannot fence an in-flight write. L8's separate content-revision identity plus signed destination and owned-prefix sweep is the chosen follow-up. The cutover must show failed/pending sync honestly and cannot claim the stronger provider assurance has shipped.

### Resource authority

Draft creator owns private text and Stop/delete. Workspace owner determines models, wallet, connections, and grants. Fleet:write applies reviewed setup and Kill; fleet:admin deletes killed fleets. Library:write publishes/removes owned entries; platform entries are inspectable but not writable. The model can propose content but cannot create grants, Save, publish, or resolve credentials.

### Cutover and public documentation

Remove raw setup fields from fleet PATCH, require If-Match, require UUIDv7 message operation IDs, replace dollar budget fields, stop storing trigger Markdown, and move clients entirely to `/fleet-libraries`. Reset acceptance-test setup data at the chosen release boundary; do not maintain old-client routes or infer conversions. Keep historical migration/spec evidence intact.

[The public handoff](public-changes.md) includes an exact approved REST-guide amendment and a separate docs-repository change list. No published documentation is edited by this worktree.

## Limits

The queue-placement refactor proposed in M226 is not a dependency: implement against the inspected runtime and its current admissions. Repository-wide checks and current coverage have not been rerun. Any pending gate remains pending until the actual implementation and declared tests run.

## Related pages

- [Canonical types and routes](../../architecture/api_resources.md)
- [Decision ledger](decisions.md)
- [Creation](../../architecture/data_flow.md#fleet-creation-and-setup-recovery)
- [Runner](../../architecture/runner_fleet.md#authoring-leases)
