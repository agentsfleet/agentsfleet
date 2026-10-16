---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Workspace chat layouts and options

## What it is

The approved centered composer, charcoal surfaces, typography, signal mark, and existing design-system primitives remain. This record applies the locked backend choices to navigation, drawers, Save, and recovery. It describes proposed screens; it is not a claim the chat cutover is implemented.

## Why it exists

The person needs clear workspace/account ownership, a private authoring prefix, and reliable next actions. The [21 decisions](../reviews/workspace-setup/decisions.md) and [API reference](../architecture/api_resources.md) govern behavior.

## How it behaves

### Navigation and entry

```text
+----------------------+----------------------------------------------------------+
| agentsfleet          | Workspace: Engineering             Account: Indy        |
| Engineering v        |                                                          |
| [+ New fleet]        |                 What should your fleet handle?           |
|                      |                 Describe the job. Review its setup.      |
| Fleets [Search]      |                                                          |
| Checkout watch       | +------------------------------------------------------+ |
| Build triage         | | Watch failed checkout deployments...                 | |
| [More]               | | Owner model v                                 [Send] | |
|                      | +------------------------------------------------------+ |
| Drafts (private)     | [Incident] [Test regressions] [Use a library setup]       |
| Fleet library        |                                                          |
| Activity / Approvals | Loading, empty, and failed states are distinct.          |
| Workspace settings   | No sample runs or fabricated quality counters.           |
| Account v            |                                                          |
+----------------------+----------------------------------------------------------+
```

| Surface | Options and behavior |
|---|---|
| Workspace switcher | Confirm access before accepting work. Failed list shows Retry; only confirmed emptiness offers Create workspace. Subject switch resets caches/stream. |
| New fleet | Blank composer creates no draft. Suggestions fill editable text without sending. First Send/drawer mutation creates private draft. |
| Fleets | Real paged/searchable instances with Working, Waiting, Paused, Stopped, Needs attention. Clicking opens shared conversation. |
| Drafts | Creator's drafts only; resume pending message state/usage. No fake fleet Working badge. |
| Fleet library | Platform+owned gallery, Inspect, Install, Customize, explicit Save to library, version history, remove owned only. |
| Activity / Approvals | Existing run detail and approval links remain usable; actual outstanding counts only. |
| Workspace settings | Members/access where supported, Connections, Secrets; delete Defaults/Security placeholders. |
| Account menu | Clearly name owning account for Models, API Keys, Billing, account members, appearance, help, sign out. Joined members cannot manage another account's keys. |
| Administration | Existing authorized operator menu, separate from ordinary workspace navigation. |

Current navigation comes from `ui/packages/app/components/layout/sidebar-navigation-data.ts`; the shell uses `ShellFrame.tsx`. The live `lib/onboarding-refresh.ts` remains; delete the unmounted `GettingStartedWidget.tsx` and its isolated tests.

### Private authoring and contextual drawers

```text
+----------------------+------------------------------------+----------------------+
| agentsfleet          | New fleet - Private to you         | Tools & access [x]   |
| Engineering v        |                                    |                      |
| [+ New fleet]        | You: Watch failed deployments.     | GitHub [Connect]     |
| Fleets [Search]      | Assistant: Which repository?       | Secret names only    |
| Checkout watch       | You: agentsfleet/linkwarden        |                      |
| Drafts               | Assistant: Review this setup.      | Requirements         |
| > Checkout draft     |                                    | Repository: selected |
| Fleet library        | Job / timing / access / budget     | Write access: review |
| Activity / Approvals | [Review setup]                     | Grants: Awaiting you |
| Workspace settings   |                                    | [Open Approvals]     |
| Account v            | +--------------------------------+ |                      |
|                      | | Refine the job...      [Send]  | |                      |
|                      | +--------------------------------+ |                      |
|                      | [Job] [Triggers] [Access] [Usage]  |                      |
+----------------------+------------------------------------+----------------------+
```

| Drawer / control | Options | Guard / recovery |
|---|---|---|
| Job | Name, display description, Markdown instructions | Idle typed candidate, If-Match; stale edit shows refreshed difference. |
| Access | Tools, credential names, repository/network policy, gates, anomaly rules | No aliases or secret values in chat. Dedicated connection/secret forms; actual grant readiness. |
| Triggers | Supported API/event/cron/mention fields | Exactly reviewed configuration; registration after activation. No hidden trigger mutation on Save. |
| Authoring model | Owner-eligible authoring entries | Idle If-Match selection update survives reload; queued/running work blocks changes. Earlier turns keep their model. Unavailable selection offers another eligible model or owner recovery. |
| Runtime model | Owner-eligible runtime entries | Candidate config.model; review reflects selection. |
| Budget | Daily and optional monthly amount in readable currency | Translate deterministically to positive decimal-string nanos; show exact ceiling in review. No float wire fields. |
| Send | One body operation_id | Keep text on refusal; keep same key/body on uncertain receipt; recover through messages. |
| Stop | Last accepted sequence cutoff | Cancel queued/running through cutoff; later sends survive delayed Stop. |
| Usage | Known input/output/cost, or explicitly Unknown | Never turn missing provider usage into a zero spend display. |

### Review and two Save choices

```text
+------------------------------------------------------------------------+
| Review Checkout watch                                                  |
| Job: Investigate failed checkout deployments with linked evidence.      |
| Runtime model: Owner model                 Policy: reviewed            |
| Tools/access: exact validated scope       Budget: daily + monthly       |
| Triggers: API + the displayed event/schedule/mention recipes             |
| Requirements: immutable names/scope       Readiness: observed just now   |
| Missing approval? [Open Approvals]         Missing model? [Owner Models]  |
|                                                                        |
| [Save] [Save and run now]                    [Save to library]           |
+------------------------------------------------------------------------+
```

Save starts no explicit run. Save and run now admits the reviewed message once when activation succeeds. Both preserve triggers. Save to library publishes independently; a later Save reuses its verified source receipt. Digest drift refetches review; readiness changes show missing checks.

### Setup progress and runtime history

```text
+------------------------------------------------------------------------+
| Checkout watch - Setting up                                             |
| Source ready / Model ready / Connection ready / Grant waiting           |
| [Open Approvals] [Resume setup] [Kill]                                   |
| Fleet ID remains the same on Retry.                                     |
+------------------------------------------------------------------------+
| Checkout watch - Active                                                 |
| Initial request: Pending / Accepted / Blocked with action                          |
| Schedule: Synced / Needs attention [Retry sync]                          |
| Private setup prefix (creator only): expires <visible date>             |
| Shared runs: timestamps, evidence, result, delivery state                |
| [Improve setup] [Run message] [Existing Stop/Resume/Kill controls]         |
+------------------------------------------------------------------------+
```

Other authorized members see shared fleet checks/history, never the creator's private prefix. Kill returns that creator's draft to editing. A new Save creates a new fleet/key. Model/policy drift shows Needs re-review and blocks new admission. Run completed and Slack delivered remain distinct.

### Library and improvement

| Action | Result |
|---|---|
| Inspect | Exact immutable Library revision, reusable requirements, live workspace readiness. |
| Install unchanged | POST fleets from pinned Library revision; no authoring draft required. |
| Customize | First mutation creates private draft from exact Library revision. |
| Save to library | Explicit owned entry/revision; no transcript. Platform entries are read-only. |
| Library history | Immutable versions; existing fleets stay pinned. |
| Improve selected fleet | Private draft targeting exact setup tag; Apply posts fleet revision. |
| Restore inherited schedule | Version-checked override:null restores the reviewed inherited recipe. |
| Remove owned entry | Deletes discoverability/history while installed fleet references remain valid. |

### Mobile and accessibility

One column, existing workspace drawer, conversation first, contextual details in a sheet. Review and Save choices stay explicit. Restore focus to the opening control; retain keyboard navigation, reduced-motion preferences, labels, live-message announcement limits, and touch targets. Long instructions/requirements wrap without forcing whole-page horizontal scrolling. Retain the approved visual system; no new design variants are requested.

### Failure and deletion copy

Failed workspace list says Retry. Unknown send says Recover message, preserving its key. Fleet messages awaiting delivery remain visible after reload. Partial setup names missing checks.

Durable failures show the stored recovery action after reload: owner Billing, Choose model, Reduce context, Retry message, Await accounting, or Review setup. Unknown usage says Unknown and blocks another retry until resolved. Model replacement stays selected after reload; prior turns keep their pins. Stopped shows Stopped.

Delete explains the server text boundary, provider/backups retention, and shared setup. Runner copies expire logically after 24 hours; powered-off copies may remain until startup cleanup. Closed chat displays its 30-day expiry; active work never expires.

## Limits

Future-only pause, credential aliases, independent child roles, placeholder settings, and quality counters without measured data are absent. Backend names and resource details belong in the API reference, not ordinary user screens.

## Related pages

- [Approved interface](chat-within-a-workspace.md)
- [Detailed ASCII flow](../architecture/data_flow.md#fleet-setup-and-runtime-sequence)
- [Resource reference](../architecture/api_resources.md)
