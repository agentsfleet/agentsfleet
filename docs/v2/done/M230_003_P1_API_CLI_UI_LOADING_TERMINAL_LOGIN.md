# M230_003: Consistent page loading and terminal authorization

**Prototype:** v2.0.0
**Milestone:** M230
**Workstream:** 003
**Date:** Oct 24, 2026
**Status:** DONE
**Closure:** Indy requested archival on 2026-10-25. Open evidence below remains recorded; PR #784 still requires green Continuous Integration (CI) and review.
**Priority:** P1 — consistent page loading
**Categories:** UI
**Batch:** B1 — existing approved stream
**Branch:** fix/approved-fleet-chat
**Baseline revision:** 74a39c60ef260973894a68f29d0b6e4f6f1c6eda
**Test Baseline:** unit=11270 integration=561
**Baseline evidence:** /tmp/loader-baseline-report.md
**Baseline notes:** Unit: 0 failed, 16 skipped, 1371 ignored. Integration: 1 failed; selection incomplete. The report retains both original logs.
**Depends on:** none
**Provenance:** LLM-drafted (Codex, Oct 24, 2026)
**Canonical architecture:** `docs/DESIGN_SYSTEM.md` — page loading

## Overview

**Goal (testable):** Every page shows one shared accessible loader. A deliberate browser approval completes same-computer command-line interface (CLI) login automatically; explicit remote login retains manual verification.
**Problem:** Route loading boundaries and page data use inconsistent skeleton layouts.
**Solution summary:** Center the existing Spinner inside the persistent shell; fill the viewport before that shell resolves. Complete explicitly authorized local terminal sign-in through a bound loopback callback; retain manual verification for remote terminals.

**Additional approved scope:** Shorten public API descriptions and reconcile contributor guides. Indy also approved shared setup/runtime compaction and the sidebar-aware Billing design. Complete compaction first; Billing reporting and integration follow. The compaction file inventory and design are in [Shared conversation compaction](../../architecture/context_lifecycle.md).

## PR Intent & comprehension handshake

- **PR title (eventual):** Share conversation compaction and add settled Billing reporting
- **Intent (one sentence):** Continue setup and runtime conversations near the selected model's window, expose settled account spending, and retain the approved page-loading, local terminal-login and documentation changes.
- **Handshake:** Indy approved the loader and Authorize wording, then requested the Muse-inspired login improvements with everything in PR #784. The later instruction supersedes the earlier manual-only scope. The unsafe experimental branch `fix/terminal-login-proof` supplies no implementation or proof for this design; §5 requires fresh binding and adversarial evidence.

## Implementing agent — read these first

1. `ui/packages/app/components/layout/ShellFrame.tsx` — persistent navigation.
2. `docs/DESIGN_SYSTEM.md` — tokens, motion and components.
3. `ui/packages/app/components/layout/RouteLoading.tsx` — shared indicator.
4. `ui/packages/app/app/cli-auth/[session_id]/page.tsx` — existing sign-in behavior and button.

## Files Changed (blast radius)

The following inventory covers this delivery; no experimental authentication changes belong in it.

| File | Action | Why |
|---|---|---|
| `README.md` | EDIT | Describe automatic same-computer approval and explicit manual login for a browser on another computer. |
| `deploy/local/acceptance.sh`<br>`deploy/local/compose.yml` | EDIT | Owner-approved local HTTPS dashboard frontdoor for unchanged concurrent fleet acceptance. |
| `make/acceptance.mk`<br>`.github/workflows/deploy-dev-acceptance.yml`<br>`.github/workflows/deploy-dev.yml` | EDIT | Keep full local acceptance and the short deployment check; report the owner-deferred manual ten-fleet check. |
| `.github/workflows/acceptance-ten-fleets.yml` | CREATE | Run ten concurrent fleets against api-dev only on Indy's manual dispatch after merge. |
| `ui/packages/app/playwright.acceptance.config.ts`<br>`ui/packages/app/tests/e2e/acceptance/fleet-execution.spec.ts`<br>`ui/packages/app/tests/e2e/acceptance/fleet-continuity-final.spec.ts`<br>`playbooks/deploy/dev/001_playbook.md` | EDIT | Use native tags, skip and project selection; document separate short and ten-fleet results. |
| `docs/DESIGN_SYSTEM.md`<br>`docs/v2/done/M230_003_P1_API_CLI_UI_LOADING_TERMINAL_LOGIN.md` | EDIT | Shared loading, authorization button wording or its evidence; owner-requested spec closure. |
| `docs/architecture/sandbox_evaluation.md` | CREATE | Source-backed SPACE comparison, proposed coding journey, failure proofs and performance measurements; no backend adoption claim. |
| `docs/architecture/billing_reporting.md` | CREATE | Approved Billing read flow, implementation scope, UI behavior and query-plan proofs. |
| `concepts_brief.md` | CREATE | Working sandbox brainstorm based on owner inputs; native source reuse, performance ownership and unresolved recovery boundaries. |
| `docs/reviews/space-sandbox/README.md` | CREATE | Preserve the concept-review state and delivery order; compaction and Billing remain in this stream. |
| `docs/reviews/space-sandbox/review-evidence.json` | CREATE | Archive the original concept-review inputs, reports and digests without claiming a final implementation review. |
| `docs/reviews/pr784-verification-history.json` | CREATE | Preserve the original verification results and failures verbatim outside the implementation spec. |
| `docs/reviews/space-sandbox/sandbox-handoff-prompt.md` | CREATE | Restart the separately specified sandbox implementation only after pull request 784 merges. |
| `docs/reviews/space-sandbox/orly-egm-jev-handoff-prompt.md` | CREATE | Hand a separate Orly agent the requested design-process and rubric-grading improvement. |
| `ui/packages/app/app/(dashboard)/admin/fleet-libraries/loading.tsx`<br>`ui/packages/app/app/(dashboard)/admin/models/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/loading.tsx`<br>`ui/packages/app/app/(dashboard)/admin/runners/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/invites/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/loading.tsx` | CREATE | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/settings/account/[[...account]]/page.tsx`<br>`ui/packages/app/app/(dashboard)/settings/account/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/loading.tsx`<br>`ui/packages/app/app/(dashboard)/settings/billing/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/settings/members/loading.tsx`<br>`ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/[gateId]/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/loading.tsx`<br>`ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/page.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/events/loading.tsx`<br>`ui/packages/app/app/(dashboard)/w/[workspaceId]/events/page.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/loading.tsx`<br>`ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/new/loading.tsx`<br>`ui/packages/app/app/(dashboard)/w/[workspaceId]/integrations/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/library/loading.tsx`<br>`ui/packages/app/app/(dashboard)/w/[workspaceId]/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/loading.tsx`<br>`ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/loading.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/page.tsx`<br>`ui/packages/app/app/cli-auth/[session_id]/page.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/app/loading.tsx` | CREATE | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/components/layout/RouteLoading.tsx`<br>`ui/packages/app/components/layout/ShellFrame.tsx` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/tests/cli-auth-approval-transport.test.ts`<br>`ui/packages/app/tests/cli-auth-page.test.ts` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/tests/fleets-routes/list-and-loading.test.ts` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/tests/api-keys-page.test.ts` | EDIT | Remove the obsolete duplicate skeleton assertion; the route is covered by the shared-loader table. |
| `ui/packages/app/tests/loading-states.test.ts` | EDIT | Shared loading, authorization button wording or its evidence. |
| `ui/packages/app/tests/e2e/acceptance/dashboard-performance.spec.ts` | EDIT | Align loader assertions and wait for the sidebar's first fleet read before measuring hover script downloads. |
| `ui/packages/app/tests/e2e/acceptance/auth-routing.spec.ts` | EDIT | Verify streamed missing-settings output and noindex while retaining unmatched workspace 404 and available access notices. |
| `ui/packages/app/tests/e2e/acceptance/provider-credential-reference.spec.ts` | EDIT | Select a configured catalogue model and require successful credential storage, registration and activation. |
| `ui/packages/app/tests/e2e/acceptance/multi-fleet-grant-journey.spec.ts` | EDIT | Await the approved fleet's active state before navigating to the next fleet, preserving strict grant and execution assertions. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/AddModelEntryDialog.tsx` | EDIT | Refresh the registry after successful activation while retaining the committed entry after activation refusal. |
| `ui/packages/app/tests/models-registry-created-activation.test.tsx` | CREATE | Prove visible activation state and retained registration across success and refusal through the real table and HTTP boundary. |
| `ui/packages/app/tests/e2e/acceptance/workspace-chat-privacy.spec.ts` | EDIT | Reload only after acknowledged message persistence while preserving exact privacy and recovery assertions. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/private-chat.ts` | EDIT IF NEEDED | Synchronize the actual composer submission with its server acknowledgement. |
| `cli/src/commands/login.ts` | EDIT | Select local automatic handoff or explicit remote manual verification. |
| `cli/src/commands/login-device-flow.ts` | EDIT | Reuse verification and decryption after a bound local handoff. |
| `cli/src/commands/login-helpers.ts` | EDIT | Remove the secret session identifier from login-completed telemetry. |
| `cli/src/commands/login-browser-flow.ts` | CREATE | Own callback lifetime, waiting cancellation and credential finalization. |
| `cli/src/commands/login-callback.ts` | CREATE | Bound loopback listener with single-winner validation. |
| `cli/src/commands/login-callback-protocol.ts` | CREATE | Validate the trusted destination and define matching-code encoding. |
| `cli/src/commands/login-callback-page.ts` | CREATE | Static secret-free callback outcome page. |
| `cli/test/login-callback.unit.test.ts` | CREATE | Attack listener validation, replay, races, expiry and cleanup. |
| `cli/test/login-browser-flow.unit.test.ts` | CREATE | Prove automatic verification, cancellation and finalization through real helpers. |
| `cli/test/login-effect.unit.test.ts` | EDIT | Retain manual behavior and cover automatic browser selection. |
| `cli/test/helpers-login-effect.ts` | EDIT | Extend the external browser fixture for the local callback. |
| `cli/test/login-logout-identity.unit.test.ts` | EDIT | Verify login analytics omits the secret session identifier. |
| `cli/test/acceptance/fixtures/browser.ts` | EDIT | Select the approved Authorize label in the existing manual live journey. |
| `ui/packages/app/lib/auth/cli-loopback.ts` | CREATE | Validate callback parameters and matching-code encoding. |
| `ui/packages/app/lib/auth/cli-loopback-store.ts` | CREATE | Preserve a bounded expiring handoff across browser sign-in. |
| `ui/packages/app/app/cli-auth/[session_id]/cli-loopback-approval.tsx` | CREATE | Display identity and comparison code before mutually exclusive Authorize or Deny. |
| `ui/packages/app/app/(auth)/layout.tsx` | EDIT | Block session recording of sign-in and sign-up surfaces. |
| `ui/packages/app/instrumentation-client.ts` | EDIT | Capture and strip sensitive fragments before instrumentation. |
| `ui/packages/app/tests/instrumentation-client.test.ts` | EDIT | Supply browser location in the external instrumentation fixture. |
| `ui/packages/app/lib/analytics/posthog.ts` | EDIT | Exclude authentication routes and destinations from collection. |
| `ui/packages/app/lib/analytics/identity-gate.ts` | CREATE | Hold telemetry until the committed browser identity is ready and synchronize it before releasing events. |
| `ui/packages/app/components/analytics/AnalyticsBootstrap.tsx` | EDIT | Resume safe analytics after committed navigation away from authentication. |
| `ui/packages/app/tests/analytics-bootstrap.test.tsx` | CREATE | Prove authentication entry and interrupted initialization recover on dashboard navigation. |
| `ui/packages/app/tests/app-components.test.ts` | EDIT | Preserve bootstrap identity assertions across asynchronous analytics initialization. |
| `ui/packages/app/lib/analytics/auth-privacy.ts` | CREATE | Filter authentication URLs before analytics capture or delivery. |
| `ui/packages/app/next.config.ts` | EDIT | Apply the callback form's route-specific origin referrer policy. |
| `ui/packages/app/tests/cli-loopback.test.ts` | CREATE | Reject malformed, substituted, conflicting and expired browser handoffs. |
| `ui/packages/app/tests/cli-loopback-analytics.test.ts` | CREATE | Prove authentication and secret-bearing telemetry is discarded. |
| `ui/packages/app/tests/posthog.test.ts` | EDIT | Verify authentication exclusion with initialized and deferred analytics. |
| `ui/packages/app/tests/cli-loopback-approval.test.tsx` | CREATE | Prove approval identity binding, denial and uncertain outcomes. |
| `ui/packages/app/tests/cli-loopback-approval-errors.test.tsx` | CREATE | Prove browser transport, cancellation and malformed-response failures. |
| `ui/packages/app/tests/helpers/cli-loopback-fixture.tsx` | CREATE | Share external identity and network fixtures while keeping internal approval code real. |
| `ui/packages/app/tests/e2e/acceptance/cli-loopback-login.spec.ts` | CREATE | Exercise real browser form headers, sign-in continuity and automatic completion. |
| `docs/v2/pending/M218_001_P2_API_DOCS_SHARED_CRATES_REPLACE_HANDROLLED_RUST.md` | DELETE | Remove the duplicate pending spec; the completed spec remains in done. |
| `docs/v2/pending/M224_001_P0_DOCS_INFRA_FIRST_PRODUCTION_LAUNCH.md` | EDIT | Preserve the existing launch plan and update its release-version guidance. |
| `docs/v2/pending/M232_001_P2_DOCS_INFRA_DAGGER_LOCAL_CI_PILOT.md` | CREATE | Include the approved Dagger evaluation spec in this PR; measurements and Cloud setup remain unimplemented. |
| `ui/packages/app/tests/approvals-pages.test.ts`<br>`ui/packages/app/tests/dashboard-placeholder.test.ts` | EDIT | Assert the full page waits behind the shared loader. |
| `ui/packages/app/tests/models-secrets-page.test.ts` | EDIT | Assert the full page waits behind the shared loader. |
| `ui/packages/app/app/cli-auth/[session_id]/cli-auth-ui.tsx` | EDIT | Remove the unused terminal-name formatter after the copy update. |
| `rustd/crates/afd_inference/Cargo.toml` | EDIT | Remove the unused direct serde dependency. |
| `rustd/Cargo.lock` | EDIT | Regenerate dependency edges with Cargo metadata. |
| `ui/packages/app/components/domain/chat/ChatThread.tsx` | EDIT | Remove the unused after prop. |
| `ui/packages/app/components/domain/FleetReplyFailure.tsx` | EDIT | Reuse parsePayload while preserving the original request text. |
| `public/openapi.json` | REGENERATE | Publish short, plain descriptions from their Rust sources. |
| `public/llms.txt`<br>`public/agentsfleet-manifest.json` | EDIT | Add the approved Billing and private checkpoint operations to the discovery indexes; align stored-usage wording. |
| `rustd/crates/afd_http/src/openapi/mod.rs` | EDIT | Explain shared responses in plain language. |
| `rustd/crates/afd_http/src/openapi/path.rs` | EDIT | Clarify identifiers and path values. |
| `rustd/crates/afd_http/src/openapi/query.rs` | EDIT | Clarify pagination and filters without changing their rules. |
| `rustd/crates/afd_api/src/openapi/mod.rs` | EDIT | Correct authentication guidance and simplify the API title. |
| `rustd/crates/afd_api/src/router/probes.rs`<br>`rustd/crates/afd_api_ingress/src/handler/events/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_ingress/src/handler/webhook/app_route.rs`<br>`rustd/crates/afd_api_ingress/src/handler/webhook/approval_route.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_ingress/src/handler/webhook/qstash_route/mod.rs`<br>`rustd/crates/afd_api_ingress/src/handler/webhook/source_route.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_operator/src/handler/admin/models/mod.rs`<br>`rustd/crates/afd_api_operator/src/handler/admin/platform_keys/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_operator/src/handler/operator/events.rs`<br>`rustd/crates/afd_api_operator/src/handler/operator/leases.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_operator/src/handler/operator/runner_delete.rs`<br>`rustd/crates/afd_api_operator/src/handler/operator/runner_patch.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_operator/src/handler/operator/runners.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/activity.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_runner/src/handler/runner/authoring/activity.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/authoring/model_call.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_runner/src/handler/runner/authoring/renew.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/authoring/report.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_runner/src/handler/runner/credential.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/enrolment.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_runner/src/handler/runner/heartbeat.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/lease/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_runner/src/handler/runner/memory.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/schedule.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_runner/src/handler/runner/schedule_edit.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/schedule_run.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_runner/src/handler/runner/self_record.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/tool_call.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/approval/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/auth/session/dashboard.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/auth/session/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/connector/callback.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/connector/status.rs`<br>`rustd/crates/afd_api_tenant/src/handler/draft/message.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/draft/models.rs`<br>`rustd/crates/afd_api_tenant/src/handler/draft/read.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/draft/stream/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/draft/write.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/event/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/event/tool_call/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/fleet/memory.rs`<br>`rustd/crates/afd_api_tenant/src/handler/fleet/memory_access.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/fleet/message/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/fleet/message_steer/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/fleet/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/fleet/native/create.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/fleet/native/patch.rs`<br>`rustd/crates/afd_api_tenant/src/handler/preference.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/schedule/read.rs`<br>`rustd/crates/afd_api_tenant/src/handler/schedule/write.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/secret/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/tenant/api_key.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/billing.rs`<br>`rustd/crates/afd_api_tenant/src/handler/tenant/cli_credential.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/identity.rs`<br>`rustd/crates/afd_api_tenant/src/handler/tenant/invite.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/invite_email.rs`<br>`rustd/crates/afd_api_tenant/src/handler/tenant/member.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/model_entry/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/tenant/model_entry/write.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/models/mod.rs`<br>`rustd/crates/afd_api_tenant/src/handler/tenant/provider/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_tenant/src/handler/tenant/provider/write.rs`<br>`rustd/crates/afd_api_tenant/src/handler/tenant/workspace/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/admin/mod.rs`<br>`rustd/crates/afd_api_wire/src/admin_catalogue.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/admin_library_native.rs` | EDIT | Correct timestamp wording to match the existing string format. |
| `rustd/crates/afd_api_wire/src/approval.rs`<br>`rustd/crates/afd_api_wire/src/auth.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/authoring/input.rs`<br>`rustd/crates/afd_api_wire/src/authoring/models.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/authoring/output.rs`<br>`rustd/crates/afd_api_wire/src/connector.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/fleet/collection.rs`<br>`rustd/crates/afd_api_wire/src/fleet/input.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/fleet/output.rs` | EDIT | Correct timestamp wording to match the existing string format. |
| `rustd/crates/afd_api_wire/src/health.rs`<br>`rustd/crates/afd_api_wire/src/ingress/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/library/output.rs`<br>`rustd/crates/afd_api_wire/src/models.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/operator.rs`<br>`rustd/crates/afd_api_wire/src/schedule/view.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/secret.rs`<br>`rustd/crates/afd_api_wire/src/tenant.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_wire/src/workspace.rs`<br>`rustd/crates/afd_http/src/openapi/header.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_tenant/src/session/response.rs`<br>`rustd/crates/afd_wire/src/activity/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/authoring/activity.rs`<br>`rustd/crates/afd_wire/src/authoring/lease.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/authoring/meter.rs`<br>`rustd/crates/afd_wire/src/authoring/model.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/authoring/renew.rs`<br>`rustd/crates/afd_wire/src/authoring/report.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/credentials.rs`<br>`rustd/crates/afd_wire/src/event/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/event/steer.rs`<br>`rustd/crates/afd_wire/src/lease/limits.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/lease/payload.rs`<br>`rustd/crates/afd_wire/src/lease/request.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/lease/response.rs`<br>`rustd/crates/afd_wire/src/policy/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/report/mod.rs`<br>`rustd/crates/afd_wire/src/runner/capability.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/runner/mod.rs`<br>`rustd/crates/afd_wire/src/runner/selftest.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/schedule_verb/mod.rs`<br>`rustd/crates/afd_wire/src/tool_detail/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_wire/src/tool_trace/mod.rs`<br>`rustd/crates/afd_wire/src/usage/mod.rs` | EDIT | Shorten generated API prose; preserve behavior and field formats. |
| `rustd/crates/afd_api_runner/src/handler/runner/message.rs`<br>`rustd/crates/afd_api_runner/src/handler/runner/renew.rs` | EDIT | Shorten API guidance while retaining limits and stream recovery. |
| `rustd/crates/afd_api_runner/src/handler/runner/report.rs`<br>`rustd/crates/afd_api_tenant/src/handler/stream/mod.rs` | EDIT | Shorten API guidance while retaining limits and stream recovery. |
| `rustd/crates/afd_api_tenant/src/handler/approval/resolve.rs`<br>`rustd/crates/afd_api_tenant/src/handler/draft/patch.rs` | EDIT | Use plain API titles and support references. |

| `docs/architecture/README.md`<br>`docs/architecture/api_resources.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/billing_and_provider_keys.md`<br>`docs/architecture/capabilities.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/concurrency.md`<br>`docs/architecture/connectors.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/data_flow.md`<br>`docs/architecture/datastore_scaling.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/direction.md`<br>`docs/architecture/fleet_definitions.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/high_level.md`<br>`docs/architecture/lease_flow.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/memory.md`<br>`docs/architecture/observability.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/postgres_schemas.md`<br>`docs/architecture/product_analytics.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/runner_execution.md`<br>`docs/architecture/runner_fleet.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/rust-ownership.md`<br>`docs/architecture/scaling.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/scenarios/README.md`<br>`docs/architecture/scenarios/github-pr-reviewer.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/scenarios/production-deploy-repair.md`<br>`docs/architecture/scenarios/slack-channel-resident.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/scenarios/slack-incident-responder.md`<br>`docs/architecture/scenarios/two-fleets-github-slack.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/shared_chat_verification.md`<br>`docs/architecture/testing.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/user_flow.md`<br>`docs/architecture/web_app.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/architecture/website-job-entrance.md`<br>`docs/AUTH.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |
| `docs/AUTH_DEVICE_LOGIN.md`<br>`docs/RUST_ERROR_STANDARD.md` | REVIEW / EDIT IF NEEDED | Reconcile authoring scenarios and data flows with current source; apply Astra findings. |

## Applicable Rules

- `.orly/dispatch/write_ts_adhere_bun.md` — shared controls, tokens and named constants.
- `.orly/dispatch/write_auth.md`, `docs/AUTH.md` — preserve existing human authorization and credential handling.
- `.orly/docs/greptile-learnings/RULES.md` — remove replaced skeletons; keep state transitions and errors explicit.
- `.orly/docs/DOCUMENTATION_RULES.md` — document current behavior and distinguish pending evidence.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| UI / Design Token | yes | Reuse Spinner and layout spacing tokens. |
| File & Function Length | yes | Small shared loader; remove page-specific skeleton helpers. |
| Auth / Error / Logging | yes | Bind the local callback to the original key, session and trusted browser origin; exclude authentication data from telemetry and preserve manual verification. |
| Spec / UFS / Orphan | yes | Exact file inventory, mapped proofs and unused-import cleanup. |

## Prior-Art / Reference Implementations

- **Reference:** Existing `ShellFrame` keeps header and sidebar mounted during page navigation.
- **Reference:** The design-system Spinner supplies the shared visual mark, accessible status and reduced-motion behavior.
- **Reference:** Existing CLI authorization tests cover encryption, failures and code presentation; retain those assertions while changing the button selector.

## Sections (implementation slices)

### §1 — Shared page loader and authorization wording

Use the same centered accessible indicator in every route boundary and empty shell fallback. Before the shell resolves it fills the viewport; afterward it fills main content. Local refresh indicators remain local. The CLI page uses the same loader; its ready button reads Authorize and its busy button Authorizing….

- **Dimension 1.1 — DONE** — Common indicator at every route boundary → Test `shared_route_loading`
- **Dimension 1.2 — DONE** — First load fills viewport and slow navigation preserves shell → Test `loading_browser_walk`
- **Dimension 1.3 — DONE** — Authorize appears only when ready; current sign-in behavior survives → Test `authorize_wording`
- **Dimension 1.4 — DONE** — Indy reviews the local visual evidence → Test `indy_acceptance`

### §2 — Short, accurate API descriptions

Update canonical Rust descriptions and regenerate the public API document. Preserve every non-prose value of the saved pre-wording input at `74a39c60ef260973894a68f29d0b6e4f6f1c6eda`. Earlier authoring schema changes remain part of PR #784.

- **Dimension 2.1 — DONE** — Compare every non-prose value and retain all 140 operations → Test `openapi_prose_comparison`
- **Dimension 2.2 — DONE** — Astra challenges user and operator guidance, then re-reviews supported corrections → Test `astra_api_accuracy`

### §3 — Complete authoring architecture and scenarios

Read all 31 architecture and scenario pages plus the three named authorization and Rust error guides. Compare their flows and guarantees with source.

Repair supported gaps and keep current behavior separate from unimplemented scenarios. A different Astra reviewer checks each edited group before publication.

- **Dimension 3.1 — DONE** — Complete the document coverage matrix and source-backed corrections → Test `astra_architecture_audit`
- **Dimension 3.2 — IN_PROGRESS** — Validate edited documentation, links and final adversarial review → Test `architecture_documentation_checks`

### §4 — Complete local acceptance

Repair the observed model-status display and synchronize acceptance journeys with completed user actions. Keep exact activation, grant, hover and execution assertions.

- **Dimension 4.1 — IN_PROGRESS** — Successful activation refreshes the visible registry; refusal retains the committed inactive entry and error → Test `created_model_activation`
- **Dimension 4.2 — IN_PROGRESS** — Sidebar loading completes before the zero-download hover measurement; clicking still loads the editor → Test `intent_loading_client_capabilities`
- **Dimension 4.3 — IN_PROGRESS** — Approval continuation reaches the exact fleet's active state before navigating away → Test `multi_fleet_grant_journey`
- **Dimension 4.4 — IN_PROGRESS** — Full local acceptance remains available; deployment selects one execution journey and preflight, with ten fleets available by manual api-dev dispatch → Test `acceptance_scope_discovery`.

The existing `acceptance-e2e` target accepts `ACCEPTANCE_SCOPE=full|post-deploy|ten-fleets`, defaulting to full. Native Playwright tags, skip and project filters select the requested tests. Both selections retain prerequisites, original assertions and zero retries. Ten pages remain concurrent. Separate fixture identities isolate setup and cleanup. Indy manually dispatches ten fleets against api-dev after merge and deployment verification. The deployment verdict records this check as deferred. No remote deployment is authorized by this configuration change.

> Indy (2026-10-25 22:20): "I think defer the ten-fleet on indy's signoff, indy will deal with it upon merge manual." — context: remove automatic ten-fleet deployment execution and its release dependency; retain the manual workflow and unchanged test.
> Indy (2026-10-25 22:20): "and move the spec to done." — context: archive this spec under `done/`; retain incomplete evidence and continue repairing PR #784's failing checks. Archival does not claim a manual login test, complete model-count accuracy, exhausted reviews or documentation checks have passed.

### §5 — Bound local terminal authorization

Default browser login binds only literal `127.0.0.1` on an operating-system-assigned port. The configured dashboard origin and exact session route are validated before appending `?flow=loopback` and a fragment containing version, session identifier, original public key, callback address and 256-bit random state. Both displays use the first eight uppercase hexadecimal digits of SHA-256 over UTF-8 `agentsfleet.cli.login.loopback.v1\n{session_id}\n{cli_public_key}`, grouped four-four. The full decoded key must match before authorization; the short code is a human aid.

Capture recognized fragments synchronously before analytics or Clerk navigation, strip them even on refusal, and retain only validated handoff metadata in bounded per-tab session storage across sign-in. Bind sign-in capture to its same-origin marked return route; revalidate on restore, preserve original expiry, cap each record at 2 KiB and two records total. Never store a token, verification code or private key there. Authentication routes and navigation targets are excluded from analytics at initialization and capture. Missing, conflicting, malformed or unavailable storage fails visibly without manual downgrade.

Authorize and Deny share a synchronous once-only guard. Show signed-in identity; require stable subject and browser session while acquiring both tokens. Approval retains existing encryption and single-write approval, then top-level form-POSTs the six-digit code to the local callback. Route-specific `Referrer-Policy: origin` preserves a non-null origin header on HTTPS-to-loopback POST. Deny sends only the bound denial; the unapproved API session expires normally. Uncertain approval requires a fresh session; no retry mint or fallback releases its code.

The listener checks exact Host, non-null trusted Origin, method, path, content type, unique known fields, matching session and constant-time state before consuming its waiting slot. Bound headers, streamed body at 1 KiB and request lifetime; invalid or slow requests cannot win. Approval, denial, expiry and cancellation have one atomic winner. Esc and interrupt cancel only WAITING; approval moves to visible FINALIZING before verification, mint, persistence and identity validation. Keep the interrupt handler until finalization ends. Existing mint replacement and rollback limitations remain documented. Return static no-store secret-free HTML only after the actual outcome; a lost browser response never repeats exchange. Close listeners, timers and input modes on every exit. The maximum wait is the existing five-minute session window.

Explicit `--no-open` or configured no-open retains the current six-digit manual flow for remote terminals. Browser-open failure offers same-computer URL or explicit restart with `--no-open`; callback failure never silently changes mode. No new backend endpoint, approval capability, token format or polling release is introduced. Reference: RFC 8252 §§7.3,8.3,8.9; Astra's source-backed design review requires actual browser proof for redirect and header behavior.

- **Dimension 5.1 — IN_PROGRESS** — Local callback rejects forged and replayed handoffs and cleans every resource → Test `loopback_listener_security`
- **Dimension 5.2 — IN_PROGRESS** — Browser code, key, account and session are bound before one approval or denial → Test `loopback_browser_binding`
- **Dimension 5.3 — IN_PROGRESS** — Real sign-in and form navigation complete locally without secret telemetry; remote manual flow survives → Test `loopback_login_journey`

### §6 — Shared compaction and checkpoint continuation

**Status:** IN_PROGRESS. Use the approved shared `afd_context` engine for private setup and fleet execution, with traits, trait objects and correctly scoped `Fn`, `FnMut` and `FnOnce` callbacks. The [architecture scope](../../architecture/context_lifecycle.md#implementation-scope) extends Files Changed for this approved follow-up. Preserve original history, private ownership, immutable revisions, native replay integrity, guarded publication and metered physical calls. Remove the independent 32,768-token and 64-history-row refusals; compact near 90 percent of the selected physical window and continue.

**Owner instructions:** “lets finish the compaction first” and “and you run the same test after compaction to prove we can test beyond the 47th or whatever 41st turn you stopped.” Run the unchanged full local acceptance lane after integration, retaining its 181-message/four-save assertions and the historical turn-42 failure evidence. Forced repeated-compaction tests for both hosts supplement that rerun.

**Local transport approval, Oct 25:** Indy approved applying `/private/tmp/pr784-local-http2-proposal.patch`, limited to the two local deployment files above, with transport verification before paid model work. The applied patch's isolated transport smoke returned HTTP/2 and ten HTTP 200 responses on one connection, with no paid calls (`/private/tmp/pr784-local-http2-proof-20261025/result.json`). Full authenticated continuity remains open.

**Quality ceiling:** One shared lifecycle and checkpoint repository prevent divergent recovery and duplicate algorithms.
**Surface area:** OpenAPI yes (runner checkpoint/protocol); CLI no new command; user guidance yes; release behavior yes; additive schema yes; no harness/hook changes, review-counter reset or merge.

Preparation must grow linearly with history and keep independent conversations concurrent. The [context lifecycle](../../architecture/context_lifecycle.md#preparation-and-future-sandbox-boundary) records the approved implementation and failure requirements.

**Subsequent sandbox agent:** After PR #784 merges, use [sandbox-handoff-prompt.md](../../reviews/space-sandbox/sandbox-handoff-prompt.md) for the SPACE-derived Firecracker migration. It carries the concept, detailed design/spec review requirements and separate implementation sequence.

**Subsequent workflow agent:** Use [orly-egm-jev-handoff-prompt.md](../../reviews/space-sandbox/orly-egm-jev-handoff-prompt.md) in the Orly source repository. It covers the Elephant-Goldfish Model (EGM) workflow and TypeSafe Jev spec-rubric grading.

- **Dimension 6.1 — DONE** — Storage-independent shared lifecycle and unchanged private checkpoint semantics → Test `checkpoint_retry_is_idempotent_and_superseded_assignment_cannot_read_or_replace_notes`.
- **Dimension 6.2 — DONE** — Linear preparation work and bounded reads without a lifetime conversation cap → Test `cumulative_provider_encoding_grows_linearly_as_history_grows_tenfold`.
- **Dimension 6.3 — DONE** — Independent conversations progress while another summary is blocked; cancellation stays local → Test `a_blocked_summary_does_not_block_another_conversations_checkpoint`.
- **Dimension 6.4 — DONE** — One prepared authoring body is funded and revalidated before physical dispatch → Test `changed_history_after_funding_cannot_dispatch_the_previously_measured_body`.
- **Dimension 6.5 — IN_PROGRESS** — Repeated compaction preserves goals, original history, tool groups and deletion ownership → Test `repeated_authoring_compaction_restores_notes_and_continues_with_all_181_messages_intact`.

**Failure proofs:** Reject a wrong prefix digest before provider access. Rebuild the runtime index when pending child input changes. Reject source changes after funding. Keep the previous checkpoint after cancellation, stale generation, unknown usage or failed publication. Retain known charges after a model reply. A blocked conversation cannot hold another conversation's progress.

**Signals:** Content-free operational traces pair summary start with committed completion or failure. Authenticated progress notices announce summarization and continuation. No product analytics event is added. Test counters record page reads, encoded groups and concurrent progress without conversation text. Wall-time and throughput remain unverified until measured.

**Compaction punch list:**

- Document how the selected model generates the bounded JSON summary, `tiktoken-rs` measurement and the conservative byte fallback. Source-checked in `context_lifecycle.md`.
- Document fleet-private and workspace-shared memory loading, tool access and semantic capture separately from conversation checkpoints. Source-checked in `memory.md`; no account-wide scope is implemented.
- Verified locally: both hosts publish content-free summary status through authenticated streams. Trace pairing, runtime ordering, private setup delivery and React recovery tests pass; the final browser continuity journeys remain open. See the verification record below.
- Open: review sensitive conversation and tokenizer buffer lifetimes, including ordinary strings, vectors, serialization and clones. Credential wiping exists; complete summary-buffer wiping is unproven. Recheck cross-tenant, private-draft and superseded-lease refusals through real storage before security signoff.
- Keep cross-lease Git worktree recovery and the Go/Firecracker changes in the subsequent sandbox session requested by the owner.

**Model counting follow-up, approved Oct 25:** Evaluate the experimental `tiktoken` crate alongside the installed `tiktoken-rs`, Hugging Face `tokenizers` and Kitoken. Keep the shared planner and its 90/95 percent policies. Measure the complete selected provider request before each physical call. Treat byte fallback and serialized-JSON counts as estimates, never proven request bounds.

The evaluation must compare token identifiers against pinned reference vocabularies before comparing speed. Cover every configured model in `scripts/model-library-allowlist.json`, plus unknown and configured Ollama models. A family alias alone cannot prove compatibility. Pin vocabulary revisions, templates, checksums and licenses; model weights and remote model code are unnecessary.

Keep local estimation synchronous. Any native count request must use the selected authorized provider route outside database transactions, then recheck the prepared request's freshness. Count failures must preserve cancellation, usage uncertainty and checkpoint authority. Never send a routed customer's input to another provider for counting.

- **Dimension 6.6 — IN_PROGRESS** — Compare tokenizer correctness, retained private input, supported models and cold/warm performance → Test `tokenizer_candidate_evaluation`.
- **Dimension 6.7 — IN_PROGRESS** — Exercise model changes, complete request shapes and finite output admission for supported routes → Test `supported_models_reserve_complete_requests_before_output`.
- **Dimension 6.8 — IN_PROGRESS** — Replace unverified model approximations with proved local counts or scoped native confirmation; record estimate versus actual usage without prompt text → Test `native_count_requires_the_same_prepared_request_and_authorized_route`.

The counting follow-up extends Files Changed to `afd_inference` counting modules and tests, host estimate adapters, pinned tokenizer configuration, their manifests/lockfile and the evaluation report. Both production hosts call near-capacity confirmation for documented native routes. Shared and runtime count/freshness/failure tests and real-storage authoring integration pass. Live-provider agreement and the complete model/request matrix remain open. No candidate is selected merely from its published performance claims.

The existing runner verification follow-up includes `afr_supervisor/src/test_util.rs` and `test_util_tests.rs`. Its fake daemon must route checkpoint reads and writes to the checkpoint response shape. Test `context_routes_answer_through_the_native_client` must reproduce the strict-decoder failure before the routing repair. Keep all kernel assertions and trial counts unchanged; a local routing proof does not replace the Linux kernel run.

> Indy (2026-10-25 21:36): "1. Approve diagnostice chage - Signed off approved by indy" — context: extend Files Changed to `rustd/crates/afr_sandbox/examples/kernel_lane/admission.rs` solely to report the source chain with `error.told().into()`; retain assertions, trial counts and timeouts.
> Indy (2026-10-25 21:36): "2. Yes approve progres publication with these limits - Signed off approved by indy - we  can alway improve thi later." — context: permit progress publication to existing PR784 with the three-cycle implementation review incomplete, two-attempt documentation audit blocked, partial coverage and model-count estimates/live-accuracy gaps recorded. This does not waive failing tests, secret gates, merge review or deployment approval.

**Counting failure proofs:** Unknown models and unsupported media retain an explicit approximate status. Switching model, provider, dialect, template or tokenizer revision invalidates old counts. Include prose, source, JSON, logs, Unicode, literal special tokens, tool schemas, tool results, image/file references and reasoning reservations. Refuse irreducible input. A provider context rejection permits at most one fresh recount/replan; unknown charged outcomes never authorize another request. Keep count errors, cancellation and rate limiting from becoming endless summary loops.

### §7 — Settled Billing reporting and local review

Implement the approved sidebar-aware page and the [Billing reporting design](../../architecture/billing_reporting.md), including its domain, handler, query, wire, component and test scope. Reuse the design system. Keep totals over the complete selected period, group billed work and retain paginated individual details. Existing balances and raw charges remain compatible; reporting never moves money.

- **Dimension 7.1 — DONE** — Scoped settled aggregates, grouped paging and bounded detail reads → Test `settled_report_totals_cover_more_than_one_activity_page`
- **Dimension 7.2 — DONE** — Real-query plans, concurrent reads and clean failure/resource paths → Test `report_queries_bound_scans_to_tenant_and_period`
- **Dimension 7.3 — DONE** — Approved filters, exact amounts and disclosures render on the real local route → Test `renders full-period totals independently of the single displayed activity`
- **Dimension 7.4 — DONE** — Correct the approved shared-component presentation and inspect populated Billing through the real runner → Test `settings-billing.spec.ts` and a scoped real setup/fleet charge walkthrough.

**Owner visual approval, Oct 25:** “Okay approved both the spending and activity (empty) + prior approval holds”. Both separate empty previews are approved. Their sidebar now fills the viewport, with the account menu anchored at the bottom. Keep the original approved populated mock unchanged. Additional Billing rows and spending bars belong beneath the tabs; an empty result retains its explanatory message.

**Owner correction, Oct 25:** “just adopt the same font and boldness”; “especially this area is messed up”; “You need to align with the mock billing you created”; “Also show me with 3 fleets”. Match the original system font, compact table text, medium-weight names and totals, darker detail surface and aligned label/value disclosure. Fleet names remain recorded data; Inference and Runtime remain fixed labels. Extend Files Changed to the shared theme font utility needed by that approved typography. Retain exact settled amounts and continuation boundaries; never invent separate prices for a combined historical charge. Populate the review account with three owned fleet fixtures through ordinary execution, without editing ledger rows.

**Final Billing visual sign-off, Oct 25:** “I sign off / approve the Billing visual then.” This completes the manual visual evidence for Dimension 7.4 alongside the populated browser and real-runner charge evidence below. The later missing-balance and workspace question is being investigated separately; its account/session mapping remains unconfirmed.

**Owner publishing approval, Oct 25:** “you have approval for publishing”. Publish the completed changes to the existing product and documentation pull requests after their required checks. This approval does not authorize merging or deployment.

## Interfaces

- `RouteLoading({ fullScreen?: boolean })`: content-sized by default, viewport-sized when requested.
- Status accessible name: Loading page. Visible label: Loading….
- Ready heading: Connect your terminal. Show identity and matching code before local authorization; offer Authorize and Deny. Explicit remote manual mode retains code entry. Existing API endpoint names and request bodies remain unchanged.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|---|---|---|
| Slow initial load | Shell data unresolved | Viewport-sized shared loader. |
| Slow navigation | Next page unresolved | Shared loader fills content; header and sidebar persist. |
| Local refresh | Existing table or button request | Existing local indicator remains in place. |
| Expired or refused login | Existing authentication failure | Existing error state and disabled action remain; no false success. |
| Reduced motion | User accessibility preference | Existing Spinner styles reduce animation. |
| Model activation refused after registration | Registration commits before the provider change | Retain the saved inactive entry and open dialog with its activation error. |

## Invariants

1. A page loader is a status indicator and cannot authorize a session or bypass page access checks.
2. Loaded dashboard navigation remains mounted around pending page content.
3. Every route boundary uses the same component; no route-specific skeleton layout remains.
4. Login encryption, server code verification, credential persistence and identity validation retain their existing behavior; automatic delivery additionally requires the same-computer callback binding in §5.

## Metrics & Observability

No new analytics events or identifiers. Use browser screenshots and DOM geometry for loading evidence; package tests prove indicator and action states. Record repository checks separately.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|---|---|---|---|
| 1.1 | unit | `shared_route_loading` | All route boundaries render one status with Loading page and Loading…; no pulse skeleton or page title. |
| 1.2 | e2e | `loading_browser_walk` | Slow navigation leaves header and sidebar visible with a centered content indicator; initial loading fills viewport. |
| 1.3 | unit | `authorize_wording` | Loading hides the action; pending session enables Authorize; approval, encryption and failure assertions still hold. |
| 1.4 | manual | `indy_acceptance` | Indy reviews the local loader and renamed ready button; record his verdict separately. |
| 2.1 | generated document | `openapi_prose_comparison` | Every non-prose value matches the saved pre-wording input at `74a39c60`; all 140 operation identifiers survive. |
| 2.2 | manual review | `astra_api_accuracy` | Correctness findings are repaired and the focused Astra re-review is clean. |
| 3.1 | manual review | `astra_architecture_audit` | All 34 documents are read; source evidence supports corrections and each scenario's implementation status. |
| 3.2 | documentation | `architecture_documentation_checks` | Declared documentation checks pass and independent Astra reviews find no unresolved supported defect. |
| 4.1 | unit / e2e | `created_model_activation` | The real table shows the newly active model after success; registration and activation refusals preserve the correct visible state. |
| 4.2 | e2e | `intent_loading_client_capabilities` | After the sidebar's initial read, Save-Data hover downloads no scripts; opening the editor loads its scripts and dialog. |
| 4.3 | e2e | `multi_fleet_grant_journey` | Each retained fleet reaches active after grant approval before another fleet is opened; exact ownership and execution assertions remain. |
| 4.4 | discovery / workflow | `acceptance_scope_discovery` | The existing make target's flag selects preflight and one execution test or only the concurrent ten-fleet test. Invalid flags refuse before browser startup. Full local discovery retains every project; deployment requires the short check and records Indy's manual ten-fleet deferral. |
| 5.1 | unit / integration | `loopback_listener_security` | Real listener refuses malformed, forged, oversized, slow, expired, cross-session and replay requests; cancellation races have one winner and no leaks. |
| 5.2 | unit | `loopback_browser_binding` | Full key, stable account/session and comparison code precede one decision; denial and uncertain approval never release a credential; storage and analytics refuse sensitive leakage. |
| 5.3 | e2e | `loopback_login_journey` | Signed-in and signed-out login preserve marked handoff safely, real callback headers pass, terminal identity matches, and manual other-device verification remains supported. |
| 6.1 | integration | `checkpoint_retry_is_idempotent_and_superseded_assignment_cannot_read_or_replace_notes` | Retries preserve one generation and stale authority cannot publish private notes; `one_of_one_hundred_conflicting_checkpoint_writers_publishes_exactly_one_generation` proves contention. |
| 6.2 | unit | `cumulative_provider_encoding_grows_linearly_as_history_grows_tenfold` | Estimator group visits stay below three times source groups; `runtime_history_index_invalidates_changed_pending_input` proves tail refresh and the runtime encoding ladder bounds actual serializer input. |
| 6.3 | unit | `a_blocked_summary_does_not_block_another_conversations_checkpoint` | A second conversation publishes while the first model response remains deliberately blocked. |
| 6.4 | integration | `changed_history_after_funding_cannot_dispatch_the_previously_measured_body` | Funding releases datastore locks; changed source or authority rejects dispatch before provider access. `each_platform_send_funds_measured_input_and_keeps_prior_known_cost` proves reservation growth. |
| 6.5 | unit / integration / e2e | `repeated_authoring_compaction_restores_notes_and_continues_with_all_181_messages_intact` | Runtime proof `repeated_compaction_continues_past_turn_forty_two_and_keeps_tools_and_goal` and unchanged `fleet-continuity-final.spec.ts` retain 181 messages and four saves, plus the separate ten-fleet journey. |
| 6.6 | isolated evaluation / source inspection | `tokenizer_candidate_evaluation` | Run the saved experiment described in `docs/reviews/pr784-tokenizer-evaluation.md`. Compare token identifiers, model mappings, private-input caches and cold/warm timings; incomplete model or timing coverage stays open. |
| 6.7 | unit | `supported_models_reserve_complete_requests_before_output` | Every catalogue route and configured unknown/Ollama model reserves complete input before finite output. Tool schemas, results, inline media and selected reasoning replay reach native count requests unchanged. |
| 6.8 | unit / integration | `native_count_requires_the_same_prepared_request_and_authorized_route` | Freshness binds body, model, route and window; count failures never become zero or generation retries. Both hosts compare measured input with authoritative usage without private text; authoring rechecks source and authority after releasing its locks. |
| 7.1 | integration | `settled_report_totals_cover_more_than_one_activity_page` | Full-period sums and breakdowns include every filtered settled row; pending, foreign and private text stay out. Tied boundaries and mismatched continuations are proved. |
| 7.2 | integration | `report_queries_bound_scans_to_tenant_and_period` | Production EXPLAIN plans retain tenant/time and work/sequence bounds across a history ladder; concurrent reads overlap and return connections; datastore failure changes no money. |
| 7.3 | unit / e2e | `renders full-period totals independently of the single displayed activity` | The Billing component suite proves filter resets, exact tiny charges, customer-key wording, empty and failure states. `settings-billing.spec.ts` proves real reads, themes and sidebar widths; keyboard disclosure opens with Enter and closes with Space in the component proof. |
| 7.4 | e2e / owner review | `settings-billing.spec.ts` | The populated account renders three real fleets and setup chat with exact settled values, keyboard disclosures and readable totals across themes, sidebar states and narrow screens. Compare spacing with the approved original mock and preserve the owner's recorded visual feedback. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Loader and wording are reviewable | Local browser evidence walk | Screenshots and preview; Indy verdict recorded separately | P0 | |
| S1 | Conform | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint | `make lint-all` | exit 0 | P0 | |
| S3 | Unit | `make test-unit-all` | exit 0 | P0 | |
| S4 | Integration | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version | `make check-version` | exit 0 | P0 | |
| S6 | Secrets | `gitleaks git --staged --redact` | no leaks | P0 | |

## Dead Code Sweep

Remove page-specific skeleton bodies and their unused imports. Retain shared CLI verification helpers and explicit manual code entry; automatic mode reuses the encrypted exchange. Search route boundaries and page Suspense fallbacks for remaining page-specific skeletons.

## Out of Scope

- New backend approval capabilities, completion endpoints, token formats and automatic remote-device polling.
- Remote deployment and merging PR #784. Indy subsequently requested the full local acceptance test; it remains required.
- Component-sized loading for buttons or background refresh.

## Product Clarity (authoring record)

Consistent loading preserves recognizable navigation, human approval and terminal identity. Reuse shell, spacing tokens, Spinner and accessibility; avoid new loading dependencies or forced delays.

Show the loader across the viewport before the shell and within content afterward. Persistent navigation and existing error recovery remain available. Remote manual verification stays explicit; loading never implies completion or approval.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Small shared loader and wording update in the approved PR.
- **Alternatives considered:** Route-specific skeletons preserve inconsistency. Server polling alone removes the manual human binding; §5 instead binds automatic delivery to a same-computer listener and original public key.
- **Patch-vs-refactor verdict:** Reuse existing components and remove duplicated markup.

## Discovery (consult log)

- **Visual acceptance** — Indy: “Okay we are fine on both loader and the cli - signed off approved”. He then requested no handrolled or duplicated code, including Rust, and checks for performance, concurrency and races.
- **Consults** — Indy approved app-wide loading, then retained the current CLI flow and requested Authorize as the button label. He requested evidence he can inspect.
- **Metrics review** — No event changes required.
- **Skill-chain outcomes** — Visual acceptance recorded above. Focused loader and authorization checks: 110 passed across nine files. Shared-parser retry checks: 12 passed. Repository checks remain in progress.
- **Scope separation** — The experimental CLI branch is local only. Its real-router attack proof reproduced unauthorized approval, HTTP 200 where refusal was expected. No experimental protocol code ships here.
- **Acceptance routing** — Indy requested a small deployment check, full `acceptance-local`, and ten parallel fleets on api-dev. Latest direction: “I prefer to keep to accept this as a pass, the 10 fleet is move out to test upon deploy. So move on to next”. This accepts the local result described below; it does not change the stored test verdict or disposition either exhausted review.
- **OpenAPI wording** — Indy requested a large review of `public/openapi.json`, with short descriptions and no cryptic terminology. Edit canonical Rust prose and regenerate the document. Preserve request and response shapes, permissions, limits, retry behavior, and deletion warnings; compare every non-prose value after generation.
- **Astra review** — Indy requested an engineer and site reliability engineer review after the docs and OpenAPI changes. Its source checks identified six accuracy gaps. Correct the network-access scope, schedule registration outcomes, 24-hour retry window, required-value lookup instructions, timestamp wording, and published runner maintenance guidance. Runtime behavior remains outside this wording change.

- **Architecture completeness** — Indy requested every `docs/architecture/*.md` page, its scenarios, `docs/AUTH.md`, `docs/AUTH_DEVICE_LOGIN.md`, and `docs/RUST_ERROR_STANDARD.md` be reviewed against recent authoring changes and pushed in PR #784. He then explicitly requested an adversarial review with Astra and corrections. Audit the complete set, repair supported gaps, and re-review the final edits.
- **Dagger evaluation plan** — Indy directed: “the dagger spec must go in this PR”. Include pending M232 in PR #784; its local, cache-invalidation and Cloud measurements remain unimplemented.
- **Later login scope** — The saved owner instruction includes Muse-style login improvements in this same PR. Fresh Astra design review supports the §5 loopback design subject to its explicit browser, race and privacy proofs. This supersedes the earlier manual-only implementation scope, without adopting the unsafe experimental branch.

## Verification record

[Historical verification](../../reviews/pr784-verification-history.json) preserves the original results, failed acceptance runs and terminal implementation-review disposition verbatim. Final compaction, Billing and repository checks remain required on the settled candidate.

Final `make test-integration-rustd` passed 1,394 regular and three exclusive tests, exit 0 (`/private/tmp/pr784-resumed-test-integration-final.log`). It includes native-count lock release, stale source/Stop/credential refusal, clean count failure and private progress-stream proofs. Shared inference passed all 71 focused cases. Dimension 6.5 still requires both unchanged browser continuity journeys. Publication approval is recorded in §7; acceptance and exhausted review gates remain open.

Final `make test-unit-all` passed 11,666 tests, zero failures, 16 skipped and 1,402 ignored (`/private/tmp/pr784-resumed-test-unit-all-4.log`). Selections: Rust 4,756, local launcher 25, app 4,283, website 133, CLI 1,950 and design system 519; all four package coverage gates passed at 100 percent. The comparable unit baseline was 11,270, a net increase of 396. The incomplete integration baseline does not support a comparable total delta.

Billing browser recheck passed all three selected checks with populated settled activity, three real fleets and setup chat (`/private/tmp/pr784-billing-spacing-browser-results.json`). The eight themed layout observations had zero accessibility violations or horizontal overflow; keyboard disclosure and filter scope passed (`/private/tmp/pr784-billing-spacing-details.log`). The approved original mock remains unchanged. Final owner visual approval is recorded in §7; full continuity acceptance remains open.

Final documentation attempt two of two is blocked (`/private/tmp/pr784-product-docs-final-report.json`): 109 paths were actually read or inspected, with ten partial document reads explicitly disclosed. Supported prose, error-census and discovery-index findings were repaired afterward; this does not create a fresh completed audit. Two older diagram blocks remain unchanged and are explicitly labeled historical. Broader source coverage, diagram replacement and the blocked audit disposition remain open; Dimension 3.2 is therefore IN_PROGRESS.

Docs PR #228 merged at `d12ed66` after the login exit-code correction passed `make lint`, 25 documentation tests and normal publication hooks. Its Greptile thread is resolved. The docs checkout is clean on `main`; four pending Billing/context edits are preserved in stash `bbcb780238b852fc62d6450e7aff4736b68ea4d3`. Those operations still depend on the product's new OpenAPI and are not published by that merge.

The fresh unchanged single-fleet browser run verified 160 exchanges: 70 setup turns and 90 runtime replies, through three saved setups. Its original 30-minute overall deadline expired while runtime reply 91 remained received; the ten-fleet test was skipped (`/private/tmp/pr784-continuity-current/results.json`). Indy accepted this local result and moved ten-fleet verification to deployment. The raw timeout remains recorded; a complete 181-message result and deployed ten-fleet result have not been observed.

Native test discovery selects nine prerequisite checks plus one real execution test for post-deploy, or nine prerequisites plus only the ten-fleet test. Full discovery retains 121 tests across 55 files (`/private/tmp/pr784-acceptance-{post-deploy,ten-fleets,full}-list.log`). An invalid scope refuses before browser startup. `make test-unit-app` passed 4,283 tests; `make lint-app` and `make check-gh-actions-valid` passed (`/private/tmp/pr784-acceptance-native-{unit,lint,workflows}.log`). This proves selection and configuration; deployed execution remains unobserved.

Fresh `make acceptance-build` passed for the current daemon, Linux runner, command-line client and dashboard (`/private/tmp/pr784-current-acceptance-build-result.json`). The earlier kernel run passed 60 tests and failed `test_toolbox_admission_survives_path_swap` with an input/output error (`/private/tmp/pr784-current-kernel-isolated.log`). The approved source-chain diagnostic preceded a fresh canonical kernel pass: 61 tests, zero failures, plus 31 startup checks (`/private/tmp/pr784-approved-kernel-diagnostic-canonical.log`). Hosted kernel verification also passed all 61 tests for commit `07672e528`; the original intermittent cause remains unproved.

Hosted aggregate coverage at `07672e528` failed despite all four Rust shard suites passing: lines 99.4732 percent against 99.5; changed lines 96.6667 against 99; 36 touched files below 99. The [coverage repair record](../../reviews/pr784-coverage-repair.md) maps added failure/privacy cases and current verification without changing thresholds. The exhausted reviews remain recorded above.
