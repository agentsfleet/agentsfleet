# Milestone Specification Template

> **CANONICAL TEMPLATE — written for the executing agent (Oracle / Orly).**
> Copy this into `docs/v{N}/pending/M{N}_{NNN}_{NAME}.md`, then fill every section.
> Each section exists so the agent reads the **intent** and emits **deterministic, invariant** output — not so a human has a pretty document. If a section won't change what the agent does, it shouldn't be here.
> Lifecycle: `AGENTS.md → Specification Standards`.

---

## What this template is

A milestone spec is a **goal contract** the executing agent plans and ships from *without playing 20-questions with the author*. It answers, in agent-actionable terms:

- **Intent** — what Pull Request (PR) is this, and what does success look like as a test?
- **Product behaviour** — whose moment is this, what stays unchanged, and which surfaces stay restrained?
- **Surface** — which files, interfaces, gates, and invariants are in play?
- **Prior art** — what existing code/pattern does the agent mirror (so it doesn't reinvent)?
- **Alternatives** — why this shape, and not a larger refactor or a smaller patch?
- **Proof** — which tests and commands prove the claims?

It must NOT pin implementation detail that rots within a sprint: exact allocator/capacity, library version, line-by-line code, exact Structured Query Language (SQL) Data Definition Language (DDL), or import statements. The agent derives those from the repository.

> **Pseudocode litmus.** If the spec names a version (`postcard 1.1`), a variable (`var ctx = …`), or a DDL clause (`CREATE TABLE … DEFAULT 'foo'`), it's pseudocode — it will be wrong within a sprint. Replace with *"use the project's existing X"* + a pointer to where X lives. The middle path is **contract + invariants + tests + pointers**; the agent has agency on the rest.

---

## Anti-patterns (guardrails the agent rejects on sight)

> Surfaced near the top, because an author who finds them on line 173 has already drifted.

| # | Anti-pattern | Do instead | Why |
|---|---|---|---|
| 1 | Code blocks inside section bodies | Describe WHAT the slice delivers; pin precise behaviour as a Test (`test_x asserts foo() returns bar`). | Implementation lives in the codebase. Prose code drifts; tests don't. |
| 2 | Listing every variable name | Point at an existing implementation to mirror. | Names are the agent's call; they'll match local style. |
| 3 | SQL DDL line-by-line | Show table shape + constraints in prose. | The agent reads existing migrations and conforms. |
| 4 | Pinning library versions | *"Use the existing Redis client"* + import pointer. | The package manifest is the source of truth. |
| 5 | Step-by-step ordering ("Step 1… Step 2…") | Use **Sections** (value slices); let the agent sequence within. | Step lists become stale ordering; slices stay coherent. |
| 6 | Writing test code in the spec | Name tests + assert behaviour in prose; the agent writes them. | Inline test code drifts the moment the framework changes. |

Slipped into one? The fix is usually **move the detail to the implementing-agent prologue** (point at a file) or **delete it** (the agent figures it out).

---

## Hierarchy & terminology

```
v{N}.0.0 (Prototype)                  major release
└── Milestone (M40, M41…)             one demoable end-to-end capability + evidence
    └── Workstream (M40_001…)         one working function; ≤4 (a 5th only for a
        │                             cross-cutting concern; never beyond 5 — split)
        └── Section (§1, §2…)         implementation slice (what + why); 3–9 per spec
            └── Dimension (3.1, 3.2…) verifiable sub-unit — the unit of DONE
```

**Dimension** is the atomic, verifiable unit: **1 Dimension → 1 Test → 1 Acceptance Criterion**, marked `DONE` individually in the same commit as its code. "Mark Dimensions DONE" (per `AGENTS.md`) resolves here. A Section is DONE when all its Dimensions are.

**Batch (B1, B2…)** — parallel execution group. Workstreams in the same batch run concurrently; batches are sequential (B2 starts after B1's gates clear).

A milestone is not complete until evidence is captured: commands, logs, screenshots, or a recorded walkthrough.

---

## Status markers

- `PENDING` — not started. `IN_PROGRESS` — being worked. `DONE` / ✅ — complete, verified, tested.
- `DEFERRED` — designed/attempted and explicitly not shipped; lives in `done/` as a record. The **Discovery** section MUST capture the rationale and the reactivation conditions.

## Prohibited (gate-enforced — `dispatch/write_spec.md`)

- No time/effort estimates ("5 min", "1 hour", "2 days"). No effort columns or complexity ratings. No percentage-complete. No assigned owners. No implementation dates.
- Sizing signal is **Priority** (P0/P1/P2/P3); sequencing is **Dependencies**.

## Guardrails

- **Length — hard upper bound 320 lines.** Typical specs 150–300. Past the bound before Verification Evidence → scope is two specs sharing one file; split.
- **Sections** — 3–9. **Workstreams** — ≤4 (5th cross-cutting only).
- **File naming** — `docs/v{N}/{pending|active|done}/M{N}_{NNN}_P{Priority}_{CATEGORIES}_{NAME}.md` (e.g. `M40_001_P0_API_WORKER_SUBSTRATE.md`). Categories alphabetised.

---

# Spec Body — Copy Everything Below This Line

<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere. No effort columns, complexity ratings,
  percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only sequencing signal.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (dispatch/write_spec.md) and audits/spec-template.sh,
  which also assert the determinism-critical sections below are present and filled (not left as {placeholders}).
-->

# M{Milestone}_{Workstream}: {Title — testable, not vague}

**Prototype:** v{major}.{minor}.{patch}
**Milestone:** M{Number}
**Workstream:** {001-009}
**Date:** {MMM DD, YYYY}
**Status:** PENDING | IN_PROGRESS | DONE
**Priority:** P0 | P1 | P2 | P3 — {one-line reason}
**Categories:** {API | CLI | UI | SKILL | DOCS | OBS | INFRA — alphabetised, one or more}
**Batch:** B{1-4} — {parallel execution context}
**Branch:** {feat/mNN-name — added when work begins}
**Depends on:** {M{N}_{NNN} (one-line reason), …}
**Provenance:** human-written | LLM-drafted ({model}, {date}) | agent-generated (pre-spec, {source doc})

> **Provenance is load-bearing.** The implementing agent calibrates trust by who wrote the spec. LLM-drafted specs get extra cross-checking against the codebase; human-written specs assume the author read the relevant code.

**Canonical architecture:** `docs/architecture/{relevant-doc}.md` §{N} — the directory is the source of truth (Architecture Consult & Update Gate). Greenfield → say so and point at the doc that defines the shape.

---

## Implementing agent — read these first

> **Required prologue. 3–5 pointers.** Fewer than 3 = homework not done; the agent repeats it. More than 5 = a tutorial; trim. Point at existing code/docs to read BEFORE touching any file — this is where judgment is preserved without pseudocode.

1. `path/to/file.ext` — {why this is the right pattern to mirror}
2. `path/to/spec_or_doc.md` — {what canonical knowledge lives there}
3. {external doc URL, if relevant} — {what convention to follow}

Greenfield (no existing pattern)? Say so explicitly and point at the `docs/architecture/` doc that defines the shape.

---

## PR Intent & comprehension handshake

> The bridge from spec to the merged PR. Makes the agent confirm it understood intent *before* writing code.

- **PR title (eventual):** {imperative, ≤72 chars — what the merged PR is called}
- **Intent (one sentence):** {why this PR exists, in user-facing-outcome terms}
- **Handshake (agent fills at PLAN, before EXECUTE):** the implementing agent restates the intent in its own words and lists the assumptions it is proceeding on (`ASSUMPTIONS I'M MAKING: …`). A mismatch between this restatement and the Intent above → STOP and reconcile before any edit.

---

## Product Clarity (answer in order, at authoring)

> Indy's product questions, in the order they must be answered — BEFORE the
> implementation sections below are written. They exist so the authoring agent
> (Orly) holds the product behaviour, not just the file list. One short paragraph
> or list per item; a question that can't be answered yet is a `[?]` that blocks
> the spec (golden-path rule, `AGENTS.md`).

1. **Successful user moment** — the single observable moment that proves this
   worked. Write it as a scene ("run N+1 opens and the agent already knows…"),
   not a metric.
2. **Preserved user behaviour** — what users do today that keeps working
   unchanged. Breaking any of it is a redesign, not a feature.
3. **Optimal-way check** — is this the most direct way to deliver moment #1?
   Sketch the unconstrained-optimal shape; name the gap to it and why the gap is
   acceptable now.
4. **Rebuild-vs-iterate** — would a larger refactor serve better? Weigh against
   the platform constants (`docs/architecture/direction.md`) and run-to-run
   determinism; a refactor that trades determinism away is wrong by default.
   Verdict here; full rationale in Decomposition & alternatives below.
5. **What we build** — the shortest artifact list that delivers moment #1.
6. **What we do NOT build** — adjacent scope rejected, one-line reason each.
   Seed Out of Scope from this list.
7. **Fit with existing features** — what this compounds with, and the one
   feature it must not destabilize.
8. **Surface order** — CLI-first, UI-first, or both. Repo default: CLI-first
   (`agentsfleet`), UI later as a read-only view; justify divergence.
9. **Dashboard restraint** — what the UI must hide until the signal behind it is
   real: no controls before evidence, no quality claims before counters.
10. **Confused-user next step** — when this confuses someone, what is their
    self-serve move (a command, an error message, a doc)? If the answer is
    "file a ticket," a surface is missing from item 5.

---

## Applicable Rules

> The rule files the agent re-reads BEFORE EXECUTE and re-checks during VERIFY. Without this list the agent has no anchored prompt to consult them at the right moment. Add specific rule IDs where scope is narrow — naming the exact greptile rule IDs the diff trips is what makes the resulting PR review-clean by construction.

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline (always applies).
- **`dispatch/write_zig.md`** — when the diff touches `*.zig` (name sections: pg-drain lifecycle, tagged-union results, multi-step `errdefer`, cross-compile).
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — when the diff touches `src/http/handlers/**` or `public/openapi/**` (name §: URL design, route registration, handler signature).
- **`docs/SCHEMA_CONVENTIONS.md`** — when the diff touches `schema/*.sql` or `schema/embed.zig`.
- **`dispatch/write_ts_adhere_bun.md`** / **`docs/LOGGING_STANDARD.md`** / **`docs/LIFECYCLE_PATTERNS.md`** — when the relevant surface is touched.

Fully greenfield with no project rules → write "Standard set only — `docs/greptile-learnings/RULES.md`; no other rule files apply."

---

## Applicable Gates

> Which Action-Triggered Guards (`AGENTS.md` dispatch index) this PR WILL trip, and how each stays clean. Pre-declaring them means the agent plans for them — it doesn't rediscover them mid-EXECUTE and stall. Rules ≠ Gates: rules are knowledge to read; gates are guards that fire on edits.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes/no — {why} | {e.g. cross-compile both linux targets; read dispatch/write_zig.md} |
| PUB / Struct-Shape | yes/no | {shape verdict per new pub surface} |
| File & Function Length (≤350/≤50/≤70) | yes/no | {split plan if a file approaches the cap} |
| UFS (repeated/semantic literals) | yes/no | {named constants; cross-runtime identifier shared verbatim} |
| UI Substitution / DESIGN TOKEN | yes/no | {design-system primitive; theme.css token} |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes/no | {per applicable surface} |

Touch nothing a gate watches → "N/A — docs/markdown only."

---

## Overview

**Goal (testable):** one sentence that could be a test name. Bad: "Implement streaming." Good: "Server-Sent Events (SSE) handler streams Redis pubsub messages as `text/event-stream` with stable ordering and reconnection, p95 latency under 200ms."

**Problem:** observable symptoms in user-facing terms ("operators can't see the agent's tool calls in real time"), not implementation language ("we don't have a foo handler").

**Solution summary:** one paragraph — what changes, at what layer, what the user-visible outcome is. Implementation steps belong in Sections, not here.

---

## Prior-Art / Reference Implementations

> **SOUL.md rule: before proposing architecture, find the reference codebase — there almost always is one.** Name it so the agent mirrors a known-good pattern instead of inventing. State the alignment, or justify the divergence.

- **CLI** → the **"7 Pillars"** of CLI developer experience (vendored from the supabase-style `oss/cli`): command → handler → errors split; **handler purity** (no `console.log` / `process.exit` in handlers); **output as a service** (human vs JSON vs env rendering chosen by the renderer, not the handler); **structured JSON errors** with `suggestion`/`retry` fields; **3-tier test pyramid** (handler unit / in-process integration / subprocess e2e); **auto-JSON when stdout is piped** (LLM-native). State per CLI spec: which pillars this aligns with, and the reason for any divergence.
- **API** → `docs/REST_API_DESIGN_GUIDELINES.md` + the closest existing handler under `src/http/handlers/`.
- **Schema** → the nearest existing migration + `docs/SCHEMA_CONVENTIONS.md`.
- **UI** → design-system primitives + `theme.css` tokens.

Name the reference path and one line on alignment/divergence. Greenfield → "no prior art; shape defined in `docs/architecture/{doc}.md`."

---

## Files Changed (blast radius)

> Every file created, modified, or deleted. Scopes file-length gates, orphan sweeps, and review effort. The agent may only edit files in this table without explicit override.

| File | Action | Why |
|------|--------|-----|
| `path/to/file.ext` | CREATE / EDIT / DELETE | one line — what changes about this file's role |

> **Anti-pattern:** don't list line numbers or function names (they drift). List FILES and ROLES.

---

## Decomposition & alternatives (patch vs refactor)

> **Indy's rule: don't ship a mud-patch when the problem wants a refactor — and don't refactor when a patch is right.** Match solution-size to problem-size, and surface the call to Indy *before* approval rather than discovering it mid-PR.

- **Chosen shape:** {why this Section/Workstream split — the decomposition rationale}
- **Alternatives considered:** {≥1 — e.g. "the larger refactor that unifies X and Y" or "the minimal patch touching only Z" — and why it was rejected for now}
- **Patch-vs-refactor verdict:** this is a **{patch | refactor}** because {reason}. If a larger refactor is the right long game but out of scope here, name the follow-up spec rather than silently mud-patching.

---

## Sections (implementation slices)

> Each Section: WHAT one slice delivers and WHY (not how). Decompose into numbered **Dimensions** — the verifiable sub-units that map 1:1 to Tests and Acceptance Criteria, and that get marked DONE.

### §1 — {Slice title}

What this slice delivers in goal terms; why it must exist; what it unblocks. Non-obvious choice → name it: **Implementation default:** `<choice>` because `<reason>` (the agent picks the default unless it has evidence to deviate).

- **Dimension 1.1** — {smallest verifiable behaviour} → Test `test_…`
- **Dimension 1.2** — {…} → Test `test_…`

### §2 — {Next slice}

Same shape.

> **Good:** "§3 — Replay idempotency. Receiver dedupes on the delivery UUID. Implementation default: 24h dedupe window matching the upstream retry window; storage is a Redis key with TTL — the agent picks the key shape from existing dedupe patterns. Dimension 3.1 → `test_dedupes_within_window`; 3.2 → `test_evicts_after_ttl`."
> **Bad:** "§3 — Use `redis.SET("webhook:dedupe:"+id,"1","NX","EX",86400)` and check the return." — pseudocode; the agent reads the existing dedupe pattern and writes the call.

---

## Interfaces

> Lock the contract — public functions, endpoints, data shapes other code depends on. This is the surface the agent must NOT change without amending the spec.

```
{HTTP endpoints, request/response shapes, internal signatures}
```

Specify input/output/error shapes. Use a real example payload where shape isn't self-evident. Write the contract, not the implementation.

---

## Failure Modes

> Every failure path the agent must handle. Each row → a negative/integration test below.

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| {short name} | {trigger} | {response + observable} |

Cover at minimum: timeout, malformed input, auth failure, network blip, race, replay, exceeded quota, dependency unavailable.

---

## Invariants

> Each MUST be enforceable by code (compiler, lint, comptime assertion, runtime check) — NOT by review discipline. If a human can violate it silently, it's not an invariant.

1. {Invariant} — {how it's enforced}
2. {Invariant} — {how it's enforced}

No guardrails of this kind → "N/A — no invariants."

---

## Test Specification (tiered)

> **Prose-and-assertions only. No test code.** One row per **Dimension**. Bound to the `write-unit-test` skill: pick the tier; cover ≥50% negative paths; every Failure Mode row gets a test. Hard-to-describe behaviour in prose ⇒ the Goal is fuzzy — fix the Goal, not this table.

**Tiers** (the implementing agent writes the actual test in project style):

- **unit** — `write-unit-test` categories Behaviour / Failure / Invariant. Pure logic, handlers, boundaries (empty, null, max, malformed, unicode).
- **integration** — `write-unit-test` Integration category. Real stack, mock only at system boundaries; deterministic failure injection for each Failure Mode.
- **e2e** — for any **user-facing Category (CLI / UI / API)**, at least one **user-centric scenario** via `test-e2e*` walking the real path end-to-end (subprocess CLI / real HTTP request / rendered UI). A unit test is not a substitute.

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 3.1 | unit / integration / e2e | `test_<short_name>` | {one-line behavioural claim} |

Also include: **regression** tests (pre-existing behaviour that must not change — "N/A — greenfield" if none) and **idempotency/replay** tests (any retry semantics). Non-self-evident input shape → point at a fixture (`samples/fixtures/m{N}-fixtures/{name}.json`); don't inline JSON.

---

## Acceptance Criteria

> Each verifiable by a command or output inspection. Not "works correctly" — a command.

- [ ] {Criterion} — verify: `{command}`
- [ ] {Criterion} — verify: `{command}`

Standard set (apply when relevant):

- [ ] `make lint` clean · `make test` passes
- [ ] `make test-integration` passes (HTTP/schema/Redis touched)
- [ ] `make memleak` clean (allocator wiring touched)
- [ ] Cross-compile clean: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` (Zig touched)
- [ ] `gitleaks detect` clean · no file over 350 lines added

---

## Eval Commands (post-implementation)

> Copy-pasteable. ALL must pass before opening a PR. Adapt to the primary language; omit what doesn't apply.

```bash
# E1: {short description}
{command} && echo "PASS" || echo "FAIL"
# E2: Build  — {project build command}
# E3: Tests  — {test command}
# E4: Lint   — make lint 2>&1 | grep -E "✓|FAIL"
# E5: Cross-compile (Zig only) — zig build -Dtarget=x86_64-linux 2>&1 | tail -3
# E6: Gitleaks — gitleaks detect 2>&1 | tail -3
# E7: 350-line gate (exempts .md) —
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'
# E8: Orphan sweep (empty = pass) — grep -rn "{old_symbol}" src/ | head
```

---

## Dead Code Sweep

> Mandatory when the spec deletes or replaces files.

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `path/to/old_file.ext` | `test ! -f path/to/old_file.ext` |

**2. Orphaned references — zero remaining imports/uses.** For every deleted file and removed/renamed public symbol, grep the repo; non-zero = stale.

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `old_symbol` | `grep -rn "old_symbol" src/ \| head` | 0 matches |

No deletions → "N/A — no files deleted."

---

## Discovery (consult log)

> **Empty at creation.** Append as the work surfaces consults and decisions — this is the spec's running record where deferrals and skill outcomes are proven.

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Skill chain outcomes** — `/write-unit-test`, `/review`, `/review-pr`, `kishore-babysit-prs` results (iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.

---

## Skill-Driven Review Chain (mandatory)

> Three skills gate implementation-complete → PR-merged, in order. Each one's output is recorded in **Discovery (consult log)** above.

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. Catches happy-path-only, missing negatives, fixture drift. | Clean. Iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, `docs/architecture/`, REST guide (HTTP), `dispatch/write_zig.md` (Zig), Failure Modes, Invariants. | Clean OR every finding dispositioned (fixed / deferred-with-quote / rejected-with-reason). |
| After `gh pr create` | `/review-pr` | Review-comments the open PR against the immutable diff (squash artifacts, post-rebase races, codegen drift). | Comments addressed (fixup/amend) before human review/merge. |

Skipping any one violates CHORE(close). Skill unavailable (MCP down) → document the skip in Discovery AND the PR description with a timestamp and a "rerun before merge" note.

---

## Verification Evidence

> Filled during VERIFY. Proves spec claims are met.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {paste snippet} | |
| Integration tests | `make test-integration` | {paste snippet} | |
| e2e (user-centric) | `{test-e2e command}` | {paste snippet} | |
| Lint | `make lint` | {paste snippet} | |
| Cross-compile (Zig) | `zig build -Dtarget=x86_64-linux` | {paste snippet} | |
| Gitleaks | `gitleaks detect` | {paste snippet} | |
| Dead code sweep | `grep -rn {symbol} src/` | {paste snippet} | |

---

## Out of Scope

- {Item explicitly not in this spec — points at a follow-up spec or "future work"}
- {Item}
