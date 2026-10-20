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
**Status:** IN_PROGRESS
**Priority:** P1 — setup chat can remain blocked after a provider failure and Stop
**Categories:** API | DOCS | UI
**Batch:** B1 — existing MiniMax and setup-chat stream
**Branch:** fix/minimax-default-chat-polish
**Baseline revision:** 211bcea539b62b5a13f5e2f3cb89dd342ca7dd6f
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending
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
| `scripts/model-catalogue/catalogue.ts` | EDIT | Accept zero token prices for curated free models |
| `scripts/model-catalogue/catalogue.test.ts` | EDIT | Free-model price regressions |
| `scripts/model-catalogue/curated.ts` | EDIT | Reuse paid/free model identity |
| `scripts/model-catalogue/curated.test.ts` | EDIT | Paid/free identity regression |
| `scripts/model-catalogue/rows.test.ts` | EDIT | Free-model rows |
| `scripts/model-library-allowlist.json` | EDIT | Add MiniMax M3.1 Flash Free |
| `rustd/crates/afd_authoring/src/lease/mod.rs` | EDIT | Shared pending-work query |
| `rustd/crates/afd_authoring/src/lease/pending.rs` | CREATE | One authoritative current-attempt check |
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
| `rustd/crates/afd_authoring/tests/draft/model_calls/mod.rs` | EDIT | Register key rejection proofs |
| `rustd/crates/afd_authoring/tests/draft/model_calls/rejections.rs` | CREATE | Real dispatch and settlement regressions |
| `rustd/crates/afd_authoring/tests/draft/creation/mod.rs` | EDIT | Register lifecycle regression |
| `rustd/crates/afd_authoring/tests/draft/creation/stopped.rs` | CREATE | Stop, Save, and late report isolation |
| `ui/packages/app/components/layout/DraftSidebarItem.tsx` | EDIT | Remove chat icon and match navigation type |
| `ui/packages/app/components/layout/DraftSidebarHistory.test.tsx` | EDIT | Keep title, delete, and long-title behavior |
| `ui/packages/app/components/layout/SidebarNavigationSections.tsx` | EDIT | Use compact icon button when collapsed |
| `ui/packages/app/components/domain/chat/DraftConversation.tsx` | EDIT | Distinguish queued, running, and stopped messages |
| `ui/packages/app/components/domain/chat/ChatComposer.tsx` | EDIT | Offer Stop for unresolved provider work |
| `ui/packages/app/lib/api/draft-recovery.ts` | CREATE | Recovery derived from current watermark |
| `ui/packages/app/lib/api/drafts.ts` | EDIT | Keep client recovery consistent with server admission |
| `ui/packages/app/tests/draft-recovery.test.ts` | CREATE | Recovery and stale-history cases |
| `ui/packages/app/tests/draft-chat-recovery.test.tsx` | CREATE | Rendered Stop and pending-message behavior |
| `VERSION` | EDIT | User-visible fix version |
| `cli/package.json` | EDIT | Version synchronization |
| `ui/packages/app/package.json` | EDIT | Version synchronization |
| `ui/packages/website/package.json` | EDIT | Version synchronization |
| `ui/packages/design-system/package.json` | EDIT | Version synchronization |

Published documentation uses its own branch in `~/Projects/docs`, as previously authorized for this stream.
Update affected setup-chat recovery pages and the changelog there; never write through this worktree.
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

### §1 — Current chat ownership and retained accounting

Use the existing draft lock and watermark. Discovery and authoritative admission exclude stopped attempts from current chat blocking.
Send first resolves exact replay, then refuses unresolved uncancelled provider usage without accepting text or consuming a sequence.

- **Dimension 1.1** — Unknown usage refuses fresh Send but exact replay succeeds → Test `unknown_work_refuses_fresh_send_but_preserves_exact_replay`
- **Dimension 1.2** — Stop permits subsequent claim without losing old liability → Test `stopped_unknown_work_does_not_block_new_claim_or_publish_late_text`
- **Dimension 1.3** — Stop, Send, claim, and reports preserve ordering and single settlement → Test `send_racing_stop_keeps_one_lease_and_the_retained_report_settles_once`
- **Dimension 1.4** — Save and setup cancellation never restore old publication authority → Test `save_after_stop_keeps_old_reports_accounting_only`
- **Dimension 1.5** — Indexed cutoff checks remain bounded over retained history and concurrent callers → Test `stopped_history_uses_sequence_index_and_concurrent_sends_keep_unique_sequences`

### §2 — Definite key rejection

Parse key header syntax before dispatch intent. A definite HTTP 401 authentication rejection ends the visible turn as failed.
Keep already reported usage and runtime charges. Missing usage on uncertain transport or other failed responses remains unknown.

- **Dimension 2.1** — Malformed keys create no dispatch intent or physical send → Test `malformed_key_is_refused_before_dispatch_intent`
- **Dimension 2.2** — Definite authentication rejection settles once and releases unused reservation → Test `authentication_rejection_is_known_failed_usage_and_settles_once`
- **Dimension 2.3** — Rejection classification preserves known usage and uncertain errors → Test `authentication_rejection_preserves_usage_and_does_not_reclassify_uncertainty`

### §3 — Catalogue and chat recovery presentation

Register the free OpenRouter model and retain data-driven model labels.
Remove the sidebar chat icon, match navigation typography, preserve delete and ellipsis, and center the compact collapsed plus button.
Show Queued for unclaimed work. Offer Stop when unresolved provider work blocks Send, including after reload.

- **Dimension 3.1** — Curated free MiniMax rows retain correct identity and zero pricing → Test `curated_model_catalogue_tests`
- **Dimension 3.2** — Current watermark determines Stop and recovery even with older cached messages → Test `draft_recovery_uses_current_watermark`
- **Dimension 3.3** — Rendered chat distinguishes queued, running, and recoverable failure → Test `chat_recovery_shows_stop_and_queued_state`
- **Dimension 3.4** — Sidebar retains long titles, delete, and navigation typography → Test `draft_sidebar_title_and_delete`

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
| 1.3 | integration | `send_racing_stop_keeps_one_lease_and_the_retained_report_settles_once` | No duplicate attempt or debit; delayed Stop cannot cancel later sequence |
| 1.4 | integration | `save_after_stop_keeps_old_reports_accounting_only` | Save closes draft; setup Kill returns editing; old proposal/report stays fenced |
| 1.5 | integration | `stopped_history_uses_sequence_index_and_concurrent_sends_keep_unique_sequences` | Query plan uses cutoff index; concurrent requests retain unique monotone sequences |
| 2.1 | integration | `malformed_key_is_refused_before_dispatch_intent` | No physical send or pending receipt; model failure is durable |
| 2.2 | integration | `authentication_rejection_is_known_failed_usage_and_settles_once` | Known failed dispatch, single settlement, released hold, fresh retry possible |
| 2.3 | unit | `authentication_rejection_preserves_usage_and_does_not_reclassify_uncertainty` | Known usage retained; non-authentication failure stays unknown |
| 3.1 | unit | `curated_model_catalogue_tests` | Free row uses correct model identity and nonnegative pricing |
| 3.2 | unit | `draft_recovery_uses_current_watermark` | Old cached failure cannot block a stopped turn; newer unknown still blocks |
| 3.3 | e2e | `chat_recovery_shows_stop_and_queued_state` | Rendered chat exposes recovery and does not call queued work Working |
| 3.4 | e2e | `draft_sidebar_title_and_delete` | Long title truncates visually; delete and navigation remain accessible |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Recovery and rejection proofs | `make test-integration-rustd` | exit 0; named recovery tests pass | P0 | |
| R2 | Catalogue and rendered UI proofs | `make test-unit-all` | exit 0; named catalogue and recovery tests pass | P0 | |
| R3 | Scope and baseline evidence complete | `orly gate pr` | exit 0 | P0 | |
| S1 | Source checks | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint | `make lint-all` | exit 0 | P0 | |
| S3 | Version synchronization | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | Published recovery instructions | Review affected docs branch | Recovery steps agree with runtime and no secret values appear | P1 | |

## Dead Code Sweep

Remove `claim/sql/obligation.sql` and its `OBLIGATION` constant when the shared pending-work query replaces them.
Verify `rg -n '\bOBLIGATION\b|claim/sql/obligation.sql' rustd/crates/afd_authoring/src` has zero matches.
Remove the sidebar message icon import after its last use. No other file deletion is planned.

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
- **Metrics review** — No event schema or funnel definition change; existing finish and accounting events retain their ownership.
- **Skill-chain outcomes** — Spec workflow in progress; unit, integration, review, and post-push monitoring pending.
- **Deferrals** — None.
