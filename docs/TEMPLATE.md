# Milestone Specification Template

> **CANONICAL TEMPLATE** — Copy this file when creating new milestone specs.
> Place the copy in `docs/v1/pending/M{N}_{WS}_{NAME}.md`.
> See `AGENTS.md → Specification Standards → Spec Lifecycle` for the full workflow.

---

## How to Use This Template

1. Copy this file to `docs/v1/pending/M{N}_{WS}_{DESCRIPTIVE_NAME}.md`.
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
docs/v1/{pending|active|done}/M{Milestone}_{Workstream}_{DESCRIPTIVE_NAME}.md

Example: docs/v1/pending/M3_007_CLERK_AUTH.md
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

Copy everything below this line when creating a new spec.

**Rules for filling in:**
- Every section is mandatory. If a section doesn't apply, write "N/A — {reason}".
- Dimensions must be machine-readable test blueprints, not prose descriptions.
- Interfaces must specify exact function signatures, input shapes, and output shapes.
- Failure modes must enumerate every error path and what happens on each.
- Constraints must be measurable (not "fast" — "< 5ms per message").

---

# M{Milestone}_{Workstream}: {Title — must be testable, not vague}

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

## Overview

**Goal (testable):** {One sentence that could be a test name. Bad: "Implement streaming." Good: "SSE handler streams Redis pubsub messages as text/event-stream with stable ordering and reconnection support."}

**Problem:** {What is broken or missing — observable symptoms, not implementation details.}

**Solution summary:** {One paragraph. What changes, at what layer, and what the user-visible outcome is.}

---

## 1.0 {Section Title}

**Status:** PENDING

Description of this section. Explain what will be built and why.

**Dimensions (test blueprints):**
- 1.1 PENDING
  - target: `{file}:{function_or_struct}`
  - input: `{structured input — types, shapes, examples}`
  - expected: `{structured output — exact return value, side effect, or state change}`
  - test_type: unit | integration | contract
- 1.2 PENDING
  - target: `{file}:{function_or_struct}`
  - input: `{structured input}`
  - expected: `{structured output}`
  - test_type: unit | integration | contract

---

## 2.0 {Next Section}

**Status:** PENDING

{Same pattern as 1.0 — sections are implementation slices, dimensions are test blueprints.}

---

## N.0 Interfaces

**Status:** PENDING

Lock the API surface. Every public function, endpoint, and data shape that this workstream introduces or modifies.

### N.1 Public Functions

```
{language}
{exact function signature — not pseudocode}
```

### N.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| {name} | {type} | {validation rules} | {example value} |

### N.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| {name} | {type} | {condition} | {example value} |

### N.4 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Timeout | {what happens} | {return value or error code} |
| Connection lost | {what happens} | {return value or error code} |
| Malformed input | {what happens} | {return value or error code} |
| Auth failure | {what happens} | {return value or error code} |

---

## N+1.0 Failure Modes

**Status:** PENDING

Enumerate every failure path. For each: what triggers it, what the system does, and what the user/caller observes.

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| {name} | {condition} | {action taken} | {output/error} |

**Platform constraints:**
- {e.g., "SO_RCVTIMEO does not propagate through TLS record layer on Linux — timeout fires ReadFailed, not WouldBlock"}
- {e.g., "client.open() does not exist on x86_64-linux in Zig 0.15.2 — use client.request()"}

---

## N+2.0 Implementation Constraints (Enforceable)

**Status:** PENDING

Each constraint must be measurable — not "fast" or "efficient" but a number or a verification command.

| Constraint | How to verify |
|-----------|---------------|
| {e.g., "Zero heap allocations in hot path"} | {e.g., "std.testing.allocator detects leaks; grep for alloc in loop body"} |
| {e.g., "Max latency per message < 5ms"} | {e.g., "benchmark test with 1000 messages"} |
| {e.g., "File under 500 lines"} | {e.g., "wc -l < 500"} |
| {e.g., "Cross-compiles on x86_64-linux, aarch64-linux"} | {e.g., "zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux"} |

---

## N+3.0 Test Specification

**Status:** PENDING

Every dimension from sections 1.0–N.0 must map to a test case here. This section is the input to `/write-unit-test`.

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| {name} | {1.1} | {file:fn} | {input} | {output} |

### Integration Tests

| Test name | Dimension | Infra needed | Input | Expected |
|-----------|-----------|-------------|-------|----------|
| {name} | {2.1} | {DB / Redis / both} | {input} | {output} |

### Contract Tests

| Test name | Dimension | What it proves |
|-----------|-----------|---------------|
| {name} | {N.3} | {output matches exact format} |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| {e.g., "streams in real time"} | {e.g., "bytes arrive before connection closes"} | integration |
| {e.g., "reconnect replays only missed events"} | {e.g., "Last-Event-ID filters correctly"} | integration (DB) |

---

## N+4.0 Execution Plan (Ordered)

**Status:** PENDING

Ordered steps. Agent executes top-to-bottom. Each step has a verification command.

| Step | Action | Verify |
|------|--------|--------|
| 1 | {Define interfaces} | {Compiles with no impl} |
| 2 | {Implement core logic} | {make test passes} |
| 3 | {Add failure handling} | {make test passes} |
| 4 | {Generate tests via /write-unit-test} | {all tests pass} |
| 5 | {Cross-compile check} | {zig build -Dtarget=x86_64-linux} |

---

## N+5.0 Acceptance Criteria

**Status:** PENDING

Each criterion must be verifiable by running a command or inspecting output — not "works correctly".

- [ ] {Criterion} — verify: `{command}`
- [ ] {Criterion} — verify: `{command}`
- [ ] {Criterion} — verify: `{command}`

---

## N+6.0 Verification Evidence

**Status:** PENDING

Filled in during VERIFY phase. Proves the spec claims are met.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {output} | |
| Integration tests | `make test-integration` | {output} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {output} | |
| Lint | `make lint` | {output} | |
| 500L gate | `wc -l` | {output} | |

---

## N+7.0 Out of Scope

- {Item not in scope}
- {Another out of scope item}
