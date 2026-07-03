# M111_001: Standardize dashboard eyebrow typography, page headers, and Clerk appearance

**Prototype:** v2.0.0
**Milestone:** M111
**Workstream:** 001
**Date:** Jul 03, 2026
**Status:** DONE
**Priority:** P1 — customer-facing dashboard chrome; inconsistent typography and an invisible account modal read as unfinished.
**Categories:** DOCS, UI
**Batch:** B1 — standalone UI polish; no cross-workstream dependency.
**Branch:** fix/ui-design-standardization
**Test Baseline:** unit=2286 integration=247 (Zig depth gate — UI-only spec, Zig depth unchanged; UI test additions tracked in Verification Evidence: design-system + app vitest suites)
**Depends on:** none
**Provenance:** agent-generated (pre-spec, interactive design session with Indy, Jul 03 2026)

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` — the design-system primitives + `theme.css` tokens are the source of truth for dashboard chrome.

---

## Implementing agent — read these first

1. `ui/packages/design-system/src/design-system/SectionLabel.tsx` — the canonical eyebrow primitive; every other eyebrow must match its typography.
2. `ui/packages/design-system/src/theme.css` — the `text-eyebrow` / `tracking-eyebrow` / `leading-eyebrow` token utilities the eyebrow family standardizes on.
3. `ui/packages/app/lib/clerkAppearance.ts` — Clerk `appearance` map; the account-modal fix lives entirely here.
4. `dispatch/write_ts_adhere_bun.md` — UI Substitution + DESIGN TOKEN gates that fire on every `.tsx` edit.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Standardize dashboard eyebrow typography, page headers, and Clerk appearance
- **Intent (one sentence):** Make the dashboard read as one system — one eyebrow-label size everywhere, one page-header/subtitle pattern, one-click copy for the workspace identifier, and a readable account modal — with no behavioural change.
- **Handshake (agent fills at PLAN):** Restate intent + `ASSUMPTIONS I'M MAKING`; reconcile any mismatch before EXECUTE.

---

## Product Clarity

1. **Successful user moment** — a user lands on any dashboard page and every section header, column header, and nav-group label is the same size and weight; opening the account modal shows readable name/email/headings and a visible close button.
2. **Preserved user behaviour** — every existing action (switch workspace, install fleet, connect a tool, purchase-credits tooltip, sign-out) works unchanged; only presentation moves.
3. **Optimal-way check** — the direct path is one shared eyebrow constant + a shared page-header pattern, not per-page hand-tuning. Gap: card micro-labels and section headers unify to one size (12px eyebrow); the 11px "form label" tier stays for actual form field labels. Acceptable — those are a genuinely different element.
4. **Rebuild-vs-iterate** — iterate. No data-model or route change; a rewrite would trade away determinism for no user gain.
5. **What we build** — an `EYEBROW_CLASS` design-system constant, a `CopyButton` primitive, corrected Clerk v7 appearance keys, unified page-header/subtitle usage, and empty-state/copy cleanup on Dashboard/Fleets/Billing/API-Keys/Models/Workspace.
6. **What we do NOT build** — no new dashboard pages; no connector changes (M108_002 owns Integrations connectors); no restyle of badges/status-pills/buttons/stat-tiles (distinct families).
7. **Fit with existing features** — compounds with every dashboard surface; must not destabilize the Integrations connector work in flight (M108_002).
8. **Surface order** — UI-only; no CLI or API surface.
9. **Dashboard restraint** — no new controls; copy affordance only appears when the value exists (name/ID present).
10. **Confused-user next step** — presentation-only; nothing new to learn. The copy button carries an accessible label and a "Copied" confirmation.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (UFS for the shared constant; ORP orphan sweep for the deleted `InstallFlowGuide`; NLR touch-it-fix-it).
- **`dispatch/write_ts_adhere_bun.md`** — TS/Bun discipline + UI Substitution + DESIGN TOKEN gates (every `.tsx` edit).
- Standard set otherwise; no Zig / SQL / HTTP / schema surface.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG / PUB / SCHEMA / LOGGING / ERROR REGISTRY | no | No Zig/SQL/log/error-code surface. |
| File & Function Length (≤350/≤50/≤70) | yes | New files are small; no touched file approaches the cap. |
| UFS (repeated/semantic literals) | yes | `EYEBROW_CLASS` is the single named constant collapsing the repeated eyebrow token string; shared copy strings (empty-state title/description) named once in `template-docs`. |
| UI Substitution | yes | New `CopyButton` is a design-system primitive; all sites keep using primitives; no new raw HTML where a primitive exists. |
| DESIGN TOKEN | yes | Eyebrow sites move to `text-eyebrow`/`tracking-eyebrow`/`leading-eyebrow` token utilities; no arbitraries introduced (a pre-existing `text-[…]` is replaced by the token). |

---

## Overview

**Goal (testable):** Every dashboard eyebrow label renders with the design-system eyebrow tokens via one shared `EYEBROW_CLASS`; `SettingsTabs`/pages render title+subtitle through `PageHeader`; the Clerk account modal exposes readable text and a visible close button through v7 appearance keys; the Workspace identifier has a one-click copy affordance — all with the existing test suite green.

**Problem:** The dashboard mixed two near-identical eyebrow sizes (11px `text-label` vs 12px `text-eyebrow`) across dozens of hand-rolled labels; some pages had a title+subtitle and others didn't; the account modal rendered dark-on-dark (pre-v7 Clerk variable keys were silently ignored) with an invisible close button; the workspace identifier — the value the CLI and API target — had no copy affordance and sat redundantly beside the name.

**Solution summary:** Introduce one `EYEBROW_CLASS` typography constant in the design system and compose it into `SectionLabel` and the eyebrow-bearing primitives (`Card` featured badge, `MetaGrid`, `TerminalPanel` tag), then apply it to the app's hand-rolled eyebrow sites. Add a `CopyButton` primitive and place it on the Workspace name/ID rows. Correct the Clerk `appearance.variables` to the v7 key set and pin the modal close button. Standardize page headers/subtitles and tidy the first-run empty states and copy on Dashboard/Fleets/Billing/API-Keys/Models. No behaviour changes.

---

## Prior-Art / Reference Implementations

- **UI** → the design system's own `buttonClassName` / `tab-styles.ts` shared-class-constant pattern is the model for `EYEBROW_CLASS`; `SectionLabel` is the canonical eyebrow; `theme.css` carries the tokens. Clerk's own `shadcn` theme (`@clerk/themes`) confirms v7 `appearance.variables` accept `var(--token)` refs.
- No greenfield surface; every change mirrors an existing primitive or token.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/design-system/src/design-system/eyebrow.ts` | CREATE | The shared `EYEBROW_CLASS` typography constant. |
| `ui/packages/design-system/src/design-system/CopyButton.tsx` | CREATE | Icon-only clipboard primitive with copied-state + a11y label. |
| `ui/packages/design-system/src/design-system/{SectionLabel,Card,MetaGrid,TerminalPanel,Button}.tsx` | EDIT | Compose `EYEBROW_CLASS`; add `icon-sm` button size for `CopyButton`. |
| `ui/packages/design-system/src/{index.ts,design-system/index.ts}` | EDIT | Export `EYEBROW_CLASS` + `CopyButton`. |
| `ui/packages/design-system/package.json` | EDIT | Add `lucide-react` dependency (CopyButton icon). |
| `ui/packages/app/lib/clerkAppearance.ts` | EDIT | v7 variable keys + `modalCloseButton` pin. |
| `ui/packages/app/components/layout/SettingsTabs.tsx` | EDIT | Add `description` prop → standard subtitle. |
| `ui/packages/app/app/(dashboard)/settings/{page,api-keys/**}.tsx` | EDIT | Subtitles; Workspace copy affordance; API-Keys verbiage. |
| `ui/packages/app/app/(dashboard)/{page,fleets/**,settings/billing/**,settings/models/**}.tsx` | EDIT | Header/subtitle + empty-state/copy cleanup; eyebrow sites. |
| `ui/packages/app/app/(dashboard)/{credentials,fleets/[id]}/**`, `components/{layout/Shell,domain/fleetMessageRenderers}.tsx` | EDIT | Apply `EYEBROW_CLASS` to remaining app eyebrow sites. |
| `ui/packages/app/app/(dashboard)/fleets/new/InstallFlowGuide.tsx` | DELETE | The dashboard "how it works" guide (removed as redundant). |
| `~/Projects/docs/changelog.mdx` + affected `~/Projects/docs/**` pages | EDIT | Changelog entry + any doc page whose described chrome changed. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one design-system constant + primitive composition (propagates to every consumer), then leaf-site application — smallest change that unifies app-wide.
- **Alternatives considered:** (a) per-page inline token edits — rejected: leaves the drift source (duplicated class strings) in place; (b) a full typography-scale redesign — rejected: out of proportion to the problem and trades determinism.
- **Patch-vs-refactor verdict:** **patch** with one small new abstraction (`EYEBROW_CLASS`, `CopyButton`). No follow-up refactor required; the stat-tile/badge families are intentionally left for a separate call if Indy wants them folded in.

---

## Sections (implementation slices)

### §1 — Eyebrow typography unification ✅ DONE

One `EYEBROW_CLASS` constant; compose into `SectionLabel` and the eyebrow-bearing primitives; apply to app leaf sites. **Implementation default:** color stays per-site (constant is typography-only) because `cn` here is plain concatenation, not tailwind-merge — baking a color in would make per-site overrides unreliable.

- **Dimension 1.1** — `SectionLabel` renders the eyebrow tokens via the shared constant → Test `test_section_label_eyebrow_tokens`
- **Dimension 1.2** — `EYEBROW_CLASS` is exported from the package root → Test `test_design_system_exports_eyebrow`
- **Dimension 1.3** — every touched app eyebrow site renders `text-eyebrow`/`tracking-eyebrow` and no residual `text-label uppercase tracking-label` remains → Test: grep gate in Acceptance Criteria (no `text-label uppercase tracking-label` in touched app `.tsx`)

### §2 — CopyButton primitive + Workspace identifier ✅ DONE

Add an icon-only `CopyButton` (copied-state, accessible label, swallows clipboard rejection) and place it on the Workspace name/ID rows; the ID is the value the CLI/API target, so copy is one click.

- **Dimension 2.1** — clicking writes the value to the clipboard and flips the accessible name to "Copied" → Test `test_copy_button_writes_and_confirms`
- **Dimension 2.2** — a rejected clipboard write leaves the idle label (no false "Copied") → Test `test_copy_button_swallows_rejection`
- **Dimension 2.3** — the Workspace page renders a copy control for both name and ID → Test (settings page render asserts the copy affordance)

### §3 — Clerk v7 appearance correctness ✅ DONE

Correct `appearance.variables` to the v7 key set (`colorForeground`/`colorMutedForeground`/`colorInput`/`colorInputForeground`/`colorPrimaryForeground`/`colorBorder`), keeping `var(--token)` refs; pin `modalCloseButton` readable.

- **Dimension 3.1** — primary text + input variables use the v7 foreground keys, and the ignored pre-v7 keys are absent → Test `test_clerk_v7_variable_keys`
- **Dimension 3.2** — the modal close button color is pinned to a readable token → Test `test_modal_close_button_readable`

### §4 — Page-header pattern + empty-state/copy cleanup ✅ DONE

`SettingsTabs` gains a `description` prop so Workspace/API-Keys render the standard subtitle; Fleets gains a subtitle and drops the duplicate header Install button (moves into the list toolbar / empty-state action); Dashboard drops the redundant "how it works" guide and wrapping panel; empty states standardize on `Learn more` + a primary action; API-Keys adopts the "Authenticate with the agentsfleet API" verbiage; Billing "Pay as you go" right column re-aligns.

- **Dimension 4.1** — Fleets/Workspace/API-Keys render a title+subtitle via `PageHeader` → Test (route render asserts the subtitle string)
- **Dimension 4.2** — the Fleets empty state offers `Learn more` + `Install fleet` and no longer offers "Create a template" → Test `test_fleets_empty_state_actions`
- **Dimension 4.3** — the dashboard first-run surface renders the gallery/empty-state without the removed guide copy → Test `test_dashboard_first_install_no_guide`

---

## Metrics & Observability

Not applicable — no product or operator signal changes. This is presentation-only chrome standardization; no analytics events are added, renamed, or removed. Discovery records `Metrics review: no analytics/funnel playbook update required` (presentation-only).

---

## Interfaces

```
EYEBROW_CLASS: string            // "font-mono text-eyebrow uppercase leading-eyebrow tracking-eyebrow"
CopyButton(props: { value: string; label: string; className?: string })  // icon-only, client component
SettingsTabs(props: { title?: string; description?: string })            // description → PageHeader subtitle
Button size adds "icon-sm"       // 24px square, for inline icon affordances
AUTH_APPEARANCE.variables        // Clerk v7 keys, var(--token) values
```

No HTTP/CLI/data-shape interface changes.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Clipboard unavailable | insecure context / denied permission | `CopyButton` swallows the rejection; label stays idle — no false "Copied". |
| Missing workspace name/ID | list unavailable / synthesized entry | Copy control renders only when the value exists; otherwise the row shows "—" with no button. |
| Clerk key drift | future Clerk major renames keys | Test asserts the v7 keys are present and pre-v7 keys absent, failing loudly on regression. |

---

## Invariants

1. There is exactly one eyebrow typography definition — enforced by: `SectionLabel` and the eyebrow primitives import `EYEBROW_CLASS`; the Acceptance-Criteria grep gate fails if a touched app site re-introduces the old `text-label uppercase tracking-label` string.
2. The Clerk appearance uses only v7 variable keys — enforced by a unit test asserting v7 keys present and pre-v7 keys absent.
3. `CopyButton` never claims success on a failed write — enforced by the rejection unit test.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_section_label_eyebrow_tokens` | rendered `SectionLabel` className contains `text-eyebrow`, `tracking-eyebrow`, `leading-eyebrow`. |
| 1.2 | unit | `test_design_system_exports_eyebrow` | `EYEBROW_CLASS` is defined on the package root export. |
| 1.3 | unit (grep gate) | acceptance grep | no `text-label uppercase tracking-label` in touched app `.tsx`. |
| 2.1 | unit | `test_copy_button_writes_and_confirms` | click → `clipboard.writeText(value)` called; accessible name becomes "Copied". |
| 2.2 | unit | `test_copy_button_swallows_rejection` | rejected write → label stays idle; no "Copied". |
| 3.1 | unit | `test_clerk_v7_variable_keys` | `variables.colorForeground/colorInput/colorInputForeground` set; `colorText`/`colorTextSecondary`/`colorInputBackground` absent. |
| 3.2 | unit | `test_modal_close_button_readable` | `elements.modalCloseButton.color` is the readable text token. |
| 4.2 | unit | `test_fleets_empty_state_actions` | empty state markup contains "Install fleet" + "Learn more"; not "Create a template". |
| 4.3 | unit | `test_dashboard_first_install_no_guide` | first-run markup renders the gallery/empty-state without the removed guide copy. |

**Regression:** the full app + design-system suites must stay green (existing route/render/appearance tests updated to the new copy, not weakened). **Idempotency/replay:** N/A — no retry semantics.

---

## Acceptance Criteria

- [ ] design-system: `bun run lint` clean · `bun run test` passes — verify: `cd ui/packages/design-system && bun run lint && bun run test`
- [ ] app: `bun run lint` clean · `bun run test` passes — verify: `cd ui/packages/app && bun run lint && bun run test`
- [ ] design-token + UI-substitution audits clean — verify: `bash audits/design-tokens.sh && bash audits/msid-ui.sh`
- [ ] no residual old eyebrow string in touched app files — verify: `! grep -rn "text-label uppercase tracking-label" ui/packages/app --include="*.tsx" | grep -v -E "\.test\.|/tests/"`
- [ ] orphan sweep on the deleted guide — verify: `! grep -rn "InstallFlowGuide" ui/packages/app --include="*.ts*"`
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

```bash
# E1: design-system lint+test
(cd ui/packages/design-system && bun run lint && bun run test) && echo "PASS" || echo "FAIL"
# E2: app lint+test
(cd ui/packages/app && bun run lint && bun run test) && echo "PASS" || echo "FAIL"
# E3: design-token + UI-substitution audits
bash audits/design-tokens.sh && bash audits/msid-ui.sh && echo "PASS" || echo "FAIL"
# E4: eyebrow drift gate (empty = pass)
grep -rn "text-label uppercase tracking-label" ui/packages/app --include="*.tsx" | grep -v -E "\.test\.|/tests/" | head
# E5: orphan sweep (empty = pass)
grep -rn "InstallFlowGuide" ui/packages/app --include="*.ts*" | head
# E6: Gitleaks
gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `ui/packages/app/app/(dashboard)/fleets/new/InstallFlowGuide.tsx` | `test ! -f "ui/packages/app/app/(dashboard)/fleets/new/InstallFlowGuide.tsx"` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `InstallFlowGuide` | `grep -rn "InstallFlowGuide" ui/ \| head` | 0 matches |
| `CreateTemplateDocLink` (renamed → `TemplateDocsLink`) | `grep -rn "CreateTemplateDocLink" ui/ \| head` | 0 matches |

---

## Discovery (consult log)

- **Consults** — Clerk appearance root-cause: verified against `@clerk/themes` (its `shadcn` theme declares `variables: { colorForeground: "var(--card-foreground)" }`), confirming v7 `variables` accept `var()` refs; the invisible-modal bug was the pre-v7 key names being ignored, not the `var()` usage. An earlier hand-authored `DARK_PALETTE` literal-mirror module was authored then removed once this was confirmed — no literal palette is needed.
- **Metrics review** — no analytics/funnel playbook update required (presentation-only; no events changed).
- **Skill chain outcomes** — `/write-unit-test`: diff ledger resolved; added CopyButton (3), EYEBROW export, Button `icon-sm`, SectionLabel eyebrow-token, Clerk v7-keys + modal-close, and Workspace subtitle+copy tests; token/copy-swap sites covered by existing render tests + the eyebrow grep gate. `/review` (code-review, high): **0 correctness findings**; 2 cleanup items actioned (`DropdownMenuLabel` → `EYEBROW_CLASS`, killing an arbitrary; stale `FirstInstallCard` comments), the rest (cli-auth text-copy button, contextual clipboard sites, admin sans column headers) dispositioned Out of Scope.
- **Deferrals** — none; the stat-tile/badge/status-pill families and other-affordance clipboard sites are documented Out of Scope, not deferred work.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean; iteration count in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs spec, design system, Failure Modes, Invariants. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Review-comments the open PR. | Comments addressed before merge. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| design-system unit | `cd ui/packages/design-system && bun run test` | 46 files, 432 tests passed | ✅ |
| app unit | `cd ui/packages/app && bun run test` | 128 files, 1167 tests passed | ✅ |
| design-system lint | `cd ui/packages/design-system && bun run lint` | oxlint + tsc clean | ✅ |
| app lint | `cd ui/packages/app && bun run lint` | oxlint --type-aware clean | ✅ |
| design-token audit | `bash audits/design-tokens.sh` | no arbitraries with a token equivalent (--all) | ✅ |
| UI-substitution audit | `bash audits/msid-ui.sh` | 0 hits | ✅ |
| UFS audit | `bash audits/ufs.sh` | no violations across 1387 files | ✅ |
| Orphan sweep | `grep -rn "InstallFlowGuide\|CreateTemplateDocLink" ui/` | 0 matches | ✅ |
| Gitleaks | `gitleaks detect` | run pre-commit (hook) | ✅ |

---

## Out of Scope

- Restyling badges, status pills, buttons, and stat tiles (`StatusCard`/`DataTable`) — distinct component families with their own intentional treatments; a separate call if Indy wants them folded in.
- Integrations connector cards — M108_002 owns that surface.
- Any new dashboard page, route, data-model, or analytics change.
