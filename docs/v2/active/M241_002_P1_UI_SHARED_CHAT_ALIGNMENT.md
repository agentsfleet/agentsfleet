# M241_002: Align shipped chat with approved design

**Prototype:** v2.0.0
**Milestone:** M241
**Workstream:** 002
**Date:** Nov 08, 2026
**Status:** IN_PROGRESS
**Priority:** P1, approved chat controls did not ship in the earlier change.
**Categories:** UI
**Batch:** B1, follow-up to M241_001.
**Depends on:** Claude's harness PR must merge before this follow-up.
**Provenance:** LLM-drafted (Codex, Nov 08, 2026).
**Canonical architecture:** `docs/DESIGN_SYSTEM.md`, shared chat surfaces.

## Overview

Fleet setup and Fleet conversation use the approved composer, header actions, summary and loading treatment.
Runner networking uses Codex's control names, a domains popup and fewer decorative icons.
Existing endpoints and request ownership remain authoritative.

## Scope

- Shared composer: model immediately beside circular Send; preserve typing, retry and cancellation.
- Headers: pencil edit, summary toggle, and back navigation to the fleet conversation while editing saved setup.
- Summary: collapsed initially; one-action mobile drawer; durable tasks remain visible; omit empty Outputs and Changes.
- Loading: moving text highlight without the old duplicate spinner; preserve reduced-motion and forced-color behavior.
- Runner: exact public/package/domain labels, pencil popup, native policy encoding; no decorative section/engine icons.
- Chat shell: remove the desktop left navigation from these surfaces; preserve a route back to Your fleets.

## Files changed

`ui/packages/app/components/domain/{FleetReplyFigures,FleetThought,FleetWorkSidebar,SteerComposer}.tsx`,
`ui/packages/app/components/domain/chat/**`, `ui/packages/app/app/globals.css`,
`ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader.tsx`,
`ui/packages/app/app/(dashboard)/admin/runners/components/**`, relevant app/browser tests and public documentation.

## Acceptance

1. Model and Send share a row with less than 24px between them on desktop and mobile.
2. Header edit uses a named pencil icon; summary opens directly and closing it preserves the draft.
3. Both actual chat renderers use the shared text loading treatment and accessible static fallbacks.
4. Network switches and popup save the native assignment; all-off blocks outbound traffic and custom hosts survive unrelated edits.
5. App coverage remains 100%; browser checks cover both themes and mobile/desktop.
6. No merge until Claude's PR is merged, origin/main is integrated, and checks/reviews are green/resolved.

## Discovery

> Indy: "The Fleet chat and Fleet conversation chat are old level not the one we agreed. Can you fix that."
> Indy: "How about the loader too, its old."
> Indy: "I want your PR can go in but it must now wait for claude's PR to be merged, cause that PR is a heavy lift."
> Indy: "Also keep the icons in the Runner to minimal."

The single accepted bootstrap-401 acceptance follow-up remains recorded in M241_001; it is not a passing acceptance result.

## Verification

Implementation is complete; merge remains blocked on Claude’s PR and fresh checks after integrating it.
App coverage passed 4,789 tests in 493 files, with statements, branches, functions and lines at 100%.
`make test-coverage-all` reached a missing CLI dependency prerequisite after app and website coverage passed; the CLI and design-system lanes continue separately.
Focused chat and runner browser checks passed 28 tests on desktop and mobile.
Static and independent review found no remaining actionable findings after four repairs; Astra found no visual blockers.
The previous broad run’s outdated mock/spinner assertions were corrected; its result is not claimed as passing.
