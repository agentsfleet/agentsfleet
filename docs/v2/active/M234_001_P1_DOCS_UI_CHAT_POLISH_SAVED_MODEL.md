# M234_001: Fleet chat polish and saved runtime model

**Prototype:** v2.0.0
**Milestone:** M234
**Workstream:** 001
**Date:** Oct 26, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — fleet chat consistency
**Categories:** DOCS, UI
**Batch:** B1 — continue the restored chat worktree
**Branch:** fix/chat-smooth-replies
**Baseline revision:** 89415c2a0e622a920b4d66517decbf85640058b2
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — retain comparison revision, commands, counts and environment
**Depends on:** none
**Provenance:** LLM-drafted (Codex, Oct 26, 2026)
**Canonical architecture:** `docs/architecture/fleet_definitions.md` — native runtime model edits

## Overview

**Goal (testable):** Selecting a model in fleet chat saves that fleet's runtime model, preserves unsent text, and displays the confirmed choice after reload.
**Problem:** The composer displays a model without offering a selection. Reply completion and composer styles also need the restored polish verified.
**Solution summary:** Reuse private draft edits and reviewed fleet revisions for model selection. Record confirmed model changes in chat and Activity. Complete the existing reply, run-detail, header and shared-composer changes in this worktree.

## PR Intent & comprehension handshake

- **PR title (eventual):** Polish fleet chat and save runtime model selections
- **Intent (one sentence):** Let people trust the model displayed in their fleet's composer and keep conversation controls consistent.
- **Handshake:** The selected model belongs to the fleet and applies to future chat, webhook and scheduled work. ASSUMPTIONS I'M MAKING: already running work retains its admitted setup; this change adds no per-message override and no model inference while saving settings.

## Implementing agent — read these first

1. `ui/packages/app/lib/api/drafts.ts` — reviewed edits, conditional apply and retained retry identity.
2. `ui/packages/app/components/domain/chat/FleetSetupWorkspace.tsx` — persistent transcript, identity boundary and private editor.
3. `ui/packages/app/components/domain/chat/ChatToolbar.tsx` — shared model-selection controls and eligibility.
4. `docs/architecture/fleet_definitions.md` — runtime and authoring model ownership.
5. `docs/DESIGN_SYSTEM.md` — composer typography, reply completion and compact run details.

## Files Changed (blast radius)

The restored staged changes remain part of this delivery. Source families below include their existing changed tests; new siblings are named before creation.

| File | Action | Why |
|---|---|---|
| `docs/v2/active/M234_001_P1_DOCS_UI_CHAT_POLISH_SAVED_MODEL.md` | CREATE | Behavior and verification ledger; move to done after completion |
| `.github/actions/setup-bun-playwright/action.yml`, `.github/workflows/`, `package.json`, `bun.lock`, `cli/package.json`, `cli/bun.lock`, `compose.dev-infra.yml`, `docker-compose.yml` | EDIT | Preserve restored Bun 1.4.4 alignment; no new workflow changes |
| `.orly/LOCAL.md`, `README.md` | EDIT | Preserve restored fixture naming and contributor copy |
| `docs/DESIGN_SYSTEM.md`, `docs/reviews/design-system-css-audit.md`, `docs/architecture/fleet_definitions.md` | EDIT/CREATE | Shared controls, saved model semantics and CSS audit |
| `scripts/model-catalogue/quotes.integration.test.ts`, `scripts/model-catalogue/seed.integration.test.ts` | EDIT | Preserve restored runtime compatibility fixes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx`, `components/FleetHeader.tsx` | EDIT | Pass persisted model context and show readable header copy |
| `ui/packages/app/components/domain/FleetMarkdown.tsx`, `FleetReplyBody.tsx`, `FleetReplyFigures.tsx`, `fleetMessageRenderers.tsx`, `FleetThread.tsx`, `FleetThreadViewport.tsx`, `SteerComposer.tsx` and their existing tests | EDIT | Persistent replies, compact figures and composer model control |
| `ui/packages/app/components/domain/fleetMarkdownBlocks.ts`, `fleetMarkdownBlocks.test.ts`, `ui/packages/app/tests/bench/fleet-markdown-stream.bench.tsx`, `tests/helpers/fleet-markdown-corpus.ts` | DELETE | Remove the replaced markdown splitter and its fixtures |
| `ui/packages/app/components/domain/chat/ChatComposer.tsx`, `ChatComposerFrame.tsx`, `ChatToolbar.tsx`, `DraftMessage.tsx`, `DraftSaveBar.tsx`, `FleetSetup.tsx`, `FleetSetupWorkspace.tsx`, `chat-defaults.ts` | EDIT | Shared controls and isolated runtime selection context |
| `ui/packages/app/components/domain/chat/FleetModelSelect.tsx`, `FleetModelSelect.test.tsx`, `useFleetModel.ts`, `useFleetModel.test.tsx` | CREATE | Saved selection, paging, failure and replay behavior |
| `ui/packages/app/lib/api/drafts.ts`, `ui/packages/app/tests/draft-model-selection.test.ts`, `ui/packages/app/tests/draft-runtime-model.test.ts` | EDIT/CREATE | Honor runtime-only edits and prove revision writes |
| `ui/packages/app/lib/events/run-figures-format.ts` and its test | EDIT | Millisecond duration formatting |
| `ui/packages/app/tests/e2e/acceptance/`, `tests/fleet-thread/`, `tests/fleets-routes/detail-header.test.ts`, `tests/workspace-chat/`, `tests/fleet-markdown.test.tsx`, `tests/fleet-setup-recovery.test.tsx`, `vitest.setup.ts`, `package.json` | EDIT/CREATE | Existing chat regression proofs and saved-model browser check |
| `ui/packages/design-system/src/`, `ui/packages/design-system/package.json` | EDIT/CREATE | Preserve shared composer, popover and CSS cleanup |
| `cli/test/` | EDIT/CREATE | Meaningful coverage of branches exposed by Bun 1.4.4; exact files recorded before editing |

## Applicable Rules

- `.orly/docs/greptile-learnings/RULES.md`: ECL distinguishes refusal from lost acknowledgment; TVR and TCF require reachable, falsifiable tests; UFS names user copy; ORP sweeps replaced markdown imports; WAUTH scopes private reads; IDMP preserves intentional replay; ASE catches event-handler rejections.
- `.orly/dispatch/write_ts_adhere_bun.md`: validate external data, abort owned requests and use shared controls.
- `.orly/dispatch/write_any.md`: scope, file length and named constants.
- `.orly/dispatch/write_spec.md`: explicit behavior proofs and source-verified assumptions.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|---|---|---|
| UI and design tokens | Yes | Use shared Select, Button, Alert and composer primitives |
| Universal string constants | Yes | Name new copy and shared wire vocabulary |
| File and function length | Yes | Keep source files within 350 lines and functions within 50 lines |
| Spec and documentation | Yes | Keep the proof ledger complete and public prose concise |
| Authentication | Existing boundary | Use the existing same-origin private setup endpoints; no new endpoint or permission |

## Prior-Art / Reference Implementations

`ChatToolbar.tsx` supplies model selection styling. `DraftClient.edit` and `apply` supply native edit and reviewed revision writes. `DraftOperation` owns an unconfirmed mutation's retry identity. The setup workspace supplies identity lifetime and route refresh without replacing the composer.

## Sections

### 1. Save the runtime model

- Dimension 1.1 **IN_PROGRESS — Runtime selection:** Load runtime-eligible workspace models with bounded paging. Clone the current fleet setup into a private draft, edit only the runtime model and apply its reviewed revision. → Test `saves a runtime-only model change without generating a message`.
- Dimension 1.2 **IN_PROGRESS — Existing chat polish:** Preserve reply text nodes on completion, shared composer fonts, compact run details, precise durations and removal of the header access warning. → Test `fleet-composer`.

### 2. Preserve state and recover safely

- Dimension 2.1 **IN_PROGRESS — Refusal and stale setup:** Keep the saved model visible after refusal, missing models or a stale setup. Never apply unrelated editor changes. → Test `retains the saved model when the setup changes before apply`.
- Dimension 2.2 **IN_PROGRESS — Unconfirmed save:** Disable further model changes while a write is unresolved. Retry the retained mutation before continuing; show the new model only after confirmed apply. → Test `retries a lost apply acknowledgment with the same key and body`.
- Dimension 2.3 **IN_PROGRESS — Identity and input:** Abort owned reads when identity or source changes. Preserve composer text and focus through selection and refresh. → Test `preserves unsent text while the saved model changes`.

### 3. Verify the complete change

- Dimension 3.1 **IN_PROGRESS — Coverage:** Cover real command-line failure and retry paths exposed by Bun 1.4.4 without weakening the 100% gate. → Test `make test-unit-all`.
- Dimension 3.2 **IN_PROGRESS — Final validation:** Complete browser, build, repository verification and adversarial review. → Test `make test-integration-rustd`.

## Interfaces

The existing model list exposes runtime eligibility independently of authoring eligibility. Creating a private draft uses an authoring-eligible helper model but sends no message. Its native edit sends `runtime_model_ref`. Applying the reviewed candidate uses the current setup tag and retained idempotency key. The fleet definition remains the single runtime model authority for every future admission path.

## Failure Modes

| Failure | Required behavior | Negative proof |
|---|---|---|
| Model list unavailable or empty | Keep saved label, show a retryable explanation, write nothing | `shows model loading failures without changing the saved model` |
| Selection no longer eligible | Server refusal leaves saved choice visible | `retains the saved model after a refused edit` |
| Another writer saves setup | Conditional validation refuses a stale apply | `retains the saved model when the setup changes before apply` |
| Mutation reply lost | Retry exactly the retained operation before any new edit | `retries a lost apply acknowledgment with the same key and body` |
| Access lost or component removed | Clear private choices and abort owned requests | `clears models when workspace access is lost` |
| Setup editor contains unsaved changes | Runtime picker cannot apply the editor's draft | `does not apply unsaved editor changes` |

## Invariants

1. Confirmed revision responses alone update the displayed saved choice; local selection is not success.
2. A separate private draft owns picker edits. Setup tags fence concurrent writers.
3. Pending operations retain body and identity through `DraftOperation`; changed input is refused.
4. Runtime eligibility filters choices; an authoring-only model is not a runtime choice.
5. The persistent transcript is never keyed by model selection; composer text survives.
6. Same-origin authenticated requests and identity-scoped disposal preserve private state ownership.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|---|---|---|---|---|---|
| Existing reviewed setup writes | Existing service telemetry | Candidate edit and fleet revision apply | Existing resource identifiers and outcomes | No message text or private instructions added | Runtime-model request boundary tests |

No additional analytics or funnel is introduced. This control invokes existing setup edits without an authoring conversation; existing service measurements remain authoritative.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|---|---|---|---|
| 1.1 | unit | `saves a runtime-only model change without generating a message` | Same definition and name plus new model → native patch then reviewed apply, no message request |
| 1.2 | e2e | `fleet-composer` | Shared computed fonts and repeated reply completion → stable text, focus and placement |
| 2.1 | unit | `retains the saved model when the setup changes before apply` | Changed setup revision → no revision write and original label retained |
| 2.2 | unit | `retries a lost apply acknowledgment with the same key and body` | Lost response → retained request replay, one confirmed model change |
| 2.3 | e2e | `preserves unsent text while the saved model changes` | Type text then select model → text survives save and reload shows selected model |
| 3.1 | unit | `make test-unit-all` | All declared suites → 100% TypeScript package coverage |
| 3.2 | integration | `make test-integration-rustd` | Existing real dependency lane → zero failed tests |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Saved choice, refusal and replay work | `make test-unit-app` | exit 0; app coverage 100% | P0 | |
| R2 | Composer and replies survive browser interactions | Named acceptance scenarios in Test Specification | zero failed scenarios | P0 | |
| S1 | Conform gates pass | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S3 | Unit suites pass | `make test-unit-all` | exit 0 | P0 | |
| S4 | Real dependency lane passes | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version stays synchronized | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |

## Dead Code Sweep

The restored change deletes `fleetMarkdownBlocks.ts`, its test, the markdown benchmark and corpus. Search `rg -n 'fleetMarkdownBlocks|fleet-markdown-corpus|fleet-markdown-stream' ui/packages/app` and require zero references outside this spec and historical reports.

## Out of Scope

- M228 webhook verification and configuration; it retains its separate pending spec.
- M233 round-trip timing; the preexisting spec commit is excluded from the focused pull request.
- New backend endpoints, model inference, per-message overrides, changes to already admitted runs, and unrelated editor mutations.

## Product Clarity (authoring record)

1. **Successful user moment:** Choose a model, see a confirmed saved choice, and find it selected after reload.
2. **Preserved user behaviour:** Send, retry, edit setup and scroll without losing unsent text.
3. **Optimal-way check:** The existing reviewed revision path directly updates the same model setting every run uses.
4. **Rebuild-vs-iterate:** Iterate on shared controls and the current draft client; no second settings API.
5. **What we build:** A runtime picker, safe recovery, regression proofs and finished chat polish.
6. **What we do NOT build:** Webhook verification or timing instrumentation; each already has its own spec.
7. **Fit with existing features:** Runtime selection complements the setup conversation's separate helper-model selection.
8. **Surface order:** Browser first because this change repairs the fleet composer; the existing command-line setup flow remains available.
9. **Dashboard restraint:** State changes become visible only after the server confirms them.
10. **Confused-user next step:** Inline failure copy offers Retry or reopening the current setup; no internal flag is used as user copy.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One repair stream completes the restored chat worktree. Saving precedes recovery proofs and final verification.
- **Alternatives considered:** A chat-only override would require a new request field and would disagree with the displayed fleet setting. Opening the full editor for every model choice adds unnecessary steps.
- **Patch-vs-refactor verdict:** This is a patch because existing native edits, reviewed revisions, identity boundaries and shared controls supply the required behavior.

## Discovery (consult log)

- **Consults:** Indy chose continuation option A in the restored worktree. On model scope, Indy said: “Which i easier to fix? it shouldnt just limit to chat replies. For an user - what you see is what you get.” Source inspection confirms runtime model edits already update the fleet definition. Indy also asked to fix the review text beside Edit setup; remove the access warning entirely. Saved model changes must appear in chat and Activity.
- **Metrics review:** No analytics/funnel playbook update required; the picker saves through existing measured setup operations and sends no authoring message.
- **Skill-chain outcomes:** `context-restore` completed. `investigate` established that Bun 1.4.4 exposes previously credited command-line branches; evidence is `/tmp/agentsfleet-chat-cli-coverage.97WYoz`. Unit-test review, final review and post-push review remain pending.
- **Deferrals:** None. M228 and M233 are separate existing workstreams, not deferred requirements of this repair.

The review-flag investigation traced `creation/read.rs` to `review/mod.rs`: `needs_re_review` compares the accepted review digest to a fresh review. Model resolution includes pricing and catalogue version; credentials, connections and creator authority also contribute. The header previously presented every mismatch as access trouble without an action. Activation checks retain their backend authority. A confirmed model change must be distinguished from this generic comparison; do not report every mismatch as a model change. Source inspection is complete; the exact live-fleet trigger has not been reproduced.
