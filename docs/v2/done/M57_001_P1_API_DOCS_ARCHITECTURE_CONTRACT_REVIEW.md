# M57_001: Architecture Contract Review

**Prototype:** v2.0.0
**Milestone:** M57
**Workstream:** 001
**Date:** May 01, 2026
**Status:** DONE
**Priority:** P1 — architecture docs drive every downstream spec and must match shipped contracts before install-skill/docs work continues.
**Categories:** API, DOCS
**Batch:** B1 — standalone architecture hygiene and contract reconciliation.
**Branch:** feat/m57-architecture-contract-review
**Depends on:** M43_001 (webhook ingest shipped and defines the current webhook route shape), M48_001 (BYOK provider pending and defines the intended tenant-provider direction), M49_001 (install skill pending and consumes architecture URLs), M51_001 (public docs pending and consumes architecture copy).

**Canonical architecture:** `docs/architecture/README.md` (topic index and v2 runtime source of truth).

---

## Implementing agent — read these first

1. `docs/architecture/README.md` — architecture topic index and glossary.
2. `docs/architecture/high_level.md` — product thesis and launch pillars.
3. `docs/architecture/user_flow.md` — install, trigger, and model-cap origin flow.
4. `docs/architecture/data_flow.md` — stream, route, table, and worker/executor contracts.
5. `docs/architecture/capabilities.md` — platform guarantees and context lifecycle.
6. `docs/architecture/billing_and_byok.md` and `docs/architecture/scenarios/` — billing, BYOK, and end-to-end walkthroughs.
7. `src/http/router.zig`, `src/http/route_manifest.zig`, `src/http/route_matchers.zig`, `src/http/handlers/webhooks/zombie.zig`, `ui/packages/app/lib/api/zombies.ts` — shipped route contracts to compare against the docs.
8. `docs/greptile-learnings/RULES.md`, `docs/ZIG_RULES.md`, `docs/REST_API_DESIGN_GUIDELINES.md` — rule gates for any API/code edits.

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline, especially RULE UFS, RULE EMS, RULE ORP, RULE WAUTH, RULE TST-NAM, and RULE PRI if code or tests change.
- `docs/ZIG_RULES.md` — required if any `*.zig` route, handler, or test file changes.
- `docs/REST_API_DESIGN_GUIDELINES.md` — required if any HTTP route, route manifest, handler, or OpenAPI surface changes.
- `docs/AUTH.md` — required if auth middleware, route auth, webhook signing, or token-bearing request paths change.
- `AGENTS.md` action-triggered gates — Architecture Consult, ZIG Gate, Pub Surface, File Length, Milestone-ID, GREPTILE, HARNESS VERIFY, and Verification Gate.

## Overview

**Goal (testable):** A happy-path and adversarial review of `docs/architecture/**` produces a reconciled architecture corpus whose route names, auth posture, BYOK scope, billing gate semantics, context lifecycle, and user-flow claims match the current code or are explicitly captured as pending implementation contracts.

**Problem:** The architecture docs are now the canonical source for downstream specs and public docs, but they mix current implementation, recently completed milestones, pending BYOK/install-skill plans, and superseded office-hours material. At least two contract risks are visible before implementation: GitHub webhook URLs in docs mention a source-suffixed route that current route code does not expose, and BYOK docs describe tenant-scoped provider posture while current route manifests still expose workspace-scoped `credentials/llm`.

**Solution summary:** Conduct two passes over the architecture corpus. The happy-path pass confirms the intended cold install, first steer, webhook, BYOK, balance gate, and event execution story is coherent. The adversarial pass checks for stale URLs, route/auth mismatches, unsafe credential examples, contradicted BYOK scope, unimplemented claims presented as shipped, and public-doc copy risks. Fix the docs directly when the docs are wrong; make small route/OpenAPI/test changes only when the docs represent the intentionally canonical contract and code is lagging in a narrow, verifiable way.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/architecture/README.md` | EDIT | Reconcile glossary and topic descriptions with final route/provider contracts. |
| `docs/architecture/high_level.md` | EDIT | Keep product thesis and trigger examples aligned with current canonical URLs and shipped scope. |
| `docs/architecture/user_flow.md` | EDIT | Fix install, webhook, credential, and model-cap flow claims found by review. |
| `docs/architecture/data_flow.md` | EDIT | Fix route, stream, event, gate, and recovery contracts found by review. |
| `docs/architecture/capabilities.md` | EDIT | Reconcile platform guarantees and context lifecycle claims with shipped code. |
| `docs/architecture/billing_and_byok.md` | EDIT | Reconcile BYOK scope, balance gate semantics, and safe credential examples. |
| `docs/architecture/scenarios/README.md` | EDIT | Keep scenario summary aligned with corrected contracts. |
| `docs/architecture/scenarios/01_default_free_tier.md` | EDIT | Fix default install and webhook walkthrough. |
| `docs/architecture/scenarios/02_byok.md` | EDIT | Fix BYOK walkthrough and tenant/workspace scope wording. |
| `docs/architecture/scenarios/03_balance_gate_paid.md` | EDIT | Fix balance gate walkthrough if contradictions remain. |
| `docs/architecture/direction.md` | EDIT | Tighten architectural constants if the review exposes a missing invariant. |
| `docs/architecture/bastion.md` | EDIT | Fix only if it contradicts current MVP boundaries. |
| `docs/architecture/office_hours_v2.md` | EDIT | Mark superseded material more clearly or remove stale assertions that confuse canonical readers. |
| `docs/architecture/plan_engg_review_v2.md` | EDIT | Mark superseded material more clearly or remove stale assertions that confuse canonical readers. |
| `docs/v2/active/M57_001_P1_API_DOCS_ARCHITECTURE_CONTRACT_REVIEW.md` | EDIT | Lifecycle status and discovery notes during execution. |
| `src/http/router.zig` | EDIT IF NEEDED | Only if the canonical architecture route is intentionally ahead of code and the code change is narrow. |
| `src/http/route_manifest.zig` | EDIT IF NEEDED | Keep route manifest synchronized with any route change. |
| `src/http/route_matchers.zig` | EDIT IF NEEDED | Keep route parsing synchronized with any route change. |
| `src/http/handlers/webhooks/zombie.zig` | EDIT IF NEEDED | Keep webhook source routing/auth behavior synchronized with any route change. |
| `src/http/router_test.zig` | EDIT IF NEEDED | Verify any route matching change. |
| `src/http/route_matchers_test.zig` | EDIT IF NEEDED | Verify any route matcher change. |
| `src/http/route_manifest_test.zig` | EDIT IF NEEDED | Verify any route manifest change if this repo has a manifest test. |
| `public/openapi/**` | EDIT IF NEEDED | Keep OpenAPI synchronized with any route change. |
| `ui/packages/app/lib/api/zombies.ts` | EDIT IF NEEDED | Keep dashboard webhook URL helper synchronized with any canonical route change. |
| `ui/packages/app/tests/**` | EDIT IF NEEDED | Verify any dashboard URL helper change. |

## Sections (implementation slices)

### §1 — Happy-path architecture review

Review the current docs as a product operator following the intended success path: install skill, doctor, credential setup, first steer, webhook trigger, worker dispatch, executor run, Slack/evidence output, events/history, billing telemetry, and later BYOK posture. Record contradictions in the spec Discovery section before editing.

### §2 — Adversarial architecture review

Review the same corpus as a hostile maintainer and security reviewer. Check stale route names, auth fallthrough, raw credential examples, tenant/workspace isolation, event replay, continuation lineage, balance-gate bypass, unimplemented claims presented as shipped, and public-doc copy that would create false launch promises.

### §3 — Contract reconciliation

For every finding, choose one of two paths: update docs to match shipped code, or update narrow code/OpenAPI/tests to match the architecture contract. Implementation default: docs change unless the architecture contract is already consumed by pending M49/M51 specs and current code drift is narrow enough to fix safely in this workstream.

### §4 — Evidence and downstream readiness

Update the spec Discovery section with the review matrix and final decisions. Run docs grep/audit commands plus project verification gates. Confirm M49 and M51 can consume the corrected architecture without inheriting stale URLs or BYOK-scope contradictions.

## Interfaces

Architecture review does not add a new public interface by default. Any route/interface change requires amending this section before the code edit and synchronizing:

```text
HTTP route manifest: src/http/route_manifest.zig
Router/matcher: src/http/router.zig, src/http/route_matchers.zig
OpenAPI: public/openapi/**
Dashboard helpers: ui/packages/app/lib/api/zombies.ts
```

Current candidate contracts under review:

```text
Webhook ingest URL: POST /v1/webhooks/{zombie_id} versus POST /v1/webhooks/{zombie_id}/github
Manual steer path: POST /v1/workspaces/{workspace_id}/zombies/{zombie_id}/messages
BYOK provider scope: tenant-scoped provider posture versus workspace-scoped credentials/llm route
Model-caps endpoint: GET /_um/{cryptic_key}/model-caps.json
```

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Stale architecture URL survives | Docs and code disagree and only one file is fixed | Run grep across `docs/architecture`, pending specs, route manifest, OpenAPI, and dashboard helper; fix all non-historical claims in scope. |
| Docs overclaim shipped behavior | Superseded planning docs are read as current architecture | Move claims into clearly marked historical/provenance sections or update current topic docs to state shipped vs pending boundary. |
| Security regression through docs | Credential examples include token-shaped values or suggest URL/query secrets | Replace with placeholders and runtime `op read`/vault references; never include credential values. |
| BYOK scope remains ambiguous | Architecture says tenant-scoped while route/code says workspace-scoped | Make the decision explicit in current architecture and either file follow-up implementation notes or adjust narrow code contracts if already intended. |
| Route change without synchronization | Router, manifest, OpenAPI, and UI helpers drift | Apply REST guide synchronization checklist and tests before any commit. |
| Existing M43 worktree edits collide | Separate uncommitted Zig changes exist in another worktree | Do not touch that worktree; this workstream starts from `main` in its own worktree. |

## Invariants

1. Current architecture topic files must not present historical/superseded office-hours claims as current shipped contracts — enforced by grep review and spec Discovery entries.
2. Public route claims in `docs/architecture/**` must match either `src/http/route_manifest.zig` or an explicitly pending implementation spec — enforced by route grep and manifest comparison.
3. Credential examples must use placeholders, vault references, or `op read` runtime references only — enforced by gitleaks plus manual grep for token-shaped examples.
4. BYOK scope must be named consistently as tenant-scoped provider posture or workspace-scoped credential storage, not both as the same layer — enforced by grep for `tenant`, `workspace`, `llm`, and `tenant_providers` in corrected files.

## Test Specification

| Test | Asserts |
|------|---------|
| `architecture_route_claims_match_manifest` | Grep of architecture route claims matches route manifest or documented pending contract. |
| `architecture_no_token_examples` | Gitleaks plus manual grep find no real credential-shaped values introduced by the docs edits. |
| `architecture_byok_scope_consistent` | BYOK docs consistently distinguish tenant provider posture from credential storage scope. |
| `architecture_historical_docs_marked` | `office_hours_v2.md` and `plan_engg_review_v2.md` cannot be mistaken for current canonical implementation contracts. |
| `route_contract_tests` | If code routes change, router/matcher/OpenAPI/UI tests prove the new contract. |

## Acceptance Criteria

- `docs/architecture/**` has a completed happy-path and adversarial review recorded in this spec's Discovery section.
- Every non-historical architecture claim about webhook URLs, steer paths, BYOK scope, balance gate, model caps, and context lifecycle is either correct against current code or explicitly marked as pending/future.
- No credential values are introduced in docs or code; `gitleaks detect` passes before commit.
- If any route/code surface changes, `make lint`, `make test`, `make test-integration`, `make check-pg-drain`, and cross-compile gates pass as required by the touched files.
- If docs-only, `make lint` and `make test` pass; `make test-integration` is documented as N/A for no handler/schema/Redis change.

## Discovery (consult log)

- Initial review, May 01, 2026: current `main` shows architecture docs referencing `POST /v1/webhooks/{zombie_id}/github`, while route manifest and dashboard helper expose `POST /v1/webhooks/{zombie_id}`. Decision pending during §3.
- Initial review, May 01, 2026: architecture docs describe BYOK as tenant-scoped provider posture, while current route manifest exposes workspace-scoped `/v1/workspaces/{workspace_id}/credentials/llm`. Decision pending during §3.
- Decision, May 01, 2026: reconcile architecture topics to shipped `main` contracts for webhook ingest and batch `steer`; mark tenant-scoped BYOK posture as the pending M48 target rather than current behavior.
- Closeout, May 01, 2026: no code or OpenAPI route change was required for M57. The MVP-first decision was to make canonical architecture docs honest about shipped `main`, while preserving tenant-scoped provider posture as an explicitly future M48 contract in the scenario and billing topics.
