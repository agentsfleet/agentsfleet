<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M113_002: Every user-facing error reads as a sentence a non-engineer understands, not a passthrough of backend internals

**Prototype:** v2.0.0
**Milestone:** M113
**Workstream:** 002
**Date:** Jul 04, 2026
**Status:** PENDING
**Priority:** P1 — "model not in cached caps catalogue" was reported directly as a user-facing bug; a full-codebase audit this session found it is one instance of a systemic gap, not an isolated string.
**Categories:** UI
**Batch:** B1 — independent of M113_001 and M113_003.
**Branch:** feat/m108-connector-platform — folded into the SAME branch/PR (#477) as M108/M112, by Indy's explicit instruction this session.
**Depends on:** none
**Provenance:** LLM-drafted (Claude Sonnet 5, Jul 04, 2026) from a dedicated full-codebase error-copy audit this session (`lib/errors.ts`, every call site rendering an API error, and the backend error registries `error_entries.zig`/`error_entries_runtime.zig`).

**Canonical architecture:** none dedicated — `lib/errors.ts`'s `presentErrorString`/`CODE_MAP` is the de facto convention; this spec is the first to name it as a codebase-wide contract rather than a per-component convenience.

---

## Implementing agent — read these first

1. `ui/packages/app/lib/errors.ts` — the whole file. `CODE_MAP` (15 entries) and the raw-fallback branch that prepends `"Couldn't <action> — "` to whatever the backend sent — this fallback is the actual bug, not any one string.
2. `ui/packages/app/app/(dashboard)/settings/models/lib/use-provider-action.ts` — a hook with zero error-formatting at all; five call sites (`ProviderSwitchList.tsx:59,68`, `HeroChangeModelPanel.tsx:27`, `ActiveModelHero.tsx:51`, `HeroReplaceKeyPanel.tsx:35`) return the raw string straight into this hook's `error` state.
3. `ui/packages/app/lib/api/approvals.ts:134` — a structural bypass: hand-rolled `fetch` throwing a bare `Error` instead of `ApiError`, so `errorCode` is discarded before `presentErrorString` ever runs. Fix this before adding more `CODE_MAP` entries for approvals codes — otherwise they can never be reached.
4. `src/agentsfleetd/errors/error_entries.zig` / `error_entries_runtime.zig` — the backend registry `CODE_MAP` is measured against; 100+ codes exist there, 15 are curated.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** every user-facing error reads as a sentence, not a raw backend string
- **Intent (one sentence):** no error surface shows a snake_case token, an HTTP status code, a route name, or an internal noun like "cache"/"catalogue"/"payload" to a user who can't act on it.
- **Handshake:** implementing agent restates intent + assumptions at PLAN, before EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a user hits any error anywhere in the product and reads a plain sentence describing what happened and (where applicable) what to do next — never a raw backend string, a bare error code, or an engineering noun.
2. **Preserved user behaviour** — the underlying `errorCode` is still shown in a `<code>` tag alongside the friendly text where it already is today (support/debugging value preserved); no error's *presence* changes, only its wording.
3. **Optimal-way check** — the fully optimal fix would give every one of 100+ backend codes a hand-written friendly string. This spec scopes to the highest-traffic gap instead: fixing the structural bypasses (which affect users regardless of which code fires) plus the codes reachable from surfaces already flagged this session (Models page, approvals, credentials, connectors) — the long tail of rarely-hit admin/runner codes is Out of Scope, named explicitly rather than silently skipped.
4. **Rebuild-vs-iterate** — iterate. `presentErrorString`'s lookup-table design is sound; it just needs its bypasses closed and its table extended. No new error-handling architecture is justified by this complaint.
5. **What we build** — (a) route the six raw-passthrough call sites through `presentErrorString`; (b) fix the two structural bypasses (`approvals.ts`'s bare-`Error` throw, `EventsList.tsx`'s raw `failure_label` render) so their codes/tags become mappable at all; (c) add `CODE_MAP` entries for the concretely-reachable codes this session's audit named (`UZ-PROVIDER-001..004`, `UZ-VAULT-001..003`, `UZ-BUNDLE-001..002`, `UZ-APPROVAL-001..006`, plus a friendly label table for the `FailureClass` tag names `EventsList` renders raw).
6. **What we do NOT build** — a friendly string for every one of the 100+ backend codes (Out of Scope, named below); no change to the backend error registry itself (this is a frontend presentation fix only); no i18n/localization layer.
7. **Fit with existing features** — directly improves the surfaces M113_001 (Models page) and M113_003 (Secrets & ENVs) touch; sequence-independent, but implementing agent should re-check `use-provider-action.ts`'s call sites after M113_001 lands in case row-unification moved them.
8. **Surface order** — UI only.
9. **Dashboard restraint** — no new UI chrome; this is copy correctness on existing error surfaces.
10. **Confused-user next step** — every fixed error keeps (or gains, where missing) an actionable next step in its friendly text (e.g. "contact support", "check X") — matching the existing `presentErrorString` convention (`action` param → "Couldn't <action>").

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline.
- `dispatch/write_ts_adhere_bun.md` — every touched file is `.ts`/`.tsx`.
- `dispatch/write_any.md` — UFS (the friendly-string table is the single named-constant source; no error copy duplicated inline at call sites).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `.zig` touched — this is a frontend presentation fix |
| PUB / Struct-Shape | possible | if `use-provider-action.ts`'s hook signature changes to accept a `presentErrorString` call, shape-verdict at PLAN |
| File & Function Length | no | copy/lookup-table additions only |
| UFS | yes | every new friendly string lives in `CODE_MAP` (or the new `FailureClass` label table), never inlined at a call site |
| UI Substitution / DESIGN TOKEN | no | no new markup |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | frontend-only; the backend `error_entries.zig` registry itself is unchanged |

---

## Overview

**Goal (testable):** every one of the six raw-passthrough call sites and the two structural bypasses named in Files Changed routes its error through `presentErrorString` (or an equivalent friendly-label lookup for `EventsList`'s failure tags), and `CODE_MAP` covers every code reachable from the Models, Approvals, Credentials, and Connectors surfaces this session's audit examined.

**Problem:** "model not in cached caps catalogue" is not an isolated typo — six call sites bypass the friendly-copy layer entirely, two are structurally incapable of reaching it (discarded error code), and 85+ backend codes have no friendly mapping at all.

**Solution summary:** close the bypasses, extend the lookup table for the concretely-reachable codes, add a small label table for the raw Zig failure-tag names `EventsList` renders today.

---

## Prior-Art / Reference Implementations

- **UI** → `ui/packages/app/lib/errors.ts`'s existing `presentErrorString`/`CODE_MAP` — the convention every fix in this spec extends, not replaces.
- The two *already-correct* call sites in `ProviderKeyForm.tsx`/`CustomEndpointForm.tsx` (their credential-store step, not their activate step) show the right pattern to copy for the activate step next to them.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/errors.ts` | EDIT | extend `CODE_MAP` with the reachable `UZ-PROVIDER-*`/`UZ-VAULT-*`/`UZ-BUNDLE-*`/`UZ-APPROVAL-*` codes named above |
| `ui/packages/app/app/(dashboard)/settings/models/lib/use-provider-action.ts` | EDIT | route stored error through `presentErrorString` before setting state |
| `ui/packages/app/app/(dashboard)/settings/models/components/ProviderKeyForm.tsx` | EDIT | activate-step error now uses `presentErrorString` (matches its own store-step, two lines above) |
| `ui/packages/app/app/(dashboard)/settings/models/components/CustomEndpointForm.tsx` | EDIT | same fix, activate step |
| `ui/packages/app/lib/api/approvals.ts` | EDIT | replace the hand-rolled `fetch`/bare-`Error` throw with the shared `request()`/`ApiError` path so `errorCode` survives to `presentErrorString` |
| `ui/packages/app/app/(dashboard)/credentials/components/EditCredentialDialog.tsx` | EDIT | JSON-parse error no longer leaks the native `SyntaxError.message` verbatim |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | `failure_label` rendered through a new small friendly-label table instead of the raw Zig tag name |
| `ui/packages/app/app/cli-auth/[session_id]/page.tsx` | EDIT | replace "Unexpected session payload." and raw `HTTP ${status}` strings with plain sentences |
| `ui/packages/app/tests/*.test.tsx` (the ones this session's audit found pinning raw-passthrough as expected behavior, e.g. `hero-change-model-panel.test.tsx:51-58`, `provider-key-form.test.tsx:88-99`, `provider-switch-list.test.tsx:219-231`) | EDIT | assertions move from raw string to friendly string |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections — close the bypasses, extend the lookup table, fix the two standalone leaks (`EventsList`, `cli-auth`).
- **Alternatives considered:** a full sweep giving every backend code a friendly string — rejected as unbounded scope for this pass; named explicitly in Out of Scope with the reachable subset prioritized instead.
- **Patch-vs-refactor verdict:** **patch** — extends an existing, correct mechanism; no new error-handling architecture.

---

## Sections (implementation slices)

### §1 — Close the structural bypasses

- **Dimension 1.1** — the five `use-provider-action.ts` call sites route through `presentErrorString` → Test `test_provider_action_errors_use_friendly_copy`
- **Dimension 1.2** — `ProviderKeyForm`/`CustomEndpointForm`'s activate-step error matches their own store-step's existing `presentErrorString` pattern → Test `test_activate_step_error_matches_store_step_pattern`
- **Dimension 1.3** — `approvals.ts` throws `ApiError` (preserving `errorCode`) instead of a bare `Error`, so downstream `presentErrorString` calls can actually reach `CODE_MAP` → Test `test_approvals_error_preserves_error_code`
- **Dimension 1.4** — `EditCredentialDialog`'s JSON-parse error shows a fixed friendly sentence, never the native `SyntaxError.message` → Test `test_credential_json_parse_error_is_friendly`

### §2 — Extend `CODE_MAP` for the concretely-reachable codes

**Implementation default:** friendly strings follow the existing convention (`{lead noun/verb} — {what happened}. {next step if any}`); the agent writes each string reading the corresponding backend `error_entries.zig` detail for context, not copying it verbatim.

- **Dimension 2.1** — `UZ-PROVIDER-001..004` map to friendly strings (the flagship "model not in cached caps catalogue" bug plus its three siblings) → Test `test_provider_error_codes_have_friendly_copy`
- **Dimension 2.2** — `UZ-VAULT-001..003` map to friendly strings (credential store/rotate/rename paths) → Test `test_vault_error_codes_have_friendly_copy`
- **Dimension 2.3** — `UZ-BUNDLE-001..002` map to friendly strings (template/fleet-bundle onboarding validation, e.g. the raw `missing_skill`/`unsafe_path` tokens) → Test `test_bundle_error_codes_have_friendly_copy`
- **Dimension 2.4** — `UZ-APPROVAL-001..006` map to friendly strings (now reachable per Dimension 1.3) → Test `test_approval_error_codes_have_friendly_copy`

### §3 — `EventsList` failure reason + `cli-auth` hardcoded jargon

- **Dimension 3.1** — `EventsList.tsx`'s `failure_label` renders through a small friendly-label table (e.g. `oom_kill` → "Ran out of memory", `landlock_deny` → "Blocked by the sandbox policy") instead of the raw Zig enum tag name → Test `test_events_list_failure_reason_is_friendly`
- **Dimension 3.2** — `cli-auth/[session_id]/page.tsx`'s "Unexpected session payload." and raw `HTTP ${status}` strings become plain sentences → Test `test_cli_auth_page_errors_are_friendly`

---

## Metrics & Observability

Not applicable — no product/operator signal changes; this is error-copy correctness, not new instrumentation.

---

## Interfaces

Not applicable — no backend change; `CODE_MAP`'s shape (`Record<string, string>` or equivalent) is unchanged, only its entries grow.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| A code outside this spec's named list fires | rare/admin-only path not audited this session | falls through to the existing `"Couldn't <action> — <raw message>."` fallback — unchanged, not a regression, just not yet friendly (Out of Scope) |
| `FailureClass` gains a new tag the label table doesn't cover | backend adds a new failure reason | falls back to the raw tag name (same as today) rather than crashing — negative test required |

---

## Invariants

1. Every error surface that already shows the raw `errorCode` in a `<code>` tag continues to (Product Clarity #2) — friendly text is additive, not a replacement of the debug affordance.
2. A `FailureClass` tag not in the new label table renders its raw name rather than throwing — enforced by a negative test (Dimension 3.1's failure-mode case).

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_provider_action_errors_use_friendly_copy` | mock a raw backend error → the five call sites' rendered text is the `presentErrorString` output, not the raw string |
| 1.2 | unit | `test_activate_step_error_matches_store_step_pattern` | both forms' activate-step error uses the same formatting as their store-step |
| 1.3 | unit | `test_approvals_error_preserves_error_code` | a failed approve/deny call → `presentErrorString` receives a real `errorCode`, not `undefined` |
| 1.4 | unit | `test_credential_json_parse_error_is_friendly` | malformed JSON input → error text is a fixed sentence, not the native `SyntaxError.message` |
| 2.1-2.4 | unit | `test_{provider,vault,bundle,approval}_error_codes_have_friendly_copy` | for each named code, `presentErrorString({errorCode: "UZ-...", message: <raw>})` returns text that does not equal the raw backend string |
| 3.1 | unit | `test_events_list_failure_reason_is_friendly` | `failure_label: "oom_kill"` renders a friendly sentence; an unmapped tag renders its raw name (negative case) |
| 3.2 | unit | `test_cli_auth_page_errors_are_friendly` | a non-200 session-load response renders a plain sentence, no raw HTTP status leaked |

Regression: existing tests pinning raw-passthrough as expected (named in Files Changed) are updated to assert the new friendly copy, not deleted — the underlying error-triggering scenario they cover stays tested.

Idempotency/replay: N/A — no retry semantics touched.

---

## Acceptance Criteria

- [ ] No test asserts a raw backend string as expected UI output — verify: `make test-unit-app`
- [ ] `CODE_MAP` includes every code named in §2 — verify: `grep -c "UZ-PROVIDER-00[1-4]\|UZ-VAULT-00[1-3]\|UZ-BUNDLE-00[1-2]\|UZ-APPROVAL-00[1-6]" ui/packages/app/lib/errors.ts`
- [ ] `approvals.ts` throws `ApiError`, not a bare `Error` — verify: `grep -n "throw new Error" ui/packages/app/lib/api/approvals.ts` returns nothing
- [ ] `make lint-app` clean
- [ ] `gitleaks detect` clean

---

## Eval Commands (post-implementation)

```bash
# E1: no bare Error throw left in approvals.ts
grep -n "throw new Error" ui/packages/app/lib/api/approvals.ts && echo "FAIL" || echo "PASS"
# E2: Build — cd ui/packages/app && bun run build
# E3: Tests — make test-unit-app
# E4: Lint — make lint-app 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — N/A, no Zig touched
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

N/A — no files deleted.

---

## Discovery (consult log)

- **Consults:** none yet — populated during EXECUTE/VERIFY.
- **Metrics review:** not applicable — no product/operator signal changes.
- **Skill chain outcomes:** populated after `/write-unit-test` and `/review` run.
- **Deferrals:** none yet.

---

## Skill-Driven Review Chain (mandatory)

Standard chain — `/write-unit-test` → `/review` → `/review-pr`, per `AGENTS.md`.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test-unit-app` | | |
| Lint | `make lint-app` | | |
| Gitleaks | `gitleaks detect` | | |
| Dead code sweep | N/A | | |

---

## Out of Scope

- Friendly copy for the remaining ~85 backend error codes not reachable from the surfaces audited this session (`UZ-RUN-*` admin-runner codes, `UZ-AGT-*` beyond what §2 names, `UZ-CONN-*` beyond the connectors page's own already-passing coverage, `UZ-GRANT-*`) — a follow-up spec if these surface as real complaints.
- Any backend change to `error_entries.zig`/`error_entries_runtime.zig` — this is a frontend presentation fix only.
- Localization/i18n — out of scope for this product stage.
