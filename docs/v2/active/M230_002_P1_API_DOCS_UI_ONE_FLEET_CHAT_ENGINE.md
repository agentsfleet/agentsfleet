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
**Test Baseline:** unit=11280 integration=1378 kernel=61 (Linux)
**Baseline evidence:** https://github.com/agentsfleet/agentsfleet/actions/runs/39586239759
**Depends on:** M230_001 — stopped setup recovery and retained usage accounting
**Provenance:** LLM-drafted (GPT-6), based on Indy's approved seven-part proposal
**Canonical architecture:** `docs/architecture/api_resources.md`, `docs/architecture/runner_fleet.md`, `docs/architecture/web_app.md`

PR #780's Continuous Integration (CI) runs [Rust unit](https://github.com/agentsfleet/agentsfleet/actions/runs/39586239759), [UI unit](https://github.com/agentsfleet/agentsfleet/actions/runs/39586239902), and [integration coverage](https://github.com/agentsfleet/agentsfleet/actions/runs/39586239885). Its head `95f0df1d06a8483e88877a13c6eb385243aa1e92` and comparison revision have the same tree, `470797d5afabd18368ef416a42b55ec80ad65437`. Unit counts are Rust 4,808, app 3,889, website 133, CLI 1,952 and design system 498. The mixed coverage run includes unit tests; its 6,080 total is not an integration-only count. Matching named ignored datastore tests yields 1,378; benchmark and documentation examples are excluded.

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
| `docs/AUTH.md` | EDIT | Keep the authoring dispatch reference valid after its shared-stream module split |
| `docs/architecture/runner_fleet.md` | EDIT | Shared reader and private draft delivery |
| `docs/architecture/web_app.md` | EDIT | Shared UI and one fleet workspace |
| `docs/architecture/data_flow.md` | EDIT | Setup editing and version save flow |
| `docs/architecture/postgres_schemas.md` | EDIT | Register dispatch deadline and content-free failure columns from the forward migration |
| `scripts/check_architecture_doc_schema.py`, `scripts/check_architecture_doc_test_schema.sh`, `scripts/check_architecture_doc_test.sh` | EDIT | Indy approved validating additive migrations against the documented layout while retaining unsupported-change refusals |
| `docs/architecture/README.md` | EDIT | Index the shared chat explanation and verification evidence |
| `.github/workflows/test-unit-rustd.yml` | EDIT | Require the focused pinned-nightly Miri proof alongside stable unit tests in the existing required PR check |
| `docs/DESIGN_SYSTEM.md` | EDIT | Approved spacing, alignment, compact receipts and recent rows |
| `rustd/Cargo.toml`, `rustd/Cargo.lock` | EDIT | Latest stable shared chat dependencies, declared once in the workspace |
| `rustd/crates/afd_inference/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/tests.rs`, `src/tests/`, `src/stream.rs`, `src/stream/`, `src/transport.rs`, `src/transport/`, `src/document.rs`, `src/logs/` | EDIT/CREATE | Shared provider stream, bounded byte transport, failure, deadline and correlated trace ownership |
| `rustd/crates/afr_providers/src/lib.rs`, `src/turn.rs`, `src/transport/`, `src/connect/`, `src/request/`, `src/error/` | EDIT | Runtime adapter delegates to shared reader/transport and current Rig request types |
| `rustd/crates/afr_providers/tests/`, `rustd/crates/afr_agent/src/turn.rs`, `src/turn/`, `src/loop/mod.rs`, `src/loop/model_turn.rs` | EDIT/CREATE | Preserve partial usage through a failed runtime turn and prove no executable completion follows |
| `rustd/crates/afd_api_tenant/src/handler/draft/render.rs`, `rustd/crates/afd_authoring/src/lib.rs`, `src/message/read.rs` | EDIT | Carry safe recorded failure causes through durable private message reads |
| `rustd/crates/afd_authoring/Cargo.toml`, `src/model/`, `src/types/failure.rs`, `src/observe/`, `src/lease/guard/` | EDIT | Setup stream adapter, publication guard, durable deadline and failure facts |
| `rustd/crates/afd_authoring/src/sql/message/{mod.rs,running.sql}` | EDIT/CREATE | Read the running message independently of the bounded history hints before releasing a private preview |
| `rustd/crates/afd_authoring/tests/secret_wiping.rs` | CREATE | Observe private preview, daemon and runner secret allocations before free, including cloned text and replaced nested objects, with an unwiped negative control |
| `rustd/crates/afd_authoring/src/model/receipt/replay.rs` | EDIT | Keep the existing pending-deadline boundary case beside its private implementation; remove the separate test module to satisfy Rust file naming and fallible test rules |
| `rustd/crates/afd_authoring/src/message/accept.rs`, `src/replay/mod.rs`, `src/sql/message/admit.sql` | EDIT/CREATE | Carry-over bounded admission history reads |
| `rustd/crates/afd_authoring/src/draft/mod.rs`, `tests/draft/admission/history.rs`, `tests/draft/admission/history/latency.rs`, `tests/draft/admission/history/waiting.rs` | EDIT/CREATE | Indy approved per-chat waiting before database acquisition with existing Tokio and bounded Moka; prove independent service ordering, spare pool capacity and cancellation |
| `rustd/crates/afd_authoring/tests/` | EDIT/CREATE | Real dispatch, usage, privacy, replay and admission regressions |
| `rustd/crates/afd_authoring/tests/draft/model_calls/deadline.rs`, `tests/draft/model_calls/progress/deadline.rs` | CREATE | Prove elapsed authorization prevents physical sends and a blocked preview cannot stall durable completion |
| `rustd/crates/afd_authoring/tests/draft/fixture/upgrade.rs`, `tests/draft/fixture.rs` | CREATE/EDIT | Prove dispatch deadline backfill and an older daemon's inserts across the additive migration |
| `rustd/crates/afd_api/tests/authoring/models/` | EDIT | Keep the router's external-provider fixture on the shared incremental transport |
| `rustd/crates/afd_api/tests/authoring/stream/privacy.rs` | EDIT | Prove idle named heartbeat bytes and access revocation on an already open private stream |
| `rustd/crates/afd_api/tests/authoring/stream/{mod,live,preview,recovery}.rs` | EDIT/CREATE | Exercise private frame filters, real hub gaps/lag, stream capacity and durable closure through the full authenticated router |
| `rustd/crates/afd_api/tests/integration_workspace_approvals.rs` | EDIT | Isolate two concurrently seeded identities whose collision blocked the canonical coverage run |
| `rustd/crates/afd_api_tenant/src/handler/draft/stream/` | EDIT | Authorized private live frames with durable recovery |
| `rustd/crates/afd_api_wire/src/authoring/output.rs` | EDIT | Safe structured failure information |
| `rustd/crates/afd_sse/src/`, `rustd/crates/afd_dragonfly/src/streams/`, `rustd/crates/agentsfleetd/src/plane/surfaces.rs` | EDIT | Reuse bounded subscriptions for private draft subjects and wire their production publisher |
| `rustd/crates/afr_supervisor/src/client/http.rs` | EDIT | Wait deadline covers permitted setup provider work and receipt |
| `schema/`, `rustd/crates/afd_db/src/` | EDIT/CREATE | Dispatch deadline/failure storage and schema registration under current policy |
| `rustd/crates/afd_db/tests/integration_migrate/shape.rs` | EDIT | Keep the exact migrated-layout assertion aligned with the two approved dispatch columns |
| `ui/packages/app/lib/streaming/` | EDIT/CREATE | Shared scoped connection, reply ordering and recovery |
| `ui/packages/app/lib/api/draft*`, `ui/packages/app/lib/api/setup-proxy/` | EDIT | Private live deltas, independent selected-history loading and fleet listing |
| `ui/packages/app/lib/api/recent-collection.ts` | CREATE | Own bounded validated Recent pages without fetching model choices |
| `ui/packages/app/components/domain/FleetThread.tsx`, `FleetThreadViewport.tsx`, `FleetMessageRow.tsx`, `SteerComposer.tsx` | EDIT | Shared chat shell and purpose adapters |
| `ui/packages/app/components/domain/FleetStreamProvider.tsx`, `useFleetEventStream.ts`, `useFleetRunSummary.ts` | EDIT/CREATE | Own shared connections within the mounted account/workspace/subject and keep render pure |
| `ui/packages/app/components/domain/chat/` | EDIT/CREATE | Chat-first setup editor, shared composer and save flow |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/components/{platform-columns,PlatformCatalogTable,EditFleetDialog}.tsx`, `ui/packages/app/components/domain/island-dynamic/EditFleetDialogDynamic.tsx` | EDIT | Indy approved preserving the original Edit button through the loading handoff so closing the editor restores keyboard focus; use the existing accessibility case |
| `ui/packages/app/components/layout/` | EDIT | One Recent list, compact aligned navigation and hover trash |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/page.tsx`, `secrets/components/SecretsList.tsx` | EDIT | Indy approved the two existing layout wrappers filling their available height, using the existing acceptance case |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/` | EDIT/DELETE | Fleet workspace editor and removal of redundant Setup/Your fleets navigation |
| `ui/packages/app/tests/`, colocated component tests | EDIT/CREATE | Shared UI, lifecycle, privacy, saves and acceptance proofs |
| `ui/packages/design-system/src/design-system/Textarea.tsx`, `Textarea.test.tsx` | EDIT | Carry-over quiet focused caret behavior |
| `VERSION`, workspace version manifests, `docs/RUST_ERROR_STANDARD.md` | EDIT | Requested release version and shared typed response-bound failure |
| Root, CLI, scripts and UI `package.json` manifests, Bun lockfiles, `rustd/Cargo.toml`, `rustd/Cargo.lock`, `make/quality.mk` | EDIT | Requested dependency upgrades and pinned tsrs migration; run explicit checks and command-line emission through Bun, retaining TypeScript for Next.js internals |
| `cli/test/completions.unit.test.ts`, `cli/test/golden/help-no-color.txt` | EDIT | Match Effect's supported completion shells after the requested dependency upgrade |
| `docs/architecture/shared_chat_verification.md`, `rustd/crates/afr_*/src/`, `rustd/crates/agentsfleet_runner/src/` | AUDIT/EDIT | Indy requested a logging and standard-library reuse audit across at least fifteen runner-related crates; record exact inventory and fix evidenced gaps |
| `rustd/crates/afd_dragonfly/src/outbound/reader.rs` | EDIT | Preserve the actual acknowledgement error code during the requested runner dependency logging audit |
| `rustd/crates/afd_dragonfly/tests/{datastore_suite,outbound_faults}.rs` | EDIT/CREATE | Prove malformed pending-entry acknowledgement failure and recovery over the real protocol client |
| `deploy/local/`, `make/acceptance.mk`, `make/dev.mk`, `Makefile`, `scripts/local-dev.sh`, `docs/development.md` | EDIT/CREATE | Approved local deployment directory and one coordinator calling existing Make targets; Compose owns Caddy, existing development containers supply datastores, and the shipping deployment script owns Linux runner installation; remove the draft UI, runner-build and runner helpers |
| `scripts/build-acceptance.ts` | EDIT | Build the four components in order, optionally building the runner on the prepared Linux machine; remove source hashes, receipts and manifests |
| `scripts/{acceptance-artifacts,build-manifest,build-component,build-source,inspect-acceptance-build,observe-acceptance-runner,deploy-dev-dashboard,dev-dashboard}.ts`, `scripts/{observe-dev-runner,acceptance_build_binding_test}.sh` | DELETE | Approved removal of acceptance build tracking and remote process observation; direct compiler and Vercel commands take over building and deployment |
| `cli/test/{acceptance-artifacts,build-source,observe-acceptance-runner,dev-dashboard,build-acceptance}.test.ts`, `cli/test/build-fixture.ts`, `ui/packages/app/tests/build-binding*.test.ts`, `ui/packages/app/tests/e2e/acceptance/fixtures/build-binding*.ts`, `deploy/local/deployment{,.test}.ts` | DELETE | Remove tests and private dashboard copying required only by the deleted tracking checks |
| `.github/actions/acceptance-inputs/action.yml` | DELETE | Remove build downloads and remote process observation before functional acceptance |
| `.github/workflows/deploy-dev-{build,dashboard,acceptance}.yml`, `cli/test/acceptance/global-setup.ts`, `ui/packages/app/tests/e2e/acceptance/global-setup.ts` | EDIT | Remove tracking callers; retain authentication, fixture setup, deployed acceptance commands, signed toolbox checks and the development dashboard destination |
| `cli/test/acceptance/fixtures/{cli.js,cli.d.ts,browser.ts}` | EDIT | Run worktree acceptance with Bun as requested; forward the local public certificate authority to child processes without inheriting operator credentials |
| `cli/test/acceptance/{grant-approval-live,steer-live}.spec.ts`, `cli/test/acceptance/fixtures/{grant-ops,seed}.ts` | EDIT | Update existing grant and missing-model journeys for partial setup; assert the actual missing-model observation instead of accepting a timeout; retain real CLI execution and billing assertions |

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

- Runtime `afr_providers::Turns` and guarded streaming transport are the implementation to extract; Rig remains the provider codec and event framer. Indy subsequently requested latest dependencies: registry verification selects Rig 0.45.0, Tokio 1.54.0 and Axum 0.8.9. Other direct chat dependencies also use their latest stable workspace declarations.
- Runtime `ReplyStreamDecoder`, reply registry and `FleetThreadViewport` provide first-chunk rendering, final replacement and single-scroll layout.
- Existing authoring receipt and draft observation guards remain authoritative for billing and private publication.
- Approved HTML: local `unified-fleet-chat-20261021/finalized.html`, chat-first editor and pinned save, with screenshot spacing and alignment.

## Sections (implementation slices)

### §1 — Shared backend provider reader

- **Dimension 1.1** — DONE — Both adapters use one `afd_inference` stream reader over Rig → Test `setup_and_runtime_read_the_same_stream`.
- **Dimension 1.2** — DONE — Bounded shared HTTP adapter preserves status, partial response and known usage on failures → Test `failed_stream_preserves_authoritative_usage`.
- **Dimension 1.3** — DONE — One physical attempt, shared inactivity bound and caller deadline; record dispatch deadline and align runner wait/replay → Test `pending_dispatch_uses_its_recorded_deadline`.

### §2 — Private setup answer delivery

- **Dimension 2.1** — DONE — Publish bounded answer-only deltas with message/pass identity and ordering to the private draft subject → Test `draft_stream_delivers_before_completion`.
- **Dimension 2.2** — DONE — Check creator, membership, deletion generation and Stop watermark before publication → Test `revocation_and_stop_withhold_private_text`.
- **Dimension 2.3** — DONE — Persist content-free failure cause, preserve unknown usage and final durable recovery without redispatch → Test `lost_completion_recovers_without_provider_retry`.

### §3 — Shared browser chat implementation

- **Dimension 3.1** — DONE — Extract shared connection lifecycle and reply ordering using the existing decoder → Test `both_chat_adapters_handle_the_same_stream_failures`.
- **Dimension 3.2** — DONE — Share viewport, avatar rows and composer while preserving fleet pending sends and setup controls → Test `both_chat_purposes_keep_composer_focus_and_single_scroll`.
- **Dimension 3.3** — DONE — Selected private history loads independently of model/sidebar requests; bounded in-memory state and scope cleanup prevent stale conversation disclosure → Test `selected_history_does_not_wait_for_models`.

### §4 — One fleet workspace and approved presentation

- **Dimension 4.1** — DONE — New fleet supports multiple setup turns, review and explicit Save; editing opens beside fleet Chat with visible Save changes → Test `setup_edit_saves_next_revision_without_losing_chat`.
- **Dimension 4.2** — DONE — Manual and conversational edits share one candidate; conflicts/failures retain edits, and future runs use the newly saved version → Test `failed_save_retains_manual_and_chat_edits`.
- **Dimension 4.3** — DONE — Remove redundant navigation/status glyphs; union authorized fleets and private drafts in compact Recent rows with aligned brand/icon/trash → Test `recent_fleets_and_drafts_keep_authorized_routes_and_status`.

### §5 — Verification and live review

- **Dimension 5.1** — DONE — Validate carry-over admission/acceptance fixes and shared changed UI coverage → Test `concurrent_history_admission_releases_pool_connections`.
- **Dimension 5.2** — IN_PROGRESS — Run declared repository gates and real local acceptance with matching build identities → Test `local_acceptance_uses_the_implemented_builds`.
- **Dimension 5.3** — DONE — Record Indy's visual sign-off and chosen review timing before merge → Test `indy_live_visual_signoff` remains manual; Indy signed off and prefers viewing after deployment, as quoted in Discovery.
- **Dimension 5.4** — DONE — Enforce owned-secret zeroization and valid unsafe observation in PR CI → Test `owned_secret_buffers_are_wiped_before_free` passed on stable and Miri; four wipe-removal probes fail the intended assertion. The workflow passes lint and makes the existing required aggregate depend on stable and pinned-nightly Miri. Hosted execution remains part of the post-push checks.
- **Dimension 5.5** — IN_PROGRESS — `acceptance-local` builds and starts the local stack, then calls the existing acceptance targets → Test `local_acceptance_runs_existing_suites`; preserve shared datastore containers and deployed target recipes, use the shipping runner deployment, and stop the owned dashboard on exit.

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
7. React renders remain idempotent, with immutable props/state and static top-level Hooks. Streams, timers and subscriptions start outside render and clean up on identity changes and unmount. Next.js server data loading stays on the server; interactive streaming uses an explicit client boundary.

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
| 5.2 | e2e | `local_acceptance_uses_the_implemented_builds` | Build and start the local UI/API/runner → acceptance-e2e and acceptance-cli pass |
| 5.3 | manual | `indy_live_visual_signoff` | Record Indy's own visual sign-off and review timing without claiming a browser inspection |
| 5.4 | unit | `owned_secret_buffers_are_wiped_before_free` | Preview, daemon secret text/bytes/object and runner token allocations are wiped before free, including clones and nested replacements; an unwiped control stays nonzero; Miri checks the observer's raw-memory accesses |
| 5.5 | unit + e2e | `local_acceptance_runs_existing_suites` | The local command calls existing build and acceptance targets with the shipping runner deployment; shared datastore containers survive startup; prerequisite or build failures stop before suite execution; the owned dashboard stops on exit |

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

- Oct 22, 2026: Indy approved the historical `spec.ordering` exception: the original heartbeat fix at `ab3f0f0d` predates the M230 spec. Preserve the published history and record the approved exception with `orly override spec.ordering`; complete acceptance and all other checks remain required.

> Indy (Oct 22, 2026): "yes approved fo rhte spec.ordering."

- Oct 22, 2026: Indy requested finishing complete acceptance, signed off the visual requirement and chose viewing after deployment. This records owner approval, not an authenticated browser inspection. Further documentation work is skipped by instruction; complete browser and command-line acceptance remains required.

> Indy (Oct 22, 2026): "visual check - signed off i prefer to check when deployed."

> Indy (Oct 22, 2026): "skip documentation"

- Oct 22, 2026: The final `make test-unit-all` passed: Rust runner 1,316, daemon 137, daemon libraries 3,293, Python 25, app 3,973, website 133, command line 1,887 and design system 499. The command-line suite retained 16 existing skips; all four package coverage gates passed at 100%. `make test-integration-rustd` passed 1,400 regular and three exclusive tests against isolated local datastores. The focused Miri wiping test passed after the existing-test style corrections. No new files or test cases were added for these corrections.

- Oct 22, 2026: The latest complete local browser attempt reported 103 passed, five failed, one skipped and five not run. Four failures reached provider refusals. The fifth exposed an acceptance-helper race with completed workspace navigation; awaiting the new workspace URL fixed the existing operator journey, which passed its focused rerun. Command-line acceptance reported 209 deterministic passes, then 171 live passes and three failures. Two live turns reached provider refusals; the existing missing-model case now expects setup's `pending` state, retains its model/action/no-execution assertions and passed its focused rerun. Complete acceptance remains pending.

- Oct 22, 2026: The configured free MiniMax model was absent from OpenRouter's live catalogue. A zero-charge, tool-capable Nvidia model replaced it through the existing local operator API. A minimal provider request then returned HTTP 429 with `free-models-per-day`, confirming exhausted daily free quota. No credits were purchased or paid model selected. At that point the actual app remained at localhost:3100 for Indy's separate live review; the subsequent visual sign-off is recorded above.

- Oct 22, 2026: Indy approved the four-file library-editor focus fix: "Approve the four-file fix". A delayed editor-module load reproduced focus returning to the page body after Escape; the normal loaded path returned it to Edit. Carry the original button through the existing loader and editor, and exercise the loading handoff in the existing accessibility case. Add no files or test cases.

- Oct 22, 2026: Indy approved migrating to tsrs, compiling and pushing the Pull Request: "You can migrate and compile and push the PR with tsrc" and "i suggest you migrate." The earlier compilation stop applied to context-save only. Pin published tsrs `0.18.0-ts7.1.0-dev.20260929` in existing manifests and lockfiles; compare the existing configurations, rejection of a temporary type error and isolated command-line emission. Next.js calls TypeScript's `--showConfig`, which tsrs does not support, so retain its TypeScript dependency. Add no repository files or test cases for this migration.

- Oct 22, 2026: Indy approved the final file table after requesting simple local acceptance without build hashes or receipts: "Signed off approved". Delete the listed tracking files and their tests. Keep one local coordinator and Caddy Compose file, build through the existing Make target, and serve the ordinary dashboard build. Use direct development workflow commands for building and Vercel deployment. Functional acceptance, authentication and signed toolbox verification remain required. Indy subsequently requested Bun for local runtime and said: "Ask Indhy for approval on any new files you add in this acceptance work". Add no further acceptance files without that approval.

- Oct 22, 2026: Indy requested PR enforcement for zeroization and unsafe-memory checks. The stable wiping regression belongs to the existing daemon-library unit shard. Add a focused Miri job that installs pinned nightly `2026-10-22`; include its result in the existing required `test-unit-rustd` aggregate.
- Oct 22, 2026: Indy asked why the Miri check covered only authoring. Extend the isolated observer through public daemon and runner secret owners, retaining one allocator implementation. This checks selected owned buffers; it does not certify external serializer or network scratch storage.

- Oct 22, 2026: The new cancellation probe failed before cancellation while observing waiters. Its lock-holding transaction reused a cached `pg_stat_activity` session list, hiding connections opened after the first census. A separate local probe observed zero cached waiters against one live waiter; clearing the snapshot revealed that waiter. Priming the census before opening the sender makes all three waiting probes fail deterministically with the old observer. Reusing the existing independent autocommit monitor pattern makes all seven admission cases pass, with the five-second deadline and pool-capacity assertions unchanged. Canonical `make test-unit-all` passed 11,348 tests on macOS, with 16 existing CLI skips. The fresh `make test-integration-rustd` run passed 1,386 normal and three exclusive tests. `make lint-all`, `make harness-verify` and `make check-version` also passed. Matching-build acceptance, final per-file coverage and the remaining mutations are still required.

- Oct 22, 2026: Canonical integration reached the empty-database migration test and rejected its stale eleven-column dispatch expectation. Migration 2130 correctly creates thirteen columns, including `dispatch_expires_at` and `failure_cause`. Update that exact expectation without changing assertions, schema bounds or the migration. The authoring suite passed 233 integration cases, with its two unit cases filtered out, in the same run; full integration must rerun after the schema-test repair.

- Oct 22, 2026: After three failed contention approaches, Indy approved the next investigation: “Yes, investigate and implement if proved”. Use existing Tokio and bounded Moka primitives to wait per chat before taking a database connection. Postgres remains authoritative across servers. Keep the 100-sender case, its pool timeout and its two-database-waiter assertion; distribute those senders across two independently constructed services to exercise the distributed boundary. Add real pool-capacity and cancellation proofs before retaining the implementation.

- Oct 22, 2026: The contention regression reproduced both failures before the local gate: 100 delayed sends exhausted the unchanged five-second pool budget, and 100 waiting sends starved an independent chat in a two-slot pool. After the Moka/Tokio change, all seven focused admission cases pass, including two independent services, queued/current cancellation and rollback. The database row lock and fresh receipt snapshot remain unchanged. Cache eviction can split a local queue; it cannot bypass the database lock.

- Oct 22, 2026: The real HTTP idle-stream regression received only an Axum comment heartbeat before the fix. The handler now emits the shared named heartbeat after a current access check; all three private stream tests pass. A separate isolated Postgres test proves migration 2130 preserves populated deadlines and permits an older daemon's inserts during replacement. These are focused proofs; full repository verification remains pending.

- Oct 22, 2026: After the schema-layout audit rejected migration 2130, Indy approved its proposed additive-migration support: “Yes add the schema, what is repository policy”. Shipped migrations remain frozen; the audit must compare added columns with the documented layout and keep rejecting unsupported reshaping. Indy redirected the paused contention investigation to api-dev's PlanetScale database: “so use the url from 1Password ZMB_CD_DEV/planetscale-dev/migrat*url”. Start with read-only inspection; do not run the schema-resetting integration lane against that shared database.

- Oct 21, 2026: Indy requested release 0.64 or the next available version, at least 99% changed-line coverage for each Rust source file, Cargo mutation tests, concurrent/performance/race integration evidence, green local acceptance, and one pushed Pull Request. Use `orly gate pr`, gstack review and orly-babysit-prs; do not invoke gstack ship. Merge remains Indy's morning decision. Paired content-free request tracing and measured request/response timing must be included in the implementation and Pull Request evidence.

- **Consults:** Indy approved the seven-part architecture and screenshot alignment: “Signed off - approved for implementation by Indy”. Prior HTML approval is distinct from the future live visual review. Origin main was merged into the existing worktree at `a05c291ce`; provider/streaming sources were still unchanged at approval. Rig was pinned to 0.44.0 at approval; the requested dependency upgrade now uses 0.45.0. Preserve partial/failed-response usage rather than assuming error replies lack usage.
- **Metrics review:** No new analytics funnel; retain existing events and add bounded safe failure information.
- **Skill-chain outcomes:** Spec workflow applied; canonical unit and integration proofs passed; the in-host adversarial production review returned no findings. Browser acceptance, final review completion and post-push monitoring remain pending.
- **Deferrals:** None for functional acceptance. Visual review timing follows Indy's recorded sign-off; further documentation work is skipped by the recorded instruction.
