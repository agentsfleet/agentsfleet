# Codex handoff: M227 UI, CLI and TypeScript lane

You own the TypeScript side of M227 from here: `ui/packages/**`, `cli/**`, and
`scripts/*.ts`. Claude keeps the Rust backend (`rustd/**`), `public/openapi.json`,
and the docs repository. Do not edit `rustd/**`; if a fix needs a server change,
write it down and hand it back.

Worktree: `/Users/kishore/.codex/worktrees/fd5a/agentsfleet`, branch
`feat/website-completion`. Do not `git add`, `git mv`, or commit; the
coordinator stages the whole branch once. Read `CLAUDE.md`, `.orly/LOCAL.md`, and
`.orly/dispatch/write_ts_adhere_bun.md` before editing. Prove work with make
targets (`make lint-app`, `make test-unit-app`, `make lint-cli`,
`make test-unit-cli`); a package-scoped runner is inner loop only.

## Already done on this side (do not redo)

- **CLI Save reports schedules that are not live.** A chat Save, Apply, or
  `--operation-id` recovery that returns any schedule whose `sync` is not
  `synced` (`failed`, `syncing`, or an unknown value) now prints
  `agentsfleet schedule sync <fleet_id> <schedule_id>` and exits 1.
  `--json` emits `schedule_sync_failed` or `schedule_sync_unconfirmed` on stderr.
  Files: `cli/src/commands/chat/{protocol,render,actions,command-recovery,setup-session}.ts`,
  `cli/test/chat/*`. `make test-unit-cli`: 1671 pass / 16 skip / 0 fail
  (Ran 1687 tests across 192 files). The `syncing` text says "Retry in a minute"
  because the abandoned sync holds a 30 s lease (`afd_cron/src/store/mod.rs:55`).
- **Version is 0.61.0** (`VERSION`, `cli/package.json`, `rustd/Cargo.toml`;
  `make check-version` green). This release refuses runners without
  `accepted_kinds`.
- **Browser journeys and list paging (partial; a Claude agent wrote this, then
  was stopped):**
  - `tests/e2e/acceptance/fixtures/install-ui.ts` `saveFleetViaUI` now drives
    draft → Review setup → Save fleet.
  - `withStagedPlatformEntry` is in `fixtures/native-setup.ts`.
  - 13 acceptance specs are updated for the removed controls.
  - `lib/api/draft-lists.ts` is new: a `DraftLists` paging client with
    `moreModels` / `moreChats` and `loadMore(list)`, plus
    `tests/draft-lists.test.ts` (8 cases).
  - `lib/analytics/events.ts` comments are reworded.

## What needs fixing

1. **Paging is not reachable from the interface.** `DraftClient.loadMore` has no
   finished caller, so a person stops at the first 50 models or chats.
   - The stopped agent left a half-written attempt in
     `components/domain/chat/DraftLoadMore.tsx` (new),
     `components/domain/chat/ChatToolbar.tsx`,
     `components/layout/DraftSidebarHistory.tsx`, `lib/api/drafts.ts`, and
     `tests/draft-lists.test.ts`. Audit those first and finish or remove them.
   - Match an existing load-more or cursor pattern in the repo rather than
     inventing one.
   - Component tests should cover four cases: the control shows only when more
     pages exist, it appends rows, it is disabled in flight, and a failure keeps
     the loaded rows and shows the error.
2. **`make lint-app` is red.** `tsc` reports five `TS18048 'page' is possibly
   'undefined'` errors in
   `app/(dashboard)/admin/runners/components/RunnerWall.tsx` lines 36–61.
3. **`make test-unit-app` is red.** The last full run failed 7 of 3274 tests:
   - RunnerWall and RunnerHeader
   - EditFleetDialog
   - `platform-catalog-table`
   - `dashboard-placeholder`, which passes alone, so look for test-order or
     shared-state leakage
4. **None of the 13 updated acceptance specs has run.** Run them against the
   acceptance stack and fix what fails. Recheck two of the stopped agent's
   unverified calls:
   - `measured-layout.spec.ts` dropped two workspace Library table-layout tests
     on the assumption that the page has no table. Confirm that.
   - `dashboard-performance.spec.ts` dropped its `data-intent-hover`
     assertion, because nothing sets the attribute. Decide whether intent
     loading still needs a marker.
5. **Six analytics events are unimplemented (Dimension 7.2).**
   - The events are `draft_message_accepted`, `draft_message_finished`,
     `draft_review_opened`, `fleet_setup_finished`, `fleet_setup_cancelled`
     and `fleet_library_published`, per the M227 spec at
     `docs/v2/done/M227_001_*.md:237`.
   - Only kind, state, outcome and opaque authorized identifiers may be sent.
     No text, Definitions, URLs or secret values.
   - Server acceptance is authoritative; browser clicks are intent only.
   - Prove the privacy allowlist in tests, and update the product-analytics
     funnel docs.
   - If an event must be emitted server-side, list it and hand it back
     instead of editing Rust.
6. **The named-constants gate (`bash .orly/audits/ufs.sh`) flags your lane.**
   - Counts: 16 in `cli/`, 14 in `ui/packages/app`, 1 in
     `ui/packages/design-system` (`Select.tsx:88`), and 13 in `scripts/`.
   - Most are repeated nanodollar literals (`1000000000`, `10000000000`) in
     fixtures. Extract a named constant, or annotate a deliberate pin per the
     gate's own instructions.
   - Never baseline or suppress.
7. **A stale Rust path in a CLI fixture.**
   `cli/test/acceptance/fixtures/command-matrix.ts:258` cites
   `afd_api_tenant/src/handler/tenant/models.rs`. The module layout move made it
   `tenant/models/mod.rs`, so update the citation.
8. **The acceptance manifest's `revision` is never verified.**
   - `inspect-acceptance-build.ts` doesn't emit `git rev-parse HEAD`, and `build-binding.ts:14-27` doesn't compare it. A manifest naming the wrong revision passes every artifact check, so the evidence file records the wrong revision.
   - Emit `revision` from the inspector and compare it in `assertBuildArtifacts`.
   - `bash scripts/acceptance_build_binding_test.sh` drives the real preflight; add a wrong-revision case there.
   - The spec also asks for four recorded revisions, not one.
9. **`ui/packages/app/lib/errors.test.ts:74`** still cites `UZ-BUNDLE-*`, which is retired.
10. **Two decisions are already made; don't re-open them.**
   - Interactive `chat --library` / `chat --fleet` creates its draft before
     the first prompt, so an early quit leaves an empty draft. Indy accepted
     this.
   - There are no compatibility aliases or legacy paths anywhere.

## Report back

For each item:
- the files changed
- the make target run, with its pass/skip/fail line
- anything left open, with `file:line`
- anything that needs a Rust change, as its own list
