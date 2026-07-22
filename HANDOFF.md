# Handoff — M139_001 fleet event legibility

**Ephemeral.** Delete this file at CHORE(close), before the PR (AGENTS.md: handoff docs brief the next agent, never the PR).

## Scope / Status

Making a struggling fleet self-explaining: every failure carries its cause end to end, and repeated events collapse instead of flooding the chat. Spec: `docs/v2/active/M139_001_P1_API_UI_FLEET_EVENT_LEGIBILITY.md` (Status IN_PROGRESS, 9 of 23 Dimensions DONE).

- ✅ **CHORE(open)** — spec active, worktree, Test Baseline `unit=2809 integration=371`
- ✅ **Design** — `/design-shotgun` board at `docs/design/fleet-events-board.html`; **Indy picked variant B "Timeline Rail"** (activity as inline rail ticks, floating failure card, pill filter, leading Runs count column). Variant page carries a sequence-behaviour demo answering Indy's chronology question.
- ✅ **§1 Durable failure cause** (1.2, 1.3, 1.5, 1.6 DONE) — cause named at every `startup_posture` classification site, riding frame → report → row → envelope → live completion frame. 1.1/1.4 need the DB lane (below).
- ✅ **§2.1 Vocabulary** — outcome renders `sentence — cause`; live frame no longer needs a reload.
- ✅ **§8 Refactor** (8.1–8.4 DONE) — folded in mid-flight by Indy (see D4 below).
- ⏳ **§2.2** guidance line render — not started
- ⏳ **§3 / §4 / §5 / §7** — the entire dashboard surface (compact rows, coalescing, banner, grouped Events table). **Not started.**
- ⛔ **§6 filter — DEFERRED** by Indy, ack-quote in spec Discovery. Thread always shows all rows.

## Working tree

Clean. Branch `feat/m139-event-legibility`, **7 commits ahead of `main`, nothing pushed, no PR yet.**

```
4e6d3b418 refactor(contract): tagged execution outcome + one report conversion pair
59fd8ed16 feat(events): failure cause end to end — classification site to live frame
9066fc6ba feat(schema): additive migration model; failure_detail lands as slot 032
511913f35 docs(m139): defer §6 chat filter per Indy — thread always shows all
a029a69bd docs(m139): design board — variant B Timeline Rail picked
93f791e3b docs(m139): fill PLAN handshake — restatement and assumptions
0cc39fa18 chore(m139): open M139_001 — spec to active, worktree + baseline
```

Worktree: `~/Projects/agentsfleet-m139-event-legibility` (hydrated: root `bun install` + `cli` built).

## Two owner decisions that changed the rules — read before touching schema

**D3 — the schema model changed repo-wide.** Indy: *"Drop the gate entirely, and all migration will need the alter or add column and so on hence forth, the old schema should nt be touched."* So: **every schema change is now a new numbered migration** (`ALTER TABLE … ADD COLUMN`); **shipped slot files `001`–`031` are frozen history — never edit them.** `check-schema-gate` / `_schema_gate_check` were removed from `make/quality.mk`, `Makefile` help, and `.githooks/pre-commit`; `docs/SCHEMA_CONVENTIONS.md` §Migration Model was rewritten. `schema/032_fleet_events_failure_detail.sql` is the first slot under the new model.

> **Follow-up owed (not done):** the orly-side rules still describe the teardown model — `dispatch/write_sql.md` §Pre-v2.0.0 and the AGENTS.md Schema Guard prose. Needs an `edit_rules` flow sync. Recorded in spec Discovery.

**D4 — refactor scope folded into this PR.** Indy: *"I want all the finding refactor to be fixed in this PR."* Three findings surfaced by §1's cost (one nullable column touched twelve production files); all three are now landed in `4e6d3b418`:

1. `ExecutionResult` encoded its verdict as `exit_ok: bool` + `failure: ?FailureClass` — four states for two meanings, with the "cause only on a failure" invariant hand-written at three consumers. Now a tagged `outcome` union; the illegal pairing is unrepresentable.
2. The same type was disassembled into `ReportRequest` and rebuilt field-by-field on the far side. `src/lib/contract/report_mapping.zig` now owns both directions, proven by a round-trip test.
3. `publishEventComplete` took adjacent same-typed strings; now a named `FailureCause`.

Design note worth keeping: **the child `result` frame is intra-runner** (the daemon forks its own binary), so it carries the union directly. **`ReportRequest` is the genuine cross-version wire** and stays flat + defaulted. Don't collapse the two.

## Tests / checks

- ✅ `make test-unit-all` — all lanes, all package coverage gates
- ✅ `make lint-zig`
- ✅ `zig build -Dtarget=x86_64-linux` + `-Dtarget=aarch64-linux`
- ✅ `cd ui/packages/app && bun run typecheck` + `bun test` (64/64 on the two touched files)
- ⏳ `make test-integration` — **not run** (needs Postgres; §1 Dimensions 1.1/1.4 depend on it)
- ⏳ `make memleak`, `make acceptance-e2e`, `make lint-all` — not run yet
- ⚠️ Raw `zig build test` reports `agentsfleetd-tests` failed under the build-runner `--listen` protocol while the same binary passes standalone (incl. the failing seed) and `make test-unit-agentsfleetd` is green. Off-diff harness quirk, noted in spec Discovery. **Verify on the `make` lanes** (`docs/VERIFY_TIERS.md`), not raw `zig build test`.

## Next steps (ordered)

1. `make test-integration` with Postgres up → close §1 Dimensions 1.1 + 1.4 (`failure_detail` roundtrip + envelope).
2. §2.2 — render the `guidance` line (`event-summary.ts` has the hook authored; it still has no consumer, which is why RULE NDC/NLR flagged it).
3. §3 → §4 → §5 — the chat surface, built to **variant B**. Grouping is a **pure memoized function over the already-ordered event array**; the stream registry keeps ownership of order and identity.
4. §7 last — **rebase over `main` before touching `EventsList.tsx`** (see risk below).
5. Fold in the third refactor finding's UI half if you touch `fleet-stream-frames.ts`: `applyChunk` and `applyEventComplete` each do `prev.find(...)` then `prev.map(...)` — two full passes per streaming chunk. `applyToolCall` already shows the single-pass `findIndex` + `[...prev]` + index-write shape to match. (Not yet done; not blocking.)
6. `/write-unit-test` → runtime `/review` → CHORE(close) → `kishore-babysit-prs`.

## Risks / gotchas

- **`feat/data-table-tanstack` is live in a sibling worktree** (`/private/tmp/agentsfleet-data-table`, zero commits as of Jul 22 — all work uncommitted). It migrates the design-system `DataTable` that §7 renders through. Only collision point is `EventsList.tsx`. Don't leave both PRs open on it; whoever lands second rebases. If theirs lands first it's a tailwind — TanStack has native grouped/expandable row models §7 can sit on.
- **Ownership convention for `failure_detail`:** child-side values are static and serialized; parent-side values are alloc-owned and freed under the same len guard as `content`. Documented once on the wire field. A pre-existing supervisor test leaked because my change made two exit paths allocate — the leak detector caught it; expect the same if you add a classification site.
- **`protocol.zig` is at 349/350 lines** — anything new goes in a sibling file (that's why `report_mapping.zig` exists). `runner.zig` is at 349 too.
- The spec is **311/320 lines** — near the hard cap. Compress before adding sections.
- Underlying operational issue is out of scope: `github-pr-reviewer` in workspace `gentle-mesa-130` genuinely has no instructions configured. M139 makes that legible; it does not fix that fleet.
