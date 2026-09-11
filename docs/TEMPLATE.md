# Milestone Specification Template

> **CANONICAL TEMPLATE — two agents consume this file, in different ways.**
> The **authoring agent** (via the `orly-spec-new` skill) copies the body below the divider into `docs/v{N}/pending/M{N}_{NNN}_….md` and fills it — authoring order lives in the skill, not in this file. The **executing agent** reads the filled spec **top-to-bottom**: the body is physically ordered by execution need (understand → prepare → build → prove → record).
> Enforced by `audits/spec-template.sh` (SPEC TEMPLATE GATE, façade `dispatch/write_spec.md`): required sections present, zero template residue, prohibited patterns absent. Lifecycle: `AGENTS.md → Specification Standards`.
> Read a recent completed instance for examples; current rules and this template govern any conflict.

---

## Fill grammar

Three markup classes appear below the divider. The gate tells them apart mechanically:

| Marker | Meaning | Fate in the filled spec |
|---|---|---|
| `{{fill:description}}` | Authoring slot — replace with instance content | Gone before staging. Runtime parameters such as `/items/{id}` remain when defined. |
| `tpl:` guidance comment | How to fill the section — written for the authoring agent only | **Deleted after filling.** The gate BLOCKs any survivor. |
| SPEC AUTHORING RULES banner comment | Standing constraints on the instance | The one comment that survives, verbatim. |

Everything else — headings, table skeletons, the standard rubric rows — is kept and filled. The result: the executing agent reads **100% instance content, zero template noise**, and the 320-line budget buys signal, not boilerplate.

## Authoring policy authorization

Indy explicitly requested: "update the base TEMPLATE.md so the subsequent agents can come up with one shot implementation from the spec produced by orly from the template."
The [review record](./v2/reviews/M192_REVIEW_RESOLUTION.md#follow-up-at-b01294265-cutover-simplification-and-governance) preserves the full request and scope. This guidance change is paired with dispatch/write_spec.md; required sections, gates and verification commands remain unchanged.

## What a spec pins

A milestone spec is a **goal rulebook** the executing agent plans and ships from without repeated architecture questions. It pins **intent, invariants, enforcement mechanisms, tests and source pointers**. Leave routine variable names, import statements and code layout to repository conventions.

Pin details when changing them could invalidate a proof: client/server versions and feature support, identity scope, uniqueness and null semantics, privilege boundaries, transaction/release ordering, failure behavior and measured resource budgets. Record the supporting source revision and which proofs a version/configuration change invalidates. The manifest remains the dependency source of truth; a compatibility claim must identify the manifest it checked.

> **Mechanism litmus.** Could an implementer follow this sentence and still violate the invariant? “Preserve single use” is incomplete without the state authority, guarded transition, commit-before-release boundary and failover assumption. Name those; omit a line-by-line implementation. A bounded prototype may choose a performance parameter, but an unchosen security mitigation is not implementation-ready.

## Anti-patterns (reject on sight)

| # | Anti-pattern | Do instead |
|---|---|---|
| 1 | Implementation listings inside section bodies | State the mechanism and behavioral proof. Keep code to necessary interface examples, reproducible evidence commands or a referenced diagnostic. |
| 2 | Listing every variable name | Point at an existing implementation to mirror; names match local style. |
| 3 | SQL DDL line-by-line | Pin identity, null/deletion semantics, atomic effects and grants; use new migrations according to the repository's current migration policy. |
| 4 | Assuming a client supports a server feature | Verify both sides at named revisions; pin the compatibility baseline and required reruns, including managed-service differences. |
| 5 | Incidental coding recipes or circular Section dependencies | Declare an acyclic prerequisite order; explicitly pin commit/release, migration and deployment ordering when correctness depends on it. |
| 6 | Test code in the spec | Name tests + assert behaviour in prose; the agent writes them in project style. |
| 7 | One rubric row per Dimension, or pasted evidence walls | 5–12 outcome rows; Graded = ✅/❌ + one decisive output line. The Test Specification is the per-Dimension ledger. |
| 8 | Template guidance surviving in the filled spec | Delete every `tpl:` comment; the executor reads instance content only. |

Use source pointers to avoid duplicating code, but do not delegate a missing safety mechanism to the implementer's imagination.

## Implementation readiness review

Before finalizing a spec, attack the proposed design using the applicable rows below. Put the resulting requirements in the existing Sections, Failure Modes, Invariants, Tests, Files Changed and Discovery; do not copy this whole table into every spec or create another scoring surface. Small changes need only the relevant checks.

| Surface | What the author must resolve |
|---|---|
| Real execution path | Trace actual producers, readers, side effects, backgrounds and cleanup. Separate an implemented caller from a comment, fixture or proposed feature; verify critical claims regardless of who wrote the review. |
| Safety and authority | Name one authority per invariant, the guarded state change, commit/release point and retry behavior. Attack lost replies, concurrent calls, owner failover and snapshot rollback separately; an acknowledgment is not automatically durable failover proof. Preserve intentional idempotent response retries. |
| Identity and history | State identity scope across tenants/streams, ordering and cursor tie-breaks, nulls, deletion and expiry. Trace all conflict writers and downstream consumers such as billing enforcement. Separate pre-existing defects from new risks; name irrecoverable history and a bounded disposition instead of an impossible backfill. |
| Schema and privileges | Follow the current shipped-migration policy. For populated changes, cover fresh bootstrap, incremental upgrade, interrupted rerun and old/new build compatibility. Test successful operations under the actual role, including SELECT needed by UPDATE, as well as forbidden writes. |
| Client, server and platform | Check pinned client routing/reconnect/push behavior against the server and managed service. Probe advertised nodes, TLS, required commands and real ACL grants. Local source inspection and successful seed connection are not managed-service evidence. |
| Deployment and recovery | Identify the actual deployment graph, secret staging, writer fence, restart/autostart paths, import authorization and empty/nonempty deploy shape. Prove pre-mutation refusal, abort/forward recovery and later ordinary deploys; inventory ephemeral auth and provider deliveries during the fence too. |
| Performance and scope | Budget the whole request path: transaction/commit rate, fan-out, sequential round trips, connection pools, WAL, memory, backlog/drain and cost. Prefer the existing layout unless measurements require partitioning, batching or another subsystem. Name each comparison reference and prove workload/resource equivalence; distinguish planning inputs from measured thresholds. |
| Evidence and proof tier | Bind raw output to revision, parameters, resources and run identity; include lockfile/resolved production dependency closure for bench-only baselines. Distinguish inspected, proposed, NOT RUN and observed results. Unit stubs do not prove deployments; local tests do not prove Cloud. |
| Decisions and prerequisites | Record agent defaults, measured outputs and required user decisions separately. Attribute only real quotes and explicit overrides; do not infer risk acceptance from reviewer advice. Name the first executable slice and the boundary each unresolved prerequisite blocks. |

After editing, run a separate adversarial pass: search for stale/superseded requirements and circular dependencies; try to violate each critical invariant while obeying the text. Report remaining blockers by boundary (start, dependent integration, load or rollout), with a named resolving action. Do not promise that document checks prove runtime readiness.

## Hierarchy & terminology

```
v{N}.0.0 (Prototype)                  major release
└── Milestone (M40, M41…)             one demoable end-to-end capability + evidence
    └── Workstream (M40_001…)         one working function; ≤4 (a 5th only for a
        │                             cross-cutting concern; never beyond 5 — split)
        └── Section (§1, §2…)         implementation slice (what + why); 3–9 per spec
            └── Dimension (3.1, 3.2…) verifiable sub-unit — the unit of DONE
```

**Dimension** is the atomic, verifiable unit: **1 Dimension → 1 Test**, marked `DONE` individually in the same commit as its code. "Mark Dimensions DONE" (per `AGENTS.orly.md`) resolves here. A Section is DONE when all its Dimensions are. Outcome-level grading lives in the **Acceptance Rubric** — one row per Section or failure class, never one per Dimension.

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
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M{{fill:Milestone}}_{{fill:Workstream}}: {{fill:Title — testable, not vague}}

**Prototype:** v{{fill:major}}.{{fill:minor}}.{{fill:patch}}
**Milestone:** M{{fill:Number}}
**Workstream:** {{fill:001-009}}
**Date:** {{fill:MMM DD, YYYY}}
**Status:** PENDING | IN_PROGRESS | DONE
**Priority:** P0 | P1 | P2 | P3 — {{fill:one-line reason}}
**Categories:** {{fill:API | CLI | UI | SKILL | DOCS | OBS | INFRA — alphabetised, one or more}}
**Batch:** B{{fill:1-4}} — {{fill:parallel execution context}}
**Branch:** pending — set at CHORE(open)
<!-- A deliberately folded workstream adds Folded-into metadata per dispatch/lifecycle.md. -->
**Baseline revision:** pending — record the full comparison commit at CHORE(open)
**Test Baseline:** pending — measure declared unit and integration lanes before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** {{fill:spec identifiers with one-line reasons, or none}}
**Provenance:** human-written | LLM-drafted ({{fill:model}}, {{fill:date}}) | agent-generated (pre-spec, {{fill:source doc}})
**Canonical architecture:** `docs/architecture/{{fill:relevant-doc}}.md` §{{fill:N}}

<!-- tpl: Provenance records origin, not proof. Cross-check correctness-critical
claims against the actual caller, schema and pinned dependencies regardless of
author. Distinguish source-verified behavior, proposed mechanisms and observed
runtime results. Canonical architecture: docs/architecture/ owns the detailed
mechanism; keep spec defaults and acceptance tests consistent with it. -->

---

## Overview

**Goal (testable):** {{fill:one sentence that could be a test name}}
**Problem:** {{fill:observable symptoms, in user-facing terms}}
**Solution summary:** {{fill:one paragraph — what changes, at what layer, what the user-visible outcome is}}

<!-- tpl: Goal — bad: "Implement streaming." Good: "Server-Sent Events (SSE)
handler streams Redis pubsub messages as text/event-stream with stable ordering
and reconnection, p95 latency under 200ms." Problem — symptoms ("operators can't
see the agent's tool calls in real time"), never implementation language ("we
don't have a foo handler"). Implementation steps belong in Sections, not here. -->

## PR Intent & comprehension handshake

- **PR title (eventual):** {{fill:imperative, ≤72 chars — what the merged Pull Request (PR) is called}}
- **Intent (one sentence):** {{fill:why this PR exists, in user-facing-outcome terms}}
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `{{fill:path/to/file.ext}}` — {{fill:why this is the right pattern to mirror}}
2. `{{fill:path/to/spec_or_doc.md}}` — {{fill:what canonical knowledge lives there}}
3. {{fill:external doc URL, if relevant}} — {{fill:what convention to follow}}

<!-- tpl: Prefer 3–5 useful pointers; include enough to resolve the existing
implementation and governing requirements. This is where judgment is
preserved without pseudocode: point at code/docs to read BEFORE touching any
file. Greenfield (no existing pattern)? Say so explicitly and point at the
docs/architecture/ doc that defines the shape. Every pointer must resolve
in THIS repository (`test -f` each) or be a URL — a pack this repository
never selected materialised nothing here. -->

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `path/to/file.ext` | CREATE / EDIT / DELETE | {{fill:one line — what changes about this file's role}} |

<!-- tpl: Every file created, modified, or deleted. Scopes the length gates,
orphan sweeps, and review effort; per AGENTS.md the executing agent may only
edit files in this table without explicit override. List FILES and ROLES —
never line numbers or function names (they drift). Teardown/rename/flip specs
open with a blast-radius grep first: git grep -rn -w '<token>' from repo root,
no path filter (dispatch/write_spec.md, Authoring discipline). Include all real
writers/readers, schema registration, role grants, recovery tools, fixtures,
deployment/secret references and protocol docs affected by the chosen mechanism.
Distinguish source references from files to edit; do not schedule edits to
frozen migration history. Expand planned file scopes before implementation. -->

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — {{fill:specific rule IDs this diff trips: e.g. UFS, NDC, NLR, ORP, FLL}}
- {{fill:per-surface rule file}} — {{fill:why it applies to this diff}}

<!-- tpl: The rule files the executing agent re-reads BEFORE EXECUTE and
re-checks during VERIFY. Name the exact rule IDs the diff will trip — generic
"follow RULES.md" earns a greptile finding; named IDs are obeyed by
construction. Per-surface menu = the dispatch table in THIS repository's
generated AGENTS.md plus the docs/ rule files those façades cite — all
materialised locally by the selected packs; a rule file absent from this
repository is not applicable by definition. Fully greenfield → write "Standard
set only — docs/greptile-learnings/RULES.md; no other rule files apply." -->

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| {{fill:gate from this repository's AGENTS.md dispatch index}} | {{fill:yes/no — why}} | {{fill:how the diff stays clean}} |
| File & Function Length (≤350/≤50/≤70) | {{fill:yes/no}} | {{fill:split plan if a file approaches the cap}} |

<!-- tpl: Which Action-Triggered Guards this Pull Request (PR) WILL trip, and
how each stays clean — pre-declared so the agent plans for them instead of
stalling mid-EXECUTE. The menu is THIS repository's generated AGENTS.md
dispatch index — one row per materialised gate the diff can trip; never cite a
gate from a pack this repository lacks. Rules ≠ Gates: rules are knowledge to
read; gates are guards that fire on edits. Touch nothing a gate watches →
replace the table with "N/A — docs/markdown only." -->

## Prior-Art / Reference Implementations

- **Reference:** {{fill:path / codebase}} — {{fill:one line on alignment, or the justified divergence}}

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
design-system primitives + theme.css tokens. Cite each doc only where this
repository materialises it. Greenfield → "no prior art; shape
defined in docs/architecture/{{fill:doc}}.md." -->

## Sections (implementation slices)

### §1 — {{fill:Slice title}}

{{fill:what this slice delivers, why it exists, and what it unblocks}}. Non-obvious choice → **Implementation default:** `{{fill:choice}}` because `{{fill:reason}}`.

- **Dimension 1.1** — {{fill:smallest verifiable behaviour}} → Test `test_…`
- **Dimension 1.2** — {{fill:…}} → Test `test_…`

### §2 — {{fill:Slice title}}

{{fill:Same shape.}}

<!-- tpl: State prerequisites and who resolves each decision: agent choice,
measured preparation output, or required human decision. A preparation Section
may establish budgets or fixtures needed by later Sections; name its completion
condition and what missing evidence blocks; prerequisites must be acyclic.
Each Section names the delivered behavior, required mechanism and proof, without
coding recipes. For prototypes, name setup, fault, pass/fail evidence and the
selected safe design or bounded decision rule before dependent integration.
Do not leave a security invariant as "prototype, then find a mitigation".
Numbered Dimensions map 1:1 to Tests and get marked DONE with their code.
The agent follows each Implementation default unless evidence requires an
amended design. Good: "§3 — Replay-safe admission. Uniqueness covers provider,
tenant and delivery identity; acceptance and dispatch intent commit together
before acknowledgment. Retention covers the provider retry window and unfinished
work. Dimensions prove concurrent retries and crash recovery; storage follows
the canonical outbox design." Bad: "Set a dedupe key, then append" without an
atomicity/recovery rule, or a line-by-line Redis/SQL recipe. The agent writes
the calls; the spec must already establish why a crash cannot lose acceptance. -->

## Interfaces

```
{{fill:HTTP endpoints, request/response shapes, internal signatures other code depends on}}
```

<!-- tpl: Lock the interface — the surface the agent must NOT change without
amending the spec. Specify input/output/error shapes; use a real example
payload where shape isn't self-evident. Write the interface, not the
implementation. -->

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| {{fill:short name}} | {{fill:trigger}} | {{fill:response + observable}} |

<!-- tpl: Every failure path the agent must handle; each row → a negative/
integration test in the Test Specification. Cover at minimum: timeout,
malformed input, auth failure, network blip, race, replay, exceeded quota,
dependency unavailable where applicable. For stateful work, distinguish timeout
from rollback, acknowledgment from durable recovery, and automatic failover
from operator restore. State which protected response or side effect remains
closed on uncertainty; name recovery instead of promising to resolve it later. -->

## Invariants

1. {{fill:Invariant}} — {{fill:how it's enforced}}

<!-- tpl: Each MUST be enforceable by code (compiler, lint, comptime assertion,
runtime check) — NOT by review discipline. Name the authority, enforcement
mechanism and failure assumptions; database-backed does not itself prove safe
replica promotion. Test the shortest counterexample across the commit/release
boundary. An operator procedure is an operational prerequisite, not an invisible
runtime guard. None → "N/A — no invariants." -->

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `{{fill:event_name}}` | {{fill:product / ops / not applicable}} | {{fill:exact user or system action}} | {{fill:coarse product context, resource id, duration, outcome}} | {{fill:no raw email/password/token/One-Time Password (OTP)/Secure Shell (SSH) key material}} | `{{fill:test_name}}` |

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
| 1.1 | {{fill:unit / integration / e2e / manual}} | `test_<short_name>` | {{fill:one-line behavioural claim}} |

<!-- tpl: Prose-and-assertions only — no test code. One row per Dimension;
bound to the /orly-write-unit-test skill; ≥50% negative paths; every Failure Mode
row and every Metrics row gets a test. Tiers: unit (pure logic, handlers,
boundaries — empty/null/max/malformed/unicode) · integration (real stack, mock
only at system boundaries, deterministic failure injection per Failure Mode) ·
end-to-end (e2e) — any user-facing Category (CLI / UI / API) gets at least one
user-centric scenario via test-e2e* walking the real path (subprocess CLI /
real HTTP request / rendered UI); a unit test is not a substitute. Also
include regression rows (pre-existing behaviour that must not change — "N/A —
greenfield" if none) and idempotency/replay rows (any retry semantics).
Human acceptance uses tier manual, with its procedure, required person, and
durable evidence. An agent cannot manufacture human sign-off. Label stubbed
workflow decisions as unit proofs, local multi-node behavior as integration,
and actual deploy/managed-failover results at their operational boundary.
For state replacement, include concurrency, ambiguous commit, expiry/deletion,
identity collisions and fresh/populated upgrade where relevant. N/A applies
only to demonstrably unnecessary assertions, never an unrun base behavior.
Non-self-evident input shape → point at a fixture
(samples/fixtures/m{{fill:N}}-fixtures/{{fill:name}}.json); don't inline JSON. Hard-to-
describe behaviour in prose ⇒ the Goal is fuzzy — fix the Goal, not this
table. -->

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | {{fill:outcome the user can observe}} (§1) | `{{fill:command}}` | {{fill:exit 0 / substring / 0 matches}} | P0 | |
| R2 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `{{fill:conform command from .oracle/orly.json, verbatim}}` | exit 0 | P0 | |
| S2 | Unit tests pass | `{{fill:verify.unit command from .oracle/orly.json, verbatim}}` | exit 0 | P0 | |
| S3 | Slow tier green (code-carrying branch) | `{{fill:one row per remaining declared verify.* command, verbatim}}` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** copy every declared `conform` and `verify.*` invocation from `.oracle/orly.json` into a Verify cell, verbatim, with an Expected value. Include conditional suites; the final gate decides applicability from the actual branch diff. Additional spec-specific commands, secret scans, and named manual checks are allowed. Missing configuration must be completed before authoring. See `dispatch/lifecycle.md` for command timing; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each spec-specific Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point to the final `orly gate pr` results in Pull Request Session Notes, so recording those results does not require another code commit and suite run. **Ship gate:** every required check must pass before the Pull Request is ready; missing evidence or any ❌ returns to EXECUTE. A P1 ❌ requires an Indy-acked deferral quote in Discovery. A P0 may also be **MOVED** — see below.

**A P0 whose SCOPE moves is not a P0 shipped red.** Met and unmet are not the only two states a criterion has, and a gate that pretends otherwise forces an agent to invent a third. One did, twice in a day, before this clause existed.

A deferral and a transfer are different claims. A **deferral** leaves work unowned inside a closed spec, which is what the P0 gate exists to prevent — the P1 quote is as far as that goes. A **transfer** moves the criterion whole: its Dimensions, its verification and its rubric row land in a named successor spec that carries them as its own P0. Nothing is less owned afterwards; it is owned somewhere else.

Mark such a row `MOVED to M{{fill:N}}_{{fill:NNN}} R{{fill:n}}` and it is not ❌, on three conditions, all of which must hold:

1. The successor spec **exists** and carries the criterion as a rubric row of its own. A successor that does not carry the row is a deferral wearing a new word, and fails the gate as before.
2. Both specs record the mapping — the closing spec names where each Dimension went, the successor names what it inherited. One-sided assertion is not a transfer.
3. Discovery carries the **owner's verbatim quote** authorising it, in the deferral format. An agent-authored transfer is agent-authored scope reduction.

A MOVED row is never rendered ✅. The criterion has not been met; it has changed owner, and the rubric says which.

<!-- tpl: The single scoring surface — no other scoreboard. 5–12 rows after
pruning: one per Section outcome, failure class, or hygiene gate — never one
per Dimension (that ledger is the Test Specification). Repository suite commands come verbatim from `.oracle/orly.json`; expand S3
for every remaining declared lane. Additional spec-specific checks stay explicit. Expected litmus: every Expected is
mechanically checkable — an exit code, a literal substring, or a match count;
can't write it that way → the criterion is fuzzy — fix the criterion, not the
grading. Authoring fills every column except Graded; VERIFY fills Graded. -->

### Behaviour evals

- **Grounding rule:** {{fill:one sentence the output must never violate}}
- **Golden set:** `samples/fixtures/{{fill:path}}` — {{fill:N}} cases across {{fill:3–5 coverage axes, incl. the nightmare case}}. A failure found in the wild becomes a new case; the set only grows.
- **Ship threshold:** grounding 100% · task pass ≥{{fill:N}}% · 0 critical failures on {{fill:nightmare case}}. Each threshold is one rubric row with the command that computes it.
- **Fallback:** below threshold or low confidence → {{fill:named recoverable behaviour}}; fabricated output is a P0 ❌.

<!-- tpl: Delete this whole sub-section unless the diff changes prompt/model/
agent behaviour. Grounding-rule example: "responses cite only retrieved rows,
never invented identifiers." -->

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `{{fill:path/to/old_file.ext}}` | `test ! -f {{fill:path/to/old_file.ext}}` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `{{fill:old_symbol}}` | `grep -rn "{{fill:old_symbol}}" src/ \| head` | 0 matches |

<!-- tpl: Mandatory when the spec deletes or replaces files: for every deleted
file and removed/renamed public symbol, grep the repo; non-zero = stale. Use
the same word-boundary pattern as the discovery grep. No deletions → replace
both tables with "N/A — no files deleted." -->

## Out of Scope

- {{fill:Item explicitly not in this spec — points at a follow-up spec or "future work"}}

---

<!-- tpl: The two sections below are the AUTHORING RECORD — written before the
implementation sections above, read once by the executing agent at PLAN for
intent, and never re-litigated during EXECUTE. They sit last so the executor's
working sections stay front-loaded. -->

## Product Clarity (authoring record)

1. **Successful user moment** — {{fill:the single observable moment that proves this worked — a scene, not a metric}}
2. **Preserved user behaviour** — {{fill:what users do today that keeps working unchanged; breaking any of it is a redesign}}
3. **Optimal-way check** — {{fill:is this the most direct way to deliver moment #1? name the gap to the unconstrained-optimal shape and why it's acceptable now}}
4. **Rebuild-vs-iterate** — {{fill:would a larger refactor serve better? verdict here; rationale in Decomposition below. A refactor that trades run-to-run determinism away is wrong by default}}
5. **What we build** — {{fill:the shortest artifact list that delivers moment #1}}
6. **What we do NOT build** — {{fill:adjacent scope rejected, one-line reason each — seeds Out of Scope}}
7. **Fit with existing features** — {{fill:what this compounds with; the one feature it must not destabilize}}
8. **Surface order** — {{fill:CLI-first (repo default), UI-first, or both; justify divergence}}
9. **Dashboard restraint** — {{fill:what the UI must hide until the signal behind it is real: no controls before evidence, no quality claims before counters}}
10. **Confused-user next step** — {{fill:the self-serve move (a command, an error message, a doc); "file a ticket" means a surface is missing from item 5}}

<!-- tpl: Indy's product questions, answered in order at authoring BEFORE the
implementation sections are written — so the authoring agent holds the product
behaviour, not just the file list. One short paragraph or list item each. A
question that can't be answered yet is a [?] that blocks the spec (golden-path
rule, AGENTS.md). Internal-only work: answer 1–7 compactly; 8–10 may be "N/A —
no user surface" with the reason. -->

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** {{fill:why this Section/Workstream split — the decomposition rationale}}
- **Alternatives considered:** {{fill:≥1 — the larger refactor or the smaller patch — and why rejected for now}}
- **Patch-vs-refactor verdict:** this is a **{{fill:patch | refactor}}** because {{fill:reason}}. {{fill:If a larger refactor is the right long game but out of scope, name the follow-up spec rather than silently mud-patching.}}

<!-- tpl: Indy's rule — don't ship a mud-patch when the problem wants a
refactor, and don't refactor when a patch is right. Match solution-size to
problem-size; surface the call BEFORE approval, not mid-PR. Weigh against the
platform constants (docs/architecture/direction.md). -->

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: source/revision, question, actual decision and provenance. Separate direct quotes, confirmed decisions, reviewer suggestions and agent choices; never manufacture authorization.
- **Metrics review** — events added, extra events found during `/review`, analytics/funnel playbook update or the explicit no-change reason.
- **Skill-chain outcomes** — `/orly-write-unit-test`, `/review`, `orly-babysit-prs` results (order per `AGENTS.orly.md` CHORE(close); iteration counts, findings dispositioned).
- **Deferrals** — every "deferred to follow-up" needs an **Indy-acked verbatim quote** here, format `> Indy (YYYY-MM-DD HH:MM): "<quote>" — context: <which item, why>`. An agent-unilateral deferral is **incomplete scope, not deferral**, and blocks CHORE(close) until the item lands or the quote is captured.

<!-- tpl: Keep the four bullet headers. At authoring, record consultations,
source findings and user decisions already supplied; leave future skill/runtime
outcomes pending and all Graded cells empty. A confirmed planning input is not a
measured SLO; a proposed risk exception is not approval. Record superseded
requirements explicitly and update canonical docs/tests so old alternatives
cannot be mistaken for simultaneous requirements. -->
