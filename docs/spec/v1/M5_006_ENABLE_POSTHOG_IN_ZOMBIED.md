# M5_006: Enable PostHog Tracking In `zombied`

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 006
**Date:** Mar 06, 2026 (updated Mar 07, 2026)
**Status:** IN_PROGRESS
**Priority:** P1 — operational analytics baseline
**Batch:** B2 — needs M5_001 and M5_002
**Depends on:** M5_001 (Build `posthog-zig` Analytics SDK for Zig), M5_002 (Operate Multi-Tenant Harness Control Plane)

---

## 1.0 Singular Function

**Status:** DONE

Implement one working telemetry function: `zombied` emits deterministic run/control-plane events to PostHog.

**Dimensions:**
- 1.1 DONE Initialize server-side PostHog client in `zombied` runtime
- 1.2 DONE Capture lifecycle events (`run_started`, `run_completed`, `run_failed`, `agent_completed`)
- 1.3 DONE Capture policy events (`entitlement_rejected`, `profile_activated`)
- 1.4 DONE Add failure-safe async flushing without blocking run execution

---

## 2.0 Verification Units

**Status:** IN_PROGRESS

**Dimensions:**
- 2.1 DONE Unit test: event envelope contains required IDs (`workspace_id`, `run_id`)
- 2.2 IN_PROGRESS Integration test: successful and failed runs emit expected events once
- 2.3 IN_PROGRESS Integration test: PostHog outage does not fail core run execution path

---

## 3.0 Acceptance Criteria

**Status:** IN_PROGRESS

- [ ] 3.1 Core `zombied` lifecycle events are visible in PostHog with stable schema
- [x] 3.2 Analytics path is non-blocking and outage-tolerant
- [ ] 3.3 Demo evidence captured for run lifecycle events in PostHog

---

## 4.0 Out of Scope

- Website analytics instrumentation (tracked in M5_005)
- zombiectl CLI analytics (tracked in M5_007)
- Advanced attribution modeling

---

## 5.0 Implementation

### 5.1 SDK dependency

Add `posthog-zig` from `https://github.com/usezombie/posthog-zig` using current `v0.1.3` tag.

`build.zig.zon`:
```zig
.posthog = .{
    .url = "git+https://github.com/usezombie/posthog-zig.git#v0.1.3",
    .hash = "posthog-0.1.3-QwvZljuyAQBuzL5fxzbzL7OE1I8J90mB7f7vYckyasfY",
},
```

`build.zig` (add alongside nullclaw, zap, pg):
```zig
const posthog_dep = b.dependency("posthog", .{ .target = target, .optimize = optimize });
const posthog_mod = posthog_dep.module("posthog");
// add to exe imports: .{ .name = "posthog", .module = posthog_mod }
```

### 5.2 Client initialization in `src/cmd/serve.zig`

Init the PostHog client alongside the db pool and Redis queue. The pattern mirrors how all other infrastructure clients are initialized in `serve.zig`.

```zig
const posthog = @import("posthog");

// In serve.run() — after env_vars and runtime config:
const posthog_api_key = std.process.getEnvVarOwned(alloc, "POSTHOG_API_KEY") catch null;
var ph_client: ?*posthog.PostHogClient = if (posthog_api_key) |key| blk: {
    break :blk posthog.init(alloc, .{
        .api_key = key,
        .host = "https://us.i.posthog.com",
        .flush_interval_ms = 10_000,
        .flush_at = 20,
        .max_retries = 3,
    }) catch null;
} else null;
defer if (ph_client) |c| c.deinit(); // drains remaining events before exit
```

Add to `http_handler.Context`:
```zig
pub const Context = struct {
    // ... existing fields ...
    posthog: ?*posthog.PostHogClient = null,
};
```

Set in serve.zig before starting the HTTP server:
```zig
ctx.posthog = ph_client;
```

`POSTHOG_API_KEY` not set → `ctx.posthog = null` → analytics silently skipped. Never blocks the run path.

### 5.3 Event capture in handlers

Pattern: capture after the successful state transition, never on the hot path before it. Always `catch {}` — analytics must never fail a run.

**`src/http/handlers/runs.zig` — `handleStartRun`:**
```zig
// After successful queue enqueue:
if (ctx.posthog) |ph| {
    ph.capture(.{
        .distinct_id = principal.user_id,
        .event = "run_started",
        .properties = &.{
            .{ .key = "workspace_id", .value = .{ .string = req.workspace_id } },
            .{ .key = "spec_id",      .value = .{ .string = req.spec_id } },
            .{ .key = "mode",         .value = .{ .string = req.mode } },
            .{ .key = "request_id",   .value = .{ .string = req_id } },
        },
    }) catch {};
}
```

**`src/http/handlers/runs.zig` — `handleRetryRun`:**
```zig
if (ctx.posthog) |ph| {
    ph.capture(.{
        .distinct_id = principal.user_id,
        .event = "run_retried",
        .properties = &.{
            .{ .key = "run_id",  .value = .{ .string = run_id } },
            .{ .key = "attempt", .value = .{ .integer = attempt } },
        },
    }) catch {};
}
```

**`src/pipeline/worker.zig` — pipeline completion:**
```zig
// After run completes (success or failure):
if (posthog_client) |ph| {
    ph.capture(.{
        .distinct_id = run.user_id,
        .event = if (success) "run_completed" else "run_failed",
        .properties = &.{
            .{ .key = "run_id",       .value = .{ .string = run.id } },
            .{ .key = "workspace_id", .value = .{ .string = run.workspace_id } },
            .{ .key = "verdict",      .value = .{ .string = verdict } },
            .{ .key = "duration_ms",  .value = .{ .integer = duration_ms } },
        },
    }) catch {};
}
```

**`src/pipeline/worker_stage_executor.zig` — NullClaw agent completion:**
```zig
if (posthog_client) |ph| {
    ph.capture(.{
        .distinct_id = run.user_id,
        .event = "agent_completed",
        .properties = &.{
            .{ .key = "actor",      .value = .{ .string = actor } },   // Echo|Scout|Warden
            .{ .key = "tokens",     .value = .{ .integer = tokens } },
            .{ .key = "duration_ms",.value = .{ .integer = duration_ms } },
            .{ .key = "exit_status",.value = .{ .string = exit_status } },
        },
    }) catch {};
}
```

**Error tracking — any unhandled error in zombied:**
```zig
if (ctx.posthog) |ph| {
    ph.captureException(.{
        .distinct_id = principal.user_id,
        .exception_type = @errorName(err),
        .exception_message = "internal server error",
        .handled = false,
        .level = .err,
        .properties = &.{
            .{ .key = "request_id",   .value = .{ .string = req_id } },
            .{ .key = "workspace_id", .value = .{ .string = req.workspace_id } },
        },
    }) catch {};
}
```

### 5.4 Identity contract

`distinct_id` must always be the Clerk user ID extracted from the authenticated principal. This is already available as `principal.user_id` from `common.authenticate()` in every handler.

For workspace-level group events (post-MVP):
```zig
ph.group(.{
    .distinct_id = principal.user_id,
    .group_type = "workspace",
    .group_key = req.workspace_id,
}) catch {};
```

### 5.5 Configuration

| Env var | Required | Default | Notes |
|---|---|---|---|
| `POSTHOG_API_KEY` | No | — | Set to `phc_...` project key. Not set = analytics disabled. |

No other PostHog env vars needed for v0.1. Host defaults to `https://us.i.posthog.com`.

### 5.6 Outage tolerance

`posthog-zig` is non-blocking and outage-safe by design:
- `capture()` returns in < 1μs regardless of PostHog availability
- Failed batches are retried up to 3 times with exponential backoff
- After max retries: events are dropped and logged — never surface an error to the caller
- `POSTHOG_API_KEY` not set: client is `null`, all capture calls are skipped with zero overhead

The analytics path can never block, crash, or slow down run execution.
