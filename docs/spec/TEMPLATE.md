# Milestone Specification Template

> **CANONICAL TEMPLATE** — Copy this file when creating new milestone specs.
> Place the copy in `docs/spec/v1/pending/M{N}_{WS}_{NAME}.md`.
> See `AGENTS.md → Specification Standards → Spec Lifecycle` for the full workflow.

---

## How to Use This Template

1. Copy this file to `docs/spec/v1/pending/M{N}_{WS}_{DESCRIPTIVE_NAME}.md`.
2. Replace all `{placeholder}` values with real content.
3. Fill in **every** section, dimension, and acceptance criterion — no skeletons.
4. Set `Status: PENDING` and commit to `main`.
5. When work begins, the spec moves to `active/` per the Spec Lifecycle.

---

## Hierarchy

```
v1.0.0 (Prototype)
└── Milestones (M1, M2, M3...)
    └── Workstreams (M1_001, M1_002, M1_003...)
        └── Sections (1.0, 2.0, 3.0...)
            └── Dimensions (1.1, 1.2, 1.3...)
```

**Terminology:**
- **Prototype** — v1.0.0 (major release)
- **Milestone** — Major phase (M1, M2, M3). A working prototype capability that can be demoed end-to-end with evidence.
- **Workstream** — Parallel track within milestone. One singular working function that contributes to exactly one milestone capability. ID is 3-digit zero-padded (`001`, `002`, `008`). No alphabetic suffixes.
- **Batch** — Parallel execution group (B1, B2, B3...). Workstreams in the same batch can run concurrently. Batches are sequential — B2 starts after B1 gates clear.
- **Section** — Implementation slice inside a workstream (what will be built).
- **Dimension** — Verification-unit check inside a section (unit/integration/contract testable item).

A milestone is not complete until demo evidence is captured (commands, logs, screenshots, or recorded walkthrough notes).

### Examples

**Milestone examples:**
- "CLI works with zombied" (demo: login → workspace list → run status)
- "PostHog works in website" (demo: CTA click → event visible in PostHog)
- "Free plan billing works" (demo: free-tier entitlement enforcement)

**Workstream examples:**
- `M4_001` Implement CLI runtime (singular function: local operator runtime works)
- `M4_002` Publish npm package (singular function: installable distribution works)
- `M5_005` Enable PostHog in website (singular function: web analytics path works)

**Section examples:**
- Configure auth flow
- Implement run command lifecycle
- Wire event emission helpers

**Dimension examples:**
- Unit test: CLI parser handles required flags
- Integration test: run command returns deterministic state transitions
- Contract test: emitted analytics payload contains required keys

---

## Guardrails

### Workstream Count

- Default limit: each milestone may define at most **4 workstreams**.
- A **5th workstream** is allowed when it is a cross-cutting concern that feeds into multiple existing workstreams and would lose coherence if split to a separate milestone.
- Beyond 5 is never allowed — split into a new milestone.
- Goal: keep milestones small enough to demo and close quickly.

### Dimension Count

- Hard limit: each section may define at most **4 dimensions**.
- If more than 4 are needed, split into another section or create a new workstream.
- Goal: keep execution chunks small, reviewable, and demoable.

---

## File Naming

```
docs/spec/v1/{pending|active|done}/M{Milestone}_{Workstream}_{DESCRIPTIVE_NAME}.md

Example: docs/spec/v1/pending/M3_007_CLERK_AUTH.md
         └─┬─┘ └──┬──┘ └┬─┘ └──────┬────────┘
           │      │     │           └─ Descriptive name (UPPERCASE_SNAKE_CASE)
           │      │     └─ Workstream (3-digit zero-padded: 001–009)
           │      └─ Milestone (1-9)
           └─ Milestone prefix
```

---

## Status Markers

- `PENDING` — Not started, awaiting work
- `IN_PROGRESS` — Currently being worked on
- `DONE` or `✅` — Complete, verified, and tested

---

## Prohibited

- No time estimates ("5 min", "1 hour", "2 days") — meaningless and often wrong
- No effort columns or complexity ratings — use Priority instead
- No percentage complete — use binary PENDING/DONE states
- No assigned owners — use git history and handoff notes
- No implementation dates — use Priority (P0/P1/P2) instead

Use **Priority** (P0/P1/P2) and **Dependencies** for sequencing.

---

## Spec Template

Copy everything below this line when creating a new spec:

---

# M{Milestone}_{Workstream}: {Title}

**Prototype:** v{major}.{minor}.{patch}
**Milestone:** M{Number}
**Workstream:** {001-009}
**Date:** {MMM DD, YYYY}
**Status:** PENDING | IN_PROGRESS | DONE
**Priority:** P0 | P1 | P2 — {Description}
**Batch:** B{1-4} — parallel execution group
**Branch:** {feat/mNN-name — added when work begins}
**Depends on:** {Dependencies}

---

## 1.0 {Section Title}

**Status:** PENDING

Description of this section. Explain what will be built and why.

**Dimensions:**
- 1.1 PENDING First dimension
- 1.2 PENDING Second dimension
- 1.3 PENDING Third dimension

---

## 2.0 {Next Section}

**Status:** PENDING

### 2.1 {Subsection}

Description.

**Dimensions:**
- 2.1.1 PENDING Dimension item
- 2.1.2 PENDING Dimension item

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Criteria item one
- [ ] 3.2 Criteria item two
- [ ] 3.3 Criteria item three

---

## 4.0 Out of Scope

- Item not in scope
- Another out of scope item
