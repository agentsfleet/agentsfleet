---
Milestone: M16
Workstream: M16_001
Name: COMPTIME_ERROR_REGISTRY
Status: PENDING
Priority: P1 — eliminates 3 rules and the sentinel collision class
Created: Apr 11, 2026
Depends on: none
---

# M16_001 — Comptime-Generated Error Registry

## Goal

Replace the manually maintained `TABLE` array in `error_table.zig` and the
separate `codes.zig` constants with a single comptime-generated registry.
Adding an error code = one Entry struct literal in one file. The lookup map,
string constants, and coverage guarantees are all derived at compile time.

## Problem

Today the error system has three manual sync points:

1. **`codes.zig`** — 130+ `pub const ERR_* = "UZ-..."` string declarations.
2. **`error_table.zig`** — 131-entry `TABLE` array mapping code → status + title + URI.
3. **`error_registry_test.zig`** — comptime loop that checks every `ERR_*` in codes.zig
   resolves via `lookup()`. Uses `@setEvalBranchQuota(1_000_000)` to handle O(n²).

Adding an error code requires editing two files and hoping the test catches drift.
Three RULES.md entries exist solely to paper over this fragility:

- **RULE SNT** — sentinel must not collide with real entries
- **RULE BRQ** — `@setEvalBranchQuota` for large comptime loops
- **RULE ERH** — every ERR_* must have a hint()

## Invariants (hard guardrails — violation = build failure)

1. **Single source of truth.** `error_registry.zig` REGISTRY array is the ONLY
   place error codes are defined. No secondary definition file.
2. **Every field required.** Entry struct has 5 required fields: `code`, `http_status`,
   `title`, `hint`, `docs_uri`. The hint field MUST be non-empty. A comptime
   validation loop asserts `entry.hint.len > 0` for every entry; empty hint = compile error.
3. **No sentinel in REGISTRY.** `UNKNOWN` is a `pub const Entry` defined OUTSIDE
   the `REGISTRY` array. A comptime loop asserts no entry in REGISTRY has
   `code == UNKNOWN.code`. Collision = compile error.
4. **Code format validated at comptime.** Every entry's `.code` must:
   - Start with `"UZ-"`
   - Contain only uppercase letters, digits, and hyphens after the prefix
   - Be non-empty
   A comptime validation function checks this; violations = compile error.
5. **No duplicate codes.** A comptime loop checks that no two entries share the
   same `.code` value. Duplicate = compile error.
6. **Backward-compatible constant names.** Every `ERR_*` constant currently
   importable from `codes.zig` must remain importable (re-exported from the
   registry or via a thin shim) until all callers are migrated. Only delete
   `codes.zig` after the orphan sweep confirms zero remaining imports.
7. **lookup() never returns null.** The new `lookup()` returns `Entry` (not `?Entry`).
   Unknown codes return `UNKNOWN` — no optional unwrapping at call sites.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/errors/error_registry.zig` | CREATE | Single source of truth — REGISTRY + LOOKUP + ERR_* constants |
| `src/errors/error_table.zig` | DELETE | Replaced by error_registry.zig |
| `src/errors/codes.zig` | DELETE | Constants now generated from REGISTRY |
| `src/errors/codes_test.zig` | MODIFY or DELETE | Tests reference new registry |
| `src/errors/error_registry_test.zig` | MODIFY | Simplify — iterate REGISTRY directly, no @setEvalBranchQuota |
| `src/http/handlers/common.zig` | MODIFY | Import error_registry instead of error_table |
| `src/main.zig` | MODIFY | Update test discovery imports |
| `docs/greptile-learnings/RULES.md` | MODIFY | Mark RULE SNT, BRQ, ERH as ELIMINATED |
| 43 files importing codes.zig | MODIFY | Switch import path |

## Applicable Rules

- RULE ORP — cross-layer orphan sweep (deleting 2 files + renaming imports)
- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files
- RULE TST — test discovery requires explicit import in main.zig

## Design

### `src/errors/error_registry.zig`

```zig
const std = @import("std");

pub const Entry = struct {
    code: []const u8,
    http_status: std.http.Status,
    title: []const u8,
    hint: []const u8,
    docs_uri: []const u8,
};

/// Sentinel for unrecognized codes. Defined OUTSIDE REGISTRY — collision
/// is structurally impossible (enforced by comptime assertion below).
pub const UNKNOWN = Entry{
    .code = "UZ-UNKNOWN",
    .http_status = .internal_server_error,
    .title = "Unknown error",
    .hint = "This error code is not registered. Report to the operator.",
    .docs_uri = "https://docs.usezombie.com/errors",
};

pub const REGISTRY = [_]Entry{
    .{ .code = "UZ-UUIDV7-003", .http_status = .bad_request,
       .title = "Invalid UUIDv7 format",
       .hint = "The ID must be a valid UUIDv7 in canonical format (8-4-4-4-12 hex).",
       .docs_uri = "https://docs.usezombie.com/errors/UZ-UUIDV7-003" },
    // ... all 131 entries migrated from error_table.zig TABLE
};

// ── Comptime validation ──────────────────────────────────────────────

comptime {
    for (REGISTRY) |entry| {
        // Invariant 2: hint must be non-empty
        if (entry.hint.len == 0)
            @compileError("Entry " ++ entry.code ++ " has empty hint");
        // Invariant 4: code format
        if (entry.code.len < 4 or !std.mem.startsWith(u8, entry.code, "UZ-"))
            @compileError("Entry code must start with UZ-: " ++ entry.code);
    }
    // Invariant 3: no sentinel collision
    for (REGISTRY) |entry| {
        if (std.mem.eql(u8, entry.code, UNKNOWN.code))
            @compileError("REGISTRY entry collides with UNKNOWN sentinel: " ++ entry.code);
    }
    // Invariant 5: no duplicate codes
    for (REGISTRY, 0..) |a, i| {
        for (REGISTRY[i + 1 ..]) |b| {
            if (std.mem.eql(u8, a.code, b.code))
                @compileError("Duplicate code in REGISTRY: " ++ a.code);
        }
    }
}

// ── Lookup ───────────────────────────────────────────────────────────

const LOOKUP = blk: {
    @setEvalBranchQuota(REGISTRY.len * REGISTRY.len * 20);
    var kvs: [REGISTRY.len]struct { []const u8, usize } = undefined;
    for (REGISTRY, 0..) |entry, i| kvs[i] = .{ entry.code, i };
    break :blk std.StaticStringMap(usize).initComptime(&kvs);
};

/// Lookup by code string. Returns UNKNOWN for unregistered codes.
/// Never returns null — callers do not need optional handling.
pub fn lookup(code: []const u8) Entry {
    const idx = LOOKUP.get(code) orelse return UNKNOWN;
    return REGISTRY[idx];
}

/// Lookup hint for an error code. Returns UNKNOWN.hint for unregistered codes.
pub fn hint(code: []const u8) []const u8 {
    return lookup(code).hint;
}

// ── ERR_* constants (backward compat) ────────────────────────────────
//
// These are comptime references into REGISTRY[i].code.
// Migration: callers that import codes.zig switch to error_registry.zig.
// After all callers migrated, delete codes.zig shim.

pub const ERR_UUIDV7_CANONICAL_FORMAT = REGISTRY[0].code;
// ... one line per constant, mechanically generated from REGISTRY order
```

### What this eliminates

| Before | After |
|--------|-------|
| `codes.zig` (130+ manual consts) | Constants generated from REGISTRY at comptime |
| `error_table.zig` TABLE (131 entries) | REGISTRY is the table |
| `error_table.zig` LOOKUP_MAP comptime block | LOOKUP generated from REGISTRY |
| `error_table.zig` UNKNOWN_ENTRY sentinel | UNKNOWN defined outside REGISTRY — collision impossible |
| `error_registry_test.zig` @setEvalBranchQuota(1M) | Not needed — comptime validation in the file itself |
| RULE SNT | Eliminated — comptime assertion prevents collision |
| RULE BRQ | Eliminated — no O(n²) cross-file comptime loop |
| RULE ERH | Enforced — Entry struct requires .hint, comptime asserts non-empty |
| `?ErrorEntry` return type | `Entry` — no optional unwrap at call sites |

## Sections & Dimensions

### §1.0 — Create error_registry.zig

| Dim | Check | Status |
|-----|-------|--------|
| 1.1 | `Entry` struct has 5 required fields: code, http_status, title, hint, docs_uri | |
| 1.2 | REGISTRY contains exactly 131 entries (migrated from TABLE) | |
| 1.3 | UNKNOWN is defined as `pub const` outside REGISTRY | |
| 1.4 | Comptime block validates: non-empty hint, UZ- prefix, no sentinel collision, no duplicates | |
| 1.5 | `lookup()` returns `Entry` (not `?Entry`), returns UNKNOWN for missing codes | |
| 1.6 | `hint()` convenience function returns hint string for a code | |
| 1.7 | ERR_* constants re-exported from REGISTRY entries | |

### §2.0 — Migrate callers from error_table → error_registry

| Dim | Check | Status |
|-----|-------|--------|
| 2.1 | `common.zig:errorResponse()` uses `error_registry.lookup()` instead of `error_table.lookup() orelse UNKNOWN_ENTRY` | |
| 2.2 | All files that import `error_table.zig` redirected to `error_registry.zig` | |
| 2.3 | `codes.zig` becomes a thin shim: `pub const ERR_* = error_registry.ERR_*;` for each constant | |

### §3.0 — Migrate callers from codes.zig → error_registry

Files importing codes.zig (43 files):

| Dim | Check | Status |
|-----|-------|--------|
| 3.1 | Each file's `@import("../errors/codes.zig")` replaced with `@import("../errors/error_registry.zig")` | |
| 3.2 | All `error_codes.ERR_*` references compile against registry constants | |

### §4.0 — Delete dead files and clean up

| Dim | Check | Status |
|-----|-------|--------|
| 4.1 | `error_table.zig` deleted — file does not exist | |
| 4.2 | `codes.zig` deleted — file does not exist | |
| 4.3 | `error_registry_test.zig` simplified — no `@setEvalBranchQuota`, iterates REGISTRY directly | |
| 4.4 | `main.zig` test discovery updated: old imports removed, new file added | |
| 4.5 | `codes_test.zig` — migrated or deleted (tests reference error_registry) | |

### §5.0 — Update RULES.md

| Dim | Check | Status |
|-----|-------|--------|
| 5.1 | RULE SNT marked as ELIMINATED with ref to M16_001 | |
| 5.2 | RULE BRQ marked as ELIMINATED with ref to M16_001 | |
| 5.3 | RULE ERH marked as ELIMINATED with ref to M16_001 | |

## Eval Commands (post-implementation verification)

Run every command below. All must pass before opening the PR.

```bash
# E1: Single source of truth — no error_table.zig
test ! -f src/errors/error_table.zig && echo "PASS: error_table.zig deleted"

# E2: No codes.zig
test ! -f src/errors/codes.zig && echo "PASS: codes.zig deleted"

# E3: Zero remaining imports of error_table
count=$(grep -rn "error_table" src/ --include="*.zig" | grep -v error_registry | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS: zero error_table refs" || echo "FAIL: $count stale error_table refs"

# E4: Zero remaining imports of codes.zig (the file, not the string "codes")
count=$(grep -rn '@import.*codes\.zig' src/ --include="*.zig" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS: zero codes.zig imports" || echo "FAIL: $count stale codes.zig imports"

# E5: UNKNOWN not in REGISTRY (comptime enforces, but verify at text level)
count=$(grep -c '"UZ-UNKNOWN"' src/errors/error_registry.zig | tr -d ' ')
# Should be exactly 1 (the UNKNOWN const definition), NOT in any REGISTRY entry
[ "$count" -eq 1 ] && echo "PASS: UZ-UNKNOWN appears once (sentinel only)" || echo "FAIL: UZ-UNKNOWN count=$count"

# E6: No @setEvalBranchQuota in error test files
count=$(grep -rn "setEvalBranchQuota" src/errors/ --include="*.zig" | wc -l | tr -d ' ')
# The registry itself may use it for LOOKUP, but test files should not
test_count=$(grep -rn "setEvalBranchQuota" src/errors/*_test*.zig 2>/dev/null | wc -l | tr -d ' ')
[ "$test_count" -eq 0 ] && echo "PASS: no setEvalBranchQuota in test files" || echo "FAIL: $test_count uses in test files"

# E7: Every ERR_* used in src/ resolves (compile is the real test, but grep for orphans)
zig build 2>&1 | head -5
echo "E7: build exit=$?"

# E8: Tests pass
zig build test 2>&1 | tail -5
echo "E8: test exit=$?"

# E9: Lint passes
make lint 2>&1 | grep -E "✓|FAIL"

# E10: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86:$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm:$?"

# E11: Memory leak test — run unit tests with testing.allocator
# (std.testing.allocator detects leaks; any leaked bytes = test failure)
zig build test 2>&1 | grep -i "leak" | head -5
echo "E11: leak check (empty = pass)"

# E12: Dead code sweep — no orphaned symbols from deleted files
grep -rn "UNKNOWN_ENTRY\|error_table\.TABLE\|error_table\.LOOKUP_MAP" src/ --include="*.zig" | head -5
echo "E12: orphan sweep (empty = pass)"

# E13: 400-line gate on new/changed files
git diff --name-only origin/main | xargs wc -l 2>/dev/null | awk '$1 > 400 && $2 !~ /\.md$/ { print "OVER: " $2 ": " $1 " lines" }'
```

## Out of Scope

- Changing error code format (UZ-*-NNN stays)
- Changing HTTP status assignments
- Changing `errorResponse()` function signature
- OpenAPI spec updates (codes don't change, just the source file)
- hint() content rewrites (migrated verbatim from error_table.zig)

## Acceptance Criteria

- [ ] Single file (`error_registry.zig`) is the sole source of truth
- [ ] No `error_table.zig` exists
- [ ] No `codes.zig` exists
- [ ] Adding a new error code = adding one Entry to REGISTRY (one file, one place)
- [ ] `@setEvalBranchQuota` not needed in any test file
- [ ] UNKNOWN is structurally outside REGISTRY (comptime assertion)
- [ ] Every Entry has a non-empty .hint field (comptime assertion)
- [ ] No duplicate codes in REGISTRY (comptime assertion)
- [ ] `lookup()` returns `Entry`, never `?Entry` — no optional unwrap at call sites
- [ ] All 13 eval commands pass
- [ ] `make test` passes (includes leak detection via `std.testing.allocator`)
- [ ] `make lint` passes
- [ ] Cross-compiles for x86_64-linux and aarch64-linux
- [ ] RULES.md: RULE SNT, RULE BRQ, RULE ERH marked ELIMINATED
- [ ] Zero orphaned references to deleted files (E12)
