---
Milestone: M10
Workstream: M10_006
Name: BVISOR_ZIG_PATTERNS
Status: PENDING
Priority: P3 — progressive improvement, apply when touching files
Created: Apr 11, 2026
Depends on: M10_005 (PgQuery migration complete)
---

# M10_006 — Adopt bvisor Zig Patterns

## Goal

Adopt battle-tested Zig patterns from `~/Projects/oss/bvisor/src/` to improve type safety, memory safety, and dispatch clarity across the usezombie Zig codebase.

## Background

During the M10_005 PgQuery migration review, five patterns from bvisor were identified as transferable. These are not urgent but improve code quality when applied progressively during routine file touches.

Reference: `~/Projects/oss/bvisor/src/core/`

## Scope

### Pattern 1 — Comptime size assertions on structs

**Source:** `bvisor/src/core/types.zig:65-125`

Add `comptime { std.debug.assert(@sizeOf(T) == N); }` inside struct bodies that cross serialization or FFI boundaries. Catches silent layout changes at compile time.

| Target struct | File | Why |
|--------------|------|-----|
| `StateRow` | `state/workspace_billing/row.zig` | Serialized to/from pg rows |
| `CreditRow` | `state/workspace_credit_store.zig` | Serialized to/from pg rows |
| `PgQuery` | `db/pg_query.zig` | Wraps pg.Result pointer |
| `ZombieSession` | `zombie/event_loop.zig` | Multi-field owned struct |

### Pattern 2 — Conditional errdefer for shared ownership

**Source:** `bvisor/src/core/virtual/proc/Thread.zig:28`

```zig
const resource = param orelse try Resource.init(alloc);
errdefer if (param == null) resource.deinit(); // only free if WE created it
```

| Target | File | Notes |
|--------|------|-------|
| Pool connections in test helpers | `db/test_fixtures.zig` | Pool vs borrowed conn |
| Optional workspace lookup | `secrets/crypto_store.zig` | Tenant lookup with fallback |

### Pattern 3 — Future pooling with backpressure

**Source:** `bvisor/src/core/Supervisor.zig:98-106`

Fixed-size future array with select-first-done draining. Applicable when usezombie adds parallel DB operations or batch zombie event processing.

| Target | File | Notes |
|--------|------|-------|
| Batch reconcile | `state/outbox_reconciler.zig` | Currently sequential batches |
| Multi-zombie startup | `cmd/worker_zombie.zig` | Currently sequential claim |

### Pattern 4 — `inline else` union dispatch

**Source:** `bvisor/src/core/virtual/fs/File.zig:74-90`

```zig
switch (self.backend) {
    inline else => |*f| return f.read(buf),
}
```

Zero-cost polymorphism without vtables or anytype. Apply wherever tagged unions share a common interface.

| Target | File | Notes |
|--------|------|-------|
| TBD — audit for tagged union candidates | — | Scan for `anytype` params that could become union dispatch |

### Pattern 5 — Bidirectional error mapping with comptime validation

**Source:** `bvisor/src/core/linux_error.zig:190-239`

```zig
inline else => |e| {
    const name = @errorName(e);
    if (comptime !@hasField(TargetEnum, name))
        @compileError("Unhandled error: " ++ name);
    return @field(TargetEnum, name);
},
```

Exhaustive error mapping validated at compile time. Currently our UZ error code switches are manual and can miss new error variants silently.

| Target | File | Notes |
|--------|------|-------|
| `errorCode()` switches | `state/workspace_billing.zig`, `state/workspace_credit.zig` | Manual switch, no exhaustive check |
| HTTP status mapping | `errors/codes.zig` | Error code → HTTP status |

## Applicable Rules

- RULE OWN — one owner per resource (Pattern 2)
- RULE NDC — no dead code (Pattern 4 eliminates anytype indirection)
- RULE FLL — 350-line gate (patterns should simplify, not bloat)

## Eval Commands

```bash
# E1: No new anytype for query/dispatch params
grep -rn "anytype" src/ --include="*.zig" | grep -v "anyerror\|@TypeOf\|comptime\|test\|//\|log_fn" | wc -l

# E2: Build + test
zig build 2>&1 | tail -3; echo "build=$?"
zig build test 2>&1 | tail -3; echo "test=$?"

# E3: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"
```

## Out of Scope

- bvisor's fork/exec supervisor model (different architecture)
- Seccomp syscall interception
- Io.Future async model (usezombie uses blocking pg + Redis clients)
- Cross-process memory reads

## Acceptance Criteria

- [ ] At least 4 structs have comptime size assertions
- [ ] Conditional errdefer applied where ownership is optional
- [ ] At least one manual error switch replaced with comptime-validated mapping
- [ ] `make test` passes
- [ ] `make lint` passes
- [ ] Cross-compiles
