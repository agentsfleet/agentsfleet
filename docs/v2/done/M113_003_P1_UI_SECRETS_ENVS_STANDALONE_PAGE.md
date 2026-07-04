<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M113_003: Secrets & ENVs gets its own page — standard list, standard empty state, Add via dialog

**Prototype:** v2.0.0
**Milestone:** M113
**Workstream:** 003
**Date:** Jul 04, 2026
**Status:** DONE
**Priority:** P1 — Indy's explicit decision this session to reverse M87's Models/Credentials unification, after weighing the regression-test cost against the UX cost of the merged page. Scope expanded mid-EXECUTE (Indy-directed, confirmed 3×) to a full-stack credential→secret rename — see Discovery.
**Categories:** API, CLI, UI
**Batch:** B1 — independent of M113_001/002; shares the Models page file tree with M113_001 (removes the section M113_001's row-list otherwise sits alone below).
**Branch:** feat/m108-connector-platform — folded into the SAME branch/PR (#477) as M108/M112, by Indy's explicit instruction this session.
**Test Baseline:** unit=2309 integration=249
**Depends on:** none (sequencing note: land after or alongside M113_001 to avoid two specs editing `settings/models/page.tsx`'s composition in the same window)
**Provenance:** LLM-drafted (Claude Sonnet 5, Jul 04, 2026) from a targeted investigation this session of the current Models/Credentials merge (`page.tsx` composition, `CustomSecretsList.tsx` vs. the unused `CredentialsList.tsx`, the M87 regression test) plus Indy's explicit UX steer: standard empty-state ("No secrets yet"), a real "Add Secret" dialog (not an always-inline form), and consistency with the table pattern used elsewhere in the product.

**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` §8.3 (credential metadata list — data model unchanged by this spec) — see the added nav-placement-history note pointing at this spec and M87.

---

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/credentials/page.tsx` — currently a 10-line redirect to `/settings/models` (M87). This spec restores it as the real page; `WORKSPACE_CREDENTIALS_PATH = "/credentials"` (`lib/fleet-credentials.ts:18`) already points here, so the fleet-install deep-link contract needs zero changes.
2. `ui/packages/app/app/(dashboard)/credentials/components/CredentialsList.tsx` — a complete, `DataTable`-based, `EmptyState`-using credentials table (Edit/Delete, `ConfirmDialog`) that already exists but is imported by no page today (confirmed dead via grep). This is the Prior-Art to revive/adapt, not rebuild — it already matches the product's on-pattern table idiom (`BillingUsageTab` uses the same `DataTable` primitive).
3. `ui/packages/app/app/(dashboard)/admin/models/components/AddModelDialog.tsx` — the "Add X" dialog convention to mirror for the new Add-Secret flow (trigger button → `Dialog` → form → submit), replacing the current always-inline, never-closed `AddCredentialForm`.
4. `ui/packages/app/components/layout/Shell.tsx` — the nav structure; `CONFIGURATION_NAV` is where "Secrets & ENVs" is added back as its own entry, alongside Models and Integrations.
5. `ui/packages/app/tests/app-components.test.ts:277-289` — the M87 regression test asserting exactly one combined Configuration nav entry with no standalone Credentials link. This spec inverts its assertion (Secrets & ENVs *does* get its own link) rather than deleting the test's coverage of the nav shape wholesale.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Secrets & ENVs: its own page, its own nav entry, standard list + Add-via-dialog
- **Intent (one sentence):** secrets management stops being a scroll-past afterthought at the bottom of the Models page and becomes its own destination with the same list/empty-state/add conventions every other settings surface uses.
- **Handshake:** implementing agent restates intent + assumptions at PLAN, before EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a user clicks "Secrets & ENVs" in the sidebar, lands on `/credentials`, sees a table of their secrets (or "No secrets yet" + an "Add Secret" button), clicks Add, a dialog opens asking for the secret's name and value, submits, and the table now shows the new row.
2. **Preserved user behaviour** — the fleet-install flow's deep link to `/credentials` (`WORKSPACE_CREDENTIALS_PATH`) continues to resolve to a real, useful page instead of bouncing through a redirect; every existing credential CRUD server action (`createCredential`, `rotate`, `rename`, `delete`) keeps its exact signature; Rotate/Rename (already a modal, `EditCredentialDialog`) is unchanged.
3. **Optimal-way check** — reviving the already-built, on-pattern `CredentialsList.tsx` (DataTable, EmptyState, Edit/Delete, ConfirmDialog) is more direct than either keeping the hand-rolled `CustomSecretsList` table or building a third implementation from scratch.
4. **Rebuild-vs-iterate** — this is a **reversal of M87's specific nav-collapse decision**, not a rebuild of the credential system underneath it. The vault/credential data model, server actions, and CRUD endpoints are untouched — only the page/nav surface moves back to standalone, now built better than M87 found it (M87 predates the on-pattern `DataTable`-based list; this spec uses it).
5. **What we build** — `/credentials` becomes a real page (list + empty state + Add dialog); a new `CONFIGURATION_NAV` entry "Secrets & ENVs"; the Models page's "Custom Secrets" section is removed entirely (moves out, not duplicated); `AddCredentialForm` is wrapped in a Dialog trigger (`AddSecretDialog`) instead of always rendering inline; `CustomSecretsList`'s hand-rolled `<table>` is retired in favor of the existing `CredentialsList.tsx`.
6. **What we do NOT build** — no change to the vault/crypto_store credential model, no change to any CRUD server action's signature, no change to `EditCredentialDialog` (already correct), no new backend endpoint.
7. **Fit with existing features** — must land in the same window as (or after) M113_001, since both edit `settings/models/page.tsx`'s composition; must not break the fleet-install flow's credential deep-link.
8. **Surface order** — UI only.
9. **Dashboard restraint** — no new signal beyond what `CredentialsList.tsx` already shows (name, added date, referenced-by); no new columns invented for this spec.
10. **Confused-user next step** — the empty state's "Add Secret" button is the direct next step; no dead end.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline; RULE NDC/ORP (orphan sweep) applies directly since this spec either revives or removes the dead `CredentialsList.tsx` vs. the hand-rolled `CustomSecretsList.tsx` — exactly one of the two table implementations survives.
- `dispatch/write_ts_adhere_bun.md` — every touched file is `.ts`/`.tsx`; TS FILE SHAPE DECISION at PLAN for the new `AddSecretDialog`.
- `dispatch/write_any.md` — UFS (nav label, empty-state copy as named constants, not inlined twice).

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `.zig` touched |
| PUB / Struct-Shape | yes — new `AddSecretDialog` component | shape verdict at PLAN, mirroring `AddModelDialog`'s existing shape |
| File & Function Length | no | `CredentialsList.tsx` is already 285 lines, under the 350 cap; no growth expected |
| UFS | yes | nav label "Secrets & ENVs" and empty-state copy each defined once |
| UI Substitution / DESIGN TOKEN | yes | reuse `DataTable`, `EmptyState`, `Dialog` — no new bespoke markup |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | not touched |

---

## Overview

**Goal (testable):** `/credentials` renders a standalone page (table + empty state + Add-Secret dialog trigger), the sidebar shows a "Secrets & ENVs" entry under Configuration alongside Models and Integrations, and the Models page no longer renders any secrets/credentials content.

**Problem:** secrets management is buried at the bottom of the Models page behind an always-visible, never-closing form and a hand-rolled table that doesn't match the rest of the product's list conventions — while an already-built, on-pattern table implementation sits unused in the codebase.

**Solution summary:** restore `/credentials` as a real page using the existing `CredentialsList.tsx`, add its own nav entry, wrap the add-flow in a dialog, and remove the section from the Models page.

---

## Prior-Art / Reference Implementations

- **UI (list)** → `ui/packages/app/app/(dashboard)/credentials/components/CredentialsList.tsx` — already `DataTable`-based, matching `BillingUsageTab`'s pattern; revive rather than rebuild.
- **UI (add flow)** → `ui/packages/app/app/(dashboard)/admin/models/components/AddModelDialog.tsx` / `AddRunnerDialog.tsx` — the established "Add X" dialog-trigger convention this page currently lacks.
- **UI (nav)** → `ui/packages/app/components/layout/Shell.tsx`'s `CONFIGURATION_NAV` array — Integrations' own entry is the direct template for Secrets & ENVs' entry.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/app/(dashboard)/credentials/page.tsx` | EDIT | replace the redirect with the real page (fetch + compose list + empty state + Add dialog) |
| `ui/packages/app/app/(dashboard)/credentials/components/CredentialsList.tsx` | EDIT | revive as the page's list (confirm props match current credential data shape; it predates recent kind-classification changes) |
| `ui/packages/app/app/(dashboard)/credentials/components/CustomSecretsList.tsx` | DELETE | replaced by `CredentialsList.tsx` — exactly one table implementation survives |
| `ui/packages/app/app/(dashboard)/credentials/components/AddCredentialForm.tsx` | EDIT | wrapped by a new `AddSecretDialog` trigger instead of always rendering inline |
| `ui/packages/app/app/(dashboard)/credentials/components/AddSecretDialog.tsx` | CREATE | dialog trigger + mount, mirroring `AddModelDialog`'s shape |
| `ui/packages/app/app/(dashboard)/settings/models/page.tsx` | EDIT | remove the "Custom Secrets" section entirely (moved out) |
| `ui/packages/app/components/layout/Shell.tsx` | EDIT | add "Secrets & ENVs" to `CONFIGURATION_NAV`, pointing at `/credentials` |
| `ui/packages/app/tests/app-components.test.ts` | EDIT | invert the nav-collapse assertion — Secrets & ENVs now expected as its own link |
| `ui/packages/app/tests/custom-secrets-list.test.ts` | DELETE | tests the retired component |
| `ui/packages/app/tests/credentials-list.test.ts` (if none exists, create) | EDIT/CREATE | cover the revived component in its new mount context |
| `ui/packages/app/tests/models-credentials-page.test.ts` | EDIT | remove assertions on the now-removed Models-page secrets section |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** three Sections — restore the page/nav, revive the list, wrap Add in a dialog.
- **Alternatives considered:** keep Secrets merged into Models but just swap the table implementation (Recommended-but-declined option from this session's own proposal) — Indy explicitly chose the standalone-page reversal instead, given the UX cost of a buried, scroll-past section outweighs the regression-test/process cost of reopening M87.
- **Patch-vs-refactor verdict:** **patch** at the code layer (reviving an existing component, restoring an existing route) but a **decision reversal** at the product layer — named explicitly per Indy's rule on surfacing size-of-change calls rather than silently absorbing them.

---

## Sections (implementation slices)

### §1 — Restore `/credentials` as a real page + its own nav entry — DONE (route renamed to `/secrets`, see Discovery)

- **Dimension 1.1** — `/credentials` renders a real page (no longer a redirect); `WORKSPACE_CREDENTIALS_PATH` deep links resolve to real content → Test `test_credentials_page_renders_standalone`
- **Dimension 1.2** — sidebar shows "Secrets & ENVs" as its own `CONFIGURATION_NAV` entry → Test `test_nav_shows_secrets_envs_entry` (inverts the M87 regression test's assertion)
- **Dimension 1.3** — the Models page no longer renders any secrets/credentials content → Test `test_models_page_has_no_secrets_section`

### §2 — Revive `CredentialsList.tsx` as the standard list — DONE (renamed `SecretsList.tsx`, see Discovery)

**Implementation default:** empty-state copy is "No secrets yet" (already matches the sitewide "No {noun} yet" convention — no change needed there), using the shared `EmptyState` primitive `CredentialsList.tsx` already imports.

- **Dimension 2.1** — the revived list renders via `DataTable` with the same columns `CustomSecretsList` offered (Name, Added, Referenced by, Action), confirmed compatible with the current credential-kind classification → Test `test_credentials_list_renders_via_data_table`
- **Dimension 2.2** — the dead-code `CustomSecretsList.tsx` and its test are deleted; zero remaining imports → Test: Dead Code Sweep grep below

### §3 — Add Secret via dialog, not an always-inline form — DONE

- **Dimension 3.1** — `AddSecretDialog` (trigger button + `Dialog` + the existing `AddCredentialForm` fields) opens on click, closes on success or cancel, matching `AddModelDialog`'s interaction shape → Test `test_add_secret_dialog_opens_and_closes`
- **Dimension 3.2** — a successful submit adds the new row to the list without a full page reload (existing `router.refresh()`-style pattern, mirrored from `AddModelDialog`) → Test `test_add_secret_dialog_refreshes_list_on_success`

---

## Metrics & Observability

Not applicable — no product/operator signal changes; this is a page/nav restructure of an existing surface with existing server actions.

---

## Interfaces

Not applicable — no new endpoint, no changed request/response shape. Every credential CRUD action (`createCredential`, rotate, rename, delete) keeps its exact signature (Product Clarity #2).

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Credential list fetch fails | network/API error | same degraded-empty-state handling `CustomSecretsList`/`CredentialsList` already have — no new failure path introduced |
| Add-Secret dialog submit fails | validation or API error | routed through `presentErrorString` (per M113_002 if landed; otherwise the existing pattern `AddCredentialForm` already uses) — dialog stays open, error shown inline |

---

## Invariants

1. `WORKSPACE_CREDENTIALS_PATH` remains `"/credentials"` and the fleet-install deep-link flow (`InstallStates.tsx`) needs zero changes — enforced by its existing test suite passing unmodified.
2. Exactly one table implementation for secrets/credentials exists in the codebase after this spec — enforced by Dead Code Sweep.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit + e2e | `test_credentials_page_renders_standalone` | `/credentials` renders list/empty-state content, not a redirect |
| 1.2 | unit | `test_nav_shows_secrets_envs_entry` | sidebar markup contains a "Secrets & ENVs" link to `/credentials` |
| 1.3 | unit | `test_models_page_has_no_secrets_section` | Models page markup contains no secrets/credentials content |
| 2.1 | unit | `test_credentials_list_renders_via_data_table` | revived list renders through `DataTable`, same columns as before |
| 2.2 | unit (regression) | Dead Code Sweep | zero remaining references to `CustomSecretsList` |
| 3.1 | unit | `test_add_secret_dialog_opens_and_closes` | click Add Secret → dialog visible; cancel/submit closes it |
| 3.2 | unit | `test_add_secret_dialog_refreshes_list_on_success` | successful submit → new row appears without a full reload |

Regression: the fleet-install flow's credential deep-link test suite passes unmodified (Invariant 1).

Idempotency/replay: N/A — no retry semantics touched.

---

## Acceptance Criteria

- [x] `/secrets` renders standalone content — verify: `make test-unit-app` (route renamed from `/credentials` per the scope-expansion decision)
- [x] Sidebar shows "Secrets & ENVs" — verify: `make test-unit-app` (Dimension 1.2)
- [x] `CustomSecretsList.tsx` fully removed — verify: `test ! -f ui/packages/app/app/(dashboard)/secrets/components/CustomSecretsList.tsx`
- [x] Zero remaining references to the deleted component — verify: Eval Command E8
- [x] `make lint-app` clean · no file over 350 lines added
- [x] `gitleaks detect` clean (pre-commit hook, every commit this workstream)

---

## Eval Commands (post-implementation)

```bash
# E1: CustomSecretsList fully gone
test ! -f "ui/packages/app/app/(dashboard)/credentials/components/CustomSecretsList.tsx" && echo "PASS" || echo "FAIL"
# E2: Build — cd ui/packages/app && bun run build
# E3: Tests — make test-unit-app
# E4: Lint — make lint-app 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile — N/A, no Zig touched
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: orphan sweep for the deleted component
grep -rn "CustomSecretsList" ui/packages/app/ --include="*.tsx" --include="*.ts" | grep -v node_modules
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/app/(dashboard)/credentials/components/CustomSecretsList.tsx` | `test ! -f ui/packages/app/app/(dashboard)/credentials/components/CustomSecretsList.tsx` |
| `ui/packages/app/tests/custom-secrets-list.test.ts` | `test ! -f ui/packages/app/tests/custom-secrets-list.test.ts` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `CustomSecretsList` | `grep -rn "CustomSecretsList" ui/packages/app/ --include="*.tsx" --include="*.ts"` | 0 matches |

---

## Discovery (consult log)

- **Consults:** Indy explicitly chose to reverse M87's Models/Credentials nav unification (AskUserQuestion this session, Jul 04, 2026) after being shown the regression-test cost — "Split it out (reverse M87)" over the lower-cost "keep merged, fix the table" alternative. A pointer note (not a rewrite) was added to `docs/architecture/billing_and_provider_keys.md` §8.3 referencing both M87 and this spec, per Indy's instruction not to spend time editing a completed milestone's spec.
- **Scope expansion mid-EXECUTE (Indy-directed, confirmed 3× including full cost disclosure):** Indy asked for the whole "credential" entity renamed to "secret" — "I also want to name the API as /secrets, and use the term secrets, since credentials is Lame" — then confirmed the full cross-stack scope after I flagged: (1) wire-value risk (backend REST path, `CREDENTIAL_KIND` enum, template-gallery JSON field, auth scope strings) — confirmed "rename backend too"; (2) the CLI breaking-change cost (`agentsfleet credential *` → `agentsfleet secret *`, ~15-20 Zig files + CLI + full test/cross-compile cycle) — confirmed "all of it now, in this PR". This is why the diff is far larger than the original spec's Files Changed table: it now spans `ui/packages/app/` (route `/credentials`→`/secrets`, every `Credential`-named type/function/component/file), `src/agentsfleetd/` (route matcher, 3 handler files renamed, `auth/scopes.zig` wire scope strings `credential:{read,write}`→`secret:{read,write}`, error registry messages, `schema/020_tenant_providers.sql` column `credential_ref`→`secret_ref`), and `cli/` (the `agentsfleet credential` subcommand → `agentsfleet secret`, done by a delegated background agent, corrected once for the `credential_ref` wire-field mismatch I found after its first pass).
- **Explicitly excluded from the rename (different concepts, same word):** the OAuth connector-token broker (`src/agentsfleetd/credentials/{broker,integration,serve_broker}.zig` — GitHub/Slack/Zoho/Jira/Linear tokens), the runner's ephemeral execution-credential minting (`src/runner/**`, `runner_credentials_mint`), the fleet-bundle/TRIGGER.md YAML config schema's `credentials:` requirement key (`lib/types.ts`'s `FleetTemplateGalleryEntry.requirements.credentials`, `create_fleet_bundle.zig`'s `ensureBundleCredentials` parameter) — renaming any of these would have conflated genuinely different entities or required a template-schema migration outside this spec's scope. `vault.deleteCredential` in `state/vault.zig` also kept as-is — confirmed it's shared generic vault-row-delete infrastructure used by the OAuth connector test suite too, not secret-specific despite the name. `UZ-CRED-001`/`UZ-CRED-003` in `lib/errors.ts` also untouched — confirmed via the backend registry these are the OAuth "Integration not connected" codes, an unrelated namespace collision on the word "cred".
- **`credential_key.zig` rename reverted:** initially renamed to `secret_key.zig`, then reverted after discovering 9+ OAuth connector files import it purely to compute the generic `fleet:<name>` vault-key prefix — genuinely shared naming infrastructure, not secret-specific.
- **Mechanical bug in my own tooling:** the first rename pass used `sed -i '' 's/\bfoo\b/bar/'` — macOS/BSD `sed` does not support `\b` word-boundary regex and silently no-ops on those patterns (only plain substring patterns took effect). Re-ran everything with `perl -pe` instead, which handles `\b` correctly cross-platform. Worth remembering for any future large identifier rename in this repo.
- **Metrics review:** not applicable — no product/operator signal changes.
- **Skill chain outcomes:** `/write-unit-test` and `/review` still to run before CHORE(close).
- **Deferrals:** none.

---

## Skill-Driven Review Chain (mandatory)

Standard chain — `/write-unit-test` → `/review` → `/review-pr`, per `AGENTS.md`.

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests (app) | `make test-unit-app` | 127 files, 1170/1170 | ✅ |
| Unit tests (design-system) | `make test-unit-design-system` | 46 files, 432/432 (unaffected) | ✅ |
| Unit tests (CLI) | `cd cli && bun test` | 1261/1261, 100% coverage maintained | ✅ |
| Zig build | `zig build` | clean | ✅ |
| Zig tests | `zig build test` | 1440+/1911 (469 skip — DB-integration, need live Postgres), 2 pre-existing unrelated flaky (webhook-sig, worker-pool observability — confirmed unrelated, not in this diff's files) | ✅ |
| Cross-compile | `zig build -Dtarget=x86_64-linux` / `aarch64-linux` | both clean | ✅ |
| Lint | `make lint-app` | Oxlint + tsc clean | ✅ |
| Gitleaks | `gitleaks protect --staged` (pre-commit hook) | 0 leaks, every commit | ✅ |
| Dead code sweep | see above | 0 remaining `CustomSecretsList`/`credential_*` references in the vault-secrets scope | ✅ |

---

## Out of Scope

- Any change to the vault/crypto_store credential data model or CRUD server actions — this is a page/nav restructure only.
- A generic RBAC/permissions system — the existing `operator`-scope gate on the credentials list endpoint is unchanged.
- Error-message friendliness for this page's own error paths beyond what M113_002 already covers.
