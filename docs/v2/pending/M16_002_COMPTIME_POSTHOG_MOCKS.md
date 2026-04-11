---
Milestone: M16
Workstream: M16_002
Name: COMPTIME_POSTHOG_MOCKS
Status: PENDING
Priority: P2 — eliminates null-check boilerplate + enables test event assertions
Created: Apr 11, 2026
Depends on: none
---

# M16_002 — Comptime-Conditional PostHog & Telemetry Mocks

## Goal

Replace the `?*posthog.PostHogClient` nullable pointer pattern with a
comptime-conditional telemetry interface. In production builds, events fire
to PostHog. In test builds, events are recorded in a static buffer for
assertions — zero runtime cost, no null-check branches, and tests can now
verify that the correct event was emitted with the correct properties.

## Problem

Every telemetry function in `posthog_events.zig` (19 functions, 472 lines)
follows the same pattern:

```zig
pub fn trackRunStarted(
    client: ?*posthog.PostHogClient,  // nullable
    distinct_id: []const u8,
    // ... 5 more params
) void {
    if (client) |ph| {                // null-check on every call
        const props = [_]posthog.Property{ ... };
        ph.capture(.{ ... }) catch {};
    }
}
```

Issues:

1. **Boilerplate:** Every function starts with `if (client) |ph|`. The null
   branch is dead code in production (client is always non-null) and the
   live branch is dead code in tests (client is always null).

2. **No test assertions:** Tests pass `null` and verify only that the
   function doesn't crash. There's no way to assert "trackRunStarted was
   called with workspace_id=X" in a unit test. The test file
   (`posthog_events_test.zig`) is just a no-op smoke test.

3. **Parameter explosion:** 19 functions with 5-14 parameters each. No
   type safety on the event name — it's an inline string literal.

4. **Error swallowing:** `catch {}` on every capture() call hides PostHog
   connection failures silently.

5. **Threading through context.** `?*posthog.PostHogClient` is threaded through:
   - `common.zig:SharedCtx.posthog` field
   - `hx.zig` handler context
   - `serve.zig` startup
   - `worker.zig` startup
   - `reconcile/daemon.zig` → `tick.zig`
   - `writeAuthErrorWithTracking()` in `common.zig`
   Total: ~49 PostHog-related references across 9 production files.

## Invariants (hard guardrails — violation = build failure)

1. **Comptime backend selection.** The `Telemetry` type contains a `Backend`
   field whose type is `if (builtin.is_test) TestBackend else ProdBackend`.
   No runtime branching. No optional pointer.
2. **No `?*posthog.PostHogClient` in any public function signature.** After migration,
   `grep -rn '?.*PostHogClient' src/ --include='*.zig'` returns 0.
3. **Every event is a typed struct.** Event names are `pub const name = "..."` on
   the struct — a typo in the name is a compile error (field doesn't exist).
   No inline string literals for event names at call sites.
4. **TestBackend records events for assertion.** Tests MUST be able to call
   `Telemetry.TestBackend.assertLastEventIs("run_started")` and verify properties.
5. **ProdBackend uses std.testing.allocator in tests.** Any heap allocation in
   event property construction MUST be tested with `std.testing.allocator` to
   detect leaks. The allocator's built-in leak detector fires on missed frees.
6. **No orphaned files.** After full migration, `posthog_events.zig` and
   `posthog_events_test.zig` are deleted. Zero imports remain.
7. **Context struct migration.** `common.zig:SharedCtx.posthog` field type
   changes from `?*posthog.PostHogClient` to `*Telemetry`. Every file that
   accesses `ctx.posthog` or `hx.ctx.posthog` is updated.

## Applicable Rules

- RULE NDC — no dead code (deleting posthog_events.zig)
- RULE ORP — cross-layer orphan sweep after deletion
- RULE XCC — cross-compile before commit
- RULE FLL — 350-line gate on touched files
- RULE TST — test discovery requires explicit import in main.zig

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/observability/telemetry.zig` | CREATE | Comptime-conditional backend + typed event structs |
| `src/observability/telemetry_events.zig` | CREATE (if needed) | Split if telemetry.zig > 350L |
| `src/observability/telemetry_test.zig` | CREATE | TestBackend assertion tests |
| `src/observability/posthog_events.zig` | DELETE | Replaced by telemetry.zig |
| `src/observability/posthog_events_test.zig` | DELETE | Replaced by telemetry_test.zig |
| `src/http/handlers/common.zig` | MODIFY | SharedCtx.posthog → telemetry |
| `src/http/hx.zig` | MODIFY | If it stores posthog ref |
| `src/cmd/serve.zig` | MODIFY | Telemetry.initProd() at startup |
| `src/cmd/worker.zig` | MODIFY | Same |
| `src/cmd/reconcile/daemon.zig` | MODIFY | posthog_client → *Telemetry |
| `src/cmd/reconcile/tick.zig` | MODIFY | posthog_client → *Telemetry |
| `src/cmd/preflight.zig` | MODIFY | client field → *Telemetry |
| `src/http/handlers/auth_sessions_http.zig` | MODIFY | Migrate track*() calls |
| `src/http/handlers/github_callback.zig` | MODIFY | Same |
| `src/http/handlers/harness_http.zig` | MODIFY | Same |
| `src/http/handlers/workspaces_billing.zig` | MODIFY | Same |
| `src/http/handlers/workspaces_lifecycle.zig` | MODIFY | Same |
| `src/http/handlers/workspaces.zig` | MODIFY | Same |
| `src/main.zig` | MODIFY | Update test discovery imports |

## Design

### `src/observability/telemetry.zig`

```zig
const std = @import("std");
const builtin = @import("builtin");
const posthog = @import("posthog");

// ── Event types ──────────────────────────────────────────────────────

pub const EventKind = enum {
    run_started,
    run_retried,
    run_completed,
    run_failed,
    agent_completed,
    entitlement_rejected,
    profile_activated,
    billing_lifecycle_event,
    server_started,
    worker_started,
    startup_failed,
    workspace_created,
    workspace_github_connected,
    auth_login_completed,
    auth_rejected,
    run_orphan_recovered,
    run_orphan_no_agent_profile,
    api_error,
    api_error_with_context,
};

pub const RecordedEvent = struct {
    kind: EventKind,
    distinct_id: []const u8,
    workspace_id: []const u8,
};

// ── Event structs (one per event) ────────────────────────────────────

pub const RunStarted = struct {
    distinct_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
    spec_id: []const u8,
    mode: []const u8,
    request_id: []const u8,

    pub const kind: EventKind = .run_started;
    pub const name = "run_started";

    pub fn properties(self: @This()) [5]posthog.Property {
        return .{
            .{ .key = "run_id", .value = .{ .string = self.run_id } },
            .{ .key = "workspace_id", .value = .{ .string = self.workspace_id } },
            .{ .key = "spec_id", .value = .{ .string = self.spec_id } },
            .{ .key = "mode", .value = .{ .string = self.mode } },
            .{ .key = "request_id", .value = .{ .string = self.request_id } },
        };
    }
};

// ... 18 more event structs, same pattern

// ── Backends ─────────────────────────────────────────────────────────

pub const ProdBackend = struct {
    client: *posthog.PostHogClient,

    pub fn capture(self: *ProdBackend, comptime kind: EventKind, event: anytype) void {
        const props = event.properties();
        self.client.capture(.{
            .distinct_id = event.distinct_id,
            .event = @tagName(kind),
            .properties = &props,
        }) catch {};
    }
};

pub const TestBackend = struct {
    var ring: [64]?RecordedEvent = [_]?RecordedEvent{null} ** 64;
    var count: usize = 0;

    pub fn capture(_: *TestBackend, comptime kind: EventKind, event: anytype) void {
        ring[count % 64] = .{
            .kind = kind,
            .distinct_id = event.distinct_id,
            .workspace_id = if (@hasField(@TypeOf(event), "workspace_id")) event.workspace_id else "",
        };
        count += 1;
    }

    pub fn reset() void {
        ring = [_]?RecordedEvent{null} ** 64;
        count = 0;
    }

    pub fn lastEvent() ?RecordedEvent {
        if (count == 0) return null;
        return ring[(count - 1) % 64];
    }

    pub fn assertLastEventIs(expected: EventKind) !void {
        const last = lastEvent() orelse return error.NoEventsRecorded;
        try std.testing.expectEqual(expected, last.kind);
    }

    pub fn assertCount(expected: usize) !void {
        try std.testing.expectEqual(expected, count);
    }
};

// ── Telemetry (comptime-selected) ────────────────────────────────────

pub const Backend = if (builtin.is_test) TestBackend else ProdBackend;

pub const Telemetry = struct {
    backend: Backend,

    pub fn capture(self: *Telemetry, comptime kind: EventKind, event: anytype) void {
        self.backend.capture(kind, event);
    }

    /// Production init — wraps a real PostHog client.
    pub fn initProd(client: *posthog.PostHogClient) Telemetry {
        return .{ .backend = .{ .client = client } };
    }

    /// Test init — uses TestBackend (no PostHog dependency).
    pub fn initTest() Telemetry {
        TestBackend.reset();
        return .{ .backend = .{} };
    }
};
```

### Call site migration (before → after)

**Before:**
```zig
posthog_events.trackRunStarted(hx.ctx.posthog, user_id, run_id, ws_id, spec_id, mode, req_id);
```

**After:**
```zig
hx.ctx.telemetry.capture(.run_started, .{
    .distinct_id = user_id,
    .run_id = run_id,
    .workspace_id = ws_id,
    .spec_id = spec_id,
    .mode = mode,
    .request_id = req_id,
});
```

### What this eliminates

| Before | After |
|--------|-------|
| 19 functions × `if (client) \|ph\|` null check | Zero branches — comptime selects backend |
| `?*posthog.PostHogClient` threaded through 9 files | `*Telemetry` with comptime backend |
| Tests: pass null, assert nothing crashes | Tests: `TestBackend.assertLastEventIs(.run_started)` |
| Inline string event names (`"run_started"`) | Typed: `EventKind.run_started` — typo = compile error |
| 5-14 positional parameters per function | Named struct fields — self-documenting |
| `posthog_events.zig` (472 lines) | `telemetry.zig` (~350 lines) — typed + comptime |
| `posthog_events_test.zig` (no-op smoke test) | `telemetry_test.zig` — real property assertions |

## Sections & Dimensions

### §1.0 — Create telemetry.zig

| Dim | Check | Status |
|-----|-------|--------|
| 1.1 | `EventKind` enum has all 19 event names | |
| 1.2 | 19 typed event structs, each with `kind`, `name`, `properties()` | |
| 1.3 | `ProdBackend.capture()` forwards to PostHog SDK | |
| 1.4 | `TestBackend.capture()` records to ring buffer | |
| 1.5 | `TestBackend.reset()`, `lastEvent()`, `assertLastEventIs()`, `assertCount()` | |
| 1.6 | `Backend` is comptime-selected: `if (builtin.is_test) TestBackend else ProdBackend` | |
| 1.7 | `Telemetry` struct wraps Backend, exposes `capture()`, `initProd()`, `initTest()` | |
| 1.8 | File ≤ 400 lines — split event structs to `telemetry_events.zig` if needed | |

### §2.0 — Create telemetry_test.zig

| Dim | Check | Status |
|-----|-------|--------|
| 2.1 | Test: capture RunStarted → assertLastEventIs(.run_started) passes | |
| 2.2 | Test: capture 3 events → assertCount(3) passes | |
| 2.3 | Test: reset() clears ring → lastEvent() returns null | |
| 2.4 | Test: workspace_id captured correctly for events that have it | |
| 2.5 | Test: events without workspace_id field capture empty string | |
| 2.6 | Memory leak test: all event construction uses `std.testing.allocator` where applicable; zero leaks | |

### §3.0 — Migrate context structs

| Dim | Check | Status |
|-----|-------|--------|
| 3.1 | `common.zig:SharedCtx.posthog` → `telemetry: *Telemetry` | |
| 3.2 | `serve.zig` startup: create `Telemetry.initProd(ph.client)` | |
| 3.3 | `worker.zig` startup: same | |
| 3.4 | `reconcile/daemon.zig`: `posthog_client` param → `*Telemetry` | |
| 3.5 | `reconcile/tick.zig`: `posthog_client` param → `*Telemetry` | |
| 3.6 | `preflight.zig`: `client` field → `*Telemetry` | |
| 3.7 | `hx.zig`: if it stores posthog ref, update to telemetry | |

### §4.0 — Migrate callers (9 files, ~49 call sites)

| Dim | Check | Status |
|-----|-------|--------|
| 4.1 | `serve.zig` — trackServerStarted, trackStartupFailed | |
| 4.2 | `worker.zig` — trackWorkerStarted | |
| 4.3 | `auth_sessions_http.zig` — trackAuthLoginCompleted, trackAuthRejected | |
| 4.4 | `common.zig` — writeAuthErrorWithTracking, trackApiError* | |
| 4.5 | `github_callback.zig` — trackWorkspaceGithubConnected | |
| 4.6 | `harness_http.zig` — trackProfileActivated, trackEntitlementRejected | |
| 4.7 | `workspaces_billing.zig` — trackBillingLifecycleEvent | |
| 4.8 | `workspaces_lifecycle.zig` — trackWorkspaceCreated | |
| 4.9 | `workspaces.zig` — if any PostHog calls remain | |

### §5.0 — Delete dead code + orphan sweep

| Dim | Check | Status |
|-----|-------|--------|
| 5.1 | `posthog_events.zig` deleted — file does not exist | |
| 5.2 | `posthog_events_test.zig` deleted — file does not exist | |
| 5.3 | Zero imports of `posthog_events` remain in src/ | |
| 5.4 | Zero references to `?*posthog.PostHogClient` remain in src/ | |
| 5.5 | Zero references to `distinctIdOrSystem` remain (was in posthog_events.zig) | |
| 5.6 | `main.zig` test discovery: old imports removed, new file added | |
| 5.7 | No orphaned `serverStartedProps`, `startupFailedProps`, etc. (helper fns from old file) | |

## Eval Commands (post-implementation verification)

Run every command below. All must pass before opening the PR.

```bash
# E1: posthog_events.zig deleted
test ! -f src/observability/posthog_events.zig && echo "PASS" || echo "FAIL: posthog_events.zig still exists"

# E2: posthog_events_test.zig deleted
test ! -f src/observability/posthog_events_test.zig && echo "PASS" || echo "FAIL: posthog_events_test.zig still exists"

# E3: Zero imports of old module
count=$(grep -rn "posthog_events" src/ --include="*.zig" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS: zero posthog_events refs" || echo "FAIL: $count stale refs"

# E4: No ?*PostHogClient in function signatures or struct fields
count=$(grep -rn '?\*.*PostHogClient' src/ --include="*.zig" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS: zero ?*PostHogClient" || echo "FAIL: $count remaining"

# E5: No "if (client)" null-check pattern
count=$(grep -rn 'if (client)' src/observability/ --include="*.zig" | wc -l | tr -d ' ')
[ "$count" -eq 0 ] && echo "PASS: zero null-checks" || echo "FAIL: $count remaining"

# E6: No orphaned helper functions from old file
for fn in distinctIdOrSystem serverStartedProps startupFailedProps agentRunScoredProps trustTransitionProps; do
  count=$(grep -rn "$fn" src/ --include="*.zig" | wc -l | tr -d ' ')
  [ "$count" -eq 0 ] || echo "FAIL: orphan $fn ($count refs)"
done
echo "E6: orphan check done"

# E7: telemetry.zig exists and is under 400 lines
wc -l src/observability/telemetry.zig
test $(wc -l < src/observability/telemetry.zig) -le 400 && echo "PASS: under 400L" || echo "FAIL: over 400L"

# E8: Build succeeds
zig build 2>&1 | head -5; echo "E8: build exit=$?"

# E9: Tests pass (includes leak detection via std.testing.allocator)
zig build test 2>&1 | tail -10; echo "E9: test exit=$?"

# E10: Lint passes
make lint 2>&1 | grep -E "✓|FAIL"

# E11: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86:$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm:$?"

# E12: Memory leak check — grep test output for leak reports
zig build test 2>&1 | grep -i "leak" | head -5
echo "E12: leak check (empty = pass)"

# E13: TestBackend assertion coverage — verify test file exercises assertions
grep -c "assertLastEventIs\|assertCount\|lastEvent" src/observability/telemetry_test.zig
echo "E13: assertion call count (should be >= 5)"

# E14: 400-line gate
git diff --name-only origin/main | xargs wc -l 2>/dev/null | awk '$1 > 400 && $2 !~ /\.md$/ { print "OVER: " $2 ": " $1 " lines" }'
```

## Dead Code Sweep

**1. Orphaned files — must be deleted from disk and git.**

| File to delete | Verify deleted |
|---------------|----------------|
| `src/observability/posthog_events.zig` | `test ! -f src/observability/posthog_events.zig` |
| `src/observability/posthog_events_test.zig` | `test ! -f src/observability/posthog_events_test.zig` |

**2. Orphaned references — zero remaining imports or uses.**

| Deleted symbol or import | Grep command | Expected |
|-------------------------|--------------|----------|
| `posthog_events` | `grep -rn "posthog_events" src/ --include="*.zig"` | 0 matches |
| `?*posthog.PostHogClient` | `grep -rn '?\*.*PostHogClient' src/ --include="*.zig"` | 0 matches |
| `distinctIdOrSystem` | `grep -rn "distinctIdOrSystem" src/ --include="*.zig"` | 0 matches |
| `serverStartedProps` | `grep -rn "serverStartedProps" src/ --include="*.zig"` | 0 matches |
| `startupFailedProps` | `grep -rn "startupFailedProps" src/ --include="*.zig"` | 0 matches |

**3. main.zig test discovery — update imports.**
Remove posthog_events imports. Add telemetry.zig + telemetry_test.zig imports.

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Leak detection | `zig build test \| grep leak` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux` | | |
| Lint | `make lint` | | |
| Gitleaks | `gitleaks detect` | | |
| 350L gate | `wc -l` (exempts .md) | | |
| Dead code sweep | eval E1–E6 | | |

## Out of Scope

- Changing which events are emitted (same 19 events)
- Adding new events (that's future work)
- PostHog SDK changes (same library, wrapped differently)
- Metrics (Prometheus counters stay as-is — different concern)
- Reconciler PostHog integration (currently unused: `_ = posthog_client` in tick.zig)

## Acceptance Criteria

- [ ] `builtin.is_test` selects TestBackend at compile time — zero production branches
- [ ] No `?*posthog.PostHogClient` parameters remain in any function signature
- [ ] No `posthog_events.zig` file exists
- [ ] No `posthog_events_test.zig` file exists
- [ ] Tests can assert: event kind, distinct_id, workspace_id of last captured event
- [ ] Event names are `EventKind` enum variants (typo = compile error)
- [ ] All 14 eval commands pass
- [ ] `make test` passes (includes leak detection via `std.testing.allocator`)
- [ ] `make lint` passes
- [ ] Cross-compiles for x86_64-linux and aarch64-linux
- [ ] Zero orphaned references to deleted files or functions (E3, E6)
- [ ] telemetry_test.zig has ≥ 5 assertion calls (E13)
