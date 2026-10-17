---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Workspace chat authoring

## What it is

A private setup conversation helps a person write, review, and save a fleet in a workspace. Runtime conversation remains shared according to fleet access. This is the proposed target under [one cutover spec](../../v2/done/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md); these documents do not implement the backend.

The interface remains approved. The [21 owner decisions](decisions.md) and [source findings](source-findings.md) settle the backend revision. The [resource reference](../../architecture/api_resources.md) is the single proposed route and type authority.

## Why it exists

Describe work before navigating configuration menus, while preserving typed policy, grants, runtime admission, and billing. Private drafts protect setup messages. Explicit Library publication avoids turning a one-off Save into sharing.

## How it behaves

1. Opening New fleet creates no row. First Send, drawer edit, Customize, or Improve creates a private draft with one replay identity.
2. Resolve authoring/runtime models from the workspace owner's account. Pin the authoring model and prices for each turn.
3. Accept messages with a UUIDv7 body `operation_id`. Read state, answer, and usage through messages; stream private progress.
4. Runner and drawer make small edits to the same validated candidate. Stop uses a monotone accepted-message sequence cutoff.
5. GET draft supplies effective setup, immutable requirements, live readiness, and digest. Save and Save and run now use identical reviewed triggers.
6. Create one retained fleet. Bounded in-request checks activate it or return `installing` with missing checks. Retry resumes that fleet; Kill cancels setup and restores editing.
7. Admit an explicit initial run at checked activation. Register schedules afterward. A registration failure displays Retry sync without claiming the fleet was never created.
8. Show shared runtime conversation; the creator's private setup prefix appears only while separately authorized. Closed private history has a visible 30-day expiry.
9. Save to library separately creates an immutable reusable revision. Improve creates a private draft and applies its reviewed candidate through fleet revisions.

An unchanged Library install bypasses authoring. Inspect shows exact revision and review; Customize starts a draft. Account owns models, provider keys, and billing. Workspace owns members, connections, Library entries, and fleets. Approvals retain existing links.

### Cutover scope

17 new person routes, 7 changed, 2 removed. One runner claim gains a tagged `fleet|authoring` result plus authoring renew/report/activity/model-calls. Fleet PATCH becomes status/name only with required `If-Match`. Budget configuration uses nanos; stored trigger Markdown disappears; clients move entirely to `/fleet-libraries`.

Website, PostHog wiring, and branding stay in the same branch and pull request. L8 schedule content-revision work remains a follow-up. Confirm the independent schedule ownership/stable-ID fix before schedule acceptance. Public changes are handed to a separate documentation-repository branch.

## Limits

Source checks are complete. Backend implementation, repository tests, fresh coverage, live provider verification, commit, and pull request creation remain deferred. The target contains no compatibility paths.

## Related pages

- [API and gaps](api.md)
- [Definition](../../architecture/fleet_definitions.md#validated-definition)
- [Creation](../../architecture/data_flow.md#fleet-creation-and-setup-recovery)
- [Runner](../../architecture/runner_fleet.md#authoring-leases)
- [ASCII flow](../../architecture/data_flow.md#fleet-setup-and-runtime-sequence)
- [Layouts and options](../designs/workspace-chat-options.md)
- [Review packet](review-packet.md)
