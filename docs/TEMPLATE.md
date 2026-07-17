# Milestone Specification Template

> **CANONICAL TEMPLATE — two agents consume this file, in different ways.**
> The **authoring agent** (via the `kishore-spec-new` skill) copies the body below the divider into `docs/v{N}/pending/M{N}_{NNN}_….md` and fills it — authoring order lives in the skill, not in this file. The **executing agent** reads the filled spec **top-to-bottom**: the body is physically ordered by execution need (understand → prepare → build → prove → record).
> Enforced by `audits/spec-template.sh` (SPEC TEMPLATE GATE, façade `dispatch/write_spec.md`): required sections present, zero template residue, prohibited patterns absent. Lifecycle: `AGENTS.md → Specification Standards`.

---

## Fill grammar

Three markup classes appear below the divider. The gate tells them apart mechanically:

| Marker | Meaning | Fate in the filled spec |
|---|---|---|
| `{…}` | Fill slot — replace with instance content | Gone. Surviving slots are residue; the gate BLOCKs the known sentinels. |
| `tpl:` guidance comment | How to fill the section — written for the authoring agent only | **Deleted after filling.** The gate BLOCKs any survivor. |
| SPEC AUTHORING RULES banner comment | Standing constraints on the instance | The one comment that survives, verbatim. |

Everything else — headings, table skeletons, the standard rubric rows — is kept and filled. The result: the executing agent reads **100% instance content, zero template noise**, and the 320-line budget buys signal, not boilerplate.

## What a spec pins — and refuses to pin

A milestone spec is a **goal rulebook** the executing agent plans and ships from *without playing 20-questions with the author*. It pins **intent + invariants + tests + pointers**; the agent has agency on everything else. It must NOT pin implementation detail that rots within a sprint: allocator/capacity choices, library versions, line-by-line code, exact Structured Query Language (SQL) Data Definition Language (DDL), import statements — the agent derives those from the repository.

> **Pseudocode litmus.** If the spec names a version (`postcard 1.1`), a variable (`var ctx = …`), or a DDL clause (`CREATE TABLE … DEFAULT 'foo'`), it's pseudocode — it will be wrong within a sprint. Replace with *"use the project's existing X"* + a pointer to where X lives.

## Anti-patterns (reject on sight)

| # | Anti-pattern | Do instead |
|---|---|---|
| 1 | Code blocks inside section bodies | Describe WHAT the slice delivers; pin precise behaviour as a Test. Prose code drifts; tests don't. |
| 2 | Listing every variable name | Point at an existing implementation to mirror; names match local style. |
| 3 | SQL DDL line-by-line | Table shape + constraints in prose; the agent conforms to existing migrations. |
| 4 | Pinning library versions | *"Use the existing Redis client"* + import pointer; the package manifest is the source of truth. |
| 5 | Step-by-step ordering ("Step 1… Step 2…") | **Sections** (value slices); the agent sequences within. |
| 6 | Test code in the spec | Name tests + assert behaviour in prose; the agent writes them in project style. |
| 7 | One rubric row per Dimension, or pasted evidence walls | 5–12 outcome rows; Graded = ✅/❌ + one decisive output line. The Test Specification is the per-Dimension ledger. |
| 8 | Template guidance surviving in the filled spec | Delete every `tpl:` comment; the executor reads instance content only. |

Slipped into one? The fix is usually **point at a file** (read-first pointer) or **delete the detail** (the agent figures it out).

## Hierarchy & terminology

```
v{N}.0.0 (Prototype)                  major release
└── Milestone (M40, M41…)             one demoable end-to-end capability + evidence
    └── Workstream (M40_001…)         one working function; ≤4 (a 5th only for a
        │                             cross-cutting concern; never beyond 5 — split)
        └── Section (§1, §2…)         implementation slice (what + why); 3–9 per spec
            └── Dimension (3.1, 3.2…) verifiable sub-unit — the unit of DONE
```

**Dimension** is the atomic, verifiable unit: **1 Dimension → 1 Test**, marked `DONE` individually in the same commit as its code. "Mark Dimensions DONE" (per `AGENTS.md`) resolves here. A Section is DONE when all its Dimensions are. Outcome-level grading lives in the **Acceptance Rubric** — one row per Section or failure class, never one per Dimension.

**Batch (B1, B2…)** — parallel execution group. Workstreams in the same batch run concurrently; batches are sequential (B2 starts after B1's gates clear).

A milestone is not complete until evidence is captured: commands, logs, screenshots, or a recorded walkthrough.

## Status markers

- `PENDING` — not started. `IN_PROGRESS` — being worked. `DONE` / ✅ — complete, verified, tested.
- `DEFERRED` — designed/attempted and explicitly not shipped; lives in `done/` as a record. **Discovery** MUST capture the rationale and reactivation conditions.

## Prohibited (gate-enforced — `dispatch/write_spec.md`)

- No time/effort estimates ("5 min", "1 hour", "2 days"). No effort columns or complexity ratings. No percentage-complete. No assigned owners. No implementation dates.
- Sizing signal is **Priority** (P0/P1/P2/P3); sequencing is **Dependencies**.

## Guardrails

- **Length — hard upper bound 320 lines for the filled spec** (after `tpl:` comment deletion). Typical specs 150–300. Past the bound before the Acceptance Rubric → scope is two specs sharing one file; split.
- **Sections** — 3–9. **Workstreams** — ≤4 (5th cross-cutting only).
- **File naming** — `docs/v{N}/{pending|active|done}/M{N}_{NNN}_P{Priority}_{CATEGORIES}_{NAME}.md` (e.g. `M40_001_P0_API_WORKER_SUBSTRATE.md`). Categories alphabetised.

---

# Spec Body — Copy Everything Below This Line

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

# M{Milestone}_{Workstream}: {Title — testable, not vague}

**Prototype:** v{major}.{minor}.{patch}
**Milestone:** M{Number}
**Workstream:** {001-009}
**Date:** {MMM DD, YYYY}
**Status:** PENDING | IN_PROGRESS | DONE
**Priority:** P0 | P1 | P2 | P3 — {one-line reason}
**Categories:** {API | CLI | UI | SKILL | DOCS | OBS | INFRA — alphabetised, one or more}
**Batch:** B{1-4} — {parallel execution context}
**Branch:** {feat/mNN-name — added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** {M{N}_{NNN} (one-line reason), …}
**Provenance:** human-written | LLM-drafted ({model}, {date}) | agent-generated (pre-spec, {source doc})
**Canonical architecture:** `docs/architecture/{relevant-doc}.md` §{N}

<!-- tpl: Provenance is load-bearing — the implementing agent calibrates trust by
who wrote the spec: LLM-drafted gets extra cross-checking against the codebase;
human-written assumes the author read the relevant code. Canonical architecture:
the docs/architecture/ directory is the source of truth (Architecture Consult &
Update Gate); greenfield → point at the doc that defines the shape. -->

---

## Overview

**Goal (testable):** {one sentence that could be a test name}
**Problem:** {observable symptoms, in user-facing terms}
**Solution summary:** {one paragraph — what changes, at what layer, what the user-visible outcome is}

<!-- tpl: Goal — bad: "Implement streaming." Good: "Server-Sent Events (SSE)
handler streams Redis pubsub messages as text/event-stream with stable ordering
and reconnection, p95 latency under 200ms." Problem — symptoms ("operators can't
see the agent's tool calls in real time"), never implementation language ("we
don't have a foo handler"). Implementation steps belong in Sections, not here. -->

## PR Intent & comprehension handshake

- **PR title (eventual):** {imperative, ≤72 chars — what the merged Pull Request (PR) is called}
- **Intent (one sentence):** {why this PR exists, in user-facing-outcome terms}
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `{path/to/file.ext}` — {why this is the right pattern to mirror}
2. `{path/to/spec_or_doc.md}` — {what canonical knowledge lives there}
3. {external doc URL, if relevant} — {what convention to follow}

<!-- tpl: 3–5 pointers. Fewer than 3 = authoring homework not done (the executing
agent repeats it); more than 5 = a tutorial (trim). This is where judgment is
preserved without pseudocode: point at code/docs to read BEFORE touching any
file. Greenfield (no existing pattern)? Say so explicitly and point at the
docs/architecture/ doc that defines the shape. -->

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `path/to/file.ext` | CREATE / EDIT / DELETE | {one line — what changes about this file's role} |

<!-- tpl: Every file created, modified, or deleted. Scopes the length gates,
orphan sweeps, and review effort; per AGENTS.md the executing agent may only
edit files in this table without explicit override. List FILES and ROLES —
never line numbers or function names (they drift). Teardown/rename/flip specs
open with a blast-radius grep first: git grep -rn -w '<token>' from repo root,
no path filter (dispatch/write_spec.md, Authoring discipline). -->

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — {specific rule IDs this diff trips: e.g. UFS, NDC, NLR, ORP, FLL}
- {per-surface rule file} — {why it applies to this diff}

<!-- tpl: The rule files the executing agent re-reads BEFORE EXECUTE and
re-checks during VERIFY. Name the exact rule IDs the diff will trip — generic
"follow RULES.md" earns a greptile finding; named IDs are obeyed by
construction. Per-surface menu: dispatch/write_zig.md (*.zig — pg-drain,
tagged-union results, errdefer, cross-compile) · docs/REST_API_DESIGN_GUIDELINES.md
(handlers/OpenAPI — name the §) · docs/SCHEMA_CONVENTIONS.md (schema/*.sql,
schema/embed.zig) · dispatch/write_ts_adhere_bun.md / docs/LOGGING_STANDARD.md /
docs/LIFECYCLE_PATTERNS.md (when touched). Fully greenfield → write "Standard
set only — docs/greptile-learnings/RULES.md; no other rule files apply." -->

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | {yes/no — why} | {e.g. cross-compile both linux targets} |
| PUB / Struct-Shape | {yes/no} | {shape verdict per new pub surface} |
| File & Function Length (≤350/≤50/≤70) | {yes/no} | {split plan if a file approaches the cap} |
| UFS (repeated/semantic literals) | {yes/no} | {named constants; cross-runtime identifier shared verbatim} |
| UI Substitution / DESIGN TOKEN | {yes/no} | {design-system primitive; theme.css token} |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | {yes/no} | {per applicable surface} |

<!-- tpl: Which Action-Triggered Guards (AGENTS.md dispatch index) this PR WILL
trip, and how each stays clean — pre-declared so the agent plans for them
instead of stalling mid-EXECUTE. Rules ≠ Gates: rules are knowledge to read;
gates are guards that fire on edits. Touch nothing a gate watches → replace the
table with "N/A — docs/markdown only." -->

## Prior-Art / Reference Implementations

- **Reference:** {path / codebase} — {one line on alignment, or the justified divergence}

<!-- tpl: SOUL.md rule — before proposing architecture, find the reference
codebase; there almost always is one. Name it so the agent mirrors a known-good
pattern instead of inventing. Menu: Command-Line Interface (CLI) → the "7
Pillars" of CLI developer experience (vendored supabase-style oss/cli): command
→ handler → errors split; handler purity (no console.log / process.exit in
handlers); output as a service (human vs JSON vs env rendering chosen by the
renderer); structured JSON errors with suggestion/retry fields; 3-tier test
pyramid (handler unit / in-process integration / subprocess e2e); auto-JSON when
stdout is piped (LLM-native) — state which pillars this spec aligns with and any
divergence. API → docs/REST_API_DESIGN_GUIDELINES.md + the closest existing
handler. Schema → the nearest migration + docs/SCHEMA_CONVENTIONS.md. UI →
design-system primitives + theme.css tokens. Greenfield → "no prior art; shape
defined in docs/architecture/{doc}.md." -->

## Sections (implementation slices)

### §1 — {Slice title}

{What this slice delivers in goal terms; why it must exist; what it unblocks. Non-obvious choice → **Implementation default:** `{choice}` because `{reason}`.}

- **Dimension 1.1** — {smallest verifiable behaviour} → Test `test_…`
- **Dimension 1.2** — {…} → Test `test_…`

### §2 — {Slice title}

{Same shape.}

<!-- tpl: Each Section: WHAT one slice delivers and WHY (not how); numbered
Dimensions map 1:1 to Tests and get marked DONE in the same commit as their
code. The agent picks each Implementation default unless it has evidence to
deviate. Good: "§3 — Replay idempotency. Receiver dedupes on the delivery id.
Implementation default: 24h dedupe window matching the upstream retry window;
storage is a Redis key with a Time To Live (TTL) — the agent picks the key
shape from existing dedupe patterns. Dimension 3.1 → test_dedupes_within_window;
3.2 → test_evicts_after_ttl." Bad: "§3 — Use redis.SET(\"webhook:dedupe:\"+id,
\"1\",\"NX\",\"EX\",86400) and check the return." — pseudocode; the agent reads
the existing dedupe pattern and writes the call. -->

## Interfaces

```
{HTTP endpoints, request/response shapes, internal signatures other code depends on}
```

<!-- tpl: Lock the interface — the surface the agent must NOT change without
amending the spec. Specify input/output/error shapes; use a real example
payload where shape isn't self-evident. Write the interface, not the
implementation. -->

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| {short name} | {trigger} | {response + observable} |

<!-- tpl: Every failure path the agent must handle; each row → a negative/
integration test in the Test Specification. Cover at minimum: timeout,
malformed input, auth failure, network blip, race, replay, exceeded quota,
dependency unavailable. -->

## Invariants

1. {Invariant} — {how it's enforced}

<!-- tpl: Each MUST be enforceable by code (compiler, lint, comptime assertion,
runtime check) — NOT by review discipline. If a human can violate it silently,
it's not an invariant. None → "N/A — no invariants." -->

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `{event_name}` | {product / ops / not applicable} | {exact user or system action} | {coarse product context, resource id, duration, outcome} | {no raw email/password/token/One-Time Password (OTP)/Secure Shell (SSH) key material} | `{test_name}` |

<!-- tpl: Every realized spec declares what product or operational signal it
adds, or explicitly why none: internal-only cleanup → the single row "not
applicable — no product/operator signal changes". Prefer the project's
analytics architecture doc (root page telemetry for views, typed event
registries for actions, journey timers for funnels, authenticated identity
linking). If a workflow already emits analytics, state whether this spec adds,
renames, or leaves those events unchanged; if any funnel changes, update the
analytics/funnel playbook in the same PR — otherwise Discovery records
"Metrics review: no analytics/funnel playbook update required" with the reason.
The implementing agent revisits this table after implementation and during
/review; missing action events, funnel timers, or feature-flag exposure events
are implementation gaps, not notes for later. -->

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | {unit / integration / e2e} | `test_<short_name>` | {one-line behavioural claim} |

<!-- tpl: Prose-and-assertions only — no test code. One row per Dimension;
bound to the /write-unit-test skill; ≥50% negative paths; every Failure Mode
row and every Metrics row gets a test. Tiers: unit (pure logic, handlers,
boundaries — empty/null/max/malformed/unicode) · integration (real stack, mock
only at system boundaries, deterministic failure injection per Failure Mode) ·
end-to-end (e2e) — any user-facing Category (CLI / UI / API) gets at least one
user-centric scenario via test-e2e* walking the real path (subprocess CLI /
real HTTP request / rendered UI); a unit test is not a substitute. Also
include regression rows (pre-existing behaviour that must not change — "N/A —
greenfield" if none) and idempotency/replay rows (any retry semantics).
Non-self-evident input shape → point at a fixture
(samples/fixtures/m{N}-fixtures/{name}.json); don't inline JSON. Hard-to-
describe behaviour in prose ⇒ the Goal is fuzzy — fix the Goal, not this
table. -->

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | {outcome the user can observe} (§1) | `{command}` | {exit 0 / substring / 0 matches} | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Unit tests pass | `make test` | exit 0 | P0 | |
| S2 | Lint clean | `make lint` | exit 0 | P0 | |
| S3 | Integration passes (HTTP/schema/Redis touched) | `make test-integration` | exit 0 | P0 | |
| S4 | e2e walks the user path (user-facing Category) | `{test-e2e command}` | exit 0 | P0 | |
| S5 | No leaks (allocator wiring touched) | `make memleak` | exit 0 | P0 | |
| S6 | Cross-compile (Zig touched) | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | |
| S7 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S8 | No oversize source file | `git diff --name-only origin/main \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S9 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

<!-- tpl: The single scoring surface — no other scoreboard. 5–12 rows after
pruning: one per Section outcome, failure class, or hygiene gate — never one
per Dimension (that ledger is the Test Specification). Keep the standard S-rows
whose surface is touched; delete the rest. Expected litmus: every Expected is
mechanically checkable — an exit code, a literal substring, or a match count;
can't write it that way → the criterion is fuzzy — fix the criterion, not the
grading. Authoring fills every column except Graded; VERIFY fills Graded. -->

### Behaviour evals

- **Grounding rule:** {one sentence the output must never violate}
- **Golden set:** `samples/fixtures/{path}` — {N} cases across {3–5 coverage axes, incl. the nightmare case}. A failure found in the wild becomes a new case; the set only grows.
- **Ship threshold:** grounding 100% · task pass ≥{N}% · 0 critical failures on {nightmare case}. Each threshold is one rubric row with the command that computes it.
- **Fallback:** below threshold or low confidence → {named recoverable behaviour}; fabricated output is a P0 ❌.

<!-- tpl: Delete this whole sub-section unless the diff changes prompt/model/
agent behaviour. Grounding-rule example: "responses cite only retrieved rows,
never invented identifiers." -->

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `{path/to/old_file.ext}` | `test ! -f {path/to/old_file.ext}` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `{old_symbol}` | `grep -rn "{old_symbol}" src/ \| head` | 0 matches |

<!-- tpl: Mandatory when the spec deletes or replaces files: for every deleted
file and removed/renamed public symbol, grep the repo; non-zero = stale. Use
the same word-boundary pattern as the discovery grep. No deletions → replace
both tables with "N/A — no files deleted." -->

## Out of Scope

- {Item explicitly not in this spec — points at a follow-up spec or "future work"}

---

<!-- tpl: The two sections below are the AUTHORING RECORD — written before the
implementation sections above, read once by the executing agent at PLAN for
intent, and never re-litigated during EXECUTE. They sit last so the executor's
working sections stay front-loaded. -->

## Product Clarity (authoring record)

1. **Successful user moment** — {the single observable moment that proves this worked — a scene, not a metric}
2. **Preserved user behaviour** — {what users do today that keeps working unchanged; breaking any of it is a redesign}
3. **Optimal-way check** — {is this the most direct way to deliver moment #1? name the gap to the unconstrained-optimal shape and why it's acceptable now}
4. **Rebuild-vs-iterate** — {would a larger refactor serve better? verdict here; rationale in Decomposition below. A refactor that trades run-to-run determinism away is wrong by default}
5. **What we build** — {the shortest artifact list that delivers moment #1}
6. **What we do NOT build** — {adjacent scope rejected, one-line reason each — seeds Out of Scope}
7. **Fit with existing features** — {what this compounds with; the one feature it must not destabilize}
8. **Surface order** — {CLI-first (repo default), UI-first, or both; justify divergence}
9. **Dashboard restraint** — {what the UI must hide until the signal behind it is real: no controls before evidence, no quality claims before counters}
10. **Confused-user next step** — {the self-serve move (a command, an error message, a doc); "file a ticket" means a surface is missing from item 5}

<!-- tpl: Indy's product questions, answered in order at authoring BEFORE the
implementation sections are written — so the authoring agent holds the product
behaviour, not just the file list. One short paragraph or list item each. A
question that can't be answered yet is a [?] that blocks the spec (golden-path
rule, AGENTS.md). Internal-only work: answer 1–7 compactly; 8–10 may be "N/A —
no user surface" with the reason. -->

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** {why this Section/Workstream split — the decomposition rationale}
- **Alternatives considered:** {≥1 — the larger refactor or the smaller patch — and why rejected for now}
- **Patch-vs-refactor verdict:** this is a **{patch | refactor}** because {reason}. {If a larger refactor is the right long game but out of scope, name the follow-up spec rather than silently mud-patching.}

<!-- tpl: Indy's rule — don't ship a mud-patch when the problem wants a
refactor, and don't refactor when a patch is right. Match solution-size to
problem-size; surface the call BEFORE approval, not mid-PR. Weigh against the
platform constants (docs/architecture/direction.md). -->

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: the question asked + Indy's decision.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs` results (order per `AGENTS.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.

<!-- tpl: Empty at creation (keep the four bullet headers). This is the spec's
running record — where deferrals and skill outcomes are proven as the work
proceeds. -->
