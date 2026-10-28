<!--
SPEC AUTHORING RULES (load-bearing; the one comment that survives):
- Body order is the implementing agent's read order. The orly-spec skill fills
  this template under the spec standard (spec.md), then deletes every "tpl:"
  guidance comment.
- An EGM (Elephant-Goldfish Model) review checks the structure: leftover tpl
  comments, unfilled fill slots, missing or empty sections and the spec
  standard's rules. orly's gate checks no spec structure.
- No time or effort estimates anywhere: no effort columns, complexity ratings,
  percentage complete, implementation dates or assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Depends on is the only
  sequencing signal. A section that contradicts these rules loses; delete it.
-->

<!-- tpl: Copy this whole file to docs/v<P>/pending/, named as SPEC-NAME gives.
Replace every {{fill:…}} slot; a slot listing options, such as {{fill:P0 | P1}},
means choose one. Runtime parameters such as /projects/{project_id} stay when the
spec defines them. Every ## section is required and non-empty: keep each
heading as written, and use the N/A form its guidance gives when it does not
apply. -->

# M{{fill:Milestone}}_{{fill:Workstream}}: {{fill:Title — testable, not vague}}

**Prototype:** v{{fill:major}}.{{fill:minor}}.{{fill:patch}}

**Milestone:** M{{fill:Number}}

**Workstream:** {{fill:001 to 005}}

**Date:** {{fill:MMM DD, YYYY}}

**Status:** PENDING

**Priority:** {{fill:P0 | P1 | P2 | P3}}, {{fill:one-line reason}}

**Categories:** {{fill:codes from API, CLI, DOCS, GOV, INFRA, OBS, SKILL and UI, alphabetised and comma-separated}}

**Batch:** B{{fill:1 to 4}}, {{fill:parallel group context}}

**Branch:** pending, set at CHORE(open)

**Baseline revision:** pending, set to the full comparison commit at CHORE(open)

**Test Baseline:** pending, measured before the Pull Request

**Baseline evidence:** pending, set to a report path or run URL before the Pull Request

**Depends on:** {{fill:spec identifiers with one-line reasons, or none}}

**Provenance:** {{fill:human-written | LLM-drafted (model, date) | agent-generated (pre-spec, source doc)}}

**Canonical architecture:** {{fill:`docs/architecture/<doc>.md` §N, or none with the reason}}

<!-- tpl: Provenance records origin, not proof; LLM-drafted means drafted by a
large language model. Cross-check correctness-critical claims against the
actual caller, schema and pinned dependencies whoever wrote them. Canonical
architecture names the docs/architecture/ section that owns the mechanism.
A deliberately folded Workstream adds **Folded-into:** naming the owning spec.
Status, Branch and the three Baseline headers change only at their lifecycle
stages (SPEC-STATUS, SPEC-BASELINE). -->

---

## Overview

**Goal (testable):** {{fill:one sentence that could be a test name}}

**Problem:** {{fill:observable symptoms, in user-facing terms}}

**Solution summary:** {{fill:one paragraph: what changes, at what layer, and the user-visible outcome}}

**Design flow:** {{fill:ordered flow of the components and data this change adds or alters, or "unchanged" with the architecture section it matches}}

<!-- tpl: Name who benefits and the observed problem; never invent demand.
Goal (SPEC-GOAL): bad "Implement streaming." Good: "The Server-Sent Events
(SSE) handler streams project events as text/event-stream with stable ordering
and reconnection, p95 under 200ms." Problem: symptoms, never implementation
language. Design flow: a short numbered list or ASCII flow that a reader given
only this spec can follow; implementation steps belong in Sections. -->

## PR Intent & comprehension handshake

- **PR title (eventual):** {{fill:imperative title of at most 72 characters for the merged Pull Request (PR)}}
- **Intent (one sentence):** {{fill:why this PR exists, in user-facing-outcome terms}}
- **Authoring handshake:** {{fill:the author's restatement of the requester's intent, then ASSUMPTIONS I'M MAKING: …}}
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit. A restatement that differs from the Intent stops the work until the two are reconciled.

## Implementing agent — read these first

1. `{{fill:path/to/file.ext}}` — {{fill:why this is the right pattern to mirror}}
2. `{{fill:path/to/spec_or_doc.md}}` — {{fill:what canonical knowledge lives there}}
3. {{fill:external doc URL, if relevant}} — {{fill:what convention to follow}}

<!-- tpl: Three to five pointers to read before touching any file. Each item
starts with a backticked path that exists in this repository or a bare
https:// URL; files this spec creates belong in Files Changed (SPEC-READ).
Greenfield: say so and point at the docs/architecture/ page that defines the
shape. State correctness-critical requirements in the relevant section too. -->

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `{{fill:path}}` | {{fill:CREATE \| EDIT \| DELETE}} | {{fill:one line on how this file's role changes}} |

<!-- tpl: Every file created, edited or deleted, including this spec, its move
to done/, the changelog entry and any docs/architecture/ page; files and
roles, never line numbers or function names (SPEC-FILES). A teardown, rename
or flip spec starts from `git grep -n -w '<token>'` at the repository root
(SPEC-BLAST). Include real writers and readers, schema registration, grants,
fixtures, deployment and secret references and protocol docs. Never schedule
edits to frozen migration history. -->

## Applicable Rules

- **{{fill:standard, such as `tests.md` or `languages/rust.md`}}** — {{fill:exact rule identifiers this diff trips, such as TCF, NDC, ORP and FLL}}
- **{{fill:another standard}}** — {{fill:rule identifiers, and why they apply to this diff}}

<!-- tpl: The standards under .orly/standards/ that the implementing agent
rereads before building and checks again at VERIFY, with exact rule
identifiers (SPEC-RULES). A standard this repository did not install does not
apply. A diff with no source change writes "N/A: no source files change." -->

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| {{fill:linter rule, hook or orly gate criterion this diff can trip}} | {{fill:yes/no — why}} | {{fill:how the diff stays clean}} |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | {{fill:yes/no}} | {{fill:split plan if a file approaches the cap}} |

<!-- tpl: The checks this diff will trip, declared up front so the
implementing agent plans for them: the repository's linters (clippy, oxlint,
gitleaks), its hooks, and the orly gate criteria and verify.* commands it
declares. Rules are knowledge to read; gates are checks that fire. A
docs-only diff replaces the table with "N/A: docs/markdown only." -->

## Prior-Art / Reference Implementations

- **Reference:** {{fill:path / codebase}} — {{fill:one line on alignment, or the justified divergence}}

<!-- tpl: Name a known-good implementation to mirror (SPEC-PRIOR). Command-line
work states which of six practices it follows: command, handler and error
layers apart; handlers free of console output and process exit; one rendering
layer; structured JSON errors with suggestion and retry fields; tests at
handler, in-process and subprocess tiers; JSON by default when stdout is not
a terminal. API work cites the HTTP standard and the closest handler; schema
work the nearest migration and the SQL standard; UI work design-system
primitives and theme tokens. No prior art: say so and point at the
docs/architecture/ page. -->

## Sections (implementation slices)

### §1 — {{fill:Slice title}}

{{fill:what this slice delivers, why it exists, and what it unblocks}}. Non-obvious choice → **Implementation default:** `{{fill:choice}}` because `{{fill:reason}}`.

- **Dimension 1.1** — {{fill:smallest verifiable behaviour}} → Test `{{fill:test_name}}`
- **Dimension 1.2** — {{fill:…}} → Test `{{fill:test_name}}`

### §2 — {{fill:Slice title}}

{{fill:Same shape.}}

<!-- tpl: A spec has 3 to 9 Sections (SPEC-SIZE). Each names the delivered
behaviour, the required mechanism (SPEC-MECH) and its proof, without coding
recipes (SPEC-PROSE). State prerequisites in acyclic order (SPEC-ORDER) and
who resolves each decision (SPEC-DECIDE). Name the source or resolving action
for every credential, permission and connection, never a secret value. Keep
the Dimension line form above; a finished one reads `- **Dimension 1.1** —
DONE — …` and changes in the commit that delivers it (SPEC-TESTS, SPEC-WIRED).
Good: "§3 Retry-safe import. Each record carries its source identity; a rerun
after a crash skips committed records." Bad: "add a dedupe key" with no rule
for what a crash leaves behind. -->

## Interfaces

```
{{fill:HTTP endpoints, request/response shapes, internal signatures other code depends on}}
```

<!-- tpl: The surface the implementing agent must not change without amending
this spec (SPEC-IFACE): input, output and error shapes, with a real example
payload where the shape is not obvious. No interface change: replace the block
with "N/A: no interface changes." -->

## Failure Modes

| Mode | Kind | Cause | Handling (system response + what the caller observes) | Test |
|------|------|-------|--------------------------------------------------------|------|
| {{fill:short name}} | {{fill:failure \| security \| capacity}} | {{fill:trigger}} | {{fill:response + observable}} | `{{fill:test_name}}` |

<!-- tpl: Every failure path, security implication and capacity limit, each
with the test that proves its handling (SPEC-FAIL). Cover timeout, malformed
input, auth failure, network blip, race, replay, exceeded quota and dependency
unavailable where they apply. Distinguish timeout from rollback, and an
acknowledgement from durable recovery. For partial setup or delivery, state
what is retained, what resumes it and how a retry avoids duplicate effects. A
known limit with no mitigation here needs an owner disposition in Discovery. -->

## Invariants

1. {{fill:Invariant}} — {{fill:how it's enforced}}

<!-- tpl: Each is enforced by code: the compiler, a linter, a compile-time
assertion or a runtime check, never review discipline (SPEC-INVARIANT). Name
the authority, the mechanism and the failure assumptions, and test the
shortest counterexample. None: write "N/A: no invariants." -->

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|----------|------------|--------------------|---------------|------------|
| `{{fill:event_name}}` | {{fill:product \| operator}} | {{fill:exact user or system action}} | {{fill:coarse product context, resource id, duration, outcome}} | {{fill:no raw email, password, token, One-Time Password (OTP) or Secure Shell (SSH) key material}} | `{{fill:test_name}}` |

<!-- tpl: Declare each product or operator signal this spec adds, renames or
leaves unchanged (SPEC-METRICS). No signal changes: replace the table with
"Not applicable: no product or operator signal changes." When a funnel
changes, update the repository's analytics playbook in the same Pull
Request. -->

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | {{fill:unit \| integration \| e2e \| manual}} | `{{fill:test_name}}` | {{fill:one-line behavioural claim}} |

<!-- tpl: One row per Dimension, using its exact test name (SPEC-DIM); prose
and assertions only, no test code. At least half the rows test failure paths,
and every Failure Mode and Metrics row has a test (SPEC-FAIL). Tiers: unit
for logic and boundaries; integration for the real stack with deterministic
injection; e2e for any user-facing Category through the real path (SPEC-E2E);
manual for human acceptance with its procedure, person and evidence
(SPEC-DECIDE). The tests standard governs every row. For each promised
result, name a plausible false pass the check must reject. -->

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | {{fill:outcome the user can observe}} (§1) | `{{fill:command}}` | {{fill:exit 0 / substring / 0 matches}} | P0 | |
| S1 | Conform command green, where declared | `{{fill:conform command from .orly/orly.json, verbatim}}` | exit 0 | P0 | |
| S2 | Unit tests pass | `{{fill:verify.unit command from .orly/orly.json, verbatim}}` | exit 0 | P0 | |
| S3 | Remaining declared lanes green | `{{fill:one row per remaining declared verify.* command, verbatim}}` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

The spec standard (`spec.md`) defines grading, the ship rule and transfers to a successor spec (`MOVED` rows) in SPEC-GRADE, SPEC-SHIP and SPEC-MOVE.

<!-- tpl: 5 to 12 rows in total, R and S together (SPEC-RUBRIC). R rows grade
this spec's outcomes, one per Section outcome or failure class, never one per
Dimension. S rows carry every declared conform and verify.* command verbatim,
plus the secret scan. Expected is an exit code, a literal substring or a match
count. A Verify command holds no pipe; wrap a pipeline in a script. Authoring
fills every column except Graded. -->

### Behaviour evals

- **Grounding rule:** {{fill:one sentence the output must never violate}}
- **Golden set:** {{fill:N}} cases in `tests/fixtures/{{fill:path}}`, covering {{fill:3 to 5 axes, including the worst known case}}. Every failure found in use becomes a new case.
- **Ship threshold:** grounding holds in every case, at least {{fill:N}}% of cases pass, and the worst known case never fails. Each threshold is an R row whose command computes it.
- **Fallback:** {{fill:the recoverable behaviour when a threshold is missed}}. Fabricated output fails a P0 row.

<!-- tpl: Delete this subsection unless the diff changes a prompt, a model
choice or agent instructions. Grounding means the output states only facts
present in its inputs. The worst known case is the input that failed worst in
past runs. -->

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `{{fill:path/to/old_file.ext}}` | `test ! -f {{fill:path/to/old_file.ext}}` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `{{fill:old_symbol}}` | `git grep -n -w "{{fill:old_symbol}}"` | no output |

<!-- tpl: Required when the spec deletes or replaces files: search the
repository for every deleted file and every removed or renamed public symbol,
with the same word-boundary pattern as the discovery search (SPEC-BLAST). No
deletions: replace both tables with "N/A: no files deleted." -->

## Out of Scope

- {{fill:Item explicitly not in this spec — points at a follow-up spec or "future work"}}

<!-- tpl: Nothing excluded: write "None beyond the Files Changed table." Each
item points at a follow-up spec or says it is not planned. -->

---

<!-- tpl: Product Clarity and Decomposition are the authoring record: written
before the implementation sections, read once at PLAN, and not reopened during
implementation. Discovery is the running log. They sit last so the working
sections stay first. -->

## Product Clarity (authoring record)

1. **Successful user moment** — {{fill:the single observable moment that proves this worked — a scene, not a metric}}
2. **Preserved user behaviour** — {{fill:what users do today that keeps working unchanged; breaking any of it is a redesign}}
3. **Optimal-way check** — {{fill:the most direct way to deliver moment 1, and the gap to the unconstrained shape}}
4. **Rebuild-vs-iterate** — {{fill:the verdict on a larger refactor; its rationale goes in Decomposition below}}
5. **What we build** — {{fill:the shortest list of pieces that delivers moment 1}}
6. **What we do NOT build** — {{fill:adjacent scope rejected, one-line reason each — seeds Out of Scope}}
7. **Fit with existing features** — {{fill:what this compounds with; the one feature it must not destabilize}}
8. **Surface order** — {{fill:CLI-first (repo default), UI-first, or both; justify divergence}}
9. **Dashboard restraint** — {{fill:what the UI hides until the signal behind it is real}}
10. **Confused-user next step** — {{fill:the self-serve move: a command, an error message or a doc}}

<!-- tpl: The owner's ten product questions, answered in order before the
implementation sections (SPEC-CLARITY), one short paragraph or list item each.
A question that cannot be answered yet blocks the spec. Internal-only work
answers 1 to 7 compactly; 8 to 10 may be "N/A: no user surface" with the
reason. A refactor that trades run-to-run determinism away is wrong by
default. "File a ticket" in item 10 means a surface is missing from item 5. -->

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** {{fill:why this Section/Workstream split — the decomposition rationale}}
- **Alternatives considered:** {{fill:one or more different mechanisms, each with its deciding criterion: complexity, cost, performance, scale or security}}
- **Patch-vs-refactor verdict:** this is a **{{fill:patch | refactor}}** because {{fill:reason}}. {{fill:If a larger refactor is the right long-term answer but out of scope, name its follow-up spec.}}

<!-- tpl: Match the solution's size to the problem's size and surface that
call before approval, not mid-Pull Request. A different mechanism is another
way to deliver the outcome, not a bigger or smaller version of the same
design. -->

## Discovery (consult log)

- **Consults** — {{fill:architecture, legacy-design and gate-flag consults: source revision, question, decision and who decided}}
- **Model reviews** — {{fill:each model review, the EGM review included, with model and version, finding counts and dispositions}}
- **Open blockers** — {{fill:each unresolved blocker with its boundary and resolving action, or none}}
- **Metrics review** — {{fill:events added, events found during review, or the reason no signal changed}}
- **Skill-chain outcomes** — pending until VERIFY and REVIEW.
- **Deferrals and transfers** — none. Each one needs an owner quote as a blockquote below this list, in the form `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.

<!-- tpl: Keep the bullet labels. At authoring, record consults, source
findings and owner decisions already supplied; leave later outcomes pending
and every Graded cell empty. Separate direct quotes, confirmed decisions,
reviewer suggestions and agent choices, and never manufacture authorization
(SPEC-DEFER, SPEC-MODEL). Record superseded requirements explicitly, and keep
raw model-review output out of the spec. -->
