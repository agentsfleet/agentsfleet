<!--
SPEC AUTHORING RULES (the one comment that survives):
- The spec standard (spec.md) holds every rule. Each tpl: comment names the
  rules that govern its section; delete it once the section is filled.
- Body order is the implementing agent's read order.
- No time or effort estimates, owners or implementation dates (SPEC-ESTIMATE).
-->

<!-- tpl: Copy this file to docs/v<P>/pending/ under the SPEC-NAME file name.
Replace every {{fill:…}} slot; `docs.language` reports one left on a changed
spec line (SPEC-FILL). Keep each ## heading, and write "N/A:" with the reason
where a section does not apply. -->

# M{{fill:Milestone}}_{{fill:Workstream}}: {{fill:Title — testable, not vague}}

**Prototype:** v{{fill:major}}.{{fill:minor}}.{{fill:patch}}

**Milestone:** M{{fill:Number}}

**Workstream:** {{fill:001 to 005}}

**Date:** {{fill:MMM DD, YYYY}}

**Status:** PENDING

**Priority:** {{fill:P0 | P1 | P2 | P3}}, {{fill:one-line reason}}

**Categories:** {{fill:codes from API, CLI, DOCS, GOV, INFRA, OBS, SKILL and UI, alphabetised and comma-separated}}

**Batch:** B{{fill:1 to 4}}, {{fill:parallel group context}}

**Depends on:** {{fill:spec identifiers with one-line reasons, or none}}

**Provenance:** {{fill:human-written | LLM-drafted (model, date) | agent-generated (source document)}}

**Canonical architecture:** {{fill:`docs/architecture/<doc>.md` §N, or none with the reason}}

<!-- tpl: SPEC-NUM, SPEC-STATUS. LLM-drafted means a large language model
drafted it; provenance records origin, not proof. A deliberately folded
Workstream adds **Folded-into:** naming the owning spec. -->

---

## Overview

**Goal (testable):** {{fill:one sentence that could be a test name}}

**Problem:** {{fill:observable symptoms, in user-facing terms}}

**Solution summary:** {{fill:what changes, at what layer, and the user-visible outcome}}

**Design flow:** {{fill:ordered flow of the components and data this change adds or alters, or "unchanged" with the architecture section it matches}}

<!-- tpl: SPEC-GOAL, SPEC-GOLDEN. Name who benefits and the observed problem;
never invent demand. -->

## PR Intent & comprehension handshake

- **PR title (eventual):** {{fill:imperative title of at most 72 characters for the merged Pull Request (PR)}}
- **Intent (one sentence):** {{fill:why this PR exists, in user-facing-outcome terms}}
- **Authoring handshake:** {{fill:the requester's intent restated, then ASSUMPTIONS I'M MAKING: …}}
- **Implementer handshake:** recorded in the Pull Request Session Notes at PLAN, before any edit (SPEC-HANDSHAKE).

## Implementing agent — read these first

1. `{{fill:path/to/file.ext}}` — {{fill:why this is the pattern to mirror}}
2. `{{fill:path/to/spec_or_doc.md}}` — {{fill:what canonical knowledge lives there}}

<!-- tpl: SPEC-READ: three to five existing paths or https:// URLs. -->

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `{{fill:path}}` | {{fill:CREATE \| EDIT \| DELETE}} | {{fill:how this file's role changes}} |

<!-- tpl: SPEC-FILES, SPEC-BLAST. Include this spec, its move to done/, the
changelog entry and any architecture page. -->

## Applicable Rules

- **{{fill:standard, such as `tests.md`}}** — {{fill:exact rule identifiers this diff trips}}

<!-- tpl: SPEC-RULES. A diff with no source change writes "N/A: no source
files change." -->

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| {{fill:linter, hook or orly gate criterion this diff can trip}} | {{fill:yes/no — why}} | {{fill:how the diff stays clean}} |
| File & Function Length (file ≤350 lines, function ≤50, method ≤70) | {{fill:yes/no}} | {{fill:split plan if a file nears the cap}} |

<!-- tpl: A docs-only diff writes "N/A: docs/markdown only." -->

## Prior-Art / Reference Implementations

- **Reference:** {{fill:path or codebase}} — {{fill:what to mirror, or the justified divergence}}

<!-- tpl: SPEC-PRIOR. -->

## Sections (implementation slices)

### §1 — {{fill:Slice title}}

{{fill:what this slice delivers, the mechanism it requires and what it unblocks}}. Non-obvious choice → **Implementation default:** {{fill:choice}} because {{fill:reason}}.

- **Dimension 1.1** — {{fill:smallest verifiable behaviour}} → Test `{{fill:test_name}}` ({{fill:unit | integration | e2e | manual}}): {{fill:concrete input → expected output}}

<!-- tpl: SPEC-SIZE, SPEC-MECH, SPEC-PROSE, SPEC-ORDER, SPEC-DECIDE, SPEC-DIM,
SPEC-FAIL. A finished Dimension reads `- **Dimension 1.1** — DONE — …` and
changes in the commit that delivers it (SPEC-TESTS, SPEC-WIRED). -->

## Interfaces

{{fill:the surfaces other code depends on, with a real example where the shape is not obvious, or "N/A: no interface changes."}}

<!-- tpl: SPEC-IFACE. -->

## Failure Modes

| Mode | Kind | Cause | Handling (response and what the caller observes) | Test |
|------|------|-------|--------------------------------------------------|------|
| {{fill:short name}} | {{fill:failure \| security \| capacity}} | {{fill:trigger}} | {{fill:response and observable}} | `{{fill:test_name}}` |

<!-- tpl: SPEC-FAIL. Each Test cell names a Dimension's test. -->

## Invariants

1. {{fill:invariant}} — {{fill:the code that enforces it}}

<!-- tpl: SPEC-INVARIANT. None: "N/A: no invariants." -->

## Metrics & Observability

| Metric / event | Audience | Fires when | Properties allowed | Privacy guard | Test |
|----------------|----------|------------|--------------------|---------------|------|
| `{{fill:event_name}}` | {{fill:product \| operator}} | {{fill:exact user or system action}} | {{fill:coarse context, resource id, duration, outcome}} | {{fill:no raw email, password, token, one-time password or key material}} | `{{fill:test_name}}` |

<!-- tpl: SPEC-METRICS. No signal changes: "N/A: no product or operator
signal changes." -->

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | {{fill:outcome the user can observe}} (§1) | `{{fill:command}}` | {{fill:exit 0 / substring / 0 matches}} | P0 | |
| S1 | Declared commands green | `{{fill:one row per declared conform and verify.* command, verbatim}}` | exit 0 | P0 | |
| S2 | No secrets | `gitleaks detect --redact --no-banner` | exit 0 | P0 | |

<!-- tpl: SPEC-RUBRIC, SPEC-GRADE, SPEC-SHIP, SPEC-MOVE. Authoring fills
every column except Graded. -->

### Behaviour evals

<!-- tpl: Delete this subsection unless the diff changes a prompt, a model
choice or agent instructions. Grounding means the output states only facts
present in its inputs. -->

- **Grounding rule:** {{fill:one sentence the output must never violate}}
- **Golden set:** {{fill:N}} cases in `{{fill:path}}`, including the worst known case; each failure found in use becomes a case.
- **Ship threshold:** grounding holds in every case and at least {{fill:N}}% pass, each threshold an R row whose command computes it.
- **Fallback:** {{fill:the recoverable behaviour when a threshold is missed}}. Fabricated output fails a P0 row.

## Dead Code Sweep

| Deleted file or symbol | Verify | Expected |
|------------------------|--------|----------|
| `{{fill:path or symbol}}` | `git grep -n -w "{{fill:symbol}}"` | no output |

<!-- tpl: SPEC-BLAST. No deletions: "N/A: no files deleted." -->

## Out of Scope

- {{fill:item excluded, pointing at a follow-up spec or "not planned"}}

<!-- tpl: Nothing excluded: "None beyond the Files Changed table." -->

---

## Product Clarity (authoring record)

<!-- tpl: SPEC-CLARITY. Required when Categories include API, CLI or UI;
otherwise write "N/A: no user surface." in place of the list. -->

1. **Successful user moment** — {{fill:the single observable moment that proves this worked}}
2. **Preserved user behaviour** — {{fill:what users do today that keeps working unchanged}}
3. **Optimal-way check** — {{fill:the most direct way to deliver moment 1, and the gap to it}}
4. **Rebuild-vs-iterate** — {{fill:the verdict on a larger refactor}}
5. **What we build** — {{fill:the shortest list of pieces that delivers moment 1}}
6. **What we do NOT build** — {{fill:adjacent scope rejected, one-line reason each}}
7. **Fit with existing features** — {{fill:what this compounds with, and the one feature it must not destabilize}}
8. **Surface order** — {{fill:CLI-first, UI-first or both, with the reason}}
9. **Dashboard restraint** — {{fill:what the UI hides until the signal behind it is real}}
10. **Confused-user next step** — {{fill:the self-serve move: a command, an error message or a doc}}

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** {{fill:why this Section split}}
- **Alternatives considered:** {{fill:other mechanisms, each with its deciding criterion}}
- **Patch-vs-refactor verdict:** a **{{fill:patch | refactor}}** because {{fill:reason}}.

<!-- tpl: A different mechanism is another way to deliver the outcome, not a
bigger or smaller version of the same design. -->

## Discovery (consult log)

- **Consults** — {{fill:architecture, legacy-design and gate-flag consults: source revision, question, decision and who decided}}
- **Model reviews** — {{fill:each model review with its model, version, finding counts and dispositions}}
- **Open blockers** — {{fill:each unresolved blocker with its resolving action, or none}}
- **Deferrals and transfers** — none. Each one needs an owner quote below, as `> Owner (<name>, <MMM DD, YYYY HH:MM>): "<quote>" — context: <item, why>`.

<!-- tpl: SPEC-DECIDE, SPEC-DEFER, SPEC-MODEL. Keep direct quotes, confirmed
decisions, reviewer suggestions and agent choices apart. -->
