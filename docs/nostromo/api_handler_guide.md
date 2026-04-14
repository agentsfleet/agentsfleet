# API Handler Style Guide

How to add a new HTTP endpoint to zombied. Keep this guide open when writing any new handler.

## 1. The handler signature

Every handler follows this shape:

```zig
pub fn innerMyEndpoint(hx: Hx, req: *httpz.Request, ...path_params) void {
    // validation
    // db / state work
    // response via hx.ok / hx.fail
}
```

Rules:

- **Name prefix `inner`** (never `handle`). The `invokeXxx` shim in `route_table_invoke.zig` is the only public handle point — your function is the inner implementation it calls after the middleware chain has populated `hx`.
- **First parameter: `hx: Hx`** — never `ctx: *Context`, `res: *Response`, `req_id: []const u8`, or an arena allocator. All of those live inside `hx`.
- **Second parameter: `req: *httpz.Request`** — only if you actually read it (body, query, headers). Drop it if you only need path params.
- **Path params come after `req`** — as declared in the `Route` enum variant in `router.zig`.
- **Return `void`.** Errors are written to the response; never return a Zig error.

## 2. What `Hx` gives you

```zig
pub const Hx = struct {
    alloc: std.mem.Allocator,       // request-scoped arena
    principal: common.AuthPrincipal, // set by bearer/admin middleware
    req_id: []const u8,              // unique request ID
    ctx: *common.Context,            // pool, queue, oidc, telemetry, app_url
    res: *httpz.Response,            // response writer

    pub fn ok(self, status, body) void;   // standard JSON envelope
    pub fn fail(self, code, detail) void; // RFC 7807 error
};
```

**Never:**

- Build your own arena — `hx.alloc` is already request-scoped.
- Call `common.requestId(alloc)` — `hx.req_id` is set.
- Call `common.authenticate(...)` — the middleware chain did this; use `hx.principal`.
- Write `hx.res.status = 200; hx.res.json(body, .{})` — that bypasses the JSON envelope. Use `hx.ok(.ok, body)`.

**Exception (streaming):** SSE handlers (`agent_relay.zig`) write `hx.res.chunk(...)` directly because `hx.ok` is JSON-only.

## 3. Writing responses

**Success:**

```zig
hx.ok(.ok, .{ .zombie_id = id, .status = "active" });
hx.ok(.created, .{ .agent_id = id, .key = raw_key });
hx.ok(.accepted, .{ .status = "accepted", .event_id = event_id });
```

**Errors from the registry:**

```zig
hx.fail(ec.ERR_INVALID_REQUEST, "workspace_id must be a valid UUIDv7");
hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
```

The error-code registry (`src/errors/error_registry.zig`) owns the HTTP status, RFC 7807 `title`, and `docs_uri`. Your handler only supplies the code and a human-readable `detail`.

**Internal 500s (DB / operation failure):**

```zig
common.internalDbUnavailable(hx.res, hx.req_id);   // pool.acquire failed
common.internalDbError(hx.res, hx.req_id);         // query failed
common.internalOperationError(hx.res, "detail", hx.req_id);
```

These are intentionally NOT wrapped on `Hx` — they're specific conveniences with fixed error codes, and a thin method on `Hx` would just forward to the same two fields. Call them directly.

**Never call these yourself:**

- `common.errorResponse(...)` — use `hx.fail`. Writing `common.errorResponse(hx.res, code, msg, hx.req_id)` bypasses the Hx abstraction.
- `common.writeJson(...)` — use `hx.ok`. Same reason.

## 4. Registering the route

Five places, in order:

1. **`src/http/router.zig`** — add a variant to the `Route` enum (with path params).
2. **`src/http/router.zig::match()`** — add the path parser that returns your variant.
3. **`src/http/route_table.zig::specFor()`** — map the variant to a `RouteSpec`:

   ```zig
   .my_endpoint => .{ .middlewares = registry.bearer(), .invoke = invoke.invokeMyEndpoint },
   ```

   Pick the right policy:

   | Policy | When |
   |--------|------|
   | `auth_mw.MiddlewareRegistry.none` | Public endpoint; no auth, or handler does its own (OAuth callbacks, webhooks) |
   | `registry.bearer()` | Standard user-facing endpoint; workspace-scoped |
   | `registry.admin()` | Admin-only (internal telemetry, platform keys) |
   | `registry.operator()` | Operator role required |
   | `registry.webhookHmac()` | Approval webhook (HMAC-signed body) |
   | `registry.slack()` | Slack events/interactions (slack signature) |

4. **`src/http/route_table_invoke.zig`** — add the invoke shim:

   ```zig
   pub fn invokeMyEndpoint(hx: *Hx, req: *httpz.Request, route: router.Route) void {
       if (req.method != .POST) { common.respondMethodNotAllowed(hx.res); return; }
       my_handler.innerMyEndpoint(hx.*, req, route.my_endpoint);
   }
   ```

   `hx.*` (dereferenced) is passed by value — cheap (5 pointer-sized fields) and handlers take `Hx` not `*Hx`.

5. **`public/openapi.json`** — add the endpoint. This is the public contract.

## 5. What NOT to do — common mistakes

- ❌ `handleMyEndpoint(ctx, req, res)` — old signature, don't add new ones.
- ❌ Building an arena inside the handler — `hx.alloc` is already scoped.
- ❌ Calling `common.authenticate` — middleware chain already did this.
- ❌ `common.errorResponse(hx.res, ...)` — use `hx.fail(...)`.
- ❌ `hx.res.json(body, .{})` — use `hx.ok(.ok, body)`.
- ❌ Writing `res.status = 200; res.body = "{}"` inline — prefer `hx.ok(.ok, .{})`. Only acceptable for SSE streams and Slack's "ack-and-drop" pattern where no JSON envelope is needed.
- ❌ Returning a Zig error from the handler — write to `hx.res` and return void.

## 6. Middleware is already done for you

The middleware chain (bearer/admin/webhookHmac/slack) runs BEFORE your handler. When your handler runs:

- Token is verified (for non-`none` policies).
- `hx.principal` is populated with workspace_id, role, user_id.
- Short-circuits (401/403) have already happened — you never see them.

For `none` policy routes (OAuth callbacks, webhooks, public endpoints), `hx.principal` is zero-valued. Do not read `hx.principal` on `none`-policy handlers.

## 7. Reference implementations

Good examples to model after:

- **Simple list/get:** `zombie_api.zig::innerListZombies`
- **POST with body + validation:** `zombie_api.zig::innerCreateZombie`
- **DELETE with idempotency:** `zombie_api.zig::innerDeleteZombie`
- **Admin-only:** `admin_platform_keys_http.zig::innerGetAdminPlatformKeys`
- **Multi-method router (GET/PUT/DELETE on one path):** `workspace_credentials_http.zig`
- **Workspace auth + tenant context:** `zombie_telemetry.zig::innerZombieTelemetry`
- **Webhook with inline auth:** `webhooks.zig::innerReceiveWebhook`
- **Streaming (SSE):** `agent_relay.zig::innerRelay`

## 8. Testing checklist

Before opening a PR:

- [ ] `zig build` clean
- [ ] `make test-auth` passes (200/200)
- [ ] `zig build test` passes (pre-existing failures noted if any)
- [ ] Cross-compile: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `make lint` — all Zig gates pass (RULE FLL 350 lines, check-pg-drain, zlint)
- [ ] Handler file ≤ 350 lines; split if it grows
- [ ] Integration test covers happy path + at least one error path per `hx.fail` call
- [ ] OpenAPI updated — new endpoint + response schema
- [ ] `gitleaks detect` clean
