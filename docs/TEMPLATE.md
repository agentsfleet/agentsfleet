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
- Invariants are compile-time or lint-time guardrails — violations must be build failures, not review comments.
- Eval commands are executable scripts run post-implementation — every one must pass before PR.
- Dead code sweep is mandatory for any spec that deletes or replaces files.

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

## Files Changed (blast radius)

List every file that will be created, modified, or deleted. This scopes
the 350-line gate, `pub` audit, orphan sweep, and domain lint checks.

| File | Action | Why |
|------|--------|-----|
| {e.g., `src/errors/error_registry.zig`} | CREATE | {single source of truth for error codes} |
| {e.g., `src/http/handlers/common.zig`} | MODIFY | {update import from error_table → error_registry} |
| {e.g., `src/errors/error_table.zig`} | DELETE | {replaced by error_registry.zig} |

## Applicable Rules

List RULES.md rule IDs that apply to this spec's scope. The agent MUST
re-read these before EXECUTE and verify no violations during VERIFY.

- {e.g., RULE FLS — flush all layers (if touching pg queries)}
- {e.g., RULE OWN — one owner per resource (if adding init/deinit)}
- {e.g., RULE XCC — cross-compile before commit (always for Zig)}
- {e.g., RULE ORP — cross-layer orphan sweep (if renaming/deleting symbols)}

If no specific rules apply beyond the universal set (XCC, FLL, ORP): write
"Standard set only — no domain-specific rules."

---

## Sections (implementation slices)

Add as many `## §N — {Title}` sections as needed. Each section is an
implementation slice — what will be built. Max 4 dimensions per section.

### §1 — {Section Title}

**Status:** PENDING

Description of this section. Explain what will be built and why.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `{file}:{fn_or_struct}` | `{structured input}` | `{exact output or state change}` | unit / integration / contract |
| 1.2 | PENDING | `{file}:{fn_or_struct}` | `{structured input}` | `{exact output}` | unit / integration / contract |

### §2 — {Next Section}

**Status:** PENDING

{Same pattern as §1 — sections are implementation slices, dimensions are test blueprints.}

---

## Interfaces

**Status:** PENDING

Lock the API surface. Every public function, endpoint, and data shape that this workstream introduces or modifies.

### Public Functions

```
{language}
{exact function signature — not pseudocode}
```

### Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| {name} | {type} | {validation rules} | {example value} |

### Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| {name} | {type} | {condition} | {example value} |

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Timeout | {what happens} | {return value or error code} |
| Connection lost | {what happens} | {return value or error code} |
| Malformed input | {what happens} | {return value or error code} |
| Auth failure | {what happens} | {return value or error code} |

---

## Failure Modes

**Status:** PENDING

Enumerate every failure path. For each: what triggers it, what the system does, and what the user/caller observes.

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| {name} | {condition} | {action taken} | {output/error} |

**Platform constraints:**
- {e.g., "SO_RCVTIMEO does not propagate through TLS record layer on Linux — timeout fires ReadFailed, not WouldBlock"}
- {e.g., "client.open() does not exist on x86_64-linux in Zig 0.15.2 — use client.request()"}

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

Each constraint must be measurable — not "fast" or "efficient" but a number or a verification command.

| Constraint | How to verify |
|-----------|---------------|
| {e.g., "Zero heap allocations in hot path"} | {e.g., "std.testing.allocator detects leaks; grep for alloc in loop body"} |
| {e.g., "File under 350 lines"} | {e.g., "wc -l < 350"} |
| {e.g., "Cross-compiles on x86_64-linux, aarch64-linux"} | {e.g., "zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux"} |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

Each invariant MUST be enforced by the compiler, a lint check, or a comptime
assertion — NOT by documentation or code review. If a human can violate it
silently, it is not an invariant.

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | {e.g., "Every Entry has a non-empty hint field"} | {e.g., "comptime loop asserts hint.len > 0"} |
| 2 | {e.g., "No duplicate error codes"} | {e.g., "comptime loop checks pair-wise equality"} |

If the spec has no compile-time guardrails: write "N/A — no invariants."

---

## Test Specification

**Status:** PENDING

Every dimension from the §N sections must map to a test case here. This section is the input to `/write-unit-test`.

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| {name} | {1.1} | {file:fn} | {input} | {output} |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| {name} | {2.1} | {DB / Redis / both} | {input} | {output} |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| {e.g., "lookup returns UNKNOWN for empty string"} | {N.N} | `""` | `UNKNOWN` entry returned |
| {e.g., "reject malformed UUID"} | {N.N} | `"not-a-uuid"` | `ERR_UUIDV7_CANONICAL_FORMAT` |

### Edge Case Tests (boundary values)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| {e.g., "max-length code string"} | {N.N} | 256-char string | `UNKNOWN` (not crash) |
| {e.g., "zero-count query"} | {N.N} | workspace with 0 profiles | returns 0 (not error) |

### Regression Tests (pre-existing behavior that MUST NOT change)

| Test name | What it guards | File |
|-----------|---------------|------|
| {e.g., "UZ-AUTH-002 stays 401"} | Auth error = 401, not 403 | `error_registry_test.zig` |

If no pre-existing behavior is at risk: write "N/A — greenfield."

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| {name} | {N.N} | {std.testing.allocator detects zero leaks for this operation} |

Use `std.testing.allocator` (not an arena) so the built-in leak detector fires
on missed frees. Any test that constructs or destroys owned resources must
appear here.

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| {e.g., "streams in real time"} | {e.g., "bytes arrive before connection closes"} | integration |

---

## Execution Plan (Ordered)

**Status:** PENDING

Ordered steps. Agent executes top-to-bottom. Each step has a verification
command. **The codebase MUST build AND pass tests after every step.** If a
step leaves the code in a broken state, fix it before proceeding — do not
carry forward compile errors.

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | {Define interfaces} | `zig build` compiles |
| 2 | {Implement core logic} | `zig build && zig build test` |
| 3 | {Add failure handling} | `zig build test` |
| 4 | {Write tests via /write-unit-test} | `zig build test` (all pass) |
| 5 | {Delete old files + orphan sweep} | `zig build && grep -rn {old_sym} src/` returns 0 |
| 6 | {Cross-compile + lint + gitleaks} | `zig build -Dtarget=x86_64-linux && make lint && gitleaks detect` |

---

## Acceptance Criteria

**Status:** PENDING

Each criterion must be verifiable by running a command or inspecting output — not "works correctly".

- [ ] {Criterion} — verify: `{command}`
- [ ] {Criterion} — verify: `{command}`
- [ ] {Criterion} — verify: `{command}`

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

Executable script. Run every command after implementation. ALL must pass
before opening the PR. Copy-paste this block into the terminal.

```bash
# E1: {description}
{command} && echo "PASS" || echo "FAIL"

# E2: Dead code sweep — zero orphaned references to deleted/renamed symbols
grep -rn "{old_symbol}" src/ --include="*.zig" | head -5
echo "E2: orphan sweep (empty = pass)"

# E3: Memory leak test (std.testing.allocator detects leaks)
zig build test 2>&1 | grep -i "leak" | head -5
echo "E3: leak check (empty = pass)"

# E4: Build
zig build 2>&1 | head -5; echo "build=$?"

# E5: Tests
zig build test 2>&1 | tail -5; echo "test=$?"

# E6: Lint
make lint 2>&1 | grep -E "✓|FAIL"

# E7: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E8: Gitleaks — no secrets in diff
gitleaks detect 2>&1 | tail -3; echo "gitleaks=$?"

# E9: 350-line gate (exempts .md files — RULE FLL)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E10: Domain-specific lints (uncomment applicable ones)
# make check-pg-drain    # if touching pg query code
# make check-openapi-errors  # if touching error codes or HTTP endpoints
```

---

## Dead Code Sweep

**Status:** PENDING

Mandatory when the spec deletes or replaces files. Two checks:

**1. Orphaned files — must be deleted from disk and git.**
Every replaced or superseded file must be `git rm`'d. Verify with `test ! -f`.

| File to delete | Verify deleted |
|---------------|----------------|
| {e.g., `src/errors/error_table.zig`} | `test ! -f src/errors/error_table.zig` |
| {e.g., `src/errors/codes.zig`} | `test ! -f src/errors/codes.zig` |

**2. Orphaned references — zero remaining imports or uses.**
For every deleted file and every removed/renamed public symbol, grep the
entire `src/` tree. Any non-zero result = stale reference that will compile
(Zig won't catch it if behind a `comptime` or test-only path) but fail at
runtime or confuse future maintainers.

| Deleted symbol or import | Grep command | Expected |
|-------------------------|--------------|----------|
| {e.g., `error_table`} | `grep -rn "error_table" src/ --include="*.zig"` | 0 matches |
| {e.g., `UNKNOWN_ENTRY`} | `grep -rn "UNKNOWN_ENTRY" src/ --include="*.zig"` | 0 matches |
| {e.g., `posthog_events`} | `grep -rn "posthog_events" src/ --include="*.zig"` | 0 matches |

**3. main.zig test discovery — update imports.**
Remove `_ = @import("deleted_file.zig");` lines. Add imports for new files.

If a spec does not delete files: write "N/A — no files deleted."

---

## Verification Evidence

**Status:** PENDING

Filled in during VERIFY phase. Proves the spec claims are met.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | {output} | |
| Integration tests | `make test-integration` | {output} | |
| Leak detection | `zig build test \| grep leak` | {output} | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | {output} | |
| Lint | `make lint` | {output} | |
| Gitleaks | `gitleaks detect` | {output} | |
| 350L gate | `wc -l` (exempts .md — RULE FLL) | {output} | |
| Dead code sweep | `grep -rn {symbol} src/` | {output} | |

---

## Out of Scope

- {Item not in scope}
- {Another out of scope item}
