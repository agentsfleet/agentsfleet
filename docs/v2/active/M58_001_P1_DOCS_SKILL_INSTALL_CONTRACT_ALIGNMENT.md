# M58_001: Pending Install Contract Alignment

**Prototype:** v2.0.0
**Milestone:** M58
**Workstream:** 001
**Date:** May 01, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — pending install/docs artifacts now drift from canonical architecture and will mislead the next public-docs pass if left uncorrected.
**Categories:** DOCS, SKILL
**Batch:** B1 — follow-up architecture hygiene after M57; no code-path dependency beyond current `main`.
**Branch:** feat/m58-install-contract-alignment (to be created)
**Depends on:** M57_001 (canonical architecture docs already reconciled to shipped `main` contracts)

**Canonical architecture:** `docs/architecture/user_flow.md` §8.1-§8.7, `docs/architecture/data_flow.md`, and `docs/architecture/billing_and_byok.md`.

---

## Implementing agent — read these first

1. `docs/architecture/README.md` — glossary and canonical topic index after M57.
2. `docs/architecture/user_flow.md` — install, webhook, steer, and BYOK target-contract boundaries.
3. `docs/architecture/data_flow.md` — current message-ingest and webhook-ingest path shapes.
4. `docs/v2/pending/M49_001_P1_SKILL_DOCS_INSTALL_SKILL.md` — install-skill planning spec that still carries stale contract strings.
5. `docs/v2/pending/M51_001_P1_DOCS_API_DOCS_AND_INSTALL_PINGBACK.md` — public docs planning spec that consumes M49 wording.
6. `samples/platform-ops/{SKILL.md,TRIGGER.md,README.md}` — operator-facing sample artifacts that must reflect the same contract.
7. `src/http/route_manifest.zig`, `src/http/handlers/webhooks/zombie.zig`, `zombiectl/src/commands/zombie_steer.js`, `src/http/handlers/workspaces/credentials.zig` — shipped `main` contract sources for webhook URL, steer shape, and current BYOK credential scope.

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline; especially RULE UFS, RULE ORP, and RULE TST-NAM where examples or test names drift.
- `AGENTS.md` action-triggered gates — Architecture Consult, Milestone-ID, GREPTILE, HARNESS VERIFY, and Verification Gate.
- Standard set only beyond that — no Zig, REST, or AUTH edits are expected unless the scope changes.

## Overview

**Goal (testable):** Pending install/docs specs and `samples/platform-ops` describe the same shipped MVP contract as canonical `docs/architecture/**`: webhook setup uses the current zombie webhook URL, manual operator input uses batch `zombiectl steer {id} "<message>"`, and current workspace-scoped `credentials/llm` is clearly distinguished from the pending M48 tenant-provider target contract.

**Problem:** M57 corrected `docs/architecture/**`, but downstream artifacts still encode older assumptions. M49 and M51 still hardcode a source-suffixed webhook URL, still describe BYOK through `provider set` as if it were part of the current install path, and `samples/platform-ops` still references `zombiectl chat` and `/steer`. If left untouched, the next public-docs pass will re-import stale strings into user-facing docs and screenshots.

**Solution summary:** Reconcile the planning specs and sample artifacts to the same MVP-first contract M57 established. Default to docs/sample edits, not runtime changes. Keep tenant-scoped BYOK posture and `provider set` explicitly marked as the pending M48 target contract where those concepts still matter, but remove any wording that presents them as current install behavior.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M49_001_P1_SKILL_DOCS_INSTALL_SKILL.md` | EDIT | Align install-skill planning copy with the corrected webhook, steer, and current BYOK contract. |
| `docs/v2/pending/M51_001_P1_DOCS_API_DOCS_AND_INSTALL_PINGBACK.md` | EDIT | Align public-docs planning copy with the same contract so the docs pass does not reintroduce stale URLs. |
| `samples/platform-ops/SKILL.md` | EDIT | Keep the operator-facing sample prose aligned with current steer behavior. |
| `samples/platform-ops/TRIGGER.md` | EDIT | Fix trigger comments that still point at `/steer`. |
| `samples/platform-ops/README.md` | EDIT | Fix install, steer/chat, and credential examples that still describe the older operator flow. |
| `docs/v2/active/M58_001_P1_DOCS_SKILL_INSTALL_CONTRACT_ALIGNMENT.md` | EDIT | Lifecycle notes while the work is in progress. |

## Sections (implementation slices)

### §1 — Pending spec contract reconciliation

Update M49 and M51 so they consume the same contract now documented in `docs/architecture/**`. Implementation default: where the pending specs discuss webhook setup, manual smoke-test steering, or current BYOK setup, describe shipped `main` truth first and reserve tenant-provider posture for explicit M48 target-contract notes.

### §2 — Sample artifact operator-flow reconciliation

Update `samples/platform-ops` so an operator reading the sample sees the same current interaction model as the architecture docs and CLI: install via `zombiectl install --from`, manual smoke test via batch `zombiectl steer`, and current credential storage conventions. Preserve the sample's instructional value; do not rewrite its substance beyond the contract drift.

### §3 — Downstream sanity pass

Run a targeted grep across M48/M49/M51 and `samples/platform-ops` for stale webhook URL, `/steer`, `zombiectl chat`, and current-vs-target BYOK wording. Leave target-contract references in place only when they are explicitly labeled as future M48 behavior.

## Interfaces

Current contract under alignment:

```text
Webhook ingest URL (current main): POST /v1/webhooks/{zombie_id}
Manual steer path (current main):  POST /v1/workspaces/{workspace_id}/zombies/{zombie_id}/messages
Operator smoke test (current CLI): zombiectl steer {id} "<message>"
Current BYOK storage surface:      PUT /v1/workspaces/{workspace_id}/credentials/llm
Target M48 provider posture:       tenant_providers + zombiectl provider set
```

This workstream does not add or change runtime interfaces; it aligns planning and sample docs to the contracts above.

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Pending specs reintroduce stale URLs | M49/M51 preserve older webhook strings after M57 | Grep for the stale forms and update them in the same diff. |
| Sample docs contradict CLI behavior | `samples/platform-ops` keeps `zombiectl chat` or `/steer` wording | Rewrite those operator-flow lines to current `zombiectl steer` batch behavior. |
| Future contract is presented as shipped | `provider set` / `tenant_providers` wording lacks target-contract framing | Keep the concept only with explicit M48-target wording or replace it with current workspace-scoped truth. |
| Scope creeps into runtime code | Planner artifacts appear easier to “fix in code” than to rewrite | Default to docs/sample reconciliation; do not touch handlers, router, OpenAPI, or CLI unless a newly discovered contradiction proves the architecture doc is wrong. |

## Invariants

1. Pending install/docs specs must not present `POST /v1/webhooks/{zombie_id}/github` as the current shipped webhook URL — enforced by grep audit before closeout.
2. `samples/platform-ops` must not describe manual operator input via `zombiectl chat` or `/steer` when current `main` uses batch `zombiectl steer` over `/messages` — enforced by grep audit before closeout.
3. BYOK wording must distinguish current workspace-scoped `credentials/llm` from the pending M48 tenant-provider posture wherever both are mentioned — enforced by targeted grep and manual review.

## Test Specification

| Test | Asserts |
|------|---------|
| `pending_specs_no_stale_webhook_url` | M49 and M51 no longer present `/v1/webhooks/{zombie_id}/github` as shipped current behavior. |
| `samples_no_legacy_chat_contract` | `samples/platform-ops` no longer tells operators to use `zombiectl chat` or `/steer` for the current manual flow. |
| `byok_scope_wording_explicit` | Any remaining `provider set` / `tenant_providers` references in M49/M51 are explicitly framed as the pending M48 target contract. |

## Acceptance Criteria

- `docs/v2/pending/M49_001_P1_SKILL_DOCS_INSTALL_SKILL.md` matches canonical architecture for webhook setup, smoke-test steering, and current-vs-target BYOK wording.
- `docs/v2/pending/M51_001_P1_DOCS_API_DOCS_AND_INSTALL_PINGBACK.md` matches the same contract and does not reintroduce stale install copy.
- `samples/platform-ops/{SKILL.md,TRIGGER.md,README.md}` reflect current operator interaction surfaces on `main`.
- `gitleaks detect` passes before commit.
- `make lint` and `make test` pass, or any non-diff baseline failure is explicitly documented before closeout.

## Discovery (consult log)

N/A at creation.
