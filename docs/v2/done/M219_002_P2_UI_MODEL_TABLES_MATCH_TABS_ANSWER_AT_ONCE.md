<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M219_002: The two model tables read alike, fleet tabs answer at once, invite actions line up, and memory access reads as switches

**Prototype:** v2.0.0
**Milestone:** M219
**Workstream:** 002
**Date:** Oct 09, 2026
**Status:** DONE
**Priority:** P2 — dashboard polish Indy reported while eyeballing the branch; no boundary or money path changes
**Categories:** UI
**Batch:** B1 — the Sections touch different screens and ship together
**Branch:** docs/event-runtime-positioning
**Folded-into:** `M219_001`
**Baseline revision:** 2bf456efdf7be559b6c67a2e254dc92600d257b2
**Test Baseline:** unit=4485 integration=899 — inherited from `M219_001`: one branch, measured once at `2bf456efd`
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M219_001-2bf456efd.md`
**Depends on:** none. Ships in the `docs/event-runtime-positioning` Pull Request (PR) beside `M219_001`, as Indy chose for scope arriving on this branch.
**Provenance:** LLM-drafted (Claude Opus 5.5, Oct 09, 2026) from Indy's four screenshots of the dev dashboard on Oct 09, 2026.
**Canonical architecture:** `docs/architecture/billing_and_provider_keys.md` (the model library), `ui/packages/design-system` (table and tab primitives)

---

## Overview

**Goal (testable):** A model row reads the same in the admin Model library and the workspace Models table, by a human name with its provider id on hover; a clicked fleet tab is marked pending before the server answers; invite actions sit right-aligned like every other table's, a secret's row icons line up whatever its name's length; a fleet's shared-memory grants read as two named switches; and every npm package runs its latest release.
**Problem:** The Model column shows raw provider ids (`claude-fable-5`). The workspace Models table prints context as `1049k tokens` and rates as `$0.15 in · $0.03 cached · $0.50 out` while the library prints `1,048,576` and `0.15 / 0.03 / 0.50`. Fleet tabs are `?view=` queries on one server-rendered page, so a click shows nothing until the fleet and the view's data come back. The Members invite row puts "Email sent" text and a bordered icon inside the actions cell.
**Solution summary:** One display module formats a model's name, context and rates for both tables. Each fleet tab link shows its own pending state while the view loads. Invite email status moves to the Time column and the actions become a plain icon row. The memory grants become a design-system `Switch` each, named and described, and the four packages move to their latest releases.

## PR Intent & comprehension handshake

- **PR title (eventual):** shared with `M219_001`.
- **Intent (one sentence):** The dashboard reads one way for one thing — a model, a tab, a row's actions — wherever it appears.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` — the library's columns; its context and rate formats are the ones both tables keep.
2. `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryCells.tsx` — the workspace table's cells and its own `formatContext` and `formatRates`, which §1 replaces.
3. `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/page.tsx` — one server render per tab; §2 leaves it as it is and marks the clicked tab.
4. `ui/packages/app/app/(dashboard)/settings/members/components/TeamTable.tsx` and `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/SecretsList.tsx` — the invite actions and the row actions they should match.
5. `.orly/dispatch/write_ts_adhere_bun.md` — design-system primitives and token utilities only.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/packages/app/lib/models/display.ts`, `ui/packages/app/lib/models/display.test.ts` | CREATE | §1: a model's name, context and rates, formatted once |
| `ui/packages/app/app/(dashboard)/admin/models/components/CatalogueList.tsx` | EDIT | §1: the library reads through the display module |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/{ModelsRegistryCells.tsx,ModelsRegistryTable.tsx,registry-view.ts,ModelDetailsDialog.tsx}` | EDIT | §1: the workspace table and its details dialog read through it too |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/settings/models/components/ProviderModelSelect.tsx`, `ui/packages/app/tests/{provider-model-select.test.tsx,helpers/models-component-mocks.tsx}` | EDIT | §1: the picker names a model as the tables do, the id on its own line, the closed value one truncating line |
| `ui/packages/app/tests/{models-registry-table.test.tsx,models-registry-edit-remove.test.tsx,models-registry-cells.test.tsx,admin-models-ui.test.ts,admin-models-management.test.ts}` | EDIT | §1: rows found by the id's hover; formats follow the shared module |
| `ui/packages/app/tests/timestamp-standard.test.ts` | EDIT | §1: the model library no longer calls a locale formatter, so its exemption goes |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/{FleetSubnavigation.tsx,FleetSubnavigation.test.tsx,FleetTabLink.tsx,FleetTabLink.test.tsx}` | EDIT, CREATE | §2: a pending tab |
| `ui/packages/app/tests/{helpers/dashboard-mocks.tsx,fleets-routes/detail-views.test.ts}` | EDIT | §2: the link mock answers `useLinkStatus`; the route test proves the panel paints with its tabs |
| `ui/packages/app/app/(dashboard)/settings/members/components/{TeamTable.tsx,MembersView.test.tsx}`, `ui/packages/app/tests/{helpers/members-fixtures.tsx,e2e/acceptance/team-members.spec.ts}` | EDIT | §3: email status in the Time column as "Invite emailed"; plain icon actions |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/secrets/components/{secret-row-cells.tsx,SecretsList.test.tsx}` | EDIT | §3: the name sits in a fixed slot that ellipsizes, its icons after it |
| `ui/packages/design-system/src/design-system/{Switch.tsx,Switch.test.tsx,index.ts,DataTable.types.ts,DataTableModel.ts,DataTableModel.test.tsx}`, `ui/packages/design-system/src/{index.ts,index.test.ts}` | CREATE, EDIT | §5: a `Switch` primitive over `@radix-ui/react-switch`; §1: a `DataTable` column may bring its own `compare` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/{MemoryPanel.tsx,MemoryPanel.test.tsx}` | EDIT | §5: each grant a named switch with its description |
| `package.json`, `bun.lock`, `cli/{package.json,bun.lock}`, `ui/packages/{app,design-system,website}/package.json` | EDIT | §6: every package on its latest release; §5 adds `@radix-ui/react-switch` |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/{FleetTile.tsx,FleetTile.test.tsx}` | EDIT | §7: agent and status first, the live line second, the fleet name in the footer |
| `ui/packages/app/components/domain/fleet-library/LibrarySourceTabs.tsx` | EDIT | §6: a tab click reports one source change on Radix tabs 1.1.22 |
| `scripts/toolbox/manifest.txt` | EDIT | §6: the toolbox snapshot moves to one serving Debian 13.7 |
| `docs/v2/pending/M220_001_P2_DOCS_INFRA_KERNEL_VM_RUNS_DEBIAN_TRIXIE.md` | CREATE | §6: the deferred kernel-lane machine and doc drifts, as their own spec on this branch |
| `ui/packages/app/AGENTS.md` | EDIT | §6: Next 16.4 rewrites its managed `nextjs-agent-rules` block |
| `docs/v2/{active,done}/M219_002_P2_UI_MODEL_TABLES_MATCH_TABS_ANSWER_AT_ONCE.md` | CREATE | This spec |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — UFS: column headers and formats are named constants in the display module, and ui/ literals repeated in a file become constants by hand. NDC: the workspace table's own formatters are deleted, not left beside the shared ones. NLR: stale comments about the old formats go in the same commit. TST-NAM: test names describe behaviour.
- `.orly/dispatch/write_ts_adhere_bun.md` — UI GATE (design-system primitives: `DataTable`, `IconAction`, `TabNav`, `Tooltip`) and DESIGN TOKEN GATE (token utilities, no arbitrary values).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI / DESIGN TOKEN | Yes | Only design-system components and token classes; no raw `<button>` or `*-[...]` |
| File & Function Length (≤350/≤50/≤70) | Yes | `ModelsRegistryCells.tsx` shrinks; the memory rows are their own component |
| UFS | Yes | ui/ manual pass: repeated literals become constants |
| MILESTONE-ID | Yes | No milestone identifiers in code or test names |
| SPEC TEMPLATE | Yes | `bash .orly/audits/spec-template.sh --staged` on every spec edit |

## Prior-Art / Reference Implementations

- **Formats:** the library's own cells in `CatalogueList.tsx` — pinned `en-US` grouping and two-decimal rates.
- **Row actions:** `SecretsList.tsx` — ghost `IconAction`s in a right-aligned row, destructive last.
- **Pending navigation:** Next.js `useLinkStatus` (`next/link`), which marks the one link whose navigation is in flight.
- **Switch:** `RadioGroup.tsx` in the design system — a Radix primitive composed with token classes and a client boundary.

## Sections (implementation slices)

### §1 — A model reads the same in both tables — DONE

`display.ts` turns a provider id into the name the provider markets (`claude-fable-5` → `Fable 5`, `accounts/fireworks/models/glm-5p3-flash` → `GLM 5.3 Flash`), formats context with pinned `en-US` grouping, and prints rates as `in / cached / out` at two decimals. Both tables render the name with the exact id on hover, sort by the name and then the id (`compareModelIds`), and keep accessible labels on the id. **Implementation default:** derive the name from the id rather than add a column, because the catalogue has no display field and the id stays one hover away.

- **Dimension 1.1** — DONE — Provider ids read as marketed names → Test `names a provider id the way its provider markets the model` (`ui/packages/app/lib/models/display.test.ts`)
- **Dimension 1.2** — DONE — Both tables print one row's context and rates identically → Test `formats context and rates the way the model library prints them` (`ui/packages/app/lib/models/display.test.ts`)
- **Dimension 1.3** — DONE — The exact id stays on the row → Test `the model cell shows the name and keeps the id on hover` (`ui/packages/app/tests/admin-models-ui.test.ts`)

### §2 — A clicked fleet tab answers at once — DONE

The tab the user clicked shows its pending state through `useLinkStatus` while its navigation is in flight, and the view replaces the last one in a single paint. A panel skeleton was built and measured: React keeps a shown fallback up for at least 300 ms (`FALLBACK_THROTTLE_MS`, `react-dom` 19.3.0), which made Chat land 210 ms after its own reads, so the panel renders with its tabs. **Implementation default:** keep the views as `?view=` queries; the pulse answers the click and routes would change every link and test.

- **Dimension 2.1** — DONE — A clicked tab is marked pending before the server answers → Test `a clicked fleet tab reads as loading while its view is on the way` (`FleetTabLink.test.tsx`)
- **Dimension 2.2** — DONE — A view paints in the same pass as its tabs, with no skeleton between → Test `renders fleet-local navigation with Chat as the focused default` (`tests/fleets-routes/detail-views.test.ts`)
- **Dimension 2.3** — DONE — Click to first paint, measured on the running app before and after → Test `fleet_tab_paints_before_its_data` (manual) (Discovery: first change 375–674 ms → 5–11 ms)

### §3 — Row icons line up — DONE

The invite email status moves into the Time column under the dates, reading "Invite emailed", "Email not sent" or "Email not set up" (the last keeps its tooltip). The actions cell becomes resend, copy and revoke as right-aligned ghost icons, destructive last, as Secrets does. A secret's name sits in a `max-w-trim` slot that ellipsizes past it, with the full name on hover; its copy and rename icons follow the slot, so they line up on every row.

- **Dimension 3.1** — DONE — An invite row's actions are icons only → Test `an invite row's actions are icons only, right-aligned` (`MembersView.test.tsx`)
- **Dimension 3.2** — DONE — The Time column says whether the invite email went → Test `the time column says whether the invite email went` (`MembersView.test.tsx`)
- **Dimension 3.3** — DONE — A long secret name ellipsizes and its icons stay in line → Test `a long secret name ellipsizes and keeps its icons in line` (`SecretsList.test.tsx`)

### §4 — Indy eyeballs the dashboard — DONE

The app runs on `http://localhost:3000` from `AGENTSFLEET_UI_ENV_FILE`; the browse tool captures each changed screen for the PR, and Indy reviews the running app.

- **Dimension 4.1** — DONE — Indy accepts the four screens on the running app → Test `indy_eyeballs_the_changed_screens` (manual) (Discovery)

### §5 — Shared-memory access reads as two named switches — DONE

The Memory tab's two grants become a "Shared memory" fieldset of two `DashboardRow`s, each a label, a one-line description and a `Switch` on the right. **Use shared memory** — "See what other fleets in this workspace have shared." — is the read grant. **Share this fleet's memory** — "Let this fleet share what it learns with other fleets in this workspace." — is the publish grant. `Switch` is a new design-system primitive over `@radix-ui/react-switch`, as `RadioGroup` wraps Radix. A flip calls `setMemoryAccessAction` as today; a refusal leaves the switch where it was and shows the warning. **Implementation default:** the names Indy picked from three proposals.

- **Dimension 5.1** — DONE — The switch states its value and flips → Test `a switch announces its state and flips on click` (`ui/packages/design-system/src/design-system/Switch.test.tsx`)
- **Dimension 5.2** — DONE — Each grant is a named switch with what it does → Test `each shared-memory grant is a named switch with what it does` (`MemoryPanel.test.tsx`)
- **Dimension 5.3** — DONE — A refused flip leaves the switch unchanged and says why → Test `a refused shared-memory change leaves the switch where it was` (`MemoryPanel.test.tsx`)

### §6 — Packages run their latest releases — DONE

Every npm dependency in `cli`, `ui/packages/app`, `ui/packages/design-system` and `ui/packages/website` moves to its latest release, keeping its pin style, and the root `playwright-core` override follows Playwright to 1.64.0. `typescript-jsapi` stays on TypeScript 6, the alias Indy accepted on Jul 30, 2026 (`M151_001`). Radix tabs 1.1.22 focuses a trigger on mousedown, so one click selects its tab twice before a re-render; `LibrarySourceTabs` now reports only a real change of source.

The toolbox image moves to Debian 13.7. Its snapshot pin goes from `20260901T000000Z`, which serves 13.6, to `20261010T000000Z`. That snapshot serves 13.7 with security updates through Oct 09, 2026. The image hash changes; no code does.

- **Dimension 6.1** — DONE — No package reports an update → Test `bun_outdated_reports_only_the_jsapi_alias` (package audit)
- **Dimension 6.2** — DONE — One click on a source tab is one source change → Test `tells the caller when the operator changes source` (`ui/packages/app/components/domain/fleet-library/LibrarySourceTabs.test.tsx`)
- **Dimension 6.3** — DONE — The toolbox builds from Debian 13.7 and records the snapshot it used → Test `test_toolbox_carries_the_tools`, whose first step is `release_manifest_names_the_image` (kernel lane, `rustd/crates/afr_sandbox/examples/kernel_lane/toolbox.rs`)

### §7 — A fleet card leads with its agent and what it is doing — DONE

The card's first line is the agent label with its status beside it, and the second is the live line, "Waiting for the next event." or the latest activity, in body text. The fixed sentence "Runs in a loop: wakes on events, gathers evidence." goes. The fleet's own name, the `name:` its bundle declares, moves to the footer beside "Manage fleet". **Implementation default:** the layout Indy picked from three mockups.

- **Dimension 7.1** — DONE — Agent and status lead, the activity follows, the name sits in the footer → Test `leads with the agent and its status, then what it is doing, and keeps the fleet's name in the footer` (`FleetTile.test.tsx`)

## Interfaces

```
ui/packages/app/lib/models/display.ts
  modelLabel(modelId: string): string
  formatContextTokens(tokens: number | undefined): string
  formatRatesPerMtok(rates: RateFields | null): string
```

No API, route or wire change.

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Unrecognised id shape | A provider id the name rules do not cover | `modelLabel` returns the id's last path segment unchanged; the hover still shows the full id |
| Missing context or rates | A row with no catalogue match | The cell prints the existing empty marker, as today |
| Slow view data | The view's API read is slow or fails | The clicked tab pulses and the last view stays until the new one resolves or its error boundary renders |

## Invariants

1. One formatter per value — both tables import `display.ts`; R1 counts no second `formatRates` definition under `ui/packages/app/app`.
2. Both tables sort one way — through `compareModelIds`, the shown name under a numeric `en-US` collation and then the id, which the workspace table calls directly and the library's `DataTable` column passes as its `compare` (`a name sorts before a longer one it starts, and a number inside a name sorts as a number`); accessible labels read `model_id`.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| No product or operator signal changes | not applicable | never | none | Existing tab and table events keep their names | `renders fleet-local navigation with Chat as the focused default` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `names a provider id the way its provider markets the model` | `claude-fable-5` → `Fable 5`; `claude-opus-5-5` → `Opus 5.5`; `accounts/fireworks/models/glm-5p3-flash` → `GLM 5.3 Flash`; `gpt-6.1-sol` → `GPT-6.1 Sol`; an unknown shape → its last segment |
| 1.2 | unit | `formats context and rates the way the model library prints them` | `1048576` → `1,048,576`; rates in nanos → `0.15 / 0.03 / 0.50` |
| 1.3 | unit | `the model cell shows the name and keeps the id on hover` | Row `claude-fable-5` → text `Fable 5`, hover title `claude-fable-5` |
| 2.1 | unit | `a clicked fleet tab reads as loading while its view is on the way` | Link status pending → the tab carries the pending marker |
| 2.2 | unit | `renders fleet-local navigation with Chat as the focused default` | Default view → the tabs and the `Fleet summary` panel in one synchronous render |
| 2.3 | manual | `fleet_tab_paints_before_its_data` | Click to first paint on `localhost:3000`, before and after, recorded in the PR Session Notes |
| 3.1 | unit | `an invite row's actions are icons only, right-aligned` | Invite row → no status text in the actions cell; resend, copy, revoke as icons |
| 3.2 | unit | `the time column says whether the invite email went` | Sent → `Invite emailed`; failed → `Email not sent` |
| 3.3 | unit | `a long secret name ellipsizes and keeps its icons in line` | A 60-character name → the slot truncates with the full name as its title; copy and rename sit in the same row |
| 4.1 | manual | `indy_eyeballs_the_changed_screens` | Indy reviews the running app; his reply is recorded in Discovery |
| 5.1 | unit | `a switch announces its state and flips on click` | Off switch → `role="switch"`, `aria-checked="false"`; click → `onCheckedChange(true)`, `aria-checked="true"`; click → back to false |
| 5.2 | unit | `each shared-memory grant is a named switch with what it does` | Access `{read: true, publish: false}` → switch "Use shared memory" checked, "Share this fleet's memory" unchecked, each described by its line |
| 5.3 | unit | `a refused shared-memory change leaves the switch where it was` | Action refused with 403 → the switch stays unchecked and enabled, and a warning alert shows |
| 6.1 | package audit | `bun_outdated_reports_only_the_jsapi_alias` | `bun outdated` in the four packages → only `typescript` (the `typescript-jsapi` alias) |
| 6.2 | unit | `tells the caller when the operator changes source` | Click Upload, then GitHub → `onSourceChange` called exactly twice |
| 6.3 | kernel lane | `test_toolbox_carries_the_tools` | `make test-runner-kernel` builds from `manifest.txt` → the release records `snapshot` `20261010T000000Z`; that snapshot's trixie `Release` → `Version: 13.7` |
| 7.1 | unit | `leads with the agent and its status, then what it is doing, and keeps the fleet's name in the footer` | Live fleet `alpha` → first line the agent label and `Active`, then `Waiting for the next event.`, footer `alpha`; no fixed sentence |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | One rate formatter in the app (§1) | `git grep -n 'function formatRates' -- ui/packages/app` | one line, in `lib/models/display.ts` | P1 | ✅ `lib/models/display.ts:129:export function formatRatesPerMtok` |
| R2 | A clicked fleet tab shows its pending pulse (§2; amended after Indy chose "Pulse only", which removed the skeleton boundary) | `git grep -c 'useLinkStatus' -- 'ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetTabLink.tsx'` | `1` or more | P1 | ✅ `FleetTabLink.tsx:3` |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | see PR Session Notes, `orly gate pr` |
| S2 | Lint passes | `make lint-all` | exit 0 | P0 | see PR Session Notes, `orly gate pr` |
| S3 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | see PR Session Notes, `orly gate pr` |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | see PR Session Notes, `orly gate pr` |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | see PR Session Notes, `orly gate pr` |

## Dead Code Sweep

`formatContext` and `formatRates` in `ModelsRegistryCells.tsx` are deleted; R1 proves no copy remains.

## Out of Scope

- A display-name column in `core.model_library`; the name derives from the id.
- Separate routes per fleet tab.
- Any change to what the tables list or to the model library's data.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An admin reads `Fable 5` in either table and sees `claude-fable-5` on hover; a fleet tab responds the moment it is clicked.
2. **Preserved user behaviour** — Sorting, editing, defaults, resend, copy and revoke work as today.
3. **Optimal-way check** — One formatter beats two tables keeping two formats in step.
4. **Rebuild-vs-iterate** — Iterate; the tables and tabs exist and only their presentation changes.
5. **What we build** — A display module, a pending tab, a streamed panel, a rearranged invite row.
6. **What we do NOT build** — Catalogue display names, tab routes, new data.
7. **Fit with existing features** — Uses `DataTable`, `IconAction`, `TabNav` and the library's existing formats.
8. **Surface order** — Admin Model library, workspace Models, fleet tabs, Members.
9. **Dashboard restraint** — No new columns, badges or controls.
10. **Confused-user next step** — Hover a model or secret name for its full id; a pending tab shows its view is loading.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one shared module and three local presentation edits.
- **Alternatives considered:** a catalogue display-name column (rejected: a schema change for a presentation need); client-side tab switching with all views prefetched (rejected: fetches every view's data on every page open).
- **Patch-vs-refactor verdict:** this is a **patch** because each screen keeps its structure and data.

## Discovery (consult log)

- **Consults** — Indy, Oct 09, 2026, from four screenshots: the Model column should read `Fable 5` with `claude-fable-5` on hover; Events and Memory tabs take a while to load; the Models columns must match the Model library; Members icons should align like Secrets, and "Email sent" reads awkwardly. He asked to eyeball the result on `http://localhost:3000` run with `AGENTSFLEET_UI_ENV_FILE`. Then: "is it possible to align the copy clipboard icon, edit pencil" on Secrets, and how a long name should truncate.
- **Tab reads** — Indy, Oct 09, 2026, after clicking through §2: "it seems performant now", then asked whether a tab loads only what it needs. Each tab starts only its own reads (`view-data.ts`), but every click re-reads the fleet and tenant billing, because the header renders in `page.tsx`. A `fleets/[id]/layout.tsx` holding the header would drop the billing read from every click and the fleet read from Events, at the cost of a header status that refreshes only on reload. Indy chose "Measure first": Dimension 2.3's timing decides whether the header moves.
- **Tab timing** — Oct 09, 2026, `next dev` on `localhost:3000`, median of 5 clicks, ms. Before §2 every tab showed nothing until done: Events 390, Memory 375, Skill 385, Trigger 674, Chat 473. With the pulse and a skeleton: the tab answered in 5–11; done Events 368, Memory 374, Skill 385, Trigger 656, Chat 683. Every view lands no sooner than the fleet read (~370), which four of five tabs need, so the header stays in `page.tsx`. Indy chose "Pulse only": the skeleton goes, and Chat returns to its own read time.
- **Shared-memory switches** — Indy, Oct 09, 2026: "i think we must keep it simple and have it NAME PROPERLY PROPOSE FIRST, SO INDY APPROVES", with a screenshot of on/off switches. Three name sets were proposed; he picked "Use shared memory" / "Share this fleet's memory" and chose to ship it here as §5.
- **Packages** — Indy, Oct 09, 2026: "can we check and update all the packages you have (npm) to the latest in ui/packages/app, cli, design-system, website?", then asked for it in this PR (§6). The new releases are one to four days old; `posthog-js` 1.438.5 was hours old. On Oct 10, 2026 he wrote "if there is an update on posthot do so". `posthog-js` moved to 1.438.7, published Oct 09, 2026 21:02 UTC. The same day's `bun outdated` re-audit found patch releases since Oct 09. Those moved too: `@clerk/{nextjs,ui,testing}`, `uuid`, `@types/node` and `@vercel/detect-agent`.
- **Toolbox Debian** — Indy, Oct 10, 2026: "can that be changed to use the debian latest 13?", then "Debian 13.7". The pinned snapshot served 13.6. He chose "Bump snapshot in this PR" (§6).
- **Close-out (Oct 10, 2026)** — Indy accepted the screens ("Accepted (Recommended)"), and asked that the two known limits be recorded rather than fixed, docs be skipped under an override, and the branch be pushed and opened as a PR once green. For the fleet card he wrote "the agent slug name and status is important that must be the first line" and "Runs in loop static text doesnt have value, but rather the Waiting for the event is valuable", then picked "Footer name" (§7).
- **Metrics review** — No product event changes.
- **Skill-chain outcomes** — `/orly-write-unit-test` boundary audit (Oct 10, 2026): every Dimension maps to a test; TypeScript 100% under `make test-unit-all`. Dimension 6.3 ran on the `afr-kernel` kernel lane: 49 of 51 passed under full load, and the two failures passed alone (one left a cgroup behind, which was cleared). gstack `/review` covered this spec with M219_001 on the same branch; its design findings on the switch's touch target, focus while saving and the status gap were fixed in `70041d524`. The nested memory card and the pill-shaped switch are left for Indy. `orly-babysit-prs`: recorded in the PR.
- **Deferrals** — the `afr-kernel` VM rebuild on Debian 13, and three doc drifts: `docs/architecture/runner_execution.md:164`, `.github/workflows/test-integration-rustd.yml:131` and `rustd/Cargo.toml:344`. They go to their own spec, `M220_001` in `docs/v2/pending/` on this branch.
  > Indy (2026-10-10 10:00): "Bump snapshot in this PR" — context: the option he chose read "The VM and doc fixes go to their own spec."; then "all changes have to go in 1 branch/worktree".
