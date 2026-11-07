# M241_001: Edit runner configuration in a grouped panel

**Prototype:** v2.0.0

**Milestone:** M241

**Workstream:** 001

**Date:** Nov 07, 2026

**Status:** IN_PROGRESS

**Priority:** P1, operators need readable runner settings and reachable save controls.

**Categories:** UI

**Batch:** B1, one isolated presentation workstream.

**Depends on:** none.

**Provenance:** LLM-drafted (Codex, Nov 07, 2026).

**Canonical architecture:** `docs/DESIGN_SYSTEM.md`, settings and configuration surfaces.

## Overview

**Goal (testable):** An operator can edit runner settings in a responsive panel and save the complete assignment through the existing action.

**Problem:** The runner editor presents a long sequence of fields without clear grouping or a fixed save area.

**Solution summary:** Group execution, network access, durable tasks, and advanced settings in a right-side panel using shared components and tokens.

**Design flow:** Runner header → existing editor → shared policy fields → existing `updateRunnerAction` → existing PATCH → refresh after success.
`ui/packages/app/lib/api/runners.ts` owns the PATCH path.
`PolicyFields.tsx` owns assignment conversion and validation.
No API, authentication, model, or runner daemon behavior changes.

## PR Intent & comprehension handshake

- **PR title (eventual):** Improve runner configuration editing
- **Intent (one sentence):** Make runner configuration understandable and comfortable to edit within the existing product theme.
- **Authoring handshake:** Implement the approved runner editor first. ASSUMPTIONS I'M MAKING: existing assignment semantics stay authoritative; chat follows later; unsupported network controls stay absent.
- **Implementer handshake:** Same intent. Preserve mounts, durable-task settings, decision model, and untouched labels when saving another field.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.tsx` — save, reset, and error behavior.
2. `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.tsx` — shared enrollment and edit assignment schema.
3. `ui/packages/app/lib/api/runners-types.ts` — supported isolation and network policies.
4. `ui/packages/design-system/src/design-system/Dialog.tsx` — focus ownership and accessible dialog primitives.
5. `ui/packages/app/tests/e2e/fixtures/chat-probe.ts` — browser fixture serving real components with an HTTP test boundary.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| This spec, active and done paths | CREATE / MOVE | Opening, decisions, and proof record |
| `docs/DESIGN_SYSTEM.md` | EDIT | Document configuration panel composition and its design-history entry |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.tsx` | EDIT | Right-side editor, stable header and save footer, preserved action behavior |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.tsx` | EDIT | Compose the shared grouped fields without changing conversion semantics |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicySection.tsx` | CREATE | Reusable local section heading and description |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyIsolationField.tsx` | CREATE | Isolation choices separated from unrelated settings |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyNetworkFields.tsx` | CREATE | Group supported network policies and registry domains |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyWorkerField.tsx` | CREATE | Worker count and restart guidance beside the control |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.test.tsx` | EDIT | Preserve save, error, pending, and reset guarantees |
| `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyFields.test.tsx` | EDIT | Exercise grouped network choices and existing conversion guarantees |
| `ui/packages/app/tests/e2e/runner-configuration.spec.ts` | CREATE | Rendered desktop and mobile editing journey |
| `ui/packages/app/tests/e2e/fixtures/runner-configuration-probe.tsx` | CREATE | Render the production editor and enrollment fields |
| `ui/packages/app/tests/e2e/fixtures/runner-configuration-action.ts` | CREATE | Existing action replaced at the browser test HTTP boundary |
| `ui/packages/app/tests/e2e/fixtures/runner-configuration-build.ts` | CREATE | Build the browser probe using the established fixture pattern |
| `ui/packages/app/tests/e2e/fixtures/runner-configuration.ts` | CREATE | Serve browser fixture and shared stylesheet |

## Applicable Rules

- `code.md`: NDC, NRC, NLG, HLP, GRD, FLL, CODE-SEAM.
- `languages/typescript.md`: TS-ANY, TS-CAST, DID, ASE, TS-PRIMITIVE, TS-TOKEN, TS-LAYOUT.
- `tests.md`: TCF, TEST-OUTCOME, TEST-NEG, TEST-PUBLIC, TEST-INJECT, TEST-REPORT, TEST-SKIP.
- `docs.md`: DOC-02, DOC-03, DOC-05, DOC-S6.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| `orly gate work` | yes, changed source | Split field concerns and use shared components |
| `orly gate verify` | yes, user surface and documentation | Update the design reference and run declared non-test checks |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | yes | Split rendering by policy concern |
| Browser dry lane | yes, rendered page change | Desktop and mobile geometry, keyboard, network, and save checks |

## Prior-Art / Reference Implementations

- `ui/packages/app/components/domain/chat/DraftChatHeading.tsx` — right-side configuration dialog with bounded content.
- `ui/packages/design-system/src/design-system/OptionCard.tsx` — accessible selected policy choices.
- `ui/packages/app/tests/e2e/fixtures/chat-browser-build.ts` — browser fixture build and HTTP action boundary.

## Sections (implementation slices)

### §1 — Group the supported policy controls

Keep the assignment schema and conversion in their existing module.
Extract field rendering by concern and compose named sections for both enrollment and editing.
Isolation remains an assignment, rather than an engine selector.
The network interface offers only the three policies declared in `runners-types.ts`.
Registry hosts retain their existing meaning and empty-list defaults.

- **Dimension 1.1** — Supported network choices update the assignment → Test `saves each supported network policy` (unit): select each network mode → matching wire enum.
- **Dimension 1.2** — Invalid worker input cannot save → Test `rejects worker counts outside the supported range` (unit): zero or 65 workers → error and no action call.

### §2 — Keep editing and saving comfortable

Render a full-height right panel with a scrollable body and fixed header and footer.
Keep the current reset, error, label-only, and whole-assignment save behavior.
Retain pending protection against closing or duplicate submission.
Show heartbeat timing once and worker restart guidance beside its control.

- **Dimension 2.1** — Hidden assignment fields survive edits → Test `carries a stored bind through an edit that never touches it` (unit): change workers → retain mount data.
- **Dimension 2.2** — Failed saves keep the edit available → Test `surfaces a refused re-assignment instead of closing` (unit): 404 result → visible error and open dialog.

### §3 — Prove the rendered editing journey

Render the actual editor with shared fonts and tokens through the browser fixture pattern.
Inject save results at the HTTP test boundary without reaching a deployed account.
Measure the body and footer at desktop and mobile sizes, then capture the resulting panel.
Ask Astra to review the actual rendered implementation.

- **Dimension 3.1** — Editing works on desktop and mobile → Test `edits runner configuration with a reachable save footer` (e2e): open, scroll, edit, save → recorded complete assignment and refreshed values.
- **Dimension 3.2** — Network failure preserves edits for retry → Test `keeps runner edits after a failed save` (e2e): failed save → retained input, visible error, successful retry.

## Interfaces

Existing `EditPolicyDialog` props and `updateRunnerAction` payload semantics stay unchanged.
The local field components take the existing `Control<PolicyFormValues>`.
The API still accepts the same `AssignedPolicy` and optional labels.

## Failure Modes

| Mode | Kind | Cause | Handling | Test |
|------|------|-------|----------|------|
| Invalid workers | failure | Count outside 1–64 | Inline field error; no write | `rejects worker counts outside the supported range` |
| Runner unavailable | failure | Action returns 404 | Keep dialog open and show existing error sentence | `surfaces a refused re-assignment instead of closing` |
| Network failure | failure | Save action fails | Retain edit; allow deliberate retry | `keeps runner edits after a failed save` |

## Invariants

1. Complete assignments preserve untouched policy fields — `formFromPolicy` and `policyFromForm`, covered by stored-mount tests.
2. Untouched labels never enter a policy-only PATCH — original label comparison, covered by existing stored-label tests.
3. Pending writes cannot close the editor — transition state guards both dismissal and controls.
4. Unsupported network controls cannot write invented settings — typed policy enums and the existing schema remain authoritative.

## Metrics & Observability

N/A: no product or operator signal changes.

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|-----------|---------------------|----------|----------|-----------------|
| R1 | Existing assignment and edit guarantees pass | `make test-unit-app` | exit 0 | P0 | |
| R2 | Rendered editor supports desktop, mobile, and retry | `make dry-app` | exit 0 | P0 | |
| S1 | Lint | `make lint-all` | exit 0 | P0 | |
| S2 | Unit checks | `make test-unit-all` | exit 0 | P0 | |
| S3 | Integration checks | `make test-integration-rustd` | exit 0 | P0 | |
| S4 | Version alignment | `make check-version` | exit 0 | P0 | |
| S5 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

Cloud execution follows `orly-build`: run work and verify gates, push nothing, and stop before the PR gate.
The spec stays active if a required check lacks proof.

## Dead Code Sweep

N/A: no files deleted. Field rendering moves out of `PolicyFields` without compatibility exports or duplicate implementations.

## Out of Scope

- Runner chat, shared conversation changes, task-summary redesign, and fleet setup navigation follow later under the approved priority.
- New endpoints, harness changes, engine switching, independent package-domain toggles, logs, and benchmarks are not part of this editor.

## Product Clarity (authoring record)

1. **Successful user moment** — Save a runner change while understanding its effect and preserving other settings.
2. **Preserved user behaviour** — Enrollment, labels, decision models, mounts, and pending/error behavior keep their existing semantics.
3. **Optimal-way check** — A grouped panel with an always-visible save action directly addresses the long editor.
4. **Rebuild-vs-iterate** — Iterate on the shared field and dialog components.
5. **What we build** — Policy sections, supported network choices, stable save controls, and rendered browser proof.
6. **What we do NOT build** — A new runner backend or assistant transport.
7. **Fit with existing features** — Enrollment shares the same fields; fleet chat transport remains untouched.
8. **Surface order** — UI first, using existing APIs, as the owner requested.
9. **Dashboard restraint** — Show settings the API actually supports; omit speculative engine and network controls.
10. **Confused-user next step** — Read the nearby field guidance or the existing runner policy documentation link.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Separate field composition, editor layout, and browser proof.
- **Alternatives considered:** A wider centered modal still requires a long scroll. A new settings route adds navigation and duplicates editing ownership.
- **Patch-vs-refactor verdict:** A focused refactor because grouping field rendering lets enrollment and editing improve together without changing policy semantics.

## Discovery (consult log)

- **Consults** — Source revision `7c2c3167b`; existing runner types and action remain the source of supported settings.
- **Model reviews** — Earlier Astra reviews covered the conversation prototype. The actual editor still needs its rendered review.
- **Open blockers** — None at opening.
- **Deferrals and transfers** — Conversation work follows later under the owner's explicit priority.

> Owner (Indy, Nov 07, 2026): "Also since you have everthing you could implement the edit of the runner config, as per you earlier design?" — context: runner editor implementation.

> Owner (Indy, Nov 07, 2026): "the chat can be changed later" — context: runner editor comes first.

### Session Notes

PLAN: Implement the approved runner editor using existing actions and assignment validation.
ASSUMPTIONS I'M MAKING: UI presentation is the scope; chat follows later; no harness or backend changes.
`orly doctor` passed before opening the worktree at `7c2c3167b`.
