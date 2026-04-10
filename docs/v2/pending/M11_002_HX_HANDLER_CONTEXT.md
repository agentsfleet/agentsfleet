---
Milestone: M11
Workstream: M11_002
Name: HX_HANDLER_CONTEXT
Status: PENDING
Branch: feat/m11-api-error-standardization
Depends-on: M11_001
Created: Apr 10, 2026
---

# M11_002 — `hx.zig` Handler Context

## Goal

Eliminate the 9-line setup ritual duplicated in every HTTP handler by
introducing a request-scoped `Hx` struct and a comptime `authenticated()`
wrapper. New handlers have zero boilerplate — only logic. Existing handlers
are converted in one pass after M11_001 lands.

**Demo:**

```zig
// Before M11_002 — every handler starts with this:
pub fn handleCreateZombie(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);
    const principal = common.authenticate(alloc, req, ctx) catch {
        common.errorResponse(res, ec.ERR_UNAUTHORIZED, "...", req_id);
        return;
    };
    _ = principal;
    const conn = ctx.pool.acquire() catch { ... return; };
    defer ctx.pool.release(conn);
    // ... actual logic starts here
}

// After M11_002 — only logic:
fn innerCreateZombie(hx: hx_mod.Hx, req: *httpz.Request) void {
    const conn = hx.db() catch return hx.fail("UZ-DB-001", "database unavailable");
    defer hx.releaseDb(conn);
    // ... actual logic
    hx.ok(.created, .{ .zombie_id = id });
}
pub const handleCreateZombie = hx_mod.authenticated(innerCreateZombie);
```

---

## Depends on

M11_001 must ship first. `hx.fail()` calls M11_001's `errorResponse(res, code,
detail, req_id)` — the new signature without `std.http.Status`. Without M11_001,
`hx.fail` would call the old 5-arg signature and the conversion pass would need
two passes.

---

## Surface Area Checklist

- [ ] **OpenAPI spec update** — no: handler signatures are internal, API surface unchanged.
- [ ] **`zombiectl` CLI changes** — no.
- [ ] **User-facing doc changes** — no.
- [x] **Release notes** — patch bump: `0.8.1` → `0.8.2` (internal refactor, no API change).
- [ ] **Schema changes** — no.

---

## Section 1: `src/http/handlers/hx.zig`

### 1.1 — `Hx` struct definition

```zig
pub const Hx = struct {
    alloc:     std.mem.Allocator,
    principal: common.AuthPrincipal,
    req_id:    []const u8,
    ctx:       *common.Context,
    res:       *httpz.Response,

    /// Acquire a Postgres connection from the pool.
    /// Caller must call hx.releaseDb(conn) when done.
    pub fn db(self: Hx) !*pg.Conn {
        return self.ctx.pool.acquire();
    }

    /// Release a Postgres connection back to the pool.
    pub fn releaseDb(self: Hx, conn: *pg.Conn) void {
        self.ctx.pool.release(conn);
    }

    /// Redis client reference — no allocation.
    pub fn redis(self: Hx) *queue_redis.Client {
        return self.ctx.queue;
    }

    /// Write a successful JSON response.
    /// Equivalent to: common.writeJson(res, status, body)
    pub fn ok(self: Hx, status: std.http.Status, body: anytype) void {
        common.writeJson(self.res, status, body);
    }

    /// Write a problem+json error response.
    /// Code owns its HTTP status via error_table (M11_001).
    /// Equivalent to: common.errorResponse(res, code, detail, req_id)
    pub fn fail(self: Hx, code: []const u8, detail: []const u8) void {
        common.errorResponse(self.res, code, detail, self.req_id);
    }
};
```

Fields are by value (not pointer) — `Hx` is passed by value on the stack. The
allocator, principal, req_id, and pointers inside are all cheap to copy.

### 1.2 — `authenticated()` comptime wrapper

```zig
/// Returns an httpz-compatible handler fn that:
///   1. Sets up an arena allocator (freed on return).
///   2. Generates a request ID.
///   3. Calls common.authenticate — returns 401 on failure.
///   4. Builds Hx and calls inner(hx, req).
///
/// Zero runtime overhead — generates identical code to hand-written boilerplate.
/// comptime means a new concrete function is emitted per call site.
pub fn authenticated(
    comptime inner: fn (hx: Hx, req: *httpz.Request) void,
) fn (*common.Context, *httpz.Request, *httpz.Response) void {
    return struct {
        fn handle(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response) void {
            var arena = std.heap.ArenaAllocator.init(ctx.alloc);
            defer arena.deinit();
            const alloc = arena.allocator();
            const req_id = common.requestId(alloc);

            const principal = common.authenticate(alloc, req, ctx) catch {
                common.errorResponse(res, "UZ-UNAUTHORIZED", "Invalid or missing token", req_id);
                return;
            };

            inner(.{
                .alloc     = alloc,
                .principal = principal,
                .req_id    = req_id,
                .ctx       = ctx,
                .res       = res,
            }, req);
        }
    }.handle;
}
```

### 1.3 — Unit test: `authenticated` calls inner with correct Hx fields
Construct a mock `Context` with a known API key. Build a fake `httpz.Request`
with the key in `Authorization`. Call the wrapped handler. Assert inner was
called with a non-empty `req_id`, non-null `principal`, correct `alloc`.

### 1.4 — Unit test: `authenticated` returns 401 on missing token
Call the wrapped handler with no `Authorization` header. Assert response
status 401 and `Content-Type: application/problem+json`. Inner must NOT be
called.

---

## Section 2: Convert existing handlers

### 2.1 — Convert `zombie_api.zig`
`handleCreateZombie`, `handleListZombies`, `handleDeleteZombie` → each becomes
an `innerXxx` function + `pub const handleXxx = authenticated(innerXxx)`.
Body parsing helpers (`parseCreateBody`, `validateCreateFields`) keep their
signatures but receive `hx.alloc` and call `hx.fail()` instead of
`common.errorResponse`.

### 2.2 — Convert `zombie_activity_api.zig`
`handleListActivity`, `handleStoreCredential`, `handleListCredentials`.

### 2.3 — Convert `zombie_api.zig` approval + webhook handlers
`approval_http.zig`, `webhooks.zig`. If a handler has no auth requirement
(e.g. inbound webhooks use HMAC not Bearer), it does NOT use `authenticated()`
— keep it as a raw handler. Document this exception in a comment.

### 2.4 — Do NOT convert streaming handlers
`zombie_stream_api.zig` (M10_002), `agent_relay.zig`, `runs/stream.zig` block
in a loop calling `res.chunk()`. These stay as raw `fn(ctx, req, res)` handlers.
Add a comment at the top of each: `// Streaming handler — does not use hx.authenticated()`.

---

## Section 3: Guardrails

### 3.1 — `hx.zig` ≤ 120 lines
The file must stay thin. Methods are one-liners that delegate to `common.zig`.
No business logic in `hx.zig` ever.

### 3.2 — No new abstractions without a second call site
`hx.zig` may only grow when a new method is needed by ≥2 converted handlers.
One-off helpers stay in the handler file.

### 3.3 — `make lint` passes — no unused `ctx.alloc` or `arena` in converted handlers
After conversion, grep for `ArenaAllocator.init` in non-streaming handler files.
Zero hits expected — all arena setup now lives inside `authenticated()`.

### 3.4 — `make test` passes
All existing handler tests pass without modification. The `authenticated()`
wrapper generates the same observable behaviour as the old hand-written prefix.

---

## Acceptance Criteria

1. `src/http/handlers/hx.zig` exists, ≤120 lines.
2. Zero `ArenaAllocator.init` calls in non-streaming handler files
   (`zombie_api.zig`, `zombie_activity_api.zig`, `approval_http.zig`,
   `webhooks.zig`).
3. `make lint` passes.
4. `make test` passes.
5. `authenticated()` unit tests pass (§1.3, §1.4).
6. Streaming handlers untouched — each has a `// Streaming handler` comment.

---

## Interfaces

### New: `src/http/handlers/hx.zig`
```zig
pub const Hx = struct {
    alloc: std.mem.Allocator,
    principal: common.AuthPrincipal,
    req_id: []const u8,
    ctx: *common.Context,
    res: *httpz.Response,

    pub fn db(self: Hx) !*pg.Conn
    pub fn releaseDb(self: Hx, conn: *pg.Conn) void
    pub fn redis(self: Hx) *queue_redis.Client
    pub fn ok(self: Hx, status: std.http.Status, body: anytype) void
    pub fn fail(self: Hx, code: []const u8, detail: []const u8) void
}

pub fn authenticated(
    comptime inner: fn (Hx, *httpz.Request) void,
) fn (*common.Context, *httpz.Request, *httpz.Response) void
```

---

## Spec-Claim Tracing

| Claim | Test | Status |
|---|---|---|
| authenticated() calls inner with correct Hx | §1.3 unit | PENDING |
| authenticated() returns 401, inner not called | §1.4 unit | PENDING |
| Zero ArenaAllocator.init in converted handlers | §3.3 grep gate | PENDING |
| Streaming handlers untouched | §2.4 comment + acceptance criterion 6 | PENDING |

---

## Verification Plan

```bash
make build
make lint
make test

# Grep gate — must be zero
grep -r "ArenaAllocator.init" src/http/handlers/ \
  --include="*.zig" \
  --exclude="*stream*" \
  --exclude="*relay*"
# Expected: no output

# Line count gate
wc -l src/http/handlers/hx.zig
# Expected: ≤120
```
