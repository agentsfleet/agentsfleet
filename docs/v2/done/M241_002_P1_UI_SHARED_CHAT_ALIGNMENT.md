# M241_002: Align shipped chat with approved design

**Prototype:** v2.0.0
**Milestone:** M241
**Workstream:** 002
**Date:** Nov 08, 2026
**Status:** DONE
**Priority:** P1, approved chat controls did not ship in the earlier change.
**Categories:** UI
**Batch:** B1, follow-up to M241_001.
**Depends on:** Claude's harness PR #815, merged into main as `febea89b85`.
**Provenance:** LLM-drafted (Codex, Nov 08, 2026).
**Canonical architecture:** `docs/DESIGN_SYSTEM.md`, shared chat surfaces.

## Overview

Fleet setup and Fleet conversation use the approved composer, header actions, summary and loading treatment.
Runner networking uses Codex's control names, a domains popup and fewer decorative icons.
Existing endpoints and request ownership remain authoritative.
Integration with #815 also guards admission lookups against internal setup event keys.

## Scope

- Shared composer: model immediately beside circular Send; preserve typing, retry and cancellation.
- Headers: pencil edit, summary toggle, and back navigation to the fleet conversation while editing saved setup.
- Summary: collapsed initially; one-action mobile drawer; durable tasks remain visible; omit empty Outputs and Changes.
- Loading: moving text highlight without the old duplicate spinner; preserve reduced-motion and forced-color behavior.
- Runner: exact public/package/domain labels, pencil popup, native policy encoding; no decorative section/engine icons.
- Chat shell: keep the existing desktop left navigation visible when a fleet conversation or setup opens; retain mobile navigation.
- Fleet conversation: omit the duplicate Back-to-list arrow; standalone saved setup retains its return to the fleet conversation.
- Integration: skip non-admission event keys in live-run lookups without changing UUID admission keys or API responses.

## Files changed

`ui/packages/app/components/domain/{FleetReplyFigures,FleetThought,FleetWorkSidebar,SteerComposer}.tsx`,
`ui/packages/app/components/domain/chat/**`, `ui/packages/app/app/globals.css`,
`ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader.tsx`,
`ui/packages/app/app/(dashboard)/admin/runners/components/**`, relevant app/browser tests and public documentation.
The #815 integration repair also touches `rustd/crates/daemon/afd_admission/src/sql/admit_one.sql`,
`rustd/crates/daemon/afd_fleet/src/lease/sql/lease/claim_inputs.sql` and its lease-fold regression tests.
Indy's approved coverage follow-up touches `make/test-integration-rustd.mk` to split API and fleet suites from substrate.

## Acceptance

1. Model and Send share a row with less than 24px between them on desktop and mobile.
2. Header edit uses a named pencil icon; summary opens directly and closing it preserves the draft.
3. Both actual chat renderers use the shared text loading treatment and accessible static fallbacks.
4. Network switches and popup save the native assignment; all-off blocks outbound traffic and custom hosts survive unrelated edits.
5. App coverage remains 100%; browser checks cover both themes and mobile/desktop, including visible fleet navigation on desktop.
6. No merge until Claude's PR is merged, origin/main is integrated, and checks/reviews are green/resolved.

## Discovery

> Indy: "The Fleet chat and Fleet conversation chat are old level not the one we agreed. Can you fix that."
> Indy: "How about the loader too, its old."
> Indy: "I want your PR can go in but it must now wait for claude's PR to be merged, cause that PR is a heavy lift."
> Indy: "Also keep the icons in the Runner to minimal."
> Indy: "I merged 815, so i request you to pull the main into your PR, and fix the greptile feedback."
> Indy: "I would like to go by the ChatGPT Codex model on having the left navigation when i click a single fleet whereit opens the fleet conversation."
> Indy: "Also i noticed a back button in the picture of a fleet converstaion, why do you need that, where does the back take you to"
> Indy: "Well i think you will have to push and hopefully see if we can squeeze the 20min"

The approved coverage split retains the 20-minute job limit, all tests and the existing coverage floor.
Its measured completion remains pending in PR #814.

The single accepted bootstrap-401 acceptance follow-up remains recorded in M241_001; it is not a passing acceptance result.

## Verification

Claude's PR #815 is integrated through merge commit `4715f7a00`; the GitHub branch-update merge is also incorporated without rewriting history.
Review corrections preserve left navigation, catch a failed pencil refresh and animate setup status only during active work.
Local `make lint-app` passed source, Oxlint and TypeScript checks.
Local `make test-coverage-all` passed: app 4,802 tests in 493 files; website 133 tests; CLI 2,035 passed, 16 live skips and no failures; design-system 522 tests.
App, website and design-system statements, branches, functions and lines are all 100%; CLI's enforced function and line coverage is 100%.
Logs: `/workspace/scratch/shared-chat-integrated-lint.log`, `/workspace/scratch/shared-chat-integrated-coverage.log`.
Astra reviewed fresh conversation, setup and summary captures with restored navigation and found no P1/P2 visual blockers.
The captures use production chat components and a representative shell with simulated API data; they are not live fleet results.
Focused Playwright navigation checks passed all four desktop/mobile tests through `bun run --cwd ui/packages/app test:e2e tests/e2e/chat-alignment.spec.ts` with the local preview URL.
Browser log: `/workspace/scratch/shared-chat-integrated-browser.log`. Final-head Continuous Integration results are recorded in PR #814.
The accepted separate Cloud bootstrap follow-up remains pending; no acceptance pass is claimed.

The first final-head substrate job exceeded its 20-minute limit. One targeted retry completed in time and reported two existing API failures; the policy-replay log confirms that an internal `setup_ready:1` key was cast to UUID in #815's new admission join.
The repair guards the cast with PostgreSQL's UUID parser; a live lease regression covers both admitting and claiming after an internal setup event.
No workflow timeout, coverage floor or test assertion is weakened. Follow-up results are recorded in PR #814 and coordination issue #813.
