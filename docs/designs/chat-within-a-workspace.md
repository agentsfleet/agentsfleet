---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Approved workspace chat interface

## What it is

Indy approved the centered welcome/composer and charcoal visual direction from the October13 interface exploration. Typography, signal mark, restrained motion, and the existing design-system primitives remain. The October14 backend choices are recorded in the [owner ledger](../reviews/workspace-setup/decisions.md). API and implementation work are pending.

## Why it exists

Users should describe work, review setup, save a fleet, and return to results within the workspace they already understand. A private setup conversation and shared runtime history need clear separation without exposing implementation detail.

## How it behaves

[The layout record](workspace-chat-options.md) gives all screens and controls; [the ASCII flow](../architecture/data_flow.md#fleet-setup-and-runtime-sequence) gives the full lifecycle. The sidebar groups workspace entry, fleets, private drafts, Library, Activity/Approvals, and supported settings. Account owns Models, API Keys, and Billing. Failed workspace discovery offers Retry.

New fleet creates no row on open. First Send/edit/Customize/Improve creates a private draft. A contextual drawer edits the same validated setup the model proposes. Review shows exact job, model, tools/access, triggers, and budgets, alongside live readiness. Idle authoring-model replacement survives reload; earlier turns keep their pins. Failed messages retain safe recovery actions after reload.

Save and Save and run now preserve triggers. Save creates no Library entry automatically; Save to library is explicit. Setup progress belongs to the retained shared fleet, with missing checks, Resume, and Kill. Kill restores the private draft to editing. A creator sees a private prefix while authorized, with a 30-day closed-chat expiry; other fleet readers see shared work only.

Statuses include Stopped. Existing Approvals remain linked. Delete the unmounted onboarding widget and Defaults/Security placeholders; retain the live onboarding-refresh module. Mobile uses a conversation-first column with contextual sheets and existing accessibility behavior.

## Limits

The approved appearance is not an approval of unimplemented backend behavior or fresh test coverage. No automatic Library publication, hidden model fallback, invented recent work, or placeholder policy controls are added.

## Related pages

- [Layouts and options](workspace-chat-options.md)
- [Owner decisions](../reviews/workspace-setup/decisions.md)
- [Review packet](../reviews/workspace-setup/review-packet.md)
