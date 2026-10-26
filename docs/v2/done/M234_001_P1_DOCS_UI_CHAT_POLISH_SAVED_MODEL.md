# M234_001: Fleet chat polish and saved runtime model

**Prototype:** v2.0.0
**Milestone:** M234
**Workstream:** 001
**Date:** Oct 26, 2026
**Status:** DONE
**Priority:** P1 — fleet chat consistency
**Categories:** DOCS, UI
**Batch:** B1 — continue the restored chat worktree
**Branch:** fix/chat-smooth-replies
**Baseline revision:** 89415c2a0e622a920b4d66517decbf85640058b2
**Test Baseline:** unit=11768 integration=1409
**Baseline counts:** App 4,283; website 133; design system 519; command line 1,949 passed / 17 skipped; Rust unit shards 4,884 passed. Continuous Integration (CI) coverage confirms 1,409 existing tests in the current integration selection.
**Baseline evidence:** https://github.com/agentsfleet/agentsfleet/actions/runs/40222122637
**Baseline provenance:** Final head of [PR 784](https://github.com/agentsfleet/agentsfleet/pull/784), `f8a68b80d409cb54e0cc7ef114d0d8a0bd629392`, has the same Git tree (`43be42593362f6adee31e34d97b429d55b9bd263`) as the comparison revision. Unit [run 40222122637](https://github.com/agentsfleet/agentsfleet/actions/runs/40222122637), Rust unit [run 40222122635](https://github.com/agentsfleet/agentsfleet/actions/runs/40222122635), and live coverage [run 40222122664](https://github.com/agentsfleet/agentsfleet/actions/runs/40222122664) provide the baseline; these are Linux CI results with Bun 1.4.3, not a local baseline run.
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
| `VERSION`, `rustd/Cargo.toml` | EDIT | Feature release version; synchronize through `make sync-version` |
| `.orly/LOCAL.md`, `README.md` | EDIT | Preserve restored fixture naming and contributor copy |
| `docs/DESIGN_SYSTEM.md`, `docs/reviews/design-system-css-audit.md`, `docs/architecture/fleet_definitions.md` | EDIT/CREATE | Shared controls, saved model semantics and CSS audit |
| `cli/README.md`, `docs/architecture/web_app.md`, `docs/architecture/shared_chat_verification.md` | EDIT | Correct supported Bun minimum and the saved-fleet header action |
| `scripts/model-catalogue/quotes.integration.test.ts`, `scripts/model-catalogue/seed.integration.test.ts` | EDIT | Preserve restored runtime compatibility fixes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx`, `components/FleetHeader.tsx` | EDIT | Pass persisted model context and show readable header copy |
| `ui/packages/app/components/domain/FleetMarkdown.tsx`, `FleetReplyBody.tsx`, `FleetReplyFigures.tsx`, `fleetMessageRenderers.tsx`, `FleetThread.tsx`, `FleetThreadViewport.tsx`, `SteerComposer.tsx` and their existing tests | EDIT | Persistent replies, compact figures and composer model control |
| `ui/packages/app/components/domain/fleetMarkdownBlocks.ts`, `fleetMarkdownBlocks.test.ts`, `ui/packages/app/tests/bench/fleet-markdown-stream.bench.tsx`, `tests/helpers/fleet-markdown-corpus.ts` | DELETE | Remove the replaced markdown splitter and its fixtures |
| `ui/packages/app/components/domain/chat/ChatComposer.tsx`, `ChatComposerFrame.tsx`, `ChatToolbar.tsx`, `DraftMessage.tsx`, `DraftSaveBar.tsx`, `FleetSetup.tsx`, `FleetSetupWorkspace.tsx`, `chat-defaults.ts` | EDIT | Shared controls and isolated runtime selection context |
| `ui/packages/app/components/domain/chat/FleetModelSelect.tsx`, `FleetModelSelect.test.tsx`, `useFleetModel.ts`, `useFleetModel.test.tsx` | CREATE | Saved selection, paging, failure and replay behavior |
| `ui/packages/app/tests/helpers/fleet-model-fixture.ts` | CREATE | Public request-boundary fixture for model selection and replay |
| `ui/packages/app/lib/api/drafts.ts`, `ui/packages/app/lib/api/draft-source.ts`, `ui/packages/app/tests/draft-model-selection.test.ts`, `ui/packages/app/tests/draft-runtime-model.test.ts` | EDIT/CREATE | Honor runtime-only edits, type fleet previews and prove revision writes |
| `ui/packages/app/lib/events/run-figures-format.ts` and its test | EDIT | Millisecond duration formatting |
| `ui/packages/app/tests/e2e/acceptance/`, `tests/fleet-thread/`, `tests/fleets-routes/detail-header.test.ts`, `tests/workspace-chat/`, `tests/fleet-markdown.test.tsx`, `tests/fleet-setup-recovery.test.tsx`, `vitest.setup.ts`, `package.json` | EDIT/CREATE | Existing chat regression proofs and saved-model browser check |
| `ui/packages/design-system/src/`, `ui/packages/design-system/package.json` | EDIT/CREATE | Preserve shared composer, popover and CSS cleanup |
| `cli/test/` | EDIT/CREATE | Meaningful coverage of branches exposed by Bun 1.4.4; exact files recorded before editing |
| `cli/test/chat/{action-retries.test.ts,session.test.ts}`, `cli/test/{run-effect-render.unit.test.ts,fleet-schedule.unit.test.ts,core-ops-funcfill.unit.test.ts,fleet-logs-linecov.unit.test.ts,fleet-steer-repl.unit.test.ts,services-http-body.unit.test.ts,config-dir.unit.test.ts,connector.integration.test.ts,fleet-status-render.unit.test.ts,http-retry.provenance.unit.test.ts,entry-flag-validation.unit.test.ts}` | EDIT/CREATE | Cover command retry dispatch, malformed reads, explicit flags and transport/error boundaries |
| `rustd/crates/afd_authoring/src/creation/{read.rs,progress.rs,activate.rs,stored.rs,sql/locked.sql,revision/}`, `rustd/crates/afd_authoring/Cargo.toml`, `rustd/Cargo.lock` | EDIT/CREATE | Remove saved-review drift blocking; record model changes with the saved revision |
| `ui/packages/app/components/domain/EventsList.tsx` and its existing tests | EDIT | Name model changes plainly in Activity |
| `rustd/crates/afd_authoring/src/review/mod.rs`, `rustd/crates/afd_authoring/tests/draft/creation/{identity.rs,grants.rs}`, `rustd/crates/afd_api/tests/authoring/saving/revisions/` | EDIT/CREATE | Remove obsolete review-flag assertions and prove concrete readiness and durable notices |
| `rustd/crates/afd_api_wire/src/fleet/output.rs`, `rustd/crates/afd_api_tenant/src/handler/fleet/native/render/mod.rs`, `public/openapi.json`, `docs/architecture/api_resources.md`, `ui/packages/app/lib/api/fleet-detail.ts`, `tests/helpers/draft-fixtures.ts` | EDIT | Remove the unused review flag through the existing response and decoder |
| `ui/packages/app/lib/api/draft-edits.ts`, `tests/draft-edits.test.ts`, `ui/packages/app/lib/events/{event-payload.ts,event-summary.ts,run-summary.ts}` and their existing tests | EDIT | Recognize confirmed native-edit replay and share model-change copy between chat and Activity |

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

- **Dimension 1.1** — DONE — **Runtime selection:** Load runtime-eligible workspace models with bounded paging. Clone the current fleet setup into a private draft, edit only the runtime model and apply its reviewed revision. → Test `saves a runtime-only model change without generating a message`.
- **Dimension 1.2** — DONE — **Existing chat polish:** Preserve reply text nodes on completion, shared composer fonts, compact run details, precise durations and removal of the header access warning. → Test `fleet-composer`.
- **Dimension 1.3** — DONE — **Owner-approved changes:** Remove the saved-review drift flag and its activation hold. Keep current readiness checks. Save one model-change notice in the revision transaction for chat and Activity. → Test `saved_model_changes_record_one_notice_even_when_the_save_is_replayed`.

### 2. Preserve state and recover safely

- **Dimension 2.1** — DONE — **Refusal and stale setup:** Keep the saved model visible after refusal, missing models or a stale setup. Never apply unrelated editor changes. → Test `retains the saved model when the setup changes before apply`.
- **Dimension 2.2** — DONE — **Unconfirmed save:** Disable further model changes while a write is unresolved. Retry the retained mutation before continuing; show the new model only after confirmed apply. → Test `retries a lost apply acknowledgment with the same key and body`.
- **Dimension 2.3** — DONE — **Identity and input:** Abort owned reads when identity or source changes. Preserve composer text and focus through selection and refresh. → Test `saving the fleet model preserves both composers and survives reload`.

### 3. Verify the complete change

- **Dimension 3.1** — DONE — **Coverage:** Cover real command-line failure and retry paths exposed by Bun 1.4.4 without weakening the 100% gate. → Test `make test-unit-all`.
- **Dimension 3.2** — DONE — **Final validation:** Complete browser, build, repository verification and adversarial review. → Test `make test-integration-rustd`.

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
| 1.3 | integration | `saved_model_changes_record_one_notice_even_when_the_save_is_replayed` | Saved model change and replay → one durable notice |
| 1.3 | integration | `catalogue_price_changes_do_not_require_review_to_resume_a_ready_fleet` | Catalogue drift with ready prerequisites → resume succeeds |
| 2.1 | unit | `retains the saved model when the setup changes before apply` | Changed setup revision → no revision write and original label retained |
| 2.2 | unit | `retries a lost apply acknowledgment with the same key and body` | Lost response → retained request replay, one confirmed model change |
| 2.3 | e2e | `saving the fleet model preserves both composers and survives reload` | Type text in both composers then select model → both inputs survive save and reload shows selected model |
| 3.1 | unit | `make test-unit-all` | All declared suites → 100% TypeScript package coverage |
| 3.2 | integration | `make test-integration-rustd` | Existing real dependency lane → zero failed tests |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|---|---|---|---|---|
| R1 | Saved choice, refusal and replay work | `make test-unit-app` | exit 0; app coverage 100% | P0 | PASS: canonical `make test-coverage-all`, app 4,334 passed; all coverage metrics 100% |
| R2 | Composer and replies survive browser interactions | Named acceptance scenarios in Test Specification | zero failed scenarios | P0 | PASS: 18 scenarios; `/tmp/agentsfleet-chat-browser-corrected.log` |
| S1 | Conform gates pass | `make harness-verify` | exit 0 | P0 | See final `orly gate pr` output in PR Session notes; required before publication |
| S2 | Lint passes | `make lint-all` | exit 0 | P0 | See final `orly gate pr` output in PR Session notes; required before publication |
| S3 | Unit suites pass | `make test-unit-all` | exit 0 | P0 | See final `orly gate pr` output in PR Session notes; required before publication |
| S4 | Real dependency lane passes | `make test-integration-rustd` | exit 0 | P0 | See final `orly gate pr` output in PR Session notes; required before publication |
| S5 | Version stays synchronized | `make check-version` | exit 0 | P0 | See final `orly gate pr` output in PR Session notes; required before publication |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | See final `orly gate pr` output in PR Session notes; required before publication |

## Dead Code Sweep

The restored change deletes `fleetMarkdownBlocks.ts`, its test, the markdown benchmark and corpus. Search `rg -n 'fleetMarkdownBlocks|fleet-markdown-corpus|fleet-markdown-stream' ui/packages/app` and require zero references outside this spec and historical reports. The runtime picker reuses the draft client's owned setup read; its separate read controller and source-preview overloads are removed. The header uses the fleet card's existing `agentDisplayName(fleet.id)` helper.

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
10. **Confused-user next step:** Inline failure copy offers Retry for an unconfirmed save, Reload choices for a failed model list, and Reload fleet for a stale setup. No internal flag is used as user copy.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One repair stream completes the restored chat worktree. Saving precedes recovery proofs and final verification.
- **Alternatives considered:** A chat-only override would require a new request field and would disagree with the displayed fleet setting. Opening the full editor for every model choice adds unnecessary steps.
- **Patch-vs-refactor verdict:** This is a patch because existing native edits, reviewed revisions, identity boundaries and shared controls supply the required behavior.

## Discovery (consult log)

- **Consults:** Indy chose continuation option A in the restored worktree. On model scope, Indy said: “Which i easier to fix? it shouldnt just limit to chat replies. For an user - what you see is what you get.” Source inspection confirms runtime model edits already update the fleet definition. Indy also asked to fix the review text beside Edit setup; remove the access warning entirely. Saved model changes must appear in chat and Activity.
- **Metrics review:** No analytics/funnel playbook update required; the picker saves through existing measured setup operations and sends no authoring message.
- **Skill-chain outcomes:** `context-restore` completed. `investigate` established that Bun 1.4.4 exposes previously credited command-line branches; evidence is `/tmp/agentsfleet-chat-cli-coverage.log`. Unit-test proofs cover refusal, access loss, retained replay, cleanup and unsent inputs. Final adversarial and parent review converged with no findings on the unchanged product tree. The coverage audit mapped 29 of 29 meaningful paths with no gap. Post-push review starts after a pull request exists.
- **Deferrals:** None. M228 and M233 are separate existing workstreams, not deferred requirements of this repair.

The review-flag investigation traced `creation/read.rs` to `review/mod.rs`: `needs_re_review` compares the accepted review digest to a fresh review. Model resolution includes pricing and catalogue version; credentials, connections and creator authority also contribute. The header previously presented every mismatch as access trouble without an action. Activation checks retain their backend authority. A confirmed model change must be distinguished from this generic comparison; do not report every mismatch as a model change. Source inspection is complete; the exact live-fleet trigger has not been reproduced.

Indy then asked to remove the redundant code: “since we dont get any value to ask the user when the team's owner had changed it.” and “if possible cut down and remove that code please”. Remove saved-versus-current review-digest comparisons and the resulting flag/block. Current model, authority, policy, wallet and grant readiness still governs activation. The digest on an explicit new save remains its conditional write check.

Indy clarified the header's second line with a fleet-card screenshot: `AGENT MOSS-965F`, not the definition name `dev-acceptance-*`. Use the same stable agent callsign helper as the card for every fleet. The focused header check passes independently of missing or different definition names.

Baseline commands are the repository's individual unit/coverage targets from those jobs. The live baseline ran `make test-coverage-rustd RUSTD_SHARD=runner`, `daemon`, and `substrate` with real dependencies and `--include-ignored`; matching their passed test names to the current canonical integration selection confirms 1,409 existing cases. The current `make test-integration-rustd` log contains those cases plus two new model-change/readiness cases: 1,411 passed, zero failed. Unit-count comparison retains the platform difference: Linux has 108 additional runner unit cases compared with this macOS run. No claim is made that the standalone baseline integration command ran locally.

Final browser verification passed 18 scenarios after repairing the test stream fixture. A development remount replaced a closed stream; the fixture now delivers to the newest open stream and never reopens a closed one. The original seven failures were fixture delivery failures. A separate deterministic lifecycle diagnostic confirmed abandoned, replacement and dropped streams. Browser evidence: `.gstack/qa-reports/m234-review-20261026-1530/candidate/.qa-evidence/002/receipt.json`; fixture diagnostic: `/tmp/agentsfleet-chat-stream-fixture-probe.log`. The exact remount trigger remains unconfirmed.

Final app build passed. TypeScript coverage reports app 4,334, website 133, design system 521 and command line 1,967 passed, with 16 command-line skips. All four coverage surfaces report 100%. The last canonical full unit boundary reported 4,776 Rust cases passed and 1,417 intentionally live-dependency cases ignored. Production Rust is unchanged since the canonical integration result: 1,411 passed. The final `orly gate pr` reruns the complete declared verification set before publication; its results belong in PR Session notes.

Release metadata follows the feature-minor rule: `VERSION` 0.66.0, synchronized to the CLI and Rust workspace with `make sync-version`. The queue lookup found no claim or active sibling collision. The private root workspace package version remains independent of the binary version, as the canonical synchronization command declares. The unrelated M233 commit is preserved on `backup/chat-smooth-replies-before-m233-trim-20261026` and excluded from this branch.
