<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order follows the executing agent's read order.
- No estimates, assigned owners, completion percentages, or unfilled slots.
- Priority and dependencies determine scope and order.
-->

# M230_002: Stream setup and runtime through one chat implementation

**Prototype:** v2.0.0
**Milestone:** M230
**Workstream:** 002
**Status:** IN_PROGRESS
**Priority:** P1 — setup replies stall and setup editing diverges from fleet chat
**Categories:** API | DOCS | UI
**Batch:** B1 — one existing branch and one Pull Request (PR)
**Branch:** fix/dev-acceptance-heartbeat
**Baseline revision:** 124c2bce303c95533745312c1ec725092bb604cb
**Test Baseline:** pending — measure declared unit and integration lanes before the PR
**Baseline evidence:** pending — record full counts and evidence before the PR
**Depends on:** M230_001 — stopped setup recovery and retained usage accounting
**Provenance:** LLM-drafted (GPT-6), based on Indy's approved seven-part proposal
**Canonical architecture:** `docs/architecture/api_resources.md`, `docs/architecture/runner_fleet.md`, `docs/architecture/web_app.md`

## Overview

**Goal (testable):** A setup message and a runtime message consume the same provider stream reader and browser reply decoder, display their first answer before provider completion, and preserve private setup authority and authoritative usage on failure.
**Problem:** Setup buffers the whole provider response behind a separate deadline; its transcript, composer, editing route and loading sequence diverge from working fleet chat.
**Solution summary:** Extract the runtime's Rig streaming and browser lifecycle code into shared implementations. Keep purpose-specific authorization, tool execution, durable history and explicit save. Show setup editing beside the fleet conversation, with a visible Save action and compact saved-version receipt.

## PR Intent & comprehension handshake

- **PR title (eventual):** Unify fleet chat streaming and setup editing
- **Intent:** Let people create, chat with and edit one fleet without a second chat experience or hidden save controls.
- **Handshake:** Indy approved the detailed seven-part architecture: “Signed off - approved for implementation by Indy”.
- **ASSUMPTIONS I'M MAKING:** Keep the existing branch/worktree and carry-over UI, admission-history and acceptance-helper fixes in one PR. Keep provider credentials in their existing daemon/runner boundary. Opening or editing setup does not run a fleet; runtime messages do not require saving.
- **Golden path:** Browser message admission stores private text; the authoring runner claims a scoped lease; the daemon checks capability, current membership, pinned model and wallet, commits dispatch intent, and opens the shared provider stream with the vault-resolved credential. Answer deltas use the private draft channel and authorized draft stream. Final usage and validated proposals enter the existing receipt before the runner receives its result. Fleet runtime calls the same stream reader from its existing provider adapter. Browser adapters share connection, decoder, viewport, message rows and composer. Save creates the next setup revision and returns to fleet Chat.
- **Quality ceiling:** Extracting the tested runtime implementation beats adding streaming only to setup: it removes duplicated lifecycle and rendering behavior. Purpose adapters retain the safety boundaries instead of merging permissions or tools.
- **Surface area:** Existing API paths remain; private stream events and safe failure details are additive. No new CLI command. User docs and release notes change. Existing dispatch storage gains deadline and bounded failure facts under the repository schema policy. No table removal. The user explicitly approved folding the backend work into this PR.

## Implementing agent — read these first

1. `rustd/crates/afr_providers/src/turn.rs` — canonical runtime stream consumption and tool replay.
2. `rustd/crates/afd_inference/src/lib.rs` — shared configured model and pinned Rig driver.
3. `rustd/crates/afd_authoring/src/model/receipt/finish.rs` — usage settlement and late-publication fence.
4. `ui/packages/app/lib/streaming/fleet-stream-reply-registry.ts` — reply ordering and durable recovery.
5. `ui/packages/app/components/domain/FleetThreadViewport.tsx` — working single-scroll viewport and reachable composer.

## Files Changed (blast radius)

Existing dirty files belong to this stream and are preserved. This table scopes cohesive source families; exact added/deleted siblings are recorded here before editing them.

| File | Action | Why |
|------|--------|-----|
| `docs/v2/active/M230_002_P1_API_DOCS_UI_ONE_FLEET_CHAT_ENGINE.md` | CREATE | Approved scope and proof ledger; move to done only after completion |
| `docs/architecture/api_resources.md` | EDIT | Private answer frames, failure facts and dispatch deadline |
| `docs/architecture/runner_fleet.md` | EDIT | Shared reader and private draft delivery |
| `docs/architecture/web_app.md` | EDIT | Shared UI and one fleet workspace |
| `docs/architecture/data_flow.md` | EDIT | Setup editing and version save flow |
| `docs/DESIGN_SYSTEM.md` | EDIT | Approved spacing, alignment, compact receipts and recent rows |
| `rustd/Cargo.lock` | EDIT | Resolved dependency changes |
| `rustd/crates/afd_inference/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/stream.rs`, `src/transport.rs`, `src/stream_tests.rs` | EDIT/CREATE | Shared provider stream, bounded byte transport, failure and deadline ownership |
| `rustd/crates/afr_providers/src/turn.rs`, `src/transport/`, `src/connect/` | EDIT | Runtime adapter delegates to shared reader and transport |
| `rustd/crates/afd_authoring/Cargo.toml`, `src/model/`, `src/types/failure.rs`, `src/observe/`, `src/lease/guard/` | EDIT | Setup stream adapter, publication guard, durable deadline and failure facts |
| `rustd/crates/afd_authoring/src/message/accept.rs`, `src/replay/mod.rs`, `src/sql/message/admit.sql` | EDIT/CREATE | Carry-over bounded admission history reads |
| `rustd/crates/afd_authoring/tests/` | EDIT/CREATE | Real dispatch, usage, privacy, replay and admission regressions |
| `rustd/crates/afd_api_tenant/src/handler/draft/stream/` | EDIT | Authorized private live frames with durable recovery |
| `rustd/crates/afd_api_wire/src/authoring/output.rs` | EDIT | Safe structured failure information |
| `rustd/crates/afd_sse/src/`, `rustd/crates/afd_dragonfly/src/streams/` | EDIT | Reuse bounded subscriptions for private draft subjects |
| `rustd/crates/afr_supervisor/src/client/http.rs` | EDIT | Wait deadline covers permitted setup provider work and receipt |
| `schema/`, `rustd/crates/afd_db/src/` | EDIT/CREATE | Dispatch deadline/failure storage and schema registration under current policy |
| `ui/packages/app/lib/streaming/` | EDIT/CREATE | Shared scoped connection, reply ordering and recovery |
| `ui/packages/app/lib/api/draft*`, `ui/packages/app/lib/api/setup-proxy/` | EDIT | Private live deltas, independent selected-history loading and fleet listing |
| `ui/packages/app/components/domain/FleetThread.tsx`, `FleetThreadViewport.tsx`, `FleetMessageRow.tsx`, `SteerComposer.tsx` | EDIT | Shared chat shell and purpose adapters |
| `ui/packages/app/components/domain/chat/` | EDIT/CREATE | Chat-first setup editor, shared composer and save flow |
| `ui/packages/app/components/layout/` | EDIT | One Recent list, compact aligned navigation and hover trash |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/` | EDIT/DELETE | Fleet workspace editor and removal of redundant Setup/Your fleets navigation |
| `ui/packages/app/tests/`, colocated component tests | EDIT/CREATE | Shared UI, lifecycle, privacy, saves and acceptance proofs |
| `ui/packages/design-system/src/design-system/Textarea.tsx`, `Textarea.test.tsx` | EDIT | Carry-over quiet focused caret behavior |

## Applicable Rules

- `.orly/docs/greptile-learnings/RULES.md` — NDC, NLR, NLG, ORP, UFS, FLL, WAUTH, CNX, ASE: remove replaced implementations; scope identity; release database connections before network work; catch asynchronous failures.
- `.orly/dispatch/write_rust.md` and `docs/RUST_ERROR_STANDARD.md` — typed failures and the shared crate error shell.
- `.orly/dispatch/write_ts_adhere_bun.md` — owned resource classes; shared design-system primitives and token utilities.
- `.orly/docs/SCHEMA_CONVENTIONS.md` — registered schema changes and privileges.
- `.orly/docs/REST_API_DESIGN_GUIDELINES.md` and `docs/AUTH.md` — private frame authorization, deletion and credential boundary.

## Applicable Gates

| Gate | Satisfaction strategy |
|------|-----------------------|
| LENGTH / public surface | Cohesive files below caps; exported shared symbols have production callers |
| UFS / error / logging | Named bounds, existing error shell, safe typed failures without provider bodies |
| UI / design tokens | Shared primitives and existing spacing/color utilities |
| Schema / architecture | Current schema policy and same-commit architecture updates |
| Lifecycle / coverage | Spec proof ledger, required Make targets, changed UI coverage and local acceptance |

## Prior-Art / Reference Implementations

- Runtime `afr_providers::Turns` and guarded streaming transport are the implementation to extract; Rig 0.44.0 remains the provider codec and event framer.
- Runtime `ReplyStreamDecoder`, reply registry and `FleetThreadViewport` provide first-chunk rendering, final replacement and single-scroll layout.
- Existing authoring receipt and draft observation guards remain authoritative for billing and private publication.
- Approved HTML: local `unified-fleet-chat-20261021/finalized.html`, chat-first editor and pinned save, with screenshot spacing and alignment.

## Sections (implementation slices)

### §1 — Shared backend provider reader

- **Dimension 1.1** — IN_PROGRESS: Both adapters use one `afd_inference` stream reader over Rig → Test `setup_and_runtime_read_the_same_stream`.
- **Dimension 1.2** — IN_PROGRESS: Bounded shared HTTP adapter preserves status, partial response and known usage on failures → Test `failed_stream_preserves_authoritative_usage`.
- **Dimension 1.3** — IN_PROGRESS: One physical attempt, shared inactivity bound and caller deadline; record dispatch deadline and align runner wait/replay → Test `pending_dispatch_uses_its_recorded_deadline`.

### §2 — Private setup answer delivery

- **Dimension 2.1** — IN_PROGRESS: Publish bounded answer-only deltas with message/pass identity and ordering to the private draft subject → Test `draft_stream_delivers_before_completion`.
- **Dimension 2.2** — IN_PROGRESS: Check creator, membership, deletion generation and Stop watermark before publication → Test `revocation_and_stop_withhold_private_text`.
- **Dimension 2.3** — IN_PROGRESS: Persist content-free failure cause, preserve unknown usage and final durable recovery without redispatch → Test `lost_completion_recovers_without_provider_retry`.

### §3 — Shared browser chat implementation

- **Dimension 3.1** — IN_PROGRESS: Extract shared connection lifecycle and reply ordering using the existing decoder → Test `both_chat_adapters_handle_the_same_stream_failures`.
- **Dimension 3.2** — IN_PROGRESS: Share viewport, avatar rows and composer while preserving fleet pending sends and setup controls → Test `both_chat_purposes_keep_composer_focus_and_single_scroll`.
- **Dimension 3.3** — IN_PROGRESS: Selected private history loads independently of model/sidebar requests; bounded in-memory state and scope cleanup prevent stale conversation disclosure → Test `selected_history_does_not_wait_for_models`.

### §4 — One fleet workspace and approved presentation

- **Dimension 4.1** — IN_PROGRESS: New fleet supports multiple setup turns, review and explicit Save; editing opens beside fleet Chat with visible Save changes → Test `setup_edit_saves_next_revision_without_losing_chat`.
- **Dimension 4.2** — IN_PROGRESS: Manual and conversational edits share one candidate; conflicts/failures retain edits, and future runs use the newly saved version → Test `failed_save_retains_manual_and_chat_edits`.
- **Dimension 4.3** — IN_PROGRESS: Remove redundant navigation/status glyphs; union authorized fleets and private drafts in compact Recent rows with aligned brand/icon/trash → Test `recent_fleets_and_drafts_keep_authorized_routes_and_status`.

### §5 — Verification and live review

- **Dimension 5.1** — IN_PROGRESS: Validate carry-over admission/acceptance fixes and shared changed UI coverage → Test `concurrent_history_admission_releases_pool_connections`.
- **Dimension 5.2** — IN_PROGRESS: Run declared repository gates and real local acceptance with matching build identities → Test `local_acceptance_uses_the_implemented_builds`.
- **Dimension 5.3** — IN_PROGRESS: Keep the actual local implementation running for Indy's visual sign-off before merge → Test `indy_live_visual_signoff` remains manual.

## Interfaces

The approved proposal defines `StreamTurn::open/next/finish`, a shared bounded `SendRequest` byte-stream transport, and typed turn controls/results in `afd_inference`. Runtime `Provider` and authoring `ModelResult` stay purpose-specific adapters. Exact Rust lifetime/types follow the pinned SDK and are recorded here when compiled.

Browser `ChatConnection` owns the network lifecycle; `ReplyStream` owns ordered live reply assembly using the existing decoder. `ChatThread` owns assistant-ui rendering and `ChatComposer` owns focus/input behavior. Fleet and private setup adapters supply their durable readers and actions.

Private `message.delta` frames carry draft/message/pass identifiers, stream sequence, start marker, answer kind and bounded text. Final message reads remain authoritative. Safe failure details distinguish timeout, transport interruption and unverified completion without exposing provider bodies. Existing routes and explicit saves remain.

## Failure Modes

| Failure | Required behavior | Negative test |
|---------|-------------------|---------------|
| Provider stalls or stream truncates | Incomplete answer; known usage retained, missing usage unknown; no setup resend | `failed_stream_preserves_authoritative_usage` |
| Failed HTTP response contains authoritative usage | Charge known facts; no executable proposal | `failed_http_response_preserves_usage` |
| Duplicate, gap, reordered or late frame | No corrupted concatenation; recover final durable answer | `both_chat_adapters_handle_the_same_stream_failures` |
| Stop, delete or lost authorization | No further private text; late usage can still settle | `revocation_and_stop_withhold_private_text` |
| Dropped final frame or reconnect | Read durable state, never repeat provider work | `lost_completion_recovers_without_provider_retry` |
| Slow models/sidebar or rapid navigation | Selected history loads independently; stale callbacks cannot paint | `selected_history_does_not_wait_for_models` |
| Save fails or revision conflicts | Preserve edits; no false Saved vN | `failed_save_retains_manual_and_chat_edits` |
| Long history or short viewport | Save/composer remain reachable in one scroll layout | `both_chat_purposes_keep_composer_focus_and_single_scroll` |

## Invariants

1. PostgreSQL dispatch intent commits before provider send, and replay cannot create another physical attempt. Receipt compare-and-update retains settled results.
2. Shared reader does not authorize tools, publish a candidate or save a fleet. Purpose adapters validate those effects.
3. Private text is creator-scoped and subject-scoped; guards check live membership/deletion/Stop before emission. No fleet/workspace broadcast of setup text.
4. Unknown usage never becomes zero or a fabricated completed answer. Partial text is provisional until durable finalization.
5. Saved revision changes only after successful explicit Save. Existing runs keep their pinned revision.
6. Streams, timers, caches and pending text carry account/workspace/person/subject identity and clear on lost access/logout.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| Existing provider outcome and stream recovery signals | ops | Call completes/fails or client recovers | IDs, duration, coarse safe cause | No prompt, provider body or credential | `failed_stream_preserves_authoritative_usage` |

Existing product actions remain; no new analytics funnel. Failure cause becomes a bounded diagnostic field rather than raw provider text.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `setup_and_runtime_read_the_same_stream` | Identical streamed fixtures → identical text, tools and completion facts |
| 1.2 | unit | `failed_stream_preserves_authoritative_usage` | Partial stream + usage + error → incomplete result with exact known usage |
| 1.3 | integration | `pending_dispatch_uses_its_recorded_deadline` | Pending replay before/after recorded deadline → dispatching/unknown, no physical resend |
| 2.1 | integration | `draft_stream_delivers_before_completion` | Provider held open after first answer → authorized browser receives answer first |
| 2.2 | integration | `revocation_and_stop_withhold_private_text` | Revoke/delete/Stop while provider is held → later private deltas withheld |
| 2.3 | integration | `lost_completion_recovers_without_provider_retry` | Drop final frame → GET recovers final result, physical-send count remains one |
| 3.1 | unit | `both_chat_adapters_handle_the_same_stream_failures` | Same duplicate/gap/disconnect fixtures → same safe live/final behavior |
| 3.2 | unit | `both_chat_purposes_keep_composer_focus_and_single_scroll` | Send/receive in both purposes → focus restored without a second scroller |
| 3.3 | unit | `selected_history_does_not_wait_for_models` | Block model request → selected history appears; old scope cannot update new scope |
| 4.1 | e2e | `setup_edit_saves_next_revision_without_losing_chat` | Multiple setup turns → Save v1 → edit alongside runtime → Save v2, runtime history retained |
| 4.2 | unit | `failed_save_retains_manual_and_chat_edits` | Manual/chat edits + conflict → same candidate/text retained and no version advance |
| 4.3 | unit | `recent_fleets_and_drafts_keep_authorized_routes_and_status` | Active/stopped fleets and unfinished drafts → compact titles, correct links and delete action |
| 5.1 | integration | `concurrent_history_admission_releases_pool_connections` | Concurrent admission with history → bounded pool occupancy and all accepted |
| 5.2 | e2e | `local_acceptance_uses_the_implemented_builds` | Matching local UI/API/runner builds → acceptance-e2e and acceptance-cli pass |
| 5.3 | manual | `indy_live_visual_signoff` | Indy opens local app and approves actual layout/flows; record real response |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Shared backend, private delivery and failure accounting | `make test-integration-rustd` | exit 0 | P0 | |
| R2 | Shared UI and changed coverage | `make test-coverage-all` | exit 0 | P0 | |
| R3 | Local browser and CLI acceptance | `make acceptance-e2e && make acceptance-cli` | exit 0 | P0 | |
| R4 | Conform gates | `make harness-verify` | exit 0 | P0 | |
| R5 | Lint | `make lint-all` | exit 0 | P0 | |
| R6 | Unit lanes | `make test-unit-all` | exit 0 | P0 | |
| R7 | Version synchronized | `make check-version` | exit 0 | P0 | |
| R8 | Secrets absent | `gitleaks detect` | exit 0 | P0 | |
| R9 | Actual local visual review | `manual: Indy reviews localhost:3100` | Recorded explicit approval before merge | P0 | |

## Dead Code Sweep

Remove the setup-only full-response collection and separate connection/viewport/composer implementations once their production callers use the shared code. Remove SetupPanel and FleetScheduleSummary carry-over deletions with zero remaining imports. Record exact replaced symbols and zero-reference searches in each implementation commit.

## Out of Scope

- Closing all of M218, M222, M224, M226 or M228; these retain their independent requirements.
- M229_002 work already merged from origin; do not claim it as this implementation.
- Automatic setup retry after uncertain provider work, shared setup/runtime permissions, provider credentials in the browser, fleet data deletion from navigation removal.
- Merge or deployment before Indy's live implementation sign-off.

## Product Clarity (authoring record)

1. **Successful user moment:** Describe a fleet, see a streamed answer, save it and edit its setup beside its working conversation.
2. **Preserved user behaviour:** Runtime messaging, pending sends, tools, shared history and explicit version saves continue.
3. **Optimal-way check:** One tested stream/render path serves both purposes; authorization and persistence remain purpose-specific.
4. **Rebuild-vs-iterate:** Extract existing runtime behavior and adapt setup; do not introduce another SDK or provider parser.
5. **What we build:** Shared backend reader, private deltas, shared browser lifecycle/rendering, one workspace and local proof.
6. **What we do NOT build:** Unrelated pending milestones, automatic retry or a new chat product.
7. **Fit with existing features:** Existing authoring receipts, fleet leases, assistant-ui, guarded transports and event subscriptions.
8. **Surface order:** Backend and UI together because live reply behavior spans both; CLI acceptance remains required.
9. **Dashboard restraint:** Compact version receipt, optional instructions, no fabricated status or duplicate save action for ordinary chat.
10. **Confused-user next step:** Actual bounded failure reason, visible Edit setup and Save changes, authoritative completion recovery.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Shared backend foundation, private delivery, browser foundation, workspace UX, verification; dependencies follow that order.
- **Alternatives considered:** Merely increasing setup timeout keeps the duplicate reader/render path. Routing setup through runtime tool execution breaks its restricted authority. Both are rejected.
- **Patch-vs-refactor verdict:** This is a focused refactor because the user explicitly requires one implementation for the duplicated behaviors, with existing purpose boundaries retained.

## Discovery (consult log)

- **Consults:** Indy approved the seven-part architecture and screenshot alignment: “Signed off - approved for implementation by Indy”. Prior HTML approval is distinct from the future live visual review. Origin main was merged into the existing worktree at `a05c291ce`; provider/streaming sources were still unchanged at approval. SDK is pinned to Rig 0.44.0; preserve partial/failed-response usage rather than assuming error replies lack usage.
- **Metrics review:** No new analytics funnel; retain existing events and add bounded safe failure information.
- **Skill-chain outcomes:** Spec workflow applied; unit/integration proofs, adversarial review and post-push monitoring pending.
- **Deferrals:** None. Live visual acceptance remains required and cannot be supplied by the implementing agent.
