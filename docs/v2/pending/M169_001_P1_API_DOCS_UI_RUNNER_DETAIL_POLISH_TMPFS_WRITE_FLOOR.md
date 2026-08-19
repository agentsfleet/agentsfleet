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

# M169_001: A lease writes its own scratch, and the runner detail page reads like a product

**Prototype:** v2.0.0
**Milestone:** M169
**Workstream:** 001
**Date:** Aug 19, 2026
**Status:** PENDING
**Priority:** P1 — the sandbox write defect fails every credentialed lease on hardened runners and blocks the M136 §1–§5 live pass; the rest is operator-facing polish on the page fronting that proof.
**Categories:** API, DOCS, UI
**Batch:** B1 — single stream, no parallel sibling.
**Branch:** feat/m169-runner-detail-polish
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none (M136_001 resumes its live pass once this merges and deploys)
**Provenance:** LLM-drafted (Claude Opus 5, Aug 19, 2026) from decisions Indy made in-session; behaviour claims verified against source at `3c98605ba`.
**Canonical architecture:** `docs/architecture/runner_fleet.md` §System guarantees

---

## Overview

**Goal (testable):** A sandboxed lease child creates a file under `/tmp` (its private per-lease tmpfs) because bwrap and landlock read one shared writable floor, and the runner detail page's actions, filter, and tables carry icons, plain names, and the app's standard time vocabulary.
**Problem:** Every credentialed model dial on a hardened runner dies in ~90ms with "The runner crashed — TempFileCreateFailed": the engine writes its Authorization header to a 0600 temp file (so tokens never ride argv), bwrap mounts `/tmp` as a writable private tmpfs, but landlock grants that same path read-only — the write-side twin of the read-set drift M136 fixed. Meanwhile the page operators watch this from says "Filter leases"/"Apply filter"/"Run self-test", shows no icons, renders live Cordon/Drain actions that are not ready, uses time column names nothing else in the app uses, hides the absolute time behind the self-test's relative stamp, and explains its states nowhere.
**Solution summary:** One writable-mounts floor in `protocol_bind.zig`, consumed by both `sandbox_args.zig` (bwrap argv) and `landlock.zig` (write rules), pinned by a property test on each side and proven by a real-sandbox integration test — the same derivation the read side got in M136. On the page: rename the filter and self-test copy, add leading lucide icons to every header action and the Apply button, disable Cordon/Drain until they are ready, add a manual refresh affordance, restore the hover timestamp, link the states chip to the published runners page, and align the time columns with the shared events-table vocabulary. Operational tail: pull the failing dev lease rows as PR evidence and sweep the leaked `acc-*` acceptance fixtures out of the dev workspace.

## PR Intent & comprehension handshake

- **PR title (eventual):** fix(runner): derive the sandbox write floor; polish the runner detail page
- **Intent (one sentence):** a hardened runner completes credentialed leases again, and the page operators use to watch it speaks plain product language.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `src/runner/engine/landlock.zig` — the read-set derivation and its property pin (the test at the file's tail) are the exact pattern the write side mirrors; `/tmp` currently sits in the read-only floor list.
2. `src/lib/contract/protocol_bind.zig` — home of the shared baseline path lists and the sensitive-path refusal; the new writable floor is a sibling of the existing read-only baseline.
3. `src/runner/sandbox_integration_test.zig` — the real-sandbox resolver proof is the template for "a lease child can write its own `/tmp`".
4. `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy.ts` — every filter string is a named constant here; renames land here first, tests that import constants follow free.
5. `ui/packages/app/app/(dashboard)/admin/runners/components/PolicyBindsField.tsx` — the repo's leading-icon Button convention (`<Icon size={14} aria-hidden="true" />` before the label).

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M169_001_P1_API_DOCS_UI_RUNNER_DETAIL_POLISH_TMPFS_WRITE_FLOOR.md` | CREATE | This spec; Dimensions marked DONE as work lands. |
| `src/lib/contract/protocol_bind.zig` | EDIT | Add the writable-mounts floor beside the read-only baseline; sensitive-path refusal unchanged. |
| `src/runner/sandbox_args.zig` | EDIT | Build the `--tmpfs` argv entries from the shared floor instead of a hand-written literal. |
| `src/runner/engine/landlock.zig` | EDIT | Write rules derive from the floor; `/tmp` leaves the read-only list; add the write-set property pin. |
| `src/runner/sandbox_args_bind_test.zig` | EDIT | Extend argv proofs to assert every floor path is a tmpfs mount. |
| `src/runner/sandbox_integration_test.zig` | EDIT | Real-sandbox proof: the lease child creates, writes, and unlinks a file under `/tmp`. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/runner-copy.ts` | EDIT | "Filter leases"→"Filter", "Apply filter"→"Apply". |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseFilterBar.tsx` | EDIT | Leading FilterIcon on Apply. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/lease-filter-query.ts` | EDIT | The bare token `and` between pairs is an accepted connective, still never a filter. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/lease-filter-query.test.ts` | EDIT | Case for `workspace:x and fleet:y`. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable.tsx` | EDIT | "When"→"Time", "Took"→"Duration". |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/LeaseTable.test.tsx` | EDIT | Replace the two hardcoded filter strings with constant imports; column assertions follow the rename. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/ActivityTable.tsx` | EDIT | "When"→"Time". |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.tsx` | EDIT | Leading icons on all five actions; Cordon/Drain disabled with reason; refresh button; states-chip learn-more link. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerListCells.tsx` | EDIT | "Run self-test"→"Run checks", pending label "Checks requested"; Cordon/Drain configs carry the disabled reason. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/EditPolicyDialog.tsx` | EDIT | Leading icon on the Edit policy trigger. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerSandboxPanel.tsx` | EDIT | Panel heading "CHECKS"; relative stamp regains its hover tooltip. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.test.tsx` | EDIT | Renamed labels, disabled Cordon/Drain, icon presence. |
| `ui/packages/app/app/(dashboard)/admin/runners/[runnerId]/components/RunnerHeader.selftest.test.tsx` | EDIT | "Run checks"/"Checks requested" strings. |
| `ui/packages/app/app/(dashboard)/admin/runners/components/RunnerDialogs.test.tsx` | EDIT | Follows the action-config changes. |

A paired branch in `~/Projects/docs` (separate repository, own Pull Request (PR)) adds a **States** section to `runners.mdx` and the anchor the learn-more link lands on — required by the public-surface rule; never edited through this worktree.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NDC (no dead code: the disabled Cordon/Drain keep their live handlers, disabled at render, not stubbed); UFS (the floor list, doc URL, and renamed labels are named constants); FLL (`landlock.zig` and `sandbox_args.zig` sit near the 350 cap — split before breaching); TST-NAM (no milestone markers in production code).
- `dispatch/write_zig.md` — pg-drain not touched, but errdefer/length/cross-compile sections apply to the three runner files.
- `dispatch/write_ts_adhere_bun.md` — design-system primitives only; icons via lucide-react per existing convention.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — three `src/runner`/`src/lib` files | cross-compile both Linux targets; errdefer audit on touched functions |
| PUB / Struct-Shape | yes — one new pub const (the floor list) | justified: consumed by two external files (`sandbox_args`, `landlock`) |
| File & Function Length (≤350/≤50/≤70) | yes — `sandbox_args.zig` at 350, `landlock.zig` near cap | measure first; extract before adding if either would breach |
| UFS (repeated/semantic literals) | yes | `/tmp` appears only in the floor list; doc URL and labels are named constants |
| UI Substitution / DESIGN TOKEN | yes — `*.tsx` edits | design-system `Button`/`Time`/`Tooltip` primitives; no raw hex/arbitrary utilities |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | no log lines, schema, or error-registry entries change |

## Prior-Art / Reference Implementations

- **Reference:** `src/runner/engine/landlock.zig` read-set derivation (M136 §0.7) — the write floor is the same move on the write axis; divergence: none.
- **Reference (UI):** `PolicyBindsField.tsx` / `AddModelDialog.tsx` leading-icon buttons; `EventsList.tsx` for the Time/Duration column vocabulary.

## Sections (implementation slices)

### §1 — The sandbox write floor is derived, not hand-synced

One list names every path bwrap mounts writable; both enforcement layers consume it. **Implementation default:** the floor starts as exactly `["/tmp"]`; landlock grants it the same access set as the workspace rule.

- **Dimension 1.1** — the writable floor lives in `protocol_bind.zig` and `sandbox_args.zig` emits one `--tmpfs` per floor entry → Test `test_every_writable_floor_path_is_a_tmpfs_in_argv`
- **Dimension 1.2** — landlock write rules enumerate the floor and `/tmp` is gone from the read-only list → Test `landlock write set contains every writable-floor path`
- **Dimension 1.3** — inside a real sandbox the lease child creates, writes, and unlinks `/tmp/probe` → Test `a lease child writes its private tmpfs` (Linux privileged lane)
- **Dimension 1.4** — an operator bind naming `/tmp` is still refused → Test `test_sensitive_paths_still_refuse_tmp`
- **Dimension 1.5** — the failing dev lease rows (`TempFileCreateFailed`, and the prior `HostResolutionFailed` class) are pulled and pasted into PR Session Notes as evidence → graded in rubric R5

### §2 — Header actions speak product

- **Dimension 2.1** — "Run self-test"→"Run checks", pending "Checks requested", panel heading "CHECKS" → Test `runner header runs checks and shows the requested state`
- **Dimension 2.2** — all five actions and Apply carry a leading lucide icon, `size={14} aria-hidden` (defaults: PencilIcon, ListChecksIcon, BanIcon, HourglassIcon, ShieldXIcon, FilterIcon; RefreshCwIcon for §2.4) → Test `every header action renders an icon beside its label`
- **Dimension 2.3** — Cordon and Drain render disabled with the reason "Not active yet"; clicking opens nothing and PATCHes nothing → Test `cordon and drain are disabled and inert`
- **Dimension 2.4** — a refresh button re-reads the page via router refresh; no polling anywhere → Test `refresh button requests a router refresh`

### §3 — The filter reads plain

- **Dimension 3.1** — label "Filter", button "Apply"; the two hardcoded test strings now import the constants → Test `LeaseFilterBar renders the renamed copy` (constant-importing suite)
- **Dimension 3.2** — `workspace:x and fleet:y` parses to both filters; `and` is an accepted connective, never a filter value → Test `tokenizer accepts the and connective`

### §4 — Time vocabulary matches the app

- **Dimension 4.1** — LeaseTable "Time"/"Duration", ActivityTable "Time", matching `EventsList` → Test `lease and activity tables use the shared time vocabulary`
- **Dimension 4.2** — the checks panel's relative stamp shows the absolute time on hover, same `Time` primitive as the table → Test `checks timestamp reveals the absolute time`

### §5 — States are explained where they are shown

- **Dimension 5.1** — a CircleHelp learn-more beside the states chip links `https://docs.agentsfleet.net/runners#when-a-runner-stops-taking-work` → Test `states chip links the runners page`
- **Dimension 5.2** — the paired docs branch adds a States section covering the chip vocabulary and the anchor above → graded in rubric R6

### §6 — The dev workspace stops showing test debris

- **Dimension 6.1** — every leaked `acc-*` acceptance fixture fleet in the dev workspace is deleted through the product's delete path; count after sweep is zero → graded in rubric R5

## Interfaces

```
protocol_bind.zig (new, locked):
  pub const BASELINE_RW_TMPFS: []const []const u8   // paths bwrap mounts as private tmpfs AND landlock grants write

Unchanged surfaces (must not drift):
  PATCH /v1/fleets/runners/{id}  body {"action": "cordon"|"drain"|"revoke"|"self_test"}
  GET  /v1/fleets/runners/{id}/leases?workspace_id&fleet   — parse and match semantics identical
  runner-copy.ts constant NAMES (values change; identifiers stay)
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| tmpfs write still fails | disk/memory pressure inside the lease cgroup | engine error propagates on the existing crash path; lease shows the error name as detail (regression-pinned) |
| floor drift | future writable mount added to one layer only | property tests on both sides fail the build — the drift cannot merge |
| operator binds `/tmp` | bind list names a sensitive path | refusal unchanged; negative test pins it |
| disabled action clicked | operator clicks Cordon/Drain | no dialog, no PATCH; button exposes the disabled reason |
| filter garbage | bare tokens other than `and`, unknown keys | dropped silently exactly as today (regression-pinned) |

## Invariants

1. Every path bwrap mounts writable appears in exactly one shared floor, and landlock's write rules enumerate that floor — enforced by the two property tests, one per layer.
2. `/tmp` is operator-unbindable — enforced by the sensitive-path refusal and its negative test.
3. Every header action pairs a text label with an `aria-hidden` icon — enforced by the icon-presence component test.
4. Filter copy lives only in `runner-copy.ts` constants — enforced by tests importing the constants (the two hardcoded strings are removed by this spec).

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | — | — | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_every_writable_floor_path_is_a_tmpfs_in_argv` | argv built for a lease contains `--tmpfs <p>` for every floor path; no writable mount outside the floor |
| 1.2 | unit | `landlock write set contains every writable-floor path` | ruleset write rules == workspace + rw binds + floor; `/tmp` absent from read-only rules |
| 1.3 | integration | `a lease child writes its private tmpfs` | create/write/unlink `/tmp/probe` inside a real sandbox succeeds (Linux privileged lane) |
| 1.4 | unit | `test_sensitive_paths_still_refuse_tmp` | extra bind naming `/tmp` → refused, same error as today |
| 2.1 | unit | `runner header runs checks and shows the requested state` | button "Run checks"; after request, disabled with "Checks requested" |
| 2.2 | unit | `every header action renders an icon beside its label` | 6 buttons each contain an svg with `aria-hidden="true"` |
| 2.3 | unit | `cordon and drain are disabled and inert` | both disabled; click fires no dialog and no PATCH |
| 2.4 | unit | `refresh button requests a router refresh` | click → router.refresh called once |
| 3.1 | unit | `LeaseFilterBar renders the renamed copy` | label "Filter", button "Apply" via imported constants |
| 3.2 | unit | `tokenizer accepts the and connective` | `workspace:x and fleet:y` → both filters set; `and:x` stays a dropped unknown key |
| 4.1 | unit | `lease and activity tables use the shared time vocabulary` | headers "Time"/"Duration" present; "When"/"Took" absent |
| 4.2 | unit | `checks timestamp reveals the absolute time` | `Time` renders with tooltip enabled in the checks panel |
| 5.1 | unit | `states chip links the runners page` | anchor href equals the published runners URL constant |
| regression | integration | existing lease-list + runner-patch suites | filter semantics and PATCH actions byte-identical |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Both sandbox layers derive from the floor (§1) | `TEST_FILTER="writable-floor" zig build test --summary all; echo RC=$?` and `zig build list-tests \| grep -c "writable-floor"` | `RC=0`; count ≥ 2 | P0 | |
| R2 | Old copy is gone (§2–§3) | `grep -rn "Apply filter\|Filter leases\|Run self-test" ui/packages/app --include='*.ts' --include='*.tsx'` | 0 matches | P0 | |
| R3 | Time vocabulary aligned (§4) | `grep -n '"When"\|"Took"' ui/packages/app/app/\(dashboard\)/admin/runners/\[runnerId\]/components/LeaseTable.tsx ui/packages/app/app/\(dashboard\)/admin/runners/\[runnerId\]/components/ActivityTable.tsx` | 0 matches | P0 | |
| R4 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the table | P0 | |
| R5 | Dev evidence + sweep (§1.5, §6) | documented lease query + fleet delete calls against dev; counts pasted to PR Session Notes | `TempFileCreateFailed` rows captured; `acc-*` fleet count 0 | P1 | |
| R6 | Docs States section paired (§5) | `gh pr checks <docs-pr> --repo agentsfleet/docs` | exit 0 | P1 | |
| S1 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S2 | Lint clean | `make lint-all` | exit 0 | P0 | |
| S3 | Integration passes | `make test-integration` | exit 0 | P0 | |
| S4 | App smoke walks the rendered page | `make dry-app` | exit 0 | P0 | |
| S5 | No leaks | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile | `zig build --build-file build_runner.zig -Dtarget=x86_64-linux-musl && zig build --build-file build_runner.zig -Dtarget=aarch64-linux-musl; echo RC=$?` | `RC=0` | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

N/A — no files deleted; constant identifiers keep their names, only values change.

## Out of Scope

- Actor identity join (render email/display name for `steer:user_…`) — Indy declined it this session; the raw actor stays.
- Platform-admin cross-tenant fleets visibility — its own milestone (admin read-only route recommended separately).
- Runner-scoped event streaming or any polling — Indy chose the manual refresh affordance.
- Model-library dialog changes — investigated, nothing optional end-to-end, nothing to change.
- Slack secret naming — both vault entries are live and distinct; working as designed.
- Filter connectives beyond the single word `and`.

---

## Product Clarity (authoring record)

1. **Successful user moment** — the platform admin clicks **Run checks**, taps the refresh icon, watches ALL CHECKS PASSED land, and the newest lease row reads SUCCEEDED instead of "The runner crashed — TempFileCreateFailed".
2. **Preserved user behaviour** — filter tokens `workspace:`/`fleet:` and their match semantics; Revoke and Edit policy flows; the self-test request round-trip; lease sort on the time column.
3. **Optimal-way check** — for the sandbox defect, the derived floor IS the unconstrained-optimal shape (the read side already proved it); for the page, copy/icon/disable changes are the direct path — no gap.
4. **Rebuild-vs-iterate** — iterate; the one structural piece (the floor) rides inside. A runner event stream would be a rebuild of the page's data path and is rejected here.
5. **What we build** — the shared writable floor + two property pins + one real-sandbox proof; renamed copy; six icons; two disabled actions; a refresh button; a restored tooltip; a learn-more link; a docs States section (paired branch); the dev evidence pull and fixture sweep.
6. **What we do NOT build** — polling/SSE (manual refresh chosen); actor identity join (declined); admin cross-tenant fleets route (separate spec); connective grammar beyond `and`.
7. **Fit with existing features** — unblocks M136 §1–§5 live proof; must not destabilize the enroll/edit policy dialogs sharing `PolicyFields` or the runner PATCH surface.
8. **Surface order** — UI-first; the runner-side fix has no CLI surface.
9. **Dashboard restraint** — Cordon/Drain stay visible but disabled until their operation is actually supported; no dead controls pretending to work.
10. **Confused-user next step** — the `?` beside the states chip lands on the published runners page section that explains exactly the states and actions on screen.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream, six Sections — the page polish rides with the sandbox fix because the fix unblocks the live pass this page fronts, and the PR budget is one per milestone.
- **Alternatives considered:** (a) hand-grant `/tmp` write in `landlock.zig` alone — rejected: the two layers stay hand-synced, the drift class that produced both live incidents; (b) TMPDIR pointed at the workspace — rejected: relocates bearer-token scratch into durable storage; (c) split the sandbox fix into its own PR — rejected: same reviewers, same deploy, double babysitting.
- **Patch-vs-refactor verdict:** this is a **patch** for the page and a **small refactor** for the sandbox floor, because the problem's shape (two layers, one truth) demands a shared source, exactly as the read side concluded in M136.

## Discovery (consult log)

- **Consults** — Indy (Aug 19, 2026, in-session): picked "Run checks" over diagnostics/verify-sandbox; replaced the proposed poll with a manual refresh icon ("A CHEAP ALTERNATIVE IS AN ICON WITH REFRESH TYPE THAT REFRESHES THE PAGE, THIS IS MANUAL BY THE PLATFORM ADMIN"); chose the derived write floor over the five-line landlock patch; approved Time/Duration renames, the `and` connective, and the fixture sweep; **declined the actor identity join**; security review of the tmpfs grant asked and answered (private per-lease tmpfs, host `/tmp` unreachable, cgroup-bounded).
- **Metrics review** — no analytics/funnel playbook update required: no event added, renamed, or removed.
- **Skill-chain outcomes** — populated at CHORE(close).
- **Deferrals** — none at authoring.
