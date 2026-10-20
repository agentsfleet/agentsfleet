<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order follows the executing agent's read order.
- No estimates, assigned owners, completion percentages, or unfilled slots.
- Priority and dependencies determine scope and order.
-->

# M230_001: Resume setup chat after Stop without losing accounting

**Prototype:** v2.0.0
**Milestone:** M230
**Workstream:** 001
**Date:** Oct 20, 2026
**Status:** DONE
**Priority:** P1 — setup chat can remain blocked after a provider failure and Stop
**Categories:** API | DOCS | UI
**Batch:** B1 — existing MiniMax and setup-chat stream
**Branch:** fix/minimax-default-chat-polish
**Baseline revision:** 211bcea539b62b5a13f5e2f3cb89dd342ca7dd6f
**Test Baseline:** 11,112 unit tests passed; 1,357 integration tests passed; zero failures
**Baseline evidence:** `make test-unit-all` and `make test-integration-rustd` at the full revision above, on the same macOS host with isolated stores.
Unit: `/tmp/minimax-baseline-unit.log`; integration: `/tmp/minimax-baseline-integration.log`.
The unit lane intentionally ignored 1,362 datastore or platform tests; the integration lane ignored zero and filtered 4,187 nonselected tests.
**Depends on:** none
**Provenance:** LLM-drafted (GPT-6, Oct 20, 2026)
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` — Authoring admission and settlement; `docs/architecture/api_resources.md` — Private drafts

## Overview

**Goal (testable):** Stop fences the old setup turn; a later message can run while its old accounting remains recoverable.
**Problem:** HOWDY displays an unknown provider failure, then later messages wait indefinitely, including after Stop.
**Solution summary:** Reuse the saved cancellation watermark to distinguish current chat work from retained accounting.
Reject new messages behind unresolved, uncancelled provider work. Reject malformed keys before dispatch intent and recognize definite authentication rejection.
Keep late usage settlement, normal capacity checks, wallet holds, private access checks, and proposal fences.

## PR Intent & comprehension handshake

- **PR title (eventual):** Restore setup chat after Stop and add free MiniMax
- **Intent:** Let a person recover setup chat without losing charges or accepting old answers.
- **Handshake:** Indy approved the existing-table design: “Yes i am signing off the design”.
- **ASSUMPTIONS I'M MAKING:** The shared-dev model configuration already exists; this branch carries durable catalogue and behavior fixes.
  Connection testing and expiry monitoring are separate features, not prerequisites for runtime rejection handling.
- **Golden path:** Send records text and the selected model. Claim checks current authority, watermark, capacity, and balance.
  Dispatch opens only the pinned model's vault key. Stop raises the watermark under the draft lock.
  New Send uses a higher sequence. Old reports account under their original attempt and cannot publish through the watermark.

## Implementing agent — read these first

1. `rustd/crates/afd_authoring/src/lease/guard/mod.rs` — existing publication and accounting guards.
2. `rustd/crates/afd_authoring/src/message/cancel.rs` — monotone Stop watermark under the draft lock.
3. `rustd/crates/afd_authoring/src/model/receipt/finish.rs` — known versus uncertain provider accounting.
4. `docs/architecture/billing_and_provider_keys.md` — holds, settlement, and cancellation.
5. `docs/DESIGN_SYSTEM.md` — navigation typography, density, and error recovery.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/active/M230_001_P1_API_DOCS_UI_STOPPED_AUTHORING_RECOVERY.md` | CREATE | Approved scope and evidence; move to done on completion |
| `docs/v2/done/M230_001_P1_API_DOCS_UI_STOPPED_AUTHORING_RECOVERY.md` | CREATE | Completed record |
| `docs/architecture/billing_and_provider_keys.md` | EDIT | Separate chat blocking from retained liability |
| `docs/architecture/api_resources.md` | EDIT | Send refusal and stopped failure recovery |
| `docs/architecture/data_flow.md` | EDIT | Stop followed by Save and late accounting |
| `docs/DESIGN_SYSTEM.md` | EDIT | Recent setup list without search or fleet recents |
| `scripts/model-catalogue/catalogue.ts` | EDIT | Accept zero token prices for curated free models |
| `scripts/model-catalogue/catalogue.test.ts` | EDIT | Free-model price regressions |
| `scripts/model-catalogue/curated.ts` | EDIT | Reuse paid/free model identity |
| `scripts/model-catalogue/curated.test.ts` | EDIT | Paid/free identity regression |
| `scripts/model-catalogue/rows.test.ts` | EDIT | Free-model rows |
| `scripts/model-catalogue/seed.integration.test.ts` | EDIT | Full curated document includes the new identity in real database writes |
| `scripts/model-library-allowlist.json` | EDIT | Add MiniMax M3.1 Flash Free |
| `rustd/crates/afd_authoring/src/lease/mod.rs` | EDIT | Shared pending-work query |
| `rustd/crates/afd_authoring/src/lease/failure.rs` | EDIT | Emit a finish only for an actual terminal message transition |
| `rustd/crates/afd_authoring/src/lease/pending.rs` | CREATE | One authoritative current-attempt check |
| `rustd/crates/afd_authoring/src/maintenance/mod.rs` | EDIT | Expose the compiled sweep statement to query-plan tests under test-util |
| `rustd/crates/afd_fleet/src/lease/billing_sweep.rs` | EDIT | Expose the compiled metering statement under test-util |
| `rustd/crates/afd_db/Cargo.toml` | EDIT | Test-only domain dependencies for executable query-plan proofs |
| `rustd/crates/afd_db/tests/migrations.rs` | EDIT | Test the migration adapter instead of reading source files |
| `rustd/crates/afd_db/tests/integration_migrate/sweep_plans.rs` | EDIT | Explain the domain statements actually compiled into production |
| `scripts/model-catalogue/seed.ts` | EDIT | Execute the shared compiled statement registry |
| `scripts/model-catalogue/statements.ts` | CREATE | Load each SQL statement once for seeding and measured query plans |
| `rustd/crates/afd_authoring/src/lease/sql/mod.rs` | EDIT | Register the shared query |
| `rustd/crates/afd_authoring/src/lease/sql/pending.sql` | CREATE | Filter attempts past the Stop watermark |
| `rustd/crates/afd_authoring/src/lease/claim/selection.rs` | EDIT | Use shared blocking rule under lock |
| `rustd/crates/afd_authoring/src/lease/claim/sql/candidates.sql` | EDIT | Discovery honors Stop |
| `rustd/crates/afd_authoring/src/lease/claim/sql/mod.rs` | EDIT | Remove obsolete private query |
| `rustd/crates/afd_authoring/src/lease/claim/sql/obligation.sql` | DELETE | Replace with shared pending-work query |
| `rustd/crates/afd_authoring/src/message/accept.rs` | EDIT | Refuse new work behind live unknown usage after replay lookup |
| `rustd/crates/afd_authoring/src/types/failure.rs` | EDIT | Derive stopped recovery from the watermark |
| `rustd/crates/afd_authoring/src/message/read.rs` | EDIT | Project current recovery on ordinary reads |
| `rustd/crates/afd_authoring/src/observe/mod.rs` | EDIT | Pass current watermark into failure notices |
| `rustd/crates/afd_authoring/src/observe/rows.rs` | EDIT | Project the same recovery on private notices |
| `rustd/crates/afd_authoring/src/model/dispatch.rs` | EDIT | Validate key syntax before durable intent |
| `rustd/crates/afd_authoring/src/model/provider/dispatch.rs` | EDIT | Keep definite authentication rejection distinct |
| `rustd/crates/afd_authoring/src/model/provider/response.rs` | EDIT | Known rejected-call usage without invented uncertain usage |
| `rustd/crates/afd_authoring/src/model/provider/mod.rs` | EDIT | Register rejection validation and tests |
| `rustd/crates/afd_authoring/src/model/provider/rejections.rs` | CREATE | Key preflight and authentication classification |
| `rustd/crates/afd_authoring/src/model/provider/rejections_tests.rs` | CREATE | Malformed-key and rejection regressions |
| `rustd/crates/afd_authoring/src/model/receipt/finish.rs` | EDIT | Record known rejected calls and durable model failure |
| `rustd/crates/afd_authoring/tests/draft/admission/mod.rs` | EDIT | Register recovery proofs |
| `rustd/crates/afd_authoring/tests/draft/admission/stop.rs` | EDIT | Stop permits new work subject to normal limits |
| `rustd/crates/afd_authoring/tests/draft/admission/recovery.rs` | CREATE | Unknown usage, replay, Stop, and concurrency proofs |
| `rustd/crates/afd_authoring/tests/draft/admission/history.rs` | CREATE | Indexed history, discovery, and concurrent Send and claim proofs |
| `rustd/crates/afd_authoring/tests/draft/admission/rejections.rs` | CREATE | Real dispatch, key preflight, holds, and settlement regressions |
| `rustd/crates/afd_authoring/tests/draft/admission/rejections/races.rs` | CREATE | Stop or model revocation racing a late key rejection finishes once |
| `rustd/crates/afd_authoring/tests/draft/creation/mod.rs` | EDIT | Register lifecycle regression |
| `rustd/crates/afd_authoring/tests/draft/creation/stopped.rs` | CREATE | Stop, Save, and late report isolation |
| `ui/packages/app/components/layout/DraftSidebarItem.tsx` | EDIT | Remove chat icon and match navigation type |
| `ui/packages/app/components/layout/DraftSidebarHistory.test.tsx` | EDIT | Keep title, delete, and long-title behavior |
| `ui/packages/app/components/layout/DraftSidebarHistory.tsx` | EDIT | Recent setup heading and plain history list |
| `ui/packages/app/components/layout/SidebarNavigation.tsx` | EDIT | Remove the fleet recents section |
| `ui/packages/app/components/layout/ShellFrame.tsx` | EDIT | Remove unused fleet-history provider |
| `ui/packages/app/components/layout/RecentFleets.tsx` | DELETE | Remove unused fleet-history tracking and rendering |
| `ui/packages/app/tests/recent-fleets.test.tsx` | DELETE | Retire tests for the removed fleet-history feature |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` | EDIT | Remove unused visit tracking |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetWall.tsx` | EDIT | Remove unused fleet-list tracking |
| `ui/packages/app/components/layout/draft-sidebar-state.tsx` | EDIT | Remove unused search refresh state |
| `ui/packages/app/components/layout/use-draft-sidebar-bridge.ts` | EDIT | Remove unused search refresh subscriptions |
| `ui/packages/app/components/layout/use-draft-search.ts` | DELETE | Remove unused dashboard search hook |
| `ui/packages/app/components/layout/DraftSearchResults.tsx` | DELETE | Remove unused dashboard search results |
| `ui/packages/app/components/layout/DraftSearchResultsDynamic.tsx` | DELETE | Remove unused dashboard search loader |
| `ui/packages/app/components/layout/DraftSearchResultsDynamic.test.tsx` | DELETE | Retire tests for the removed search loader |
| `ui/packages/app/components/layout/DraftSidebarSearch.test.tsx` | DELETE | Retire tests for the removed sidebar search |
| `ui/packages/app/components/layout/DraftSearchUpdates.test.tsx` | DELETE | Retire tests for removed sidebar search refresh |
| `ui/packages/app/components/layout/DraftSearchAccess.test.tsx` | DELETE | Retire tests for removed sidebar search access handling |
| `ui/packages/app/components/layout/SidebarNavigationSections.tsx` | EDIT | Use compact icon button when collapsed |
| `ui/packages/app/components/domain/chat/DraftConversation.tsx` | EDIT | Distinguish queued, running, and stopped messages |
| `ui/packages/app/components/domain/chat/ChatComposer.tsx` | EDIT | Offer Stop for unresolved provider work and focus a ready New fleet composer |
| `ui/packages/app/lib/api/draft-recovery.ts` | CREATE | Recovery derived from current watermark |
| `ui/packages/app/lib/api/drafts.ts` | EDIT | Remove access-reset method whose final sidebar-search caller was removed |
| `ui/packages/app/tests/draft-recovery.test.ts` | CREATE | Recovery and stale-history cases |
| `ui/packages/app/tests/draft-chat-recovery.test.tsx` | CREATE | Rendered Stop and pending-message behavior |
| `ui/packages/app/tests/app-shell-navigation.test.tsx` | EDIT | Collapsed New Fleet icon button |
| `VERSION` | EDIT | User-visible fix version |
| `cli/package.json` | EDIT | Version synchronization |
| `rustd/Cargo.toml` | EDIT | Workspace version synchronization |
| `rustd/Cargo.lock` | EDIT | Workspace package versions |

Published documentation in `~/Projects/docs` is excluded from completion by Indy's latest instruction, quoted under Discovery.
Keep this repository's architecture and completed spec accurate; preserve earlier work in the separate repository without further edits or publication.
Expand this table before additional implementation edits.

## Applicable Rules

- `.orly/docs/greptile-learnings/RULES.md` — NDC, NLR, ECL, NSQ, UFS, ORP, TIM, IDMP, CNX, TVR, and TCF.
- `.orly/dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md` — existing crate error types and cause-preserving propagation.
- `.orly/dispatch/write_sql.md` and `.orly/docs/SCHEMA_CONVENTIONS.md` — caller-owned transactions and qualified queries.
- `.orly/dispatch/write_ts_adhere_bun.md` and `docs/DESIGN_SYSTEM.md` — shared components, typography, and tokens.
- `.orly/docs/DOCUMENTATION_RULES.md` — short source-checked prose.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| Length | Yes | Files at most 350 lines; functions at most 50 and methods at most 70 |
| Named constants and errors | Yes | Reuse status/error vocabulary; key errors expose no credential |
| UI and design tokens | Yes | NavItem, Button, IconAction, and shared typography/spacing |
| SQL | Yes | Existing tables and indexes; no migration or schema edits |
| Architecture | Yes | Update the canonical flow with code or earlier |
| Spec and lifecycle | Yes | Record approval, baseline, dimensions, and final evidence |
| Logging | No new logging planned | Preserve existing typed funnel events and privacy boundaries |

## Prior-Art / Reference Implementations

- Existing draft Stop, proposal guards, retained reports, biller holds, and creation races supply the implementation pattern.
- Existing `afd_inference` HTTP header parsing and SDK transport supply key syntax and response classification.
- Shared navigation and icon-button primitives supply the requested sidebar treatment.

## Sections (implementation slices)

### §1 — Current chat ownership and retained accounting — DONE

Use the existing draft lock and watermark. Discovery and authoritative admission exclude stopped attempts from current chat blocking.
Send first resolves exact replay, then refuses unresolved uncancelled provider usage without accepting text or consuming a sequence.

- **Dimension 1.1** — DONE — Unknown usage refuses fresh Send but exact replay succeeds → Test `unknown_work_refuses_fresh_send_but_preserves_exact_replay`
- **Dimension 1.2** — DONE — Stop permits subsequent claim without losing old liability → Test `stopped_unknown_work_does_not_block_new_claim_or_publish_late_text`
- **Dimension 1.3** — DONE — Stop, Send, claim, and reports preserve ordering and single settlement → Test `send_racing_stop_admits_new_work_and_the_retained_report_settles_once`
- **Dimension 1.4** — DONE — Save and setup cancellation never restore old publication authority → Test `save_after_stop_keeps_old_reports_accounting_only`
- **Dimension 1.5** — DONE — Indexed cutoff checks remain bounded over retained history and concurrent callers → Test `stopped_history_uses_sequence_index_and_concurrent_sends_keep_unique_sequences`

### §2 — Definite key rejection — DONE

Parse key header syntax before dispatch intent. A definite HTTP 401 authentication rejection ends the visible turn as failed.
Keep already reported usage and runtime charges. Missing usage on uncertain transport or other failed responses remains unknown.

- **Dimension 2.1** — DONE — Malformed keys create no dispatch intent or physical send → Test `malformed_key_fails_before_dispatch_and_releases_unused_hold`
- **Dimension 2.2** — DONE — Definite authentication rejection settles once and releases unused reservation → Test `authentication_rejection_fails_and_settles_without_cancelling_the_chat`
- **Dimension 2.3** — DONE — Rejection classification preserves known usage and uncertain errors → Test `authentication_status_does_not_erase_reported_usage`
- **Dimension 2.4** — DONE — A late rejection never emits another finish after Stop or model revocation → Test `stopped_authentication_reply_finishes_only_once`; also `revoked_model_authentication_reply_finishes_only_once`

### §3 — Catalogue and chat recovery presentation — DONE

Register the free OpenRouter model and retain data-driven model labels.
Remove the sidebar chat icon, match navigation typography, preserve delete and ellipsis, and center the compact collapsed plus button.
Show Queued for unclaimed work. Offer Stop when unresolved provider work blocks Send, including after reload.

- **Dimension 3.1** — DONE — Curated free MiniMax rows retain correct identity and zero pricing → Test `curated_model_catalogue_tests`
- **Dimension 3.2** — DONE — Current watermark determines Stop and recovery even with older cached messages → Test `draft_recovery_uses_current_watermark`
- **Dimension 3.3** — DONE — Rendered chat distinguishes queued, running, and recoverable failure → Test `chat_recovery_shows_stop_and_queued_state`
- **Dimension 3.4** — DONE — Sidebar retains long titles, delete, and navigation typography → Test `draft_sidebar_title_and_delete`
- **Dimension 3.5** — DONE — Sidebar shows Recent setup without search or fleet recents → Test `shows Recent setup without search or fleet recents`; also expanded navigation regression
- **Dimension 3.6** — DONE — New fleet focuses a ready composer, including a second click while already new → Test `focuses New fleet ready for typing without clicking the composer`; also readiness, same-page fresh action, and existing-chat navigation

## Interfaces

Existing draft, cancellation, and runner endpoints retain their shapes.
`UZ-AUTHOR-015` refuses a fresh Send while an uncancelled attempt has uncertain provider usage.
Exact replay still returns its accepted sequence before that check.
`provider_usage_unknown` retains its reason; after Stop its recovery action becomes `retry_message`.
Key syntax and definite authentication rejection use existing `model_unavailable` and `UZ-AUTHOR-008` vocabulary.
No automatic retries, key rotation, expiry polling, or new execution table.

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Unknown provider result | Timeout or missing authoritative usage after dispatch | Retain hold; refuse new Send until Stop or accounting resolves |
| Bad key syntax | Newline or another invalid header value | Refuse before durable dispatch; record model failure |
| Authentication rejection | Definite HTTP 401 | Record known rejected usage; fail automatically; settle existing charges once |
| Other provider failure | Server, quota, malformed response, or network failure | Preserve known usage; otherwise keep uncertainty |
| Late result | Stop, Save, deletion, or newer turn won | Account original attempt; refuse private output and proposals |
| Contention | Concurrent Stop, Send, claim, or report | Draft and attempt guards determine order; no duplicate admission or debit |
| Wallet/capacity limit | Old liability still reserves balance or runner slot | Keep normal refusal/backoff; Stop never manufactures available funds |

## Invariants

1. Stopped sequences cannot publish — existing guard compares sequence with the locked draft watermark.
2. One attempt per message — existing unique draft/sequence index and draft lock.
3. Accounting remains tied to the old attempt — existing report identity, receipts, and biller work key.
4. Unknown usage never becomes invented zero — only a definite pre-dispatch refusal or authentication rejection supplies that knowledge.
5. Exact accepted replay remains available — receipt lookup precedes the new Send refusal.
6. Stop is monotone — existing GREATEST update prevents a delayed lower cutoff from affecting newer messages.

## Metrics & Observability

Existing typed message-accepted, message-finished, model-failure, and accounting signals remain authoritative.
No event names, properties, analytics identities, or funnel definitions change.
Recovery tests check single finish and unchanged private-data boundaries; no analytics playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | integration | `unknown_work_refuses_fresh_send_but_preserves_exact_replay` | Fresh message refused without sequence change; exact replay returns original |
| 1.2 | integration | `stopped_unknown_work_does_not_block_new_claim_or_publish_late_text` | Later claim succeeds subject to normal limits; old answer stays hidden |
| 1.3 | integration | `send_racing_stop_admits_new_work_and_the_retained_report_settles_once` | No duplicate attempt or debit; delayed Stop cannot cancel later sequence |
| 1.4 | integration | `save_after_stop_keeps_old_reports_accounting_only` | Save closes draft; setup Kill returns editing; old proposal/report stays fenced |
| 1.5 | integration | `stopped_history_uses_sequence_index_and_concurrent_sends_keep_unique_sequences` | Query plan uses cutoff index; concurrent requests retain unique monotone sequences |
| 2.1 | integration | `malformed_key_fails_before_dispatch_and_releases_unused_hold` | No physical send or pending receipt; model failure is durable |
| 2.2 | integration | `authentication_rejection_fails_and_settles_without_cancelling_the_chat` | Known failed dispatch, single settlement, released hold, fresh retry possible |
| 2.3 | unit | `authentication_status_does_not_erase_reported_usage` | Known usage retained; adjacent rejection tests preserve uncertain errors |
| 2.4 | integration | `stopped_authentication_reply_finishes_only_once` | Stop racing rejection emits one terminal event and settles once; paired model-revocation regression proves the same boundary |
| 3.1 | unit | `curated_model_catalogue_tests` | Free row uses correct model identity and nonnegative pricing |
| 3.2 | unit | `draft_recovery_uses_current_watermark` | Old cached failure cannot block a stopped turn; newer unknown still blocks |
| 3.3 | unit | `chat_recovery_shows_stop_and_queued_state` | Rendered component exposes recovery and does not call queued work Working |
| 3.4 | unit | `draft_sidebar_title_and_delete` | Long title has truncation styles; delete and navigation remain accessible |
| 3.5 | unit | `shows Recent setup without search or fleet recents` | Recent setup heading; no search input or fleet recents |
| 3.6 | unit and browser | `focuses New fleet ready for typing without clicking the composer` | Ready input owns the caret; waiting or existing chat does not steal focus |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Recovery and rejection proofs | `make test-integration-rustd` | exit 0; named recovery tests pass | P0 | ✅ 1,364 parallel plus 3 exclusive tests passed; zero failures |
| R2 | Catalogue and rendered UI proofs | `make test-unit-all` | exit 0; named catalogue and recovery tests pass | P0 | ✅ 11,125 passed; all four TypeScript package coverage gates at 100% |
| R3 | Scope and baseline evidence complete | `orly gate pr` | exit 0 | P0 | Publication pending: runs after commit and push, before PR creation |
| S1 | Source checks | `make harness-verify` | exit 0 | P0 | ✅ ALL GATES GREEN |
| S2 | Lint | `make lint-all` | exit 0 | P0 | ✅ All lint checks passed |
| S3 | Version synchronization | `make check-version` | exit 0 | P0 | ✅ all versions match 0.62.2 |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ staged scanner: no leaks found |
| S5 | Architecture recovery description | Review architecture diff against Send, claim, Stop, and receipt paths | Current ownership and retained accounting match code | P1 | ✅ Watermark governs admission/publication; original attempt governs settlement |

Proofs at implementation close: `/tmp/minimax-integration-final-2.log`, `/tmp/minimax-unit-final-3.log`,
`/tmp/minimax-harness-final-5.log`, `/tmp/minimax-lint-final-5.log`, and `/tmp/minimax-version-final.log`.
The additional public failure-recovery unit proof passed separately; the PR gate reruns the final tree.
Rust coverage: 166/166 changed lines and all 17 measured touched files exceed 99%; lowest file 105/106.
The five-point retained-history ladder uses 100, 200, 400, 800, and 1,000 rows; 100 Send callers retain unique sequences, and 100 claim callers produce one attempt.
New Fleet: 60.75 → 60.57 kB gzipped; shared dashboard: 275.60 → 272.74 kB; unchanged limits pass.
Local browser checks confirm 14px navigation/history text, a 36px collapsed button with an 18px plus, and repeated New fleet focus.
Browser checks use the shared-dev API without sending or stopping chat; backend recovery is verified in isolated integration tests.

## Dead Code Sweep

Remove `claim/sql/obligation.sql` and its `OBLIGATION` constant when the shared pending-work query replaces them.
Verify `rg -n '\bOBLIGATION\b|claim/sql/obligation.sql' rustd/crates/afd_authoring/src` has zero matches.
Remove the sidebar message icon import after its last use.
Remove the search hook, results, loader, and feature-specific tests when removing sidebar search.
Remove fleet-recents tracking, provider, rendering, and feature-specific tests with that section.
Retain the programmatic search API and its service/client tests.

## Out of Scope

- New execution tables or a generic execution engine.
- Automatic provider retries, model switching, secret rotation, connection-test UI, and key expiry polling.
- Cancellation of provider-side work, reversing accepted candidate edits, or interrupting existing fleet runtime leases.
- Unmeasured tokenizer, context-building, or locking rewrites unrelated to recovery.

## Product Clarity (authoring record)

1. **Successful user moment** — Stop the blocked HOWDY turn, send a new message, and receive a response.
2. **Preserved user behaviour** — Keep private chat, candidate review, explicit Save, delete, and exact retry.
3. **Optimal-way check** — Use existing sequence, guards, and receipts; add no duplicate ownership state.
4. **Rebuild-vs-iterate** — Refactor the shared blocking predicate and refusal classification; keep the existing execution architecture.
5. **What we build** — Recovery, rejection handling, catalogue entry, restrained sidebar changes, and regression evidence.
6. **What we do NOT build** — Provider cancellation, a new execution engine, automatic fallback, or key-health infrastructure.
7. **Fit with existing features** — Use biller settlement and fleet creation without changing runtime cancellation semantics.
8. **Surface order** — API behavior first, then browser recovery, with the existing terminal API behavior preserved.
9. **Dashboard restraint** — Show only queued/running/failure facts and an action supported by saved state.
10. **Confused-user next step** — Stop an uncertain turn, repair the key or choose another model, then send a new message.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Share the authoritative current-attempt predicate; keep accounting and publication guards separate.
- **Alternatives considered:** A new execution table duplicates existing message/attempt identity and needs migration and reconciliation.
  A UI-only change cannot unblock server admission. Deleting old attempts or releasing unknown liability loses billing safety.
- **Patch-vs-refactor verdict:** A bounded refactor reuses the existing tables and indexes and changes the ownership decision at its actual readers.
  Performance measurements determine whether another optimization is justified; no speculative new execution layer is required.

## Discovery (consult log)

- **Consults** — Indy approved the revised existing-table design: “Yes i am signing off the design”.
  Earlier execution-table proposals are withdrawn. Static source review confirmed both claim queries ignore the Stop cutoff for retained attempts.
  Static source review also confirmed receipt completion treats every decoding failure as unknown usage.
- **Sidebar scope** — Indy: “and no Recent Fleets under Recents. keep things imple”, then “i dont want the search, you can remove that.” and “for now”.
- **Focus and verification** — Indy requested “all the test cases to be covered on unit, integration, performance, concurency” and that clicking New Fleet sets the caret ready to type.
  Reuse the existing fresh action's message reset for focus; do not add a second chat identity or timing-dependent navigation timeout.
- **Metrics review** — No event schema or funnel definition change; existing finish and accounting events retain their ownership.
- **Lint repair** — Indy approved “Yes, repair the affected tests” for the four existing source-read lint failures.
  Explain compiled domain statements and test the migration adapter; retain the lint rule and behavioral assertions.
- **Documentation boundary** — Indy: “When i stop override documentation its in ~/Projects/docs repo”.
  Relevant architecture updates in this repository remain authorized; leave the separate published-docs repository untouched.
- **Skill-chain outcomes** — Unit and real-datastore integration checks passed; final review and post-push monitoring remain publication gates.
- **Deferrals** — Published pages and changelog in `~/Projects/docs` only; architecture updates in this repository remain required.
  > Indy (2026-10-21): “When i stop override documentation its in ~/Projects/docs repo” — context: skip separate published-docs updates for this PR.
