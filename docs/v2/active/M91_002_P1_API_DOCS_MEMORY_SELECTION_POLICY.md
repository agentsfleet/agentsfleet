# M91_002: Category-pinned memory selection — `core` survives the window and the cap

**Prototype:** v2.0.0
**Milestone:** M91
**Workstream:** 002
**Date:** Jun 11, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — fixes the user-facing forgetting failure: identity facts written once (owner, deploy target, customer plan) are today the FIRST entries both the hydration window and cap eviction discard, because write-recency is the only selection currency
**Categories:** API, DOCS
**Batch:** B2 — after M91_001 (its counters provide the before/after evidence; its `enforceCap` count-return is built on)
**Branch:** feat/m91-002-memory-selection
**Test Baseline:** unit=1912 integration=178
**Depends on:** M91_001 (eviction-count return + loss baselines)
**Provenance:** agent-generated (memory-architecture analysis session, Jun 11, 2026) — grounded in `zombie_memory.zig` (the `Compactor` comment reserves "a future selective arm"), NullClaw's category set (`core`/`daily`/`conversation`/custom), and the supermemory comparison (their static/dynamic profile split solves this same failure); re-confirm at PLAN.

**Canonical architecture:** `docs/architecture/direction.md:20` (no search infrastructure — selection policy is the permitted lever) + `docs/architecture/runner_fleet.md` §Memory continuity + `docs/architecture/capabilities.md` §4 — both describe recency-window hydration and are **reconciled by this workstream in the same diff** (Architecture Consult gate: the doc wins until amended, so the doc edits ship with the behaviour).

---

## Implementing agent — read these first

1. `src/zombied/memory/zombie_memory.zig` — the `Compactor` union and `windowByBytes`; the doc comment explicitly reserves the selective arm this workstream adds. `enforceCap` (post-M91_001 shape) gains the tier ordering.
2. `src/zombied/http/handlers/runner/memory.zig` — `innerRunnerMemoryHydrate` constructs the compactor; the only call-site change.
3. `docs/architecture/capabilities.md` §4 + `docs/architecture/runner_fleet.md` §Memory continuity — the prose being reconciled; read before editing (Architecture Consult gate).
4. `~/Projects/oss/nullclaw/src/memory/root.zig` (`MemoryCategory`: `core`, `daily`, `conversation`, `custom`) — the category vocabulary entries arrive with; durable rows store its `toString` output.
5. `dispatch/write_zig.md` — PUB-surface verdict for the union arm; cross-compile both linux targets.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** `feat(m91): category-pinned hydration + tiered cap eviction`
- **Intent (one sentence):** a zombie's `core` memories are hydrated before any recency windowing and evicted only when nothing else remains, so the facts an agent marked durable stop being the first casualties of the byte budget and the entry cap.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent; list `ASSUMPTIONS I'M MAKING:`. Confirm two facts against the branch: (a) the category strings durable rows actually carry (NullClaw `toString` output — including what `custom` categories look like in production rows), (b) the M91_001 `enforceCap` signature. A `[?]` blocks EXECUTE.

---

## Product Clarity

1. **Successful user moment** — "owner=indy", stored in run 1, is still in the context window at entry 1001 and on day 90; the ops zombie still recognises the May-18 incident even after a noisy month of `daily` writes.
2. **Preserved user behaviour** — same four agent verbs, same API shapes, same window byte budget, same cap; zombies and operators change nothing — `core` already exists on every row.
3. **Optimal-way check** — selection policy is the cheapest fix that targets the actual failure (wrong currency); a bigger window costs tokens every run, a mid-run probe costs plumbing — both stay behind M91_001's evidence gates.
4. **Rebuild-vs-iterate** — iterate: the `Compactor` seam was built for this arm. Hydration stays a pure function of (rows, budget) — determinism preserved; a supermemory-style retrieval stack would trade that determinism away and is rejected.
5. **What we build** — one selective compactor arm, tier-ordered eviction, the two architecture-doc reconciliations, and memory-hygiene guidance prose.
6. **What we do NOT build** — importance scores, time-decay, embeddings, per-zombie tier knobs, new categories.
7. **Fit with existing features** — compounds with continuation chains (each chunk re-hydrates, so pinning protects long incidents); M91_001's drop counters now mean "ephemera dropped", a calmer signal.
8. **Surface order** — no CLI/UI surface; platform policy + docs. M91_004's CLI shows categories so users can see the tiers working.
9. **Dashboard restraint** — nothing new to show; any future "pinned memories" UI stays hidden until M91_001 baselines exist.
10. **Confused-user next step** — "why does my zombie remember X but not Y?" now has a documented answer: Y was `daily`, X was `core`; the hygiene guidance teaches storing load-bearing facts as `core`.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — **RULE SSM** (StaticStringMap for the category→tier lookup), **RULE UFS** (tier-set and category-string constants single-sourced), **RULE NDC** (the old single-currency path is replaced, not left beside the new arm), **RULE FLS** (drain on the eviction query path), **RULE NSQ** (schema-qualified SQL in the eviction DELETE).
- **`dispatch/write_zig.md`** — tagged-union shape for the new arm; comptime exhaustiveness.
- **`dispatch/name_architecture.md`** — hydration-flow description changes → the architecture docs are amended in this same diff (no override exists for this gate).

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | read `dispatch/write_zig.md`; cross-compile both linux targets |
| PUB | yes — public `Compactor` union gains an arm | FILE SHAPE verdict at PLAN; arm carries the same `compact()` interface |
| Architecture consult | yes — memory flow described in two docs | reconcile `capabilities.md` §4 + `runner_fleet.md` §Memory continuity in the same diff |
| UFS | yes — tier/category constants | named constants; no inline `"core"` literals in queries (parameters from constants) |
| LENGTH | watch — `zombie_memory.zig` grows | split a `memory_tiers.zig` sibling if the file approaches 350 lines |
| SCHEMA / ERROR REGISTRY / UI | no | no schema change, no new error codes, no UI |

---

## Overview

**Goal (testable):** with a durable set over the hydration byte budget, every `core` entry that fits is hydrated before any non-core entry is considered; with a durable set over the entry cap, no `core` row is evicted while any non-core row remains — proven by an integration test that writes one `core` fact, floods 1000 `daily` entries, and finds the `core` fact both hydrated and still in Postgres.

**Problem:** both selection mechanisms key on `updated_at`, which only writes refresh. An identity fact written once in run 1 ages to the bottom: the hydration window cuts it first (newest-first byte budget) and cap eviction deletes it first (coldest-first DELETE). The agent's own discipline (re-storing stable keys) is the only defence, and nothing teaches it. This is the exact failure supermemory's static/dynamic profile split exists for — and the category column already on every row is the unused ingredient.

**Solution summary:** a selective compactor arm pins the `core` tier (all `core`, newest-first, within the unchanged byte budget) and fills the remainder with non-core newest-first; cap eviction orders non-core-coldest first, `core`-coldest only as last resort. Category→tier is a StaticStringMap with `custom`/unknown defaulting to the windowed tier. The two architecture docs that describe the old behaviour are amended in the same diff, and memory-hygiene guidance (store load-bearing facts as `core`; stable keys; `memory_forget` stale entries; keep entries small) lands in `capabilities.md` §4 plus a user-docs page.

---

## Prior-Art / Reference Implementations

- **The seam itself** → `zombie_memory.zig` `Compactor` — the union + pure `compact()` shape is the pattern; the new arm mirrors `recency_window`'s contract (slice in, slice or reordered-owned result out, no allocation if achievable — see Handshake).
- **Category lookup** → existing StaticStringMap usages in the repo (RULE SSM); mirror the nearest one.
- **Doc reconcile** → M84_005 shipped `capabilities.md` §4 + `runner_fleet.md` §Memory continuity edits the same way (small, same-diff).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/zombied/memory/zombie_memory.zig` | EDIT | selective compactor arm + tier-ordered `enforceCap` + category-tier map and constants |
| `src/zombied/http/handlers/runner/memory.zig` | EDIT | hydrate constructs the selective arm instead of plain `recency_window` |
| `src/zombied/memory/zombie_memory_integration_test.zig` | EDIT | survival + eviction-order integration tests |
| `docs/architecture/capabilities.md` | EDIT | §4: hydration described as category-pinned; memory-hygiene guidance added |
| `docs/architecture/runner_fleet.md` | EDIT | §Memory continuity: window description updated |
| `~/Projects/docs` (user docs) — memory-hygiene page | CREATE | operator/agent-facing guidance; cross-repo, own-branch flow per `AGENTS.md` Operational defaults |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** policy change in the two functions that already own selection, plus the doc truth. The prose guidance ships here (not a separate workstream) because platform pinning and agent discipline are two halves of one selection story.
- **Alternatives considered:** (1) raise `HYDRATE_WINDOW_BYTES` — rejected: pays tokens on every run and still evicts `core` first at the cap; stays behind the evidence gate. (2) per-zombie tier configuration — rejected: a knob nobody asked for; constants until evidence. (3) an importance column — rejected: a second currency to maintain; category already encodes intent.
- **Patch-vs-refactor verdict:** **patch** — the seam was reserved for exactly this arm; no structural movement.

---

## Sections (implementation slices)

### §1 — Category tiers

A single source of tier truth: pinned = `core`; windowed = everything else (`daily`, `conversation`, and any `custom` string). StaticStringMap keyed on the stored category string; unknown → windowed (safe default — never accidentally pin).

- **Dimension 1.1** — `core` maps pinned; `daily`/`conversation` map windowed → `test_tier_map_known_categories`
- **Dimension 1.2** — arbitrary custom strings map windowed → `test_tier_map_custom_defaults_windowed`

### §2 — Selective hydration arm

New compactor arm: pinned tier first (newest-first, cumulative bytes within the budget), then windowed tier newest-first in the remaining budget. Same byte arithmetic as `windowByBytes`. The never-empty guarantee is preserved: if even the newest pinned entry exceeds the budget alone, it still hydrates (mirrors the existing oversized-head rule).

- **Dimension 2.1** — mixed set over budget: all fitting `core` hydrated, remainder filled with newest non-core → `test_selective_pins_core_first`
- **Dimension 2.2** — `core` alone exceeds budget: newest `core` kept to budget, drop counters (M91_001) tick → `test_selective_core_overflow_drops_oldest_core`
- **Dimension 2.3** — set within budget: identical output to passthrough ordering (no behaviour change when nothing is dropped) → `test_selective_noop_when_fits`
- **Dimension 2.4** — determinism: same rows + same budget → byte-identical output across repeated calls → `test_selective_deterministic`

### §3 — Tier-ordered cap eviction

`enforceCap`'s DELETE selects victims windowed-tier-coldest first; pinned rows are eligible only after every windowed row is gone. Tie order within a tier stays `updated_at` then id (existing tiebreak).

- **Dimension 3.1** — over cap with mixed tiers: only windowed rows evicted; every `core` row survives → `test_evict_windowed_before_core`
- **Dimension 3.2** — over cap with `core`-only set: coldest `core` evicted (last resort), eviction counter ticks → `test_evict_core_last_resort`
- **Dimension 3.3** — headline: one `core` fact + 1000 `daily` pushes → `core` fact hydrated AND present in Postgres → `test_core_survives_thousand_dailies`

### §4 — Architecture-doc reconcile + hygiene guidance

`capabilities.md` §4 and `runner_fleet.md` §Memory continuity currently describe pure recency windowing; both are amended to the category-pinned behaviour in this diff. §4 additionally gains the memory-hygiene pattern (store load-bearing facts as `core`; stable keys so upserts refresh instead of duplicate; `memory_forget` stale entries; keep entries small — the agent's own discipline is the documented primary bound). A user-docs page mirrors the guidance for operators writing SKILL.md files.

- **Dimension 4.1** — both architecture docs describe category-pinned selection; no recency-only claim remains → verified by grep in Eval E5
- **Dimension 4.2** — hygiene guidance present in `capabilities.md` §4 and the user-docs page → content review at `/review`

---

## Interfaces

- `Compactor` (public union) gains one arm carrying the pinned-tier policy; `compact()` keeps its signature. The `recency_window` arm remains (tenant passthrough unaffected; the arm stays used by tests) — if it ends up caller-less after the switch, it is removed in this diff (RULE NDC), not left as an option.
- `enforceCap` keeps its M91_001 signature; only victim ordering changes.
- No HTTP, wire-shape, or OpenAPI changes — hydration response shape is unchanged; only which entries fill it.

---

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| `core` tier alone exceeds budget | agent hoards `core` | newest `core` kept within budget; oldest `core` dropped from hydration (still durable); M91_001 drop counters tick; never an empty hydration |
| unknown category string | future NullClaw category / custom | windowed tier by default — never silently pinned; unit-tested |
| all-`core` set over the entry cap | agent stores everything as `core` | coldest `core` evicted (last resort) + eviction counter; the hygiene docs name this as the anti-pattern it is |
| eviction DELETE fails | database blip | existing warn-and-continue; capture unaffected |
| docs and code disagree post-merge | partial diff | prevented: doc edits are Dimensions in this spec; CHORE(close) blocks until DONE |

---

## Invariants (Hard Guardrails)

1. **`compact()` is pure** — no clock, no randomness, no I/O; same inputs → byte-identical output. Enforced by the function signature (rows + budget only) and `test_selective_deterministic`.
2. **No `core` row is evicted while a non-core row remains** — enforced by the DELETE's victim-selection ordering + `test_evict_windowed_before_core`.
3. **At least one entry always hydrates** for a non-empty set — preserved from the existing arm; unit-tested.
4. **Tier truth is single-sourced** — one StaticStringMap; no inline category literals in SQL (parameters bound from named constants). Enforced by UFS gate + grep in Eval.

---

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_tier_map_known_categories` | `core`→pinned; `daily`,`conversation`→windowed |
| 1.2 | unit | `test_tier_map_custom_defaults_windowed` | `"incident-notes"`→windowed |
| 2.1 | unit | `test_selective_pins_core_first` | 2 old `core` + 5 new `daily`, budget fits 4 → both `core` + 2 newest `daily` |
| 2.2 | unit | `test_selective_core_overflow_drops_oldest_core` | `core`-only set over budget → newest-prefix kept; oldest dropped |
| 2.3 | unit | `test_selective_noop_when_fits` | mixed small set → all entries, original recency order |
| 2.4 | unit | `test_selective_deterministic` | repeated calls, same inputs → byte-identical slices |
| 3.1 | integration | `test_evict_windowed_before_core` | cap 5: 3 `core` + 4 `daily` → 2 coldest `daily` evicted, all `core` present |
| 3.2 | integration | `test_evict_core_last_resort` | cap 3: 5 `core` → 2 coldest `core` evicted, counter +2 |
| 3.3 | integration | `test_core_survives_thousand_dailies` | 1 `core` write + 1000 `daily` pushes → `core` row in Postgres AND in hydrate response |
| 4.1 | — | Eval E5 grep | architecture docs carry no recency-only hydration claim |

Negative coverage rides 2.2/3.2 (overflow paths). Regression: `test_selective_noop_when_fits` pins the no-loss case to existing behaviour; the never-empty rule keeps its existing unit test.

---

## Acceptance Criteria

- [ ] One `core` fact survives 1000 `daily` writes — hydrated and durable — verify: `make test-integration` (`test_core_survives_thousand_dailies`)
- [ ] No `core` eviction while non-core rows remain — verify: `make test-integration` (`test_evict_windowed_before_core`)
- [ ] Hydration deterministic and never empty — verify: `make test` (2.3, 2.4 + existing never-empty test)
- [ ] Architecture docs reconciled in the same diff — verify: Eval E5 grep empty
- [ ] `make lint` · `make test` · `make test-integration` · `make check-pg-drain` all pass
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `gitleaks detect` clean · no file over 350 lines

---

## Eval Commands (post-implementation)

```bash
# E1: Build + unit
zig build && make test 2>&1 | tail -3
# E2: Integration
make test-integration 2>&1 | tail -5
# E3: Lint + drain
make lint 2>&1 | grep -E "✓|FAIL"; make check-pg-drain 2>&1 | tail -2
# E4: Cross-compile
zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && echo XC-PASS
# E5: Doc reconcile — no stale recency-only claim (expect empty)
grep -rn "recency" docs/architecture/capabilities.md docs/architecture/runner_fleet.md | grep -vi "category\|pinned\|tier"
# E6: No inline category literals in SQL (expect empty)
grep -rn "category = 'core'\|category='core'" src/
# E7: Gitleaks + 350-line gate
gitleaks detect 2>&1 | tail -2; git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
```

---

## Dead Code Sweep

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `recency_window` arm IF caller-less after the hydrate switch | `grep -rn "recency_window" src/ \| head` | only live callers (or zero if removed) |

No file deletions otherwise.

---

## Discovery (consult log)

- **Consults** — (empty at creation; Architecture Consult outcomes land here)
- **Skill chain outcomes** — (`/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs`)
- **Deferrals** — none; any deferral needs an Indy-acked verbatim quote here.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | clean; iteration count + coverage in Discovery |
| After tests pass, before CHORE(close) | `/review` | clean or every finding dispositioned |
| After `gh pr create` | `/review-pr` | comments addressed before human review |
| After every push | `kishore-babysit-prs` | final report in Discovery |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Lint + drain | `make lint && make check-pg-drain` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | | |
| Doc reconcile grep | Eval E5 | | |
| Gitleaks | `gitleaks detect` | | |

---

## Out of Scope

- Importance/priority columns, time-decay scoring, embeddings — rejected currencies (and the latter is behind the governance wall, `direction.md:20`).
- Per-zombie tier configuration — constants until evidence demands a knob.
- Mid-run durable recall probe and window-size changes — Bucket-B rungs behind M91_001's evidence gates.
- `daily` retention — M91_003.
