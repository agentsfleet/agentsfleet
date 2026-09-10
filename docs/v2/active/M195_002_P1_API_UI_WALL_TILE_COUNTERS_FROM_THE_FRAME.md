<!-- SPEC AUTHORING RULES — read docs/TEMPLATE.md before editing this file. -->

# M195_002: A tile's counters are what the database says

**Prototype:** v2.0.0
**Milestone:** M195
**Workstream:** 002
**Date:** Sep 10, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the wall reports fewer events than a fleet has processed until someone reloads. Nothing is lost or double-charged; what is wrong is the number an operator reads while deciding whether a fleet is stuck.
**Categories:** API, UI
**Batch:** B1 — one workstream; the frame that carries the snapshot and the client that renders it are one change.
**Branch:** `feat/m195-wall-tile-counters`
**Baseline revision:** `a4e0ef2bda8a62742a400be18f58588056786f8c`
**Test Baseline:** pending — measured before the Pull Request (declared `verify.unit` and `verify.integration` lanes)
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M194_001 — its §4 acceptance walk recorded this defect at step 6, and its Pull Request carries the partial work this spec restores. Runs in PARALLEL with M195_001 — the two declare no file in common — except that its §2 request inventory is taken after this spec lands, since carrying the counters on the frame removes the wall's separate counter fetch.
**Provenance:** agent-generated from M194_001's acceptance walk (step 6, recorded defect) and a code read of the counter triggers, the lease park and the frame publishers. One premise carried from an earlier session note did not survive the read — see Discovery.
**Canonical architecture:** `docs/architecture/web_app.md` §The two shapes

---

## Overview

**Goal (testable):** every frame a fleet publishes carries the counters as they stand in Postgres, and a tile that receives any one of them reads the same number a page reload would show.

**Problem:** an operator watching the wall sees a fleet's event count stop advancing while the fleet is plainly working. It resumes only on reload. The count is not merely late — it is short by exactly the events that parked awaiting a human, which is the case an operator most needs the wall to be honest about.

**Solution summary:** carry an absolute counter snapshot on every frame the fleet plane publishes — `hello`, the receive, the gate frames and `event_complete` — and have the client REPLACE its values rather than accumulate them. A snapshot is idempotent where a delta is not, so a dropped, duplicated or late frame cannot leave the tile wrong, and no code has to know which frame owns which increment.

## PR Intent & comprehension handshake

- **PR title (eventual):** carry fleet counters on every frame, and let the tile replace them
- **Intent (one sentence):** the number on a wall tile is the number in the database, without a reload.
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `schema/890_fleet_activity_counter_triggers.sql` — when each counter moves. `events_processed` bumps `AFTER INSERT ON core.fleet_events`, at receive; `budget_used_nanos` bumps on the ledger write. That split is the whole argument for this design.
2. `rustd/crates/afd_fleet/src/lease/pull.rs` — the park at `:209` returns `Step::Stop` without reaching `publish_completion` at `:329`, which is why the tile never settles.
3. `rustd/crates/afd_fleet/src/lease/admit/mod.rs` — `:56` documents `Admission::Await` as durably identical to a retry. Read it before believing any note that says a charge landed.
4. `rustd/crates/afd_wire/src/tail.rs` — `pending_approvals` already ships as an absolute snapshot on these frames. This spec extends that shape; it does not invent one.
5. `docs/architecture/web_app.md` — the wall's data path and the voice the architecture set is written in.
6. `dispatch/write_ts_adhere_bun.md` — the client half is TypeScript and React; the UI and design-token gates fire on it.
7. The Discovery section of this spec, BEFORE writing anything: partial work already exists off-branch, and one of its files is superseded rather than unfinished.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------------|-----|
| `rustd/crates/afd_wire/src/tail.rs` · `rustd/crates/afd_fleet/src/lease/{bracket,pull}.rs` | EDIT | The counter snapshot rides every frame the fleet publishes, not only `event_complete`. `hello` carries it so a subscriber is correct before its first event; the receive carries it because that is when `events_processed` moves. |
| `rustd/crates/afd_sse/src/frame.rs` · `rustd/crates/afd_api_tenant/src/handler/stream/wall.rs` · `rustd/crates/afd_http/src/services/fleets.rs` | EDIT | Dimension 1.2 requires these and the table omitted them, found at PLAN and added on the user's call. `hello` is built by `Frame::hello` and published from `wall.rs:88` and `:98`. Its counters do NOT ride `live_set`: that enumeration is cached ten seconds (`LiveSets`), and a counter that old is the staleness the frame exists to remove. A new `WorkspaceFleets::counters` reads them fresh by `ANY($1)`, only when a `hello` goes out — on connect and on a change to the set, never on a steady tick. |
| `rustd/crates/afd_fleet_lifecycle/src/{live_set.rs,sql.rs,sql/live_set.rs}` · `rustd/crates/afd_fleet_lifecycle/tests/integration_wall_counters.rs` | EDIT/CREATE | The set statements move to `sql/live_set.rs` (`sql.rs` was at its 350-line cap) beside the new `SELECT_FLEET_COUNTERS_FOR_SET`, driven from `core.fleets` so a fleet that never ran answers zeros. |
| `rustd/crates/afd_events/src/{counters.rs,closed.rs,sql.rs}` · `rustd/crates/afd_approval/src/inbox{.rs,/announce.rs,/sweep.rs}` · `rustd/crates/afd_gate/src/gate/park.rs` (+ `Cargo.toml`) · `rustd/crates/afd_fleet/tests/integration_wall_counters.rs` | EDIT/CREATE | The publishers. `Closed` widens its own closing statement; the receive, the continuation, the park, the resolve and the sweep call `afd_events::fleet_counters_best_effort` right before their publish. `afd_gate` gains an `afd_events` dependency (acyclic). |
| `rustd/crates/afd_fleet_lifecycle/src/sql.rs` · `rustd/crates/afd_fleet_lifecycle/src/sql/install.rs` | EDIT/CREATE | The page statements read the counters by primary key instead of joining; the `core.fleet_library` install lookups move to their own module to keep `sql.rs` under the 350-line cap. Already written — see Carry-over. |
| `ui/packages/app/lib/api/events.ts` · `ui/packages/app/lib/streaming/workspace-store.ts` · `ui/packages/app/components/domain/useWorkspaceStream.ts` | EDIT/CREATE | The client replaces counters from the frame. The snapshot fields land on the explicit `{fleet_status?; pending_approvals?}` block, never on `EventRow`, which carries per-event figures and not fleet totals. |
| `ui/.../fleets/components/{FleetTile,FleetWall,WallLiveBadge}.tsx` | EDIT/CREATE | The tile renders the snapshot it was handed. |
| `ui/packages/app/lib/wall/tile-counters.ts` | DELETE | The client-side delta this design supersedes. `absorb` and `countersStale` go with it — but only after whatever trims `#eventsByFleet` is re-homed, since `absorb` is its only caller today (`useWorkspaceStream.ts:93`). |
| `ui/packages/app/tests/e2e/acceptance/wall-live-counters.spec.ts` | EDIT | The cross-context no-refresh guard. It fails deliberately until this spec lands; do not weaken it. |
| `docs/v2/pending/M195_001_P1_DOCS_OBS_UI_DASHBOARD_LOAD_LATENCY_INVESTIGATION.md` | EDIT | Declared out-of-band, on the user's call: `d05c0ff81` committed the same two conflict markers into that spec, and its sequencing claim about this workstream is one of the two sides. Resolved to the side its own Files Changed table supports; no other line of that spec is touched. |
| `ui/packages/design-system/src/design-system/{NavItem,Button,DataTableView,Pagination,SectionHeader}.tsx` + their `*.test.tsx` | EDIT | Folded in on the user's call after a measured design review of every left-nav page and every rendered table (§4). The defects live in the primitives, so the fix is spelled once and holds on all eleven pages by construction. |
| `ui/packages/app/components/layout/SidebarNavigation.tsx` · `ui/packages/app/tests/app-shell-navigation.test.ts` | EDIT | The Platform group was the one group that broke the nav's rhythm — a 32px form control where the other three render a 16px eyebrow, plus a 2px item gap the others lack (§4.2). |
| `ui/packages/app/lib/fleets/{identity,agent-label}.ts` (+ tests) | CREATE | §5: the derivation's home, byte-identical, and the one place the agent label is composed. |
| `ui/packages/app/app/(dashboard)/w/[workspaceId]/fleets/components/fleetIdentity.ts` | DELETE | §5: moved to `lib/fleets/identity.ts`; RULE NDC, gone with its last importer rewritten. |
| `settings/billing/lib/charges.ts` · `components/domain/AgentLabel.tsx` · `fleets/[id]/page.tsx` · `fleets/[id]/components/{ChatView,FleetThread*}.tsx` · `tests/e2e/acceptance/{multi-fleet-grant-journey,wall-live-counters}.spec.ts` · `admin/runners/[runnerId]/components/LeaseTable.tsx` (+ test) | EDIT | §5: the six import rewrites, the prop renamed for what it carries, the lease row naming its agent. All under `ui/packages/app/`. |
| `fleets/[id]/components/RunMetricsStrip.tsx` (+ test) · `lib/events/event-summary.ts` (+ test) | EDIT | §6: the outcome vocabulary gains the sentence a processed-without-body run earns; the strip stops inferring absence from an unread field. |
| the `app/(dashboard)` sites the §7 classification table marks for change, plus `ui/packages/app/tests/button-variant-rule.test.ts` | EDIT/CREATE | §7: the sweep, after the table is confirmed; the test that keeps it swept. |

## Applicable Rules

- **RULE NDC** — `tile-counters.ts`, `absorb` and `countersStale` are deleted with their last caller, not left behind.
- **RULE NLR** — the delta machinery is removed in the same diff that replaces it, never retained beside it.
- **RULE UFS** — frame-kind and field names are named constants shared by the wire crate and the client, not repeated literals.
- **RULE TCF** — a snapshot test that cannot fail is not a test: the idempotency proof delivers the same frame twice and asserts the tile did not double.
- **RULE ERR-RS** — `docs/RUST_ERROR_STANDARD.md`: one error type per crate, `?` lifts, no `map_err` that stringifies its cause.
- `dispatch/write_ts_adhere_bun.md` — design-system primitives over raw HTML, token utilities over arbitrary values, for the tile and the badge.

## Applicable Gates

| Gate | Fires on | Satisfaction strategy |
|---|---|---|
| LENGTH | `sql.rs`, `workspace-store.ts`, `FleetWall.tsx` | `sql.rs` is at 350/350 after the install split; any further growth splits again before the edit. |
| UI GATE · DESIGN TOKEN | `FleetTile.tsx`, `FleetWall.tsx`, `WallLiveBadge.tsx` | Badge and tone tokens from the design system; no arbitrary colour values. |
| UFS | both surfaces | Frame kinds and field names as shared constants. |
| ERROR REGISTRY | `tail.rs` | No new `UZ-` code expected; if a frame refusal needs one it is declared and entered in the same commit. |
| CONFORM | every commit | `make harness-verify`. |

## Prior-Art / Reference Implementations

- **Codex** does exactly this — an absolute snapshot in the frame, client replaces: `history.rs:678`, `v2/thread.rs:1885`, `chatwidget/protocol.rs:39`.
- **This repository already does it** for `pending_approvals` in `rustd/crates/afd_wire/src/tail.rs`, three variants. This spec extends the shape it already ships rather than inventing one.
- **Reconnect backoff stays as it is.** `fleet-stream-reconnect.ts:9-14` deliberately uses a saturating cap (`OFFLINE_RETRY_MS = 30_000`) against a capacity-gated route; cycling was measured at ~2.76× traffic per open tab and rejected. Add jitter only.

## Sections (implementation slices)

### §1 — The snapshot rides every frame — DONE

- **Dimension 1.1** — DONE — every frame a fleet publishes carries an absolute counter snapshot, and the client REPLACES its values rather than accumulating; a frame delivered twice leaves the tile identical → Test `a_repeated_frame_leaves_the_tile_unchanged`
- **Dimension 1.2** — DONE — the `hello` frame carries the snapshot, so a subscriber is correct before its first event rather than after it, closing both the dropped-frame hole and the gap between server render and subscribe → Test `hello_carries_the_counters_a_late_subscriber_missed`
- **Dimension 1.3** — DONE — a receive publishes the snapshot, so an event that parks awaiting approval leaves the tile showing the event it counted → Test `a_parked_event_still_reports_its_event_count`
- **Dimension 1.4** — DONE — the park writes no ledger row and moves no budget, pinning that it is durably a retry and not a charge → Test `a_parked_event_moves_no_budget`
- **Dimension 1.5** — DONE — a frame lost in transport is corrected by the next one, because each carries the whole truth rather than a difference → Test `a_dropped_frame_is_corrected_by_the_next_snapshot`
- **Dimension 1.6** — DONE — a workspace past the per-instance stream cap degrades to a snapshot tile rather than showing a stale live one → Test `a_capped_stream_degrades_to_a_snapshot_tile`

### §2 — The counters are read by key, never joined — DONE

- **Dimension 2.1** — DONE — the page statements read `core.fleet_activity_counters` by primary key per fleet; both spellings carry the same columns in the same order → Test `both_page_statements_read_the_same_columns_in_the_same_order`
- **Dimension 2.2** — DONE — a fleet with no counter row still lists, with zeroes, which an INNER JOIN would drop → Test `a_fleet_with_no_counter_row_still_lists`

### §3 — The delta machinery goes — DONE

- **Dimension 3.1** — DONE — `tile-counters.ts`, `absorb` and `countersStale` are deleted, and the event-map trimming `absorb` carried is re-homed BEFORE the deletion → Test `the_store_still_bounds_its_event_map_without_absorb`

### §4 — The wall's chrome sits on the grid (folded in on the user's call) — DONE

Measured in the operator's own browser session against `app-dev` before any edit — every left-nav destination and every rendered table. The numbers are the rendered geometry, not the class names.

- **Dimension 4.1** — DONE — a nav item is a symmetric pill: rounded on every corner, no accent rail. The rail sat 12px inside every consumer's inset and read as a clipped corner, and its two pixels pushed the icon column (22) off the eyebrow column (20). The shell's own regression pin, `renders the active navigation item as a filled pill, never a rail`, rides beside it → Test `is a symmetric pill: rounded on every corner, with no accent rail`
- **Dimension 4.2** — DONE — the four nav groups share one rhythm: a 16px eyebrow with its text on one column, no gap between items, 48px between groups. Before, Platform's eyebrow was a 32px `Button` with its text 5px right of the others and a 2px item gap, so the gaps ran 47.6 / 64 / 47.6 → Test `renders the Platform toggle at eyebrow scale, on the eyebrow column`
- **Dimension 4.3** — DONE — a paginated table lets the page scroll rather than opening a second scroll region, so header, body and footer share a 13px inset on both edges. Before, the `max-h-96` default put a 6px scrollbar inside the cell padding, so on exactly the tables with more rows than the box the header text and row actions sat at 19 while the footer stayed at 13 → Test `is unbounded by default, so a paginated table lets the page scroll`
- **Dimension 4.4** — DONE — the footer's trailing page controls sit on the body's right column. Prev and Next are ghost buttons whose 12px padding paints nothing, so the "›" glyph ended 26px from the edge against 13 for everything else → Test `pulls the trailing page controls back onto the body's right column`
- **Dimension 4.5** — DONE — cell padding and the section-label row sit on the 4px scale: `py-2` in place of `py-1.5`, and `SectionHeader` centres its label against its action so the gap to the section body is 28 rather than 29 → Test `centers the label on its action's middle rather than its baseline`

### §5 — The agent's identity has one home (finding #15, folded in on the user's call) — DONE

The derivation is identity data — `fleetIdentity.ts:19-20` says so, and `FleetTile.test.tsx:210-211` pins its outputs — so the hash and the 32-name table do not change. What changes is where it lives and who composes the label from it. Impact report: Discovery, below.

- **Dimension 5.1** — DONE — `deriveFleetIdentity` lives in `lib/fleets/identity.ts`, byte-identical, and all six importers point there; the pinned sigil and callsign for the fixture id are unchanged → Test `the moved derivation still yields the pinned sigil and callsign`
- **Dimension 5.2** — DONE — the `Agent <callsign>` string is composed in exactly one function, which `AgentLabel.tsx` renders and `chargeAgentLabel` sorts by; billing no longer imports from a fleets component directory → Test `billing and the domain label compose one string`
- **Dimension 5.3** — DONE — the fleet page's thread receives the callsign under a prop named for what it carries, and `fleetName` means the name on every prop that has it → Test `the thread takes its sender label under its own name`
- **Dimension 5.4** — DONE — the admin lease table names the agent by callsign, as every other agent column does, and keeps the UUID in `title` → Test `a lease row names its agent and keeps the uuid in title`

### §6 — The strip states only what the read can vouch for (finding #11, folded in on the user's call) — DONE

`RunMetricsStrip.test.tsx:68` pins `processed + response_text: null → "Completed with no reply recorded."` as correct. The list read carries no bodies (`RunMetricsStrip.tsx:127`), so that null means UNKNOWN, but `EventRow` documents it as no reply (`events.ts:76-78`) and the strip trusts the document. Every successful run's strip therefore contradicts the thread below it, which shows the reply.

- **Dimension 6.1** — DONE — a processed run whose row carries no body says the run completed, and does not claim a reply was never recorded; the test that pinned the contradiction is rewritten to pin this → Test `says a run completed rather than claiming no reply when the read carries no body`
- **Dimension 6.2** — DONE — absence of a reply is stated by the one surface that holds the body, the event detail dialog, from the body itself and in its own words; no list surface claims it, because no list read can vouch for it → Test `states no reply only when the row affirmatively carries none`

### §7 — Action buttons follow one rule (finding #10, folded in on the user's call) — DONE

**The premise was counted wrong twice — by the walk, and by the brief's grep.** `variant="…"` matches every component with a variant prop: all 8 `warning` are `<Alert>`, all 4 `cyan` are `<Badge>`, the 6 `default` are `<Badge>`/`<Link>`, and most of the 31 `destructive` are Alerts. A tag-aware count finds **76 `<Button>`s in 42 files**: destructive 6 · ghost 23 · outline 16 · secondary 5 · link 3 · 2 whose variant comes from a per-action config · **and 21 with no variant prop, which render `default` — the primary mint** — a group no grep counted. Those 21 are the real subject: nearly every dialog's submit, every stub page's one action, "Approve", "Open fleet →".

**The rule, with the clause the primary needed:** `default` for the ONE primary action on a surface · `ghost` for every other non-destructive action · `destructive` for a destructive one · `link` only for an inline action inside prose or a table cell. The design system already reserves `default` for the primary CTA (`docs/DESIGN_SYSTEM.md`, 2026-06-23), so this names what the mint button was always for.

**The classification, brought back before a single site changes.** 47 sites are not `ghost`/`destructive` today; the first-pass verdict is mine, the last column is the user's.

| Site | Today | Verdict | Why | Confirm |
|---|---|---|---|---|
| `admin/fleet-libraries/components/AddFleetDialog.tsx:203` | default | keep | dialog submit — the primary | |
| `admin/fleet-libraries/components/EditFleetDialog.tsx:241` | default | keep | dialog submit | |
| `admin/fleet-libraries/components/PlatformCatalogTable.tsx:75` | link | keep | repository link in a table cell | |
| `admin/fleet-libraries/page.tsx:36` | default | keep | the page's one recovery action | |
| `admin/models/components/EditModelDialog.tsx:145` | default | keep | dialog submit | |
| `admin/models/components/MakeDefaultDialog.tsx:117` | default | keep | dialog submit | |
| `admin/runners/[runnerId]/components/LeaseFilterBar.tsx:119` | default | keep | the filter form's primary | |
| `admin/runners/[runnerId]/components/RunnerHeader.tsx:185` | outline | **ghost** | run selftest — a secondary action | |
| `admin/runners/[runnerId]/components/RunnerHeader.tsx:231` | dynamic | keep — audit the config | `variant={variant}` comes from the per-action config (cordon/drain/revoke); the table to check is that config, not this site | |
| `admin/runners/[runnerId]/components/RunnerHeader.tsx:259` | outline | **ghost** | opens Grafana — navigational | |
| `admin/runners/components/AddRunnerDialog.tsx:178` | default | keep | "I've stored it — close" is the token dialog's one action and its primary: the operator confirms they stored the token | |
| `admin/runners/components/EditPolicyDialog.tsx:93` | outline | **ghost** | opens the editor | |
| `admin/runners/components/EditPolicyDialog.tsx:120` | default | keep | dialog submit | |
| `admin/runners/components/PolicyBindsField.tsx:196` | outline | **ghost** | removes a row from an unsaved form, not persisted data | |
| `admin/runners/components/PolicyBindsField.tsx:209` | outline | **ghost** | adds a row | |
| `error.tsx:80` | default | keep | the error boundary's one action | |
| `settings/api-keys/components/CreateApiKeyDialog.tsx:204` | default | keep | "Done" — the dialog's primary | |
| `settings/billing/components/BillingBalanceCard.tsx:93` | outline | **default** | "Buy credits" is the balance card's primary | |
| `settings/page.tsx:29` | default | keep | a stub page's one action | |
| `w/…/approvals/[gateId]/ResolveButtons.tsx:76` | default | keep | Approve — the primary | |
| `w/…/approvals/components/ApprovalsList.tsx:287` | outline | **ghost** | load more | |
| `w/…/fleets/[id]/components/KillSwitch.tsx:152` | outline | **ghost** | "Killed" — a disabled state marker | |
| `w/…/fleets/[id]/components/KillSwitch.tsx:160` | dynamic | keep — audit the config | `variant={action.variant}` comes from the per-action config (kill/pause/resume); audit that config | |
| `w/…/fleets/[id]/components/RunMetricsStrip.tsx:68` | outline | **ghost** | link to the approvals inbox | |
| `w/…/fleets/[id]/components/SkillEditor.tsx:253` | secondary | **default** | Save — the editor's primary | |
| `w/…/fleets/[id]/components/SkillEditor.tsx:271` | outline | **ghost** | Edit — opens the editor | |
| `w/…/fleets/new/InstallSourceSelector.tsx:190` | default | keep | "Install" on every library card — one primary per card, the catalogue shape; M98 §9 made Install the one-click primary | ✓ user |
| `w/…/fleets/new/InstallSourceSelector.tsx:208` | secondary | **ghost** | load more | |
| `w/…/fleets/new/InstallSourceSelector.tsx:241` | secondary | **ghost** | retry | |
| `w/…/fleets/new/InstallStates.tsx:198` | default | keep | Connect — the step's primary | |
| `w/…/fleets/new/InstallStreamSteps.tsx:94` | default | keep | "Open fleet →" — the primary | |
| `w/…/fleets/new/install-state-list.tsx:22` | link | keep | "← Back to library" — inline navigation | |
| `w/…/fleets/new/library-docs.tsx:35` | outline | **ghost** | "Learn more" — external | |
| `w/…/integrations/components/connector-rows.tsx:186` | outline | **ghost** | Disconnect — reversible in one click; the restraint principle says no alarm on a reversible action, and the word is the signal | ✓ user |
| `w/…/integrations/components/connector-rows.tsx:198` | outline | **ghost** | Connect | |
| `w/…/secrets/components/AddSecretForm.tsx:211` | link | keep | "+ Add field" — inline in a form | |
| `w/…/secrets/components/EditSecretDialog.tsx:123` | default | keep | dialog submit | |
| `w/…/secrets/components/RenameSecretDialog.tsx:199` | default | keep | dialog submit | |
| `w/…/settings/defaults/page.tsx:28` | default | keep | a stub page's one action | |
| `w/…/settings/models/components/AddModelEntryDialog.tsx:327` | outline | **ghost** | retry | |
| `w/…/settings/models/components/AddModelEntryDialog.tsx:339` | outline | **ghost** | the second submit — one primary per dialog | |
| `w/…/settings/models/components/AddModelEntryDialog.tsx:343` | default | keep | the primary submit | |
| `w/…/settings/models/components/EditModelEntryDialog.tsx:214` | outline | **ghost** | Cancel | |
| `w/…/settings/models/components/EditModelEntryDialog.tsx:217` | default | keep | Save | |
| `w/…/settings/models/components/ModelsRegistryTable.tsx:319` | secondary | **ghost** | load more | |
| `w/…/settings/models/components/ModelsRegistryTable.tsx:337` | secondary | **ghost** | retry | |
| `w/…/settings/security/page.tsx:29` | default | keep | a stub page's one action | |

Confirmed by the user (Sep 10, 2026): 47 sites — 26 keep · 19 → `ghost` · 2 → `default` · 0 open. The rule's third clause (`default` for the one primary on a surface) is accepted; Install stays the primary on every library card; Disconnect is `ghost`.

- **Dimension 7.1** — DONE — every action button under the dashboard renders `default`, `ghost`, `destructive`, or `link` as the confirmed table says for that site; a repository test enumerates the sites tag-aware and fails on any other → Test `no dashboard action button uses a variant outside the rule`

## Interfaces

```
No new endpoint. The surfaces this changes, all pre-existing:

  Frame     hello · received · gate frames · event_complete   gain a counter snapshot
  Read      core.fleet_activity_counters   by primary key, per fleet, never joined
  Client    the {fleet_status?; pending_approvals?} block on the frame type
  Deleted   lib/wall/tile-counters.ts, absorb, countersStale
```

## Failure Modes

| Mode | Trigger | Expected behaviour | Negative test |
|---|---|---|---|
| Dropped frame | transport loses one frame | the next frame's snapshot restores the tile without a reload | `a_dropped_frame_is_corrected_by_the_next_snapshot` |
| Duplicate frame | redelivery | the tile is unchanged; a snapshot is idempotent | `a_repeated_frame_leaves_the_tile_unchanged` |
| Counter-less fleet | a fleet that has never run | it lists with zeroes rather than vanishing | `a_fleet_with_no_counter_row_still_lists` |
| Stream cap reached | `SSE_MAX_STREAMS` = 64 per instance (`knobs.rs:128`) | 503 `UZ-API-002`; the client degrades to a snapshot tile rather than showing a stale live one | `a_capped_stream_degrades_to_a_snapshot_tile` |

## Invariants

- **A snapshot is idempotent** — enforced by the client assigning, never adding. Code-enforced: the store exposes no accumulate path once `absorb` is deleted.
- **The counters are never re-aggregated from events** — enforced by the statements reading `core.fleet_activity_counters` only; `core.fleet_events` is not summed at read time.
- **An INNER JOIN cannot reach the counter read** — enforced by the statement text and pinned by `a_fleet_with_no_counter_row_still_lists`.

## Metrics & Observability

No new product or operator signal. The change corrects a number already rendered; it adds no event, no metric and no funnel step. The wall's existing live/snapshot tile state is unchanged.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `a_repeated_frame_leaves_the_tile_unchanged` | Deliver the same frame twice carrying `events_processed = 7` → tile reads 7, not 14 |
| 1.2 | unit | `hello_carries_the_counters_a_late_subscriber_missed` | Subscribe after 3 events → the `hello` frame reports `events_processed = 3` before any event arrives |
| 1.3 | integration | `a_parked_event_still_reports_its_event_count` | Lease an event whose gate parks (`Admission::Await`) → a receive frame carries `events_processed` incremented, no `event_complete` is published, and the tile settles anyway |
| 1.4 | integration | `a_parked_event_moves_no_budget` | The same park → `billing.usage_ledger` gains no row and `budget_used_nanos` is unchanged |
| 2.1 | unit | `both_page_statements_read_the_same_columns_in_the_same_order` | Split each page statement on `FROM core.fleets f` → identical column lists |
| 2.2 | integration | `a_fleet_with_no_counter_row_still_lists` | A fleet with no counter row → it appears on the page with `events_processed = 0` |
| 3.1 | unit | `the_store_still_bounds_its_event_map_without_absorb` | Push events for 200 fleets with `absorb` deleted → `#eventsByFleet` stays bounded |
| 1.5 | integration | `a_dropped_frame_is_corrected_by_the_next_snapshot` | Drop one frame, deliver the next → the tile matches the database without a reload |
| 1.6 | integration | `a_capped_stream_degrades_to_a_snapshot_tile` | Open `SSE_MAX_STREAMS + 1` streams → 503 `UZ-API-002` and the extra tile renders as `snapshot`, never as a stale `live` |
| 4.1 | unit | `is a symmetric pill: rounded on every corner, with no accent rail` | Render an active `NavItem` → class carries `rounded-md` and `data-[active=true]:bg-pulse/10`, never `border-l-2` or `border-pulse` |
| 4.2 | unit | `renders the Platform toggle at eyebrow scale, on the eyebrow column` | Render the shell for a platform-scoped operator → the Platform button carries `h-4` and `px-2`, never `h-8` |
| 4.3 | unit | `is unbounded by default, so a paginated table lets the page scroll` | Render a default `DataTable` → the viewport carries no `max-h-*` class; `viewportClassName="max-h-72"` still bounds it |
| 4.4 | unit | `pulls the trailing page controls back onto the body's right column` | Render a paged table → the cluster holding Next carries `-mr-3` |
| 4.5 | unit | `centers the label on its action's middle rather than its baseline` | Render a `SectionHeader` with an action → `items-center`, never `items-baseline` |
| 5.1 | unit | `the moved derivation still yields the pinned sigil and callsign` | The fixture id `FleetTile.test.tsx:210` uses → `hashHex` ends `4bce8453`, `callsign` is `Lumen-8453`, from `lib/fleets/identity.ts` |
| 5.2 | unit | `billing and the domain label compose one string` | `chargeAgentLabel(row)` equals the text `AgentLabel` renders for the same `fleet_id`, and neither imports from `fleets/components` |
| 5.3 | unit | `the thread takes its sender label under its own name` | Render `ChatView` → the thread's sender label is the callsign; the prop is not named `fleetName` |
| 5.4 | unit | `a lease row names its agent and keeps the uuid in title` | Render `LeaseTable` with one lease → the cell text is `Agent <callsign>`, `title` is the UUID |
| 6.1 | unit | `says a run completed rather than claiming no reply when the read carries no body` | `status: processed, failure_label: null, response_text: null` → the strip's outcome is the completed sentence, not `OUTCOME.NO_REPLY` |
| 6.2 | unit | `states no reply only when the row affirmatively carries none` | Render the detail dialog with a processed row and no body → "No result recorded", never the completion sentence and never a no-reply claim |
| 7.1 | unit | `no dashboard action button uses a variant outside the rule` | Enumerate `variant="…"` on `<Button>` under `app/(dashboard)` → every value is `ghost`, `destructive`, or the table's allowlisted `(file, variant)` pair; the test lists the offenders |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | A parked event's count reaches the tile without a reload (§1) | `make test-integration-rustd` | `a_parked_event_still_reports_its_event_count` passes | P0 | |
| R2 | The wall's no-refresh guard goes green (§1, §3) | `cd ui/packages/app && bun run test:e2e -- wall-live-counters` | `wall-live-counters.spec.ts` passes, including the cross-context case | P0 | |
| R3 | A counter-less fleet still lists (§2) | `make test-integration-rustd` | `a_fleet_with_no_counter_row_still_lists` passes | P0 | |
| R4 | The delta machinery is gone (§3) | `git grep -n "tile-counters\|countersStale\|absorb(" -- ui/packages/app` | 0 matches | P0 | |
| R5 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| R6 | The nav and every table measure symmetric (§4) | `cd ui/packages/design-system && bun run test -- NavItem DataTable SectionHeader Pagination Button` | the §4 pins pass; a re-measure on `app-dev` after deploy reads 12/12 on the nav and 13/13 on each table | P1 | |
| R7 | The identity derivation moved without changing (§5) | `git grep -n "fleets/components/fleetIdentity" -- ui/packages/app` · `cd ui/packages/app && bun run test -- identity FleetTile` | 0 matches; `FleetTile.test.tsx:210-211` still pins `4bce8453` / `Lumen-8453` | P0 | |
| R8 | The strip no longer contradicts the thread (§6) | `cd ui/packages/app && bun run test -- RunMetricsStrip` | 6.1 and 6.2 pass; the old line-68 pin is gone | P0 | |
| R9 | The button rule holds and stays held (§7) | `cd ui/packages/app && bun run test -- button-variant-rule` | passes with an empty offender list | P1 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Lint green | `make lint-all` | exit 0 | P0 | |
| S3 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S4 | Integration tier green | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S7 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

## Dead Code Sweep

`ui/packages/app/lib/wall/tile-counters.ts`, `absorb` (`useWorkspaceStream.ts:93`) and `countersStale` are removed with their last caller. Sweep greps: `git grep -n "tile-counters"`, `git grep -n "countersStale"`, `git grep -n "absorb("` — each 0 matches at CHORE(close).

## Out of Scope

- **`Last-Event-ID` / `id:` lines.** The transport is Redis PUB/SUB with no replay, so an id line would be a decorative guarantee.
- **Cycling reconnect backoff.** Measured at ~2.76× traffic per open tab against a capacity-gated route and rejected; the saturating cap stays, jitter only.
- **Raising `SSE_MAX_STREAMS`.** The 64-per-instance cap is a capacity decision, not a defect of this data path.
- **The dashboard load-latency attribution.** That is M195_001, which runs in parallel; only its §2 request inventory waits on this spec.

## Product Clarity (authoring record)

1. **Successful user moment** — an operator watches a fleet park for approval, approves it, and the tile's numbers track the run without a reload.
2. **Preserved user behaviour** — the wall's live/snapshot/drained tile states, its ordering and its reconnect posture are unchanged.
3. **Optimal-way check** — an absolute snapshot is the shape Codex uses and the shape this repository already uses for `pending_approvals`; a delta needs a watermark that does not exist, because `fleets.updated_at` never moves on a counter-only run.
4. **Rebuild vs iterate** — iterate: the frame types, the store and the tile all exist. What changes is what the frame carries and whether the client adds or assigns.
5. **What we build** — the snapshot on every frame, the client replace, the primary-key counter read.
6. **What we do NOT build** — replay, id lines, a new cap, a new metric.
7. **Fit with existing features** — extends the `pending_approvals` snapshot already on the same frames.
8. **Surface order** — no new surface; the tile's existing footer figures become correct.
9. **Dashboard restraint** — nothing is added to the dashboard; one wrong number becomes right.
10. **Confused-user next step** — an operator who distrusts a tile reloads the page; after this, the reload shows the same number the tile already showed.

## Decomposition & alternatives (patch vs refactor)

The patch alternative is a client-side delta that accumulates counter changes per frame — it is what the carried-over `tile-counters.ts` implements. It was rejected on three grounds, each checked: a delta cannot be made idempotent against redelivery without a watermark; the only candidate watermark (`fleets.updated_at`) does not move on a counter-only run, because counters live in `core.fleet_activity_counters`; and it puts the reconciliation burden on every future frame author. The snapshot is a smaller change to reason about and it is the shape both the reference implementation and this repository already ship.

## Discovery (consult log)

- **Carry-over from M194_001 (Sep 10, 2026).** Part of this spec's subject was written inside `chore/m194-acceptance-walk-close` before it was clear it belonged here, and was split out of that Pull Request rather than shipped in it. **It is not on `main`.** CHORE(open) restores it before writing anything new. Two copies exist: the durable one is `~/.gstack/projects/agentsfleet-agentsfleet/m195-carryover/` — a 7-file patch, six whole untracked files and `RESTORE.md` — and the second is `stash@{0}` (`32c4eeba`) in the `agentsfleet` clone. Restore from a worktree cut off `main` AFTER M194_001 merges: `git apply ~/.gstack/projects/agentsfleet-agentsfleet/m195-carryover/m195-tracked.patch` then `cp -R ~/.gstack/projects/agentsfleet-agentsfleet/m195-carryover/m195-untracked/. .`. **Use the patch, never `git stash pop`** — the stash commit snapshots the index too, so it also carries M194's staged `schema/embed.zig` deletion and `scripts/check_orly_pin*.sh`, all of which land with that merge and would double-apply. Both copies are machine-local to the authoring Mac; a pickup elsewhere needs them pushed first.
- **What the carried work is, and what is wrong with it.** `lib/wall/tile-counters.ts`, `absorb` and `countersStale` are the client-side delta this spec supersedes — to be DELETED, not finished. `absorb` is today the only thing trimming `#eventsByFleet`, so whatever bounds that map must be re-homed before the deletion. `wall-live-counters.spec.ts` fails on purpose until the replacement lands. `FleetTile.tsx`, `FleetWall.tsx`, `WallLiveBadge.tsx` and `useWorkspaceStream.ts` carry the half-built client.
- **Correction to an earlier session note, recorded because a fix aimed where it pointed would have missed.** The note said a gate park publishes no completion "while its charge has already landed". The park leaves NO charge: `admit/mod.rs:56` documents `Admission::Await` as "durably identical to `Admission::Retry` — no row, delivery left leasable", so `budget_used_nanos` is untouched and the delivery is re-leased when the human answers. What drifts is `events_processed`, which `schema/890` bumps `AFTER INSERT ON core.fleet_events` — at receive. The client hears counters only on `event_complete`, and the park returns `Step::Stop` at `lease/pull.rs:209` without reaching `publish_completion` at `:329`.
- **Measured during the split, so it is not re-derived.** The wall's page read: `LEFT JOIN core.fleet_activity_counters` plans as a hash join whose build side sequentially scans every counter row, to answer for the fifty fleets on one page. At 50k fleets and 14k counter rows that is 1.69 ms against 0.21 ms for a per-row primary-key lookup, and the gap widens with the counters table because the scan is O(counters) where the lookups are O(page). `LEFT JOIN LATERAL` does not help — the planner flattens it back into the same join. `SELECT_FLEET_DETAIL` needs no change: one driving row already plans as a nested loop over both primary keys, 5 buffers. An INNER JOIN is wrong everywhere — 126 of 174 fleets on the development database have no counter row at all, so it renders an empty wall. The carried patch already contains this change.
- **Sequencing against M195_001 — checked, not assumed.** The two specs' Files Changed tables share NO path: M195_001 measures through `ui/packages/app/lib/acceptance/workspace-fetch-audit.ts` rather than by editing the wall. They run in parallel. The single coupling is M195_001 §2's request inventory, which this spec changes by removing the wall's separate counter fetch — so that inventory is taken after this lands, or re-taken.
- **Findings #15, #11, #10 folded in (Sep 10, 2026), on the user's call, with the investigation done before any patch.** Three of the brief's premises did not survive the read and are recorded here so nobody re-derives them. (#15) There are SIX importers of `fleetIdentity.ts`, not five — `tests/e2e/acceptance/wall-live-counters.spec.ts:45` is the sixth, and it arrived with this spec's own carry-over, so it is not on `main`. There is NO slug: the fleet wire type has none, and the only `slug` under `lib/api/` is a model-provider id (`model-library-types.ts:46,53`); the axis is name / id / callsign. (#11) The string is not missing — it is composed (`console-copy.ts:80` labels, `RunMetricsStrip.tsx:58,124-129` derives via `outcomeFor`, `event-summary.ts:189-204`). (#10) 42 files render a `<Button>` under `app/(dashboard)`, not 48 and not 8.
- **#15, the evidence behind §5.** Load-bearing beyond display: not on the wire or in Postgres (zero mentions of `callsign` under `rustd/` and `schema/`; no fetch body, query parameter or storage carries it), but `FleetTile.test.tsx:210-211` pins literal outputs and the file declares its 32 buckets identity data (`:19-20`) — operators memorise `Agent Lumen-8453`, so a hash change renames every agent in their heads. Where each surface shows which: the tile heading shows the name (`FleetTile.tsx:210`) and its subline the callsign (`:215`); the fleet page header, breadcrumb, delete confirm and install gate show the name (`FleetHeader.tsx:61,77`, `FleetConfig.tsx:60`, `FleetInstallGate.tsx:44`); the fleet page THREAD receives the callsign through a prop named `fleetName` (`page.tsx:215` → `ChatView.tsx:51`), 74 lines after the same file passes `fleet.name` to a prop of the same name; events and billing show the callsign (`EventsList.tsx:77`, `BillingUsageTab.tsx:41-42`); the admin lease table alone shows a raw UUID (`LeaseTable.tsx:74`). No surface shows a wrong VALUE; one prop is named for the wrong concept and one column uses a different convention from every other. `charges.ts:41` and `AgentLabel.tsx:23` compose the identical `Agent ${callsign}` string — the billing module reaches into `fleets/components/` for a domain fact three areas consume, which is the whole argument for `lib/`. Sequencing: on one branch the six rewrites land in one commit; the conflict the brief priced assumed a second agent.
- **#11, the case where figures and thread disagree.** A processed run with `failure_label: null` and `response_text: null` — which is EVERY processed row the list read returns, since it carries no bodies — falls through `outcomeFor` to `OUTCOME.NO_REPLY` ("Completed with no reply recorded."), while the thread below renders the reply the detail read holds. `RunMetricsStrip.test.tsx:68` pinned that fallthrough as correct; §6.1 is the test that replaces it. The fix is a rename with a new sentence — `OUTCOME.NO_REPLY` → `OUTCOME.COMPLETED`, "Completed." — because the constant's only production path was that fallthrough and every caller of it holds a bodiless row (the strip, the events table, the live thread rows); the page statement is proven never to select the body (`afd_events/src/history/statement.rs:174`), and the detail dialog already states absence with its own `NO_RESULT`. `EventRow.response_text` is documented as null "while a run is in flight, and on a run that failed before producing one" (`events.ts:76-78`) — the list read's third meaning, unread, is what the strip mistook for the second.
- **#10, the count was wrong twice and the classification precedes the sweep.** The walk said 8 pages; the brief's grep said 48 files with `variant` in eight values; both counted Alerts and Badges as buttons. Tag-aware: 76 `<Button>`s in 42 files, and 21 of them carry no `variant` and so render the primary `default` — the group that actually needed a rule. The §7 table is filled from each site's intent read in place, not its label, and is confirmed with the user before a single variant changes.
