<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M130_002: Render what the wire already carries

**Prototype:** v2.0.0
**Milestone:** M130
**Workstream:** 002
**Date:** Jul 14, 2026
**Status:** DONE
**Priority:** P1 — two security gates never ran in Continuous Integration (CI), a one-time token rode a client prop, and per-event cost was fetched and discarded on every page load.
**Categories:** UI
**Batch:** B1 — same branch and Pull Request as M130_001; folded in by Indy mid-flight, split to its own workstream at the spec-length bound.
**Branch:** feat/m130-catalog-row-edit
**Test Baseline:** unit=2598 integration=324 (shared with M130_001 — the workstreams ride one branch)
**Depends on:** M130_001 (same branch; shares the design-system and admin surfaces)
**Provenance:** LLM-drafted (claude-fable-5, Jul 14, 2026) — written at CHORE(close) to give already-landed, Indy-directed scope its own rulebook; every Dimension below shipped with its test in the same commits.
**Canonical architecture:** `docs/architecture/product_analytics.md` (no flow change; surfacing only)

---

## Overview

**Goal (testable):** Every value the API already sends is rendered or copyable — no hand-rolled clipboard code survives, the two grep-gates run in CI and pass, no one-time credential is passed as a client-component prop, the dashboard rollup is total over the status registry, tool-call frames render in the thread, and per-event tokens/wall-time appear on event rows.

**Problem:** Four independent surfacing failures with one shape — the backend ships a fact and the client discards it. The design system's `CopyButton` had zero consumers because a same-named local component shadowed it; the two credential grep-gates under `tests/grep-gates/` were never matched by the one-level test glob, so CI never ran them — and one was red on `main` (a one-time runner token passed as a prop). The dashboard counted three of five fleet statuses, so `installing`/`killed` fleets vanished from the rollup. The live thread dropped every `tool_call_*` frame while its empty state promised them. `tokens` and `wall_ms` rode every event row and were rendered nowhere.

**Solution summary:** Adopt the design-system `CopyButton` at thirteen call sites and delete every bespoke clipboard implementation, hardening the primitive to report (never swallow) a failed write. Fix the test glob so `tests/**` runs; restructure `AddRunnerDialog` so the runner token is never a prop (panel inlined where the state lives). Make the fleet rollup total-by-construction over `AGENTSFLEET_STATUS`. Fold tool-call frames onto their events and render them; render tokens/wall-time on event rows.

## PR Intent & comprehension handshake

- **PR title (eventual):** shared with M130_001 — fix(m130): the catalog row cannot lie, and the surfaces render what the wire carries
- **Intent (one sentence):** The app stops discarding facts the backend already sends — copyable identifiers, security gates, fleet counts, tool calls, and per-event cost all surface.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/design-system/src/design-system/CopyButton.tsx` — the primitive every call site adopts; failure reporting and the `showLabel` variant live here, so no call site re-solves either.
2. `ui/packages/app/tests/grep-gates/no-api-template-mint.test.ts` — the two credential gates; the token-prop rule explains the `AddRunnerDialog` restructure.
3. `ui/packages/app/lib/api/fleets.ts` — `AGENTSFLEET_STATUS` is the registry the rollup must be total over.
4. `ui/packages/app/lib/streaming/fleet-stream-frames.ts` — the frame reducer; `FRAME_KIND` names every frame the backend publishes.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/design-system/src/design-system/CopyButton.tsx` | EDIT | Failure reported via outcome state + live region; `showLabel` variant; `COPY_RESET_MS` exported. |
| `ui/packages/design-system/src/design-system/CopyButton.test.tsx` | EDIT | Failure-path tests replace the accidental swallow-assertion. |
| `ui/packages/design-system/src/design-system/index.ts` | EDIT | Export `COPY_RESET_MS`. |
| `ui/packages/design-system/src/index.ts` | EDIT | Barrel export. |
| `ui/packages/app/package.json` | EDIT | Test scope widens to everything vitest sees — the two grep-gates run in CI for the first time. The rewrite initially dropped `--testTimeout 60000` on the floor, silently putting the heavy dialog suites on vitest's 10s default cliff. |
| `ui/packages/app/vitest.config.ts` | EDIT | The timeout's real home: `TEST_TIMEOUT_MS` raised to the 60s the scripts used to carry as a CLI flag, so every entry point (scripts, bare `vitest`, watch, editor) agrees on the budget instead of flaking only outside `bun run test`. |
| `ui/packages/app/app/cli-auth/[session_id]/cli-auth-ui.tsx` | EDIT | The shadowing local `CopyButton` deleted. |
| `ui/packages/app/app/cli-auth/[session_id]/page.tsx` | EDIT | Imports the design-system primitive (`showLabel`). |
| `ui/packages/app/app/(dashboard)/admin/runners/components/AddRunnerDialog.tsx` | EDIT | Reveal panel inlined; token never a prop; inline copy on the field. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.tsx` | EDIT | Host-id copy; cells split out when the affordance crossed the 350-line cap. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerListCells.tsx` | CREATE | The table's cell components + action config, ModelsRegistryCells-shaped. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerList.test.tsx` | EDIT | Action-set assertion excludes the copy affordance by `data-slot`. |
| `ui/packages/app/app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx` | EDIT | One-time key: inline copy, failure reported. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/approvals/[gateId]/page.tsx` | EDIT | Action-id copy. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/GuidedTriggerCard.tsx` | EDIT | Bespoke clipboard machinery removed; primitive adopted. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/GuidedTriggerCard.test.tsx` | EDIT | Per-button independence + failure reporting pinned. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/TriggerPanel.tsx` | EDIT | Fallback copy adopts the primitive. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/TriggerPanel.test.tsx` | EDIT | Revert-window test pins `COPY_RESET_MS`. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/CronCard.tsx` | EDIT | Schedule copy — parity with sibling trigger types. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/FleetsList.tsx` | EDIT | Row link becomes an overlay so the id copy is not interactive-inside-anchor. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` | EDIT | Secret-name copy (the interpolation key). |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx` | EDIT | Model-id + base-URL copy. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/page.tsx` | EDIT | Rollup total over the registry; Installing/Killed tiles when non-zero. |
| `ui/packages/app/lib/fleet-rollup.ts` | CREATE | `countFleets` — buckets seeded from `AGENTSFLEET_STATUS`; unknown counted, never dropped. |
| `ui/packages/app/lib/fleet-rollup.test.ts` | CREATE | Totality + unknown-status tests. |
| `ui/packages/app/lib/streaming/fleet-stream-frames.ts` | EDIT | `tool_call_*` frames fold onto their event (`FleetToolCall`). |
| `ui/packages/app/lib/streaming/fleet-stream-frames.test.ts` | EDIT | Seven tool-frame tests. |
| `ui/packages/app/components/domain/fleetMessageRenderers.tsx` | EDIT | Tool calls render above assistant text; the file was already over the length cap on main, so the block lands in its own module. |
| `ui/packages/app/components/domain/FleetToolCalls.tsx` | CREATE | `ToolCalls` + `readTools` — the tool-call presentation, split per RULE FLL. |
| `ui/packages/app/components/domain/useFleetEventStream.ts` | EDIT | Tools ride the custom metadata bag. |
| `ui/packages/app/components/domain/EventsList.tsx` | EDIT | `EventCost` — tokens + wall time per row. |
| `ui/packages/app/tests/api-keys-create-dialog.test.ts` | EDIT | Selectors follow the primitive; failure asserted. |
| `ui/packages/app/tests/runners-create-dialog.test.ts` | EDIT | Same. |
| `ui/packages/app/tests/dashboard-fleets-list.test.ts` | EDIT | `rowOf` helper — row is the container, link is the affordance. |
| `ui/packages/app/tests/guided-trigger-card.test.tsx` | EDIT | Captured-node assertions (accessible name flips on copy). |
| `ui/packages/app/tests/secrets-list.test.ts` | EDIT | Lucide mock gains the copy icons. |
| `ui/packages/app/tests/events-components.test.ts` | EDIT | Cost-line render tests. |
| `ui/packages/app/lib/api/model_library.test.ts` | EDIT | Missing mock reset — an order-dependent flake the mandated shuffled run exposed. |
| `ui/packages/app/tests/dashboard-workspace.test.ts` | EDIT | Sync query racing a mount — same shuffle-run find, made async. |
| `ui/packages/app/lib/utils.ts` | EDIT | Review-driven: `formatMs` — the tool-call row and the event cost line had each grown an identical private ms→display copy in one branch. One home, so the next tweak cannot drift them apart (RULE UFS). |
| `ui/packages/app/tests/fleet-tool-calls.test.tsx` | CREATE | Review-driven: `readTools` narrows an untyped metadata bag crossing the assistant-ui boundary — a malformed entry must drop, never crash the thread. Plus the `ToolCalls` render states. |
| `ui/packages/app/tests/dashboard-overview.test.ts` | EDIT | Pins §8: Installing and Killed tiles appear only when they have fleets to report — the bug was that those fleets appeared in no tile at all. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **NDC** (shadow `CopyButton`, `RevealPanel`, bespoke clipboard code all deleted, not stranded), **UFS** (labels/reset-window are named constants; `COPY_RESET_MS` exported so tests pin the real window, not a re-spelled literal), **ORP** (deleted symbols swept — see Dead Code Sweep), **NLR** (stale swallow-the-failure test rewritten on touch), **TSC/TSJ** (every `.ts`/`.tsx` touched), **TVR** (rollup tests cover unknown statuses — a reachable value once the backend ships ahead).
- **`dispatch/write_ts_adhere_bun.md`** — UI Substitution: every copy affordance is the design-system primitive; no raw interactive HTML inside anchors (the FleetsList overlay restructure exists precisely for this).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no — no `*.zig` in this workstream | — |
| PUB / Struct-Shape | no | — |
| File & Function Length (≤350/≤50/≤70) | yes | All touched files under caps; largest (`fleetMessageRenderers.tsx`) verified by the S8 rubric row. |
| UFS (repeated/semantic literals) | yes | Copy labels and the reset window are named constants; `data-slot="copy-button"` is the primitive's own stable hook. |
| UI Substitution / DESIGN TOKEN | yes | Primitive-only clipboard; token utilities throughout; MS-ID/UI audit clean (comment reworded rather than carved out). |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | No logging, lifecycle, error-code, or schema surface touched. |

## Prior-Art / Reference Implementations

- **Reference:** `ui/packages/design-system/src/design-system/CopyButton.tsx` — existed, tested, unconsumed; this workstream is adoption, not invention.
- **Reference:** the wake-pulse `PULSE_CAP` pattern (`FleetsList.tsx`) for honest capped affordances; the grep-gates' own comments for the token-prop rule the restructure satisfies.

## Sections (implementation slices)

### §1 — One clipboard, thirteen surfaces

The design-system `CopyButton` becomes the only clipboard implementation. The primitive gains what the highest-stakes sites need: a failed write is REPORTED (outcome state + polite live region) and reverts after `COPY_RESET_MS`; `showLabel` renders the label beside the icon where copying is the page's action. The shadowing local component in `cli-auth-ui.tsx` — the likely reason the primitive was never found — is deleted.

- **Dimension 1.1** (DONE) — a failed clipboard write reports and reverts; never the success flash → Tests `reports a failed clipboard write…`, `announces the failure in a live region…`, `reverts from failed to idle…` (CopyButton.test.tsx)
- **Dimension 1.2** (DONE) — thirteen call sites adopt the primitive; zero `navigator.clipboard` outside it in the app → Rubric R1 grep
- **Dimension 1.3** (DONE) — per-button outcome independence (no shared copied-key coupling) → GuidedTriggerCard tests
- **Dimension 1.4** (DONE) — the fleets-list row keeps whole-row navigation with a non-nested copy affordance → `dashboard-fleets-list` rowOf tests

### §2 — The gates run, and the code obeys them

`tests/*.test.ts` matched one directory level; `tests/grep-gates/**` never ran in CI, and the token-prop gate was red on `main` (pre-existing — proved against the committed file). The glob becomes recursive, and per Indy's call the code is restructured rather than allowlisted: the reveal panel inlines where `created` lives, so no component ever receives the runner token as a prop.

- **Dimension 2.1** (DONE) — `bun run test` matches `tests/**`; both grep-gate files execute in the unit lane → suite file-count (149→150 files incl. gates)
- **Dimension 2.2** (DONE) — no token-typed prop in any client file; gate green with the restructure, no allowlist → grep-gates suite
- **Dimension 2.3** (DONE) — one-time secrets (runner token, API key) copy inline on the field with failure reporting → runners/api-keys dialog tests

### §3 — The dashboard rollup is total

`countFleets` seeds its buckets from `AGENTSFLEET_STATUS` itself and counts unknown statuses instead of discarding them, so the sum always reconciles with the fleet count. Installing and Killed get tiles when they have fleets to report.

- **Dimension 3.1** (DONE) — buckets + unknown sum to total; installing/killed counted → `fleet-rollup.test.ts`
- **Dimension 3.2** (DONE) — an unrecognised status is counted as unknown, never dropped → `fleet-rollup.test.ts`

### §4 — The thread shows the work, the rows show the cost

`tool_call_started/_progress/_completed` fold onto their event keyed by `(event_id, name)` and render above the assistant text with elapsed/final wall time. `EventCost` renders tokens + wall time on every event row that carries them.

- **Dimension 4.1** (DONE) — tool frames attach, update in place, complete; a frame for an absent event is dropped (never synthesizes a message the backfill would duplicate) → frame tests
- **Dimension 4.2** (DONE) — a second call to the same tool opens a new entry; a timing-less completion keeps reported elapsed → frame tests
- **Dimension 4.3** (DONE) — event rows render `N tok` / wall time; rows without either render no cost line → `events-components` tests

## Interfaces

```
No wire interface changes. Consumed as-is:
  EventRow.tokens: number|null, EventRow.wall_ms: number|null   (already served)
  FRAME_KIND.TOOL_CALL_STARTED|_PROGRESS|_COMPLETED             (already published)
  AGENTSFLEET_STATUS registry                                   (already defined)
CopyButton public props: { value, label, showLabel?, className? } + exported COPY_RESET_MS.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Clipboard write rejected | Permissions / insecure context | Outcome `failed`: ✗ icon, accessible name "Copy failed — select the value and copy it manually", live-region announcement; reverts after `COPY_RESET_MS`. Never the success flash. |
| Unknown fleet status | Backend ships a status ahead of the client | Counted in `unknown`; totals still reconcile; no tile silently drops it. |
| Tool frame before its event | Out-of-order delivery | Dropped; `event_received` always precedes on the wire; synthesizing would duplicate against backfill. |
| Copy inside the row link | Nested interactive content | Link is a full-bleed overlay; copy sits above it (`z-10`); row stays fully clickable. |
| Cost fields absent | Older rows / non-run events | No cost line rendered — absence, not placeholders. |

## Invariants

1. **One clipboard implementation.** `navigator.clipboard` appears in exactly one app-consumable component — enforced by rubric grep R1 (and the design-system's own `Terminal` internal use, excluded by path).
2. **Rollup totality.** `sum(byStatus) + unknown === total` — enforced by construction (buckets seeded from the registry) and pinned by test.
3. **No token-typed prop in client files** — enforced by the now-running grep-gate.
4. **Reset window single-sourced** — `COPY_RESET_MS` exported; tests import it (RULE UFS).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | copied VALUES never leave the client; no analytics added to copy actions by design (identifiers/secrets would ride the event) | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | CopyButton failure trio | rejected write → `data-outcome="failed"`, live-region text, revert after `COPY_RESET_MS`; success flash never shown. |
| 1.2 | unit | R1 grep (rubric) | 0 `navigator.clipboard` hits in `ui/packages/app` outside tests. |
| 1.3 | unit | guided-trigger independence | copying URL leaves command button idle; each reverts on its own timer. |
| 1.4 | unit | `rowOf` fleets-list suite | row carries `data-state`; link navigates; copy button excluded from action set by `data-slot`. |
| 2.1 | unit | grep-gates execute | both gate files run in `bun run test`; 8 assertions green. |
| 2.2 | unit | token-prop gate | no `token[?:=]`-shaped prop/declaration in any `"use client"` file. |
| 2.3 | unit | runners/api-keys dialogs | copy fires with the token/key value; failure path shows the failed affordance. |
| 3.1–3.2 | unit | `fleet-rollup.test.ts` | totality incl. installing/killed; `hibernating` → unknown=1, sum reconciles. |
| 4.1–4.2 | unit | frame tests | started→progress→completed folding; absent-event drop; re-call opens new entry; null-ms completion keeps 5 000 ms. |
| 4.3 | unit | events cost line | `{tokens:12480, wall_ms:3200}` → "12,480 tok"+"3.2s"; `840` → "840ms"; both null → no line. |
| regression | unit | full app suite | 150 files / 1420+ green — no surface regressed by adoption. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | One clipboard implementation (§1) | `git grep -n "navigator.clipboard" -- ui/packages/app \| grep -v "/tests/" \| wc -l` | `0` | P0 | ✅ 0 |
| R2 | Grep-gates run and pass (§2) | `cd ui/packages/app && bunx vitest run tests/grep-gates/` | exit 0, 2 files | P0 | ✅ 2 files, 8 tests green |
| R3 | Rollup totality (§3) + frames (§4) | `cd ui/packages/app && bunx vitest run lib/fleet-rollup.test.ts lib/streaming/fleet-stream-frames.test.ts` | exit 0 | P0 | ✅ 27 passed (rollup + frames) |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ test-unit-all exit 0 — app 1443 shuffled · design-system 461 · coverage 100% lines / 100% branches (app) |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | ✅ ✓ All lint checks passed |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ no leaks found |
| S8 | No source file newly over the length cap | `git diff --name-only origin/main \| grep -vE '\.md$\|_test\.zig$\|\.test\.(ts\|tsx)$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | only `Shell.tsx` (459 on main), `lib/types.ts` (356 on main), `fleetMessageRenderers.tsx` (363 on main, zero growth here) — all over the cap before this branch; splits tracked in M131/M132 | P0 | ✅ only the three named pre-existing files |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | ✅ scoped greps 0 |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted (components deleted within files).

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| local `CopyButton` (cli-auth shadow) | `git grep -n "CopyButton" -- "ui/packages/app/app/cli-auth"` | only the design-system import |
| runner `RevealPanel` | `git grep -nw "RevealPanel" -- "ui/packages/app/app/(dashboard)/admin/runners"` | 0 matches (the api-keys dialog keeps its own non-token `RevealPanel` by design) |
| `COPY_RESET_MS` (GuidedTriggerCard local) | `git grep -n "COPY_RESET_MS" -- ui/packages/app` | design-system import sites only |

## Out of Scope

- Analytics on copy actions — copied values are identifiers and secrets; instrumenting them is a privacy decision, not a default.
- The workspace-multiplexed stream and per-fleet spend rendering on list rows (M132/M133 per the frozen design; `budget_used_nanos` stays unrendered until the Wall).
- Interrupting a working fleet (composer stays disabled mid-run — backend capability).

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator creates a runner, the clipboard write silently fails in an insecure context, and the button says so — they select-and-copy manually instead of closing the dialog with an empty clipboard and a dead token.
2. **Preserved user behaviour** — Every existing copy flow keeps working; every dialog, list, and thread renders as before plus the surfaced facts. No route, no wire call changes.
3. **Optimal-way check** — Direct: adopt the primitive that already existed, fix the glob, restructure one dialog. No new abstraction anywhere.
4. **Rebuild-vs-iterate** — Iterate; the primitive was right, adoption was missing.
5. **What we build** — Hardened `CopyButton` + 13 adoptions; recursive test glob; token-prop restructure; total rollup; tool-frame rendering; per-event cost line.
6. **What we do NOT build** — Copy analytics; workspace stream; spend on list rows; a prefs surface. All named for M131–M133 or rejected.
7. **Fit with existing features** — Compounds with M130_001 (same admin surfaces gain honest affordances); must not destabilize the steer thread — frame tests pin the existing chunk/complete behaviour untouched.
8. **Surface order** — UI-only by definition; CLI/API untouched.
9. **Dashboard restraint** — Installing/Killed tiles appear only with non-zero counts; no new controls, no quality claims — counters over facts already on the wire.
10. **Confused-user next step** — A failed copy names its own remedy in the accessible label ("select the value and copy it manually").

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Four Sections by failure class — clipboard, gates, rollup, stream/cost. Split from M130_001 at CHORE(close) because the folded scope pushed one spec past the 320-line bound; the template's own remedy is a split, and Indy's constraint was one PR, not one file.
- **Alternatives considered:** (a) allowlist the token-prop gate hit instead of restructuring — rejected by Indy explicitly; (b) keep everything in M130_001 over the length bound — rejected: the bound exists so a spec stays one coherent rulebook.
- **Patch-vs-refactor verdict:** **patch** — every change adopts an existing pattern (the primitive, the registry, the frame reducer's own switch).

## Discovery (consult log)

- **Consults** — Scope folded by Indy in-session: > Indy (2026-07-13): "All of it in M130" — the 13-surface clipboard adoption. > Indy (2026-07-13): "Well i want all the tests to be tests/*.test.ts tests/*.test.tsx lib/*.test.ts lib/**/*.test.ts to be fixed and the restructure of the code so runner token is never pased in as prod." — glob fix AND restructure over allowlist. > Indy (2026-07-14): "I know you are on the secretes, I want you to fold this fix in this PR in the punchlist" — the four dashboard/thread/cost fixes. Split into this workstream recorded at CHORE(close); same branch, same PR.
- **Metrics review** — no events added; copy actions deliberately uninstrumented (values are secrets/identifiers).
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs`: recorded at close.
- **Deferrals** — none.
