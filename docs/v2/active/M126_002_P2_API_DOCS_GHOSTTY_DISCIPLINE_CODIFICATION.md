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

# M126_002: Ghostty Allocator and concurrency discipline becomes citable rules, deterministic checks, an architecture doc, and a retrofitted lifecycle spine — with an adherence matrix proving it

**Prototype:** v2.0.0
**Milestone:** M126
**Workstream:** 002
**Date:** Jul 11, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — governance, lint, docs, and comment/annotation retrofit; prevents the M126_001 defect classes from recurring rather than fixing a live defect.
**Categories:** API, DOCS
**Batch:** B2 — after M126_001 merges (shared lifecycle-spine files; rule A6 cites the tripwire module 001 vendors). May run parallel with M126_003 (disjoint files except `make/quality.mk` — coordinate at PLAN if both are active).
**Branch:** `feat/m126-001-shutdown-race-leak-fixes` — folded into the M126_001 worktree/PR per Indy (Jul 11, 2026)
**Test Baseline:** unit=2500 integration=299 — recorded at CHORE(open), Jul 11, 2026, via `make _lint_zig_test_depth` on `feat/m126-001-shutdown-race-leak-fixes` @ `35dc828e1`.
**Depends on:** M126_001 (tripwire module exists; spine files carry 001's fixes before retrofit annotates them).
**Provenance:** agent-generated (pre-spec, `docs/v2/reviews/m126-ghostty-adversarial-review.md` §5 — ghostty practice mining; Indy directed alignment on SPSC ownership transfer, shutdown ordering, documented lock invariants, tripwire, and a strict Allocator model, recorded in `dispatch/write_zig.md` with Invariance Suite proof).
**Canonical architecture:** `docs/architecture/concurrency.md` — created by this workstream (§3); becomes the doc `name_architecture` consults for every future stream/queue/thread naming.

---

## Overview

**Goal (testable):** Every rule in the Allocator model (A1–A6) and concurrency discipline (C1–C5) is citable by ID in `dispatch/write_zig.md`, backed by ≥1 question in the governance questionnaire (Invariance Suite green), enforced by a deterministic repo lint where mechanizable, documented in `docs/architecture/concurrency.md`, and **fully adhered to in code across a named compliance base of folders** — blocking enforcement scoped by an expandable roster file, so adding the next folder is one roster line — with per-folder results reported row-by-row in this spec's Adherence & Enforcement Matrix.
**Problem:** The adversarial review showed our race/leak confidence rests on implicit convention: ghostty's equivalent discipline is structural and reviewable (Single-Producer Single-Consumer (SPSC) channels, receiver-frees ownership transfer, stop→join→deinit, one documented mutex per aggregate, tripwire-proven errdefer ladders), while our rules live in scattered file-local idioms an editing agent has no gate against violating — which is exactly how L1 diverged from its two correct sibling sweepers.
**Solution summary:** Codify the model as numbered rules in the Zig dispatch façade (dotfiles, via the `edit_rules` procedure), add a deterministic lint for the mechanizable subset (deinit poisoning, ownership doc phrases, advisory errdefer heuristic) whose **blocking scope is a roster of path prefixes** (`audits/zig-discipline-roster.txt`), write the concurrency architecture doc (thread map, channel inventory, lock-invariant registry, shutdown choreography), bring the seven-entry compliance base to full in-code adherence (ladders, poisoning, phrases, tripwire fail points, documented mutexes, confinement comments — not annotations alone), and fill an in-spec matrix at CHORE(close) proving rule ↔ enforcement ↔ questionnaire ↔ per-folder result for every rule.

## PR Intent & comprehension handshake

- **PR title (eventual):** chore(discipline): codify Allocator/concurrency rules, lint, architecture doc, spine retrofit (M126_002)
- **Intent (one sentence):** Future Zig edits in this repo are bound to ghostty-grade Allocator and concurrency discipline by gates that fire at edit time, with a one-page matrix proving what is enforced and how.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `docs/v2/reviews/m126-ghostty-adversarial-review.md` §5 — the mined practices with ghostty file:line cites; the rules below are their codification, do not re-derive them.
2. `~/Projects/dotfiles/dispatch/write_zig.md` + `~/Projects/dotfiles/dispatch/edit_rules.md` — the façade being amended and the meta-dispatch that governs amending it (Invariance Suite, questionnaire, signoff, rule-extension protocol — all four steps in the same dotfiles diff).
3. `~/Projects/oss/ghostty/src/config/Config.zig` (the `_arena` pattern), `Surface.zig:616-664` (block-scoped errdefer handoff), `renderer/State.zig:10-14` (documented mutex invariant), `termio/Termio.zig:759-764` (thread-confined comment) — the exemplars each rule cites.
4. `docs/architecture/` — existing doc set and voice; the new concurrency doc matches it. The review's thread map (§A of the race findings) is the seed content.
5. `lint-zig.py` + `make/quality.mk` — where the repo's deterministic Zig checks live and how they wire into `make lint`; new checks follow the same shape.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `~/Projects/dotfiles/dispatch/write_zig.md` | EDIT (cross-repo) | Rules A1–A6 + C1–C5 added as citable numbered rules; committed in dotfiles via `edit_rules` procedure, not this PR |
| `~/Projects/dotfiles/audits/agents-md.md` | EDIT (cross-repo) | ≥1 questionnaire question per rule (11 minimum); same dotfiles diff |
| `docs/architecture/concurrency.md` | CREATE | Thread map, channel inventory with SPSC roles, receiver-frees ownership convention, lock-invariant registry, shutdown choreography |
| `lint-zig.py` | EDIT | Deterministic checks: deinit poisoning, ownership doc phrases, advisory multi-try-no-errdefer heuristic; reads the roster to scope blocking vs advisory |
| `make/quality.mk` | EDIT | New checks wired into the lint lane |
| `audits/zig-discipline-roster.txt` | CREATE | Path prefixes where A/C rules are binding (the compliance base); the expansion lever — one line per future folder |
| `src/agentsfleetd/cmd/*.zig` | EDIT | Compliance base: full A2/A5/A6/C4/C5 adherence (boot/shutdown choreography home) |
| `src/agentsfleetd/events/*.zig` | EDIT | Compliance base (bus, subscription hub, stream registry) |
| `src/agentsfleetd/fleet/*.zig` | EDIT | Compliance base (the three sweepers and fleet runtime helpers in-folder) |
| `src/agentsfleetd/queue/*.zig` | EDIT | Compliance base (outbound decoder, redis pool/subscriber) |
| `src/runner/daemon/*.zig` | EDIT | Compliance base (worker pool, control loop) |
| `src/runner/child_supervisor*.zig` | EDIT | Compliance base (supervisor read/reap paths; file-prefix roster entry) |
| `src/lib/**/*.zig` | EDIT | Compliance base (tripwire, logging sinks, call_deadline — shared substrate) |
| colocated `*_test.zig` within the base | CREATE/EDIT | A6 loop-all-failpoints tests for every multi-step init in the base |
| `tests/lint/` fixtures (path per existing lint-fixture convention; create if none) | CREATE | Seeded-violation fixtures proving each new check detects its target, inside and outside the roster |

Cross-repo note: the two dotfiles rows ride the `edit_rules` procedure (Invariance Suite, signoff, immediate dotfiles commit+push), not this PR; the §5 matrix records their evidence.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — GRD (ground in the source of truth — every codified rule cites the ghostty or in-repo exemplar it came from), NDC (no dead code in retrofit — annotations and poisoning only, no speculative helpers), NLR (touch-it-fix-it inside retrofitted functions), UFS (lint check names/phrases as named constants in `lint-zig.py`), OBS (no new observable states expected; if retrofit surfaces one, it logs), TST-NAM (fixture test names milestone-free), ORP (lint-fixture files wired, not orphaned).
- `dispatch/write_zig.md` — fires on every retrofitted `*.zig` file (the amended façade applies to its own retrofit — the first consumers of A1–A6/C1–C5 are this workstream's edits).
- `dispatch/edit_rules.md` — the dotfiles half is governance: questionnaire all-YES + `make audit` ALL CHECKS PASSED + `.agents-invariance-signoff` are mandatory before the dotfiles push.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes — retrofit touches Zig files | cross-compile both linux targets; retrofit is annotation/comment/poisoning — behavior-preserving, existing tests stay green |
| PUB / Struct-Shape | no — no new pub surface (lint checks live in the existing script) | — |
| File & Function Length (≤350/≤50/≤70) | yes | `lint-zig.py` grows: split check functions if the file approaches its own cap; retrofitted files must not cross 350 from added comments — trim or split |
| UFS (repeated/semantic literals) | yes | ownership phrases ("caller must free", "takes ownership") and check names are named constants in the lint |
| UI Substitution / DESIGN TOKEN | no — no UI files | — |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no new log states, lifecycle edits are comment-level, no error codes, no schema | deinit-pairs audit (`audits/deinit-pairs.sh`) stays green over retrofitted files |
| Invariance Suite (dotfiles) | yes — governance edit | questionnaire all-YES, `make audit` ALL CHECKS PASSED, signoff written; evidence copied into §5's matrix |

## Prior-Art / Reference Implementations

- **Reference:** `~/Projects/oss/ghostty/src/` — each rule cites its exemplar: `Config.zig` `_arena` (A4), `PageList.zig:85-102` ladder (A2), `Surface.zig:616-664` block handoff (A2), `global.zig:74-96` allocator selection (A1), `renderer/State.zig:10-14` mutex doc (C4), `termio/mailbox.zig:61-93` unlock-send (C3), `blocking_queue.zig` SPSC design doc (C1), `Surface.zig:772-798` shutdown ordering (C2), `tripwire.zig` (A6). Divergence: ghostty's rules live in code idiom; ours land as a citable façade because our editing surface is agents bound by gates.
- **In-repo:** `audits/deinit-pairs.sh` and `_lint_zig_pg_drain` — the existing deterministic-check shapes the new lint checks mirror.

## Sections (implementation slices)

### §1 — Rule codification in the Zig dispatch façade (dotfiles)

`dispatch/write_zig.md` gains two numbered rule groups the executing agent can cite by ID.
**Allocator model:** A1 one backing allocator chosen in `main`, passed as parameter, never
held in global state for Zig code paths · A2 errdefer immediately after every acquisition;
`errdefer comptime unreachable` after the last fallible op; block-scoped errdefers with one
composite errdefer after ownership handoff · A3 leaf structures unmanaged (alloc per call,
store nothing); only lifecycle roots keep an `alloc` field · A4 arena as ownership unit
(`_arena` per config/request object; scratch arena per transient operation; arena-in-message
across threads) · A5 ownership stated in the fixed phrases "caller must free" / "takes
ownership" on every allocating pub fn; `self.* = undefined` poisoning in every deinit ·
A6 multi-step init carries tripwire fail points plus a loop-all-failpoints test under
`std.testing.allocator`. **Concurrency discipline:** C1 cross-thread channels are SPSC with
declared producer/consumer; payloads carry their allocator; receiver frees in a defer ·
C2 shutdown is stop-signal → join → deinit, never free-on-timeout · C3 no blocking push/write
while holding a lock the consumer needs; lock state is an explicit parameter · C4 one
documented mutex per shared aggregate stating exactly what it protects; `lock(); defer
unlock();` adjacent · C5 thread-confined state is the default, marked by "only touched by
thread X" comments and `*Locked` suffixes. Each rule carries its exemplar cite (ghostty or
in-repo) per RULE GRD.

- **Dimension 1.1** — A1–A6 present in `dispatch/write_zig.md`, numbered and citable, each with an exemplar cite → Test `dotfiles make audit` (ALL CHECKS PASSED) + grep verify in the rubric — DONE (dotfiles `58eedd3`; R1 grep = 11)
- **Dimension 1.2** — C1–C5 present, same shape → same verification — DONE (dotfiles `58eedd3`)
- **Dimension 1.3** — ≥1 questionnaire question per rule in `audits/agents-md.md` (≥11 new), all-YES on the suite run; signoff written → Test: `make audit` output + signoff file, evidence into §5's matrix — DONE (Scenario 25, 12 questions; `.agents-invariance-signoff` = `58eedd3 … PASS`)

### §2 — Deterministic checks, roster-scoped (repo lint)

`lint-zig.py` gains three checks wired into `make lint`, each proven by a seeded-violation
fixture. Enforcement is scoped by `audits/zig-discipline-roster.txt`: **inside a roster
prefix the poisoning and ownership-phrase checks BLOCK; outside they warn** — so the
discipline is binding where the base has been laid and visible everywhere else. The errdefer
heuristic ships advisory-only everywhere ("multi-`try` init without errdefer" has legitimate
exceptions; promotion to blocking is a later judgment with data).

- **Dimension 2.1** — poisoning check: a `deinit` lacking `self.* = undefined` fails the lint inside the roster, warns outside; fixtures prove both behaviors; tree passes → Test `test_lint_detects_missing_poison` — DONE
- **Dimension 2.2** — ownership-phrase check: an allocating pub fn (per the check's documented signature heuristic) without a "caller must free"/"takes ownership" doc line fails inside the roster, warns outside; fixtures prove both; tree passes → Test `test_lint_detects_missing_ownership_phrase` — DONE
- **Dimension 2.3** — advisory errdefer heuristic: ≥2 `try` acquisitions in one init-shaped fn with zero errdefer emits a warning listing file:fn; never blocks → Test `test_lint_warns_multi_try_no_errdefer` — DONE
- **Dimension 2.4** — roster scoping: the same seeded violation placed inside vs outside a roster prefix produces fail vs warn; roster parsing tolerates comments/blank lines → Test `test_lint_roster_scoping` — DONE

### §3 — Concurrency architecture doc

`docs/architecture/concurrency.md` becomes the durable model: the daemon+runner thread map
(every spawned thread: spawner, shared state touched, protection), the channel inventory with
SPSC roles and payload-ownership per channel, the lock-invariant registry (each mutex, what it
protects, ordering constraints), and the shutdown choreography (stop→join→deinit sequence per
subsystem, including M126_001's fixed ordering). Seed content is the review record's thread
map; the doc states the C-rules as the system's invariants so `name_architecture` consults
land here.

- **Dimension 3.1** — doc exists with the four subsections above and matches the code as merged after M126_001 → Test: rubric grep rows (subsection headings present; every mutex named in the spine appears in the registry) — DONE (R5 grep = 4; README row added)

### §4 — Compliance base: full adherence in named folders, expandable by roster

The discipline lands in code, not only prose (Indy's direction, Jul 11, 2026: adhered to in
the codebase for named folders, base laid so it keeps expanding). The base roster:

| Roster prefix | Why it is in the base |
|---|---|
| `src/agentsfleetd/cmd/` | boot/shutdown choreography — R1/R3/R7 home |
| `src/agentsfleetd/events/` | bus, subscription hub, stream registry — R2/R4/R5 home |
| `src/agentsfleetd/fleet/` | the three sweepers — L1/L2 home |
| `src/agentsfleetd/queue/` | outbound decoder, redis pool/subscriber — L3 + wire discipline |
| `src/runner/daemon/` | worker pool, control loop — thread lifecycle |
| `src/runner/child_supervisor` | supervisor read/reap paths (file-prefix entry) |
| `src/lib/` | tripwire, logging sinks, call_deadline — shared substrate |

Full adherence means: complete errdefer ladders (A2), ownership phrases + poisoning (A5),
tripwire fail points and a loop-all-failpoints test on **every multi-step init in the base**
(A6), a documented invariant on every mutex (C4), thread-confinement comments (C5), and an
A1/A3/A4 judgment pass (allocator threading, leaf-unmanaged shape, arena ownership) recorded
per folder in the §5 matrix. Runtime-behavior-preserving: fail points are comptime-erased,
poisoning touches only deinit; existing suites stay green. **Expansion is one roster line:**
add a prefix → `make lint` surfaces that folder's violations → fix → commit. RULE NLR
(touch-it-fix-it) covers files outside the roster until their folder joins.

- **Dimension 4.1** — every base prefix passes the §2 blocking checks with zero waivers → Test: `make lint` green with the roster active — DONE (18 deinits poisoned + 6 phrases; `redis_connection` uses a documented `// discipline: ok` terminal-state guard; `make lint-zig` green)
- **Dimension 4.2** — every `Mutex`/`RwLock` in the base carries an invariant doc comment; count matches the §3 registry → Test `test_base_mutexes_documented` (grep-driven assertion) — DONE (8 base mutexes documented; audit skips `test`-block helpers)
- **Dimension 4.3** — every multi-step init in the base carries tripwire fail points with a loop-all-failpoints test under `std.testing.allocator`; the per-folder ledger lands in the §5 matrix → Test: the per-folder `*_test.zig` additions (A6 shape) — DONE (queue/ tripwire exemplar `plain_init_tw` + loop-all-failpoints test in `redis_test.zig`; fleet/ sweepers carry `fetch_tw` loop-all from M126_001; the connector decoders carry `checkAllAllocationFailures`; the I/O-bound dial/handshake/spawn inits are proven by the integration connect-failure paths + worker_pool partial-spawn errdefer — per-folder ledger in §5)
- **Dimension 4.4** — the roster ships with exactly the seven base prefixes above and the expansion procedure is documented beside them (comment header in the roster + a paragraph in `docs/architecture/concurrency.md`) → Test: rubric grep — DONE (R6 grep = 7; expansion procedure in roster header + concurrency.md §Expanding the discipline base)

### §5 — Adherence & Enforcement Matrix (filled at CHORE(close))

The report Indy reads: one row per rule. Authoring leaves Result columns empty; CHORE(close)
fills them from actual outputs and PR Session Notes carries the long evidence.

| Rule | Recorded at | Enforcement | Signal | Questionnaire | Base-folder adherence (per prefix, at close) |
|------|-------------|-------------|--------|---------------|----------------------------------------------|
| A1 backing allocator | write_zig.md §A1 | façade prose + per-folder judgment pass (4.3) | 🔵 | Q-ref at close | |
| A2 errdefer ladder | write_zig.md §A2 | prose + advisory lint (2.3) + A6 tests in base | 🔵+warn | Q-ref at close | |
| A3 leaf unmanaged | write_zig.md §A3 | façade prose + per-folder judgment pass | 🔵 | Q-ref at close | |
| A4 arena ownership | write_zig.md §A4 | façade prose + per-folder judgment pass | 🔵 | Q-ref at close | |
| A5 phrases + poison | write_zig.md §A5 | blocking lint inside roster (2.1, 2.2) | 🔴/🟢 | Q-ref at close | |
| A6 tripwire on multi-step init | write_zig.md §A6 | loop-all-failpoints tests per base folder (4.3) | 🔴/🟢 | Q-ref at close | |
| C1 SPSC + receiver frees | write_zig.md §C1 | façade prose + architecture doc channel inventory | 🔵 | Q-ref at close | |
| C2 stop→join→deinit | write_zig.md §C2 | façade prose + M126_001/003 lifecycle tests | 🔵 | Q-ref at close | |
| C3 no blocking under lock | write_zig.md §C3 | façade prose + stalled-peer regression | 🔵 | Q-ref at close | |
| C4 documented mutex | write_zig.md §C4 | façade prose + base grep parity (4.2) | 🔵+grep | Q-ref at close | |
| C5 thread-confined default | write_zig.md §C5 | façade prose + base confinement comments (4.3) | 🔵 | Q-ref at close | |

The last column is graded per roster prefix, so the matrix reads as rule × folder.

- **Dimension 5.1** — matrix fully filled at CHORE(close): every row carries its questionnaire reference and retrofit result; `make audit` ALL CHECKS PASSED output cited → Test: rubric row R4 (no empty matrix cells at close)

## Interfaces

```
No HTTP endpoint, wire shape, or CLI change. Surfaces this spec pins:

dispatch/write_zig.md rule IDs A1–A6, C1–C5 — citable identifiers; future specs and reviews
reference them by ID, so renaming them later is a governance edit.

lint-zig.py exit semantics — blocking checks exit nonzero with file:line:rule-id output;
the advisory check prints warnings and exits 0. Wired into `make lint` (repo canonical lane).

docs/architecture/concurrency.md — the doc name_architecture consults; sections: Thread map,
Channel inventory, Lock-invariant registry, Shutdown choreography.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Invariance Suite fails on the dotfiles diff | missing questionnaire step / parity break | dotfiles push blocked; fix the four-step diff; matrix cannot be filled until green |
| Lint check false-positives on legitimate code | heuristic too broad | blocking checks are exact-shape by design; a real false-positive is a judgment flag → Indy decides scope per gate-flag triage, never a silent harness patch |
| Retrofit pushes a file past 350 lines | added doc comments | split the file per LENGTH gate — never trim the invariant comment to fit |
| Spine test breaks under retrofit | poisoning write exposes a use-after-deinit | that is the check working: fix the caller (a real latent bug), record in Discovery |
| Matrix row unfillable at close | a rule landed without its question or check | CHORE(close) blocks — incomplete scope, not deferral, absent an Indy-acked quote |

## Invariants

1. Every rule ID in the façade has ≥1 questionnaire hit — enforced by the dotfiles audit (`make audit` parity checks) and rubric grep.
2. Blocking lint checks are deterministic and roster-scoped — exit code + file:line output; inside-vs-outside behavior proven by seeded fixtures run in `make lint`.
3. Base adherence is runtime-behavior-preserving — fail points comptime-erased, poisoning in deinit only; enforced by the unchanged pre-existing suites (`make test` / `make test-integration` green with zero edits to pre-existing tests, additions only).
4. `AGENTS.md` byte budget untouched — rules live in the façade; enforced by the dotfiles audit's existing size check.
5. Expanding enforcement to a new folder requires exactly one roster line — enforced by the lint reading the roster at runtime; no code change needed for scope growth.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable | — | — | — | — | governance, lint, docs, and annotations only — no product or operator signal changes; Discovery records "Metrics review: no analytics/funnel playbook update required — no runtime behavior change" |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1–1.3 | audit (deterministic) | dotfiles `make audit` + rubric greps | rule headings present; ≥11 new questions; ALL CHECKS PASSED; signoff file updated |
| 2.1 | unit (fixture) | `test_lint_detects_missing_poison` | seeded deinit without poison inside roster → nonzero naming file:line; clean tree → exit 0 |
| 2.2 | unit (fixture) | `test_lint_detects_missing_ownership_phrase` | seeded allocating pub fn without phrase inside roster → nonzero; clean tree → exit 0 |
| 2.3 | unit (fixture) | `test_lint_warns_multi_try_no_errdefer` | seeded two-try init without errdefer → warning line emitted; exit 0 either way |
| 2.4 | unit (fixture) | `test_lint_roster_scoping` | same violation inside vs outside a roster prefix → fail vs warn; comments/blank roster lines tolerated |
| 3.1 | audit (grep) | rubric grep rows | four subsection headings present; every base mutex named in the registry |
| 4.1 | integration (lint lane) | `make lint` | exit 0 with the roster active — all seven base prefixes compliant, zero waivers |
| 4.2 | audit (grep) | `test_base_mutexes_documented` | mutex-decl count == invariant-comment count across the base |
| 4.3 | unit | per-folder A6 loop-all-failpoints tests | every multi-step init in the base: each fail point injected → error surfaces, zero leaks, state rolled back |
| 4.4 | audit (grep) | rubric R6 | roster carries exactly the seven base prefixes + expansion procedure documented |
| 5.1 | audit (close gate) | rubric R4 | zero empty cells in the §5 matrix at CHORE(close) |
| regression | integration | `make test && make test-integration` | retrofitted subsystems' suites unchanged and green |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Rules citable: A1–A6 + C1–C5 headings exist in the façade (§1) | `grep -cE '^#+ .*(A[1-6]|C[1-5]) ' ~/Projects/dotfiles/dispatch/write_zig.md` | ≥ 11 | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| R3 | Invariance Suite green on the dotfiles diff (§1) | `cd ~/Projects/dotfiles && make audit` | output contains `ALL CHECKS PASSED` | P0 | |
| R4 | Adherence matrix complete at close (§5) | `grep -c '| *|$' docs/v2/done/M126_002_*.md` | 0 (no empty trailing cells in matrix rows) | P0 | |
| R5 | Architecture doc carries all four subsections (§3) | `grep -cE '^## (Thread map|Channel inventory|Lock-invariant registry|Shutdown choreography)' docs/architecture/concurrency.md` | 4 | P0 | |
| R6 | Compliance-base roster ships the seven prefixes (§4) | `grep -cvE '^\s*(#|$)' audits/zig-discipline-roster.txt` | 7 | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean with new checks active | `make lint` | exit 0 | P0 | |
| S3 | Integration passes (retrofit is behavior-preserving) | `make test-integration` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

N/A — no symbols removed; retrofit adds annotations only. S8/S9-equivalent hygiene rides the rubric.

## Out of Scope

- Fixing defects — all landed in `M126_001`.
- Memleak lanes, allocator injectability, drain-lint pair-check, and lifecycle test suites → `M126_003` (the drain-lint tightening lives there even though both specs touch `lint-zig.py` — coordinate at PLAN).
- Adherence beyond the seven-prefix base — the roster is the expansion vehicle (one line per folder, each a small follow-up diff); RULE NLR (touch-it-fix-it) owns individual files until their folder joins.
- Reshaping existing channels to SPSC — C1 binds new channels; existing-channel migration is a future judgment with the architecture doc as input.
- Crash capture — parked, future milestone.

---

## Product Clarity (authoring record)

1. **Successful user moment** — a future agent editing a sweeper gets the errdefer/ownership rules surfaced at edit time and cites "A2/A5 applied" in its gate output; Indy reads one matrix page and knows exactly what is enforced, by what, and where.
2. **Preserved user behaviour** — zero runtime behavior change; every endpoint, stream, and CLI verb identical; retrofit is annotations + poisoning writes.
3. **Optimal-way check** — rules-at-the-façade + deterministic lint is this repo's proven governance shape (drain lint, deinit-pairs, UFS); the unconstrained-optimal (a full static analyzer for ownership) is out of proportion to the defect classes seen.
4. **Rebuild-vs-iterate** — iterate: extend existing façade, existing lint script, existing audit machinery; no new governance surface invented.
5. **What we build** — 11 citable rules + questionnaire questions, 3 roster-scoped lint checks + fixtures, 1 roster file (7 prefixes), 1 architecture doc, full in-code adherence across the 7-prefix base, 1 rule×folder adherence matrix.
6. **What we do NOT build** — ThreadSanitizer lane (structural discipline instead, per ghostty's evidence); blanket 636-file retrofit (NLR owns the tail); blocking errdefer lint (advisory until false-positive data exists).
7. **Fit with existing features** — compounds with M126_001's fixes (rules explain them) and M126_003's gates (rules justify them); must not destabilize the Invariance Suite parity checks in dotfiles.
8. **Surface order** — N/A — no user surface; governance and internals.
9. **Dashboard restraint** — N/A — no user surface.
10. **Confused-user next step** — a lint failure prints file:line:rule-id; the rule ID greps straight into `dispatch/write_zig.md` prose with its exemplar cite.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one workstream spanning façade + lint + doc + retrofit + matrix, because the adherence matrix is only fillable when all four land together — splitting them would leave rules without enforcement or enforcement without rules across PR boundaries.
- **Alternatives considered:** dotfiles-only rules first, repo enforcement later (rejected: prose without a firing check is the exact implicit-convention failure this milestone exists to end); folding into M126_001 (rejected: P1 fixes must not wait on governance ceremony).
- **Patch-vs-refactor verdict:** this is a **refactor** of the governance surface (rules gain structure and enforcement) executed as behavior-preserving code annotation — the runtime is deliberately untouched.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
  - Fold decision — > Indy (2026-07-11 ~12:26): "go, i chore open pull those M126_002,M126_003 into your worktree of M126_001 and commit in your PR." — all three workstreams execute on this branch, one PR.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.
