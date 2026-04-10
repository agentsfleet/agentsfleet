---
Milestone: M11
Workstream: M11_002
Name: HX_HANDLER_CONTEXT
Status: IN_PROGRESS
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

### 1.5 — `authenticatedWithParam()` comptime wrapper

For handlers that receive path params (e.g. `zombie_id`, `workspace_id`),
a second wrapper passes the param through:

```zig
/// Like authenticated(), but the inner function also receives a path param.
pub fn authenticatedWithParam(
    comptime inner: fn (hx: Hx, req: *httpz.Request, param: []const u8) void,
) fn (*common.Context, *httpz.Request, *httpz.Response, []const u8) void {
    return struct {
        fn handle(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, param: []const u8) void {
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
            }, req, param);
        }
    }.handle;
}
```

If a handler takes 2 path params (e.g. `webhooks.zig`'s `zombie_id` +
`url_secret`), it does NOT use this wrapper — it stays raw.

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

All non-streaming, authenticated handlers are converted in this workstream.
Handlers that take extra path params (e.g. `workspace_id: []const u8`) use
an `authenticatedWithParam()` variant — see §1.5.

### 2.1 — Convert `zombie_api.zig`
`handleCreateZombie`, `handleListZombies` → `authenticated(innerXxx)`.
`handleDeleteZombie` takes `zombie_id` path param → `authenticatedWithParam(innerDeleteZombie)`.
Body parsing helpers (`parseCreateBody`, `validateCreateFields`) keep their
signatures but receive `hx.alloc` and call `hx.fail()` instead of
`common.errorResponse`.

### 2.2 — Convert `zombie_activity_api.zig`
`handleListActivity`, `handleStoreCredential`, `handleListCredentials`.

### 2.3 — Convert `approval_http.zig`
Authenticated handler(s) use `authenticated()`. Any non-auth webhook-style
endpoints stay as raw handlers with a `// No Bearer auth — uses HMAC` comment.

### 2.4 — Convert `workspaces_*.zig` group
- `workspaces_billing.zig` — 3 handlers.
- `workspaces_billing_summary.zig` — 1 handler.
- `workspaces_ops.zig` — 2 handlers.
- `workspaces_lifecycle.zig` — 1 handler.

### 2.5 — Convert `workspace_credentials_http.zig`
3 handlers, all authenticated.

### 2.6 — Convert `agents.zig` + `agents/get.zig` + `agents/scores.zig`
- `agents.zig` — 4 handlers.
- `agents/get.zig` — 1 handler.
- `agents/scores.zig` — 1 handler.

### 2.7 — Convert `runs/` non-streaming handlers
- `runs/start.zig`, `runs/get.zig`, `runs/list.zig`, `runs/cancel.zig`,
  `runs/interrupt.zig`, `runs/retry.zig`, `runs/replay.zig`.
All take `ctx, req, res` (some may have path params — use appropriate wrapper).

### 2.8 — Convert `harness_http.zig`
4 handlers, all take `workspace_id` path param → `authenticatedWithParam(innerXxx)`.

### 2.9 — Convert `admin_platform_keys_http.zig`
3 handlers, all authenticated.

### 2.10 — Convert `skill_secrets_http.zig`
2 handlers, all authenticated.

### 2.11 — Convert `specs.zig`
1 handler, authenticated.

### 2.12 — Mixed-auth files: `auth_sessions_http.zig`
- `handleCreateAuthSession` — **no auth** (login endpoint), stays as raw handler
  with comment: `// No Bearer auth — creates auth session`.
- `handlePollAuthSession` — **no auth** (polling endpoint), stays raw.
- `handleCompleteAuthSession` — **authenticated**, uses `authenticatedWithParam()`.
Per-function treatment, not blanket conversion.

### 2.13 — Raw handlers: `webhooks.zig`
`handleReceiveWebhook` uses HMAC verification, not Bearer auth. Stays as a raw
`fn(ctx, req, res, ...)` handler. Add comment: `// HMAC-verified — does not use hx.authenticated()`.

### 2.14 — Raw handlers: `github_callback.zig`
OAuth callback — no Bearer auth. Stays raw. Add comment:
`// OAuth callback — does not use hx.authenticated()`.

### 2.15 — Raw handlers: `health.zig`
`handleHealthz`, `handleReadyz`, `handleMetrics` — no auth. Stays raw.
No comment needed (health endpoints are obviously unauthenticated).

### 2.16 — Do NOT convert streaming handlers
`agent_relay.zig`, `runs/stream.zig` block in a loop calling `res.chunk()`.
These stay as raw `fn(ctx, req, res)` handlers.
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

1. `src/http/handlers/hx.zig` exists, ≤150 lines (increased from 120 to accommodate `authenticatedWithParam`).
2. Zero `ArenaAllocator.init` calls in non-streaming, non-raw handler files.
   Excluded from grep: `*stream*`, `*relay*`, `webhooks.zig`, `github_callback.zig`,
   `health.zig`, `auth_sessions_http.zig` (mixed — only `handleCompleteAuthSession` converts).
3. `make lint` passes.
4. `make test` passes.
5. `authenticated()` and `authenticatedWithParam()` unit tests pass (§1.3, §1.4).
6. Streaming handlers untouched — each has a `// Streaming handler` comment.
7. Raw/no-auth handlers have descriptive comments explaining why they skip `authenticated()`.
8. All ~42 `common.authenticate` call sites in authenticated handlers are eliminated
   (auth now lives inside the wrappers).

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

pub fn authenticatedWithParam(
    comptime inner: fn (Hx, *httpz.Request, []const u8) void,
) fn (*common.Context, *httpz.Request, *httpz.Response, []const u8) void
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

# Grep gate — ArenaAllocator.init must be zero in converted files
grep -rn "ArenaAllocator.init" src/http/handlers/ \
  --include="*.zig" \
  --exclude="*stream*" \
  --exclude="*relay*" \
  --exclude="webhooks.zig" \
  --exclude="github_callback.zig" \
  --exclude="health.zig" \
  --exclude="auth_sessions_http.zig" \
  --exclude="hx.zig"
# Expected: no output (only hx.zig and excluded files may contain it)

# Grep gate — common.authenticate must only appear in hx.zig and excluded files
grep -rn "common.authenticate" src/http/handlers/ \
  --include="*.zig" \
  --exclude="hx.zig" \
  --exclude="*stream*" \
  --exclude="*relay*" \
  --exclude="webhooks.zig" \
  --exclude="github_callback.zig" \
  --exclude="auth_sessions_http.zig"
# Expected: no output

# Line count gate
wc -l src/http/handlers/hx.zig
# Expected: ≤150
```
