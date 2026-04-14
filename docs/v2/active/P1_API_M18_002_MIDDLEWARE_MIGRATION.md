---
Milestone: M18
Workstream: M18_002
Name: MIDDLEWARE_MIGRATION
Status: IN_PROGRESS
Branch: feat/m18-002-middleware-migration
Depends-on: M11_002 (introduced hx.zig — this workstream replaces hx.authenticated*)
Created: Apr 13, 2026
---

# M18_002 — Middleware Migration

## Goal

Replace the custom-invented `hx.authenticated()` / `hx.authenticatedWithParam()`
comptime wrappers with a runtime middleware chain modeled on httpz's
`Middleware(H)` interface. Every route declares its middleware list at
registration time. Handlers receive a `*hx.Hx` that is either fully authenticated
or intentionally bypassed — the handler body never calls `common.authenticate`.

All authentication, signature verification, and principal-population logic lives
under `src/auth/middleware/`. The contents of `src/auth/` must be extractable
into a standalone `zombie-auth` repository with zero edits — no imports from
`src/http/handlers/`, `src/state/`, `src/db/`, or any business-layer module.

**Demo:**

```zig
// Before (M11_002) — auth is a comptime wrapper, one arity per wrapper name.
pub const handleListZombies = hx.authenticated(innerListZombies);
pub const handleDeleteZombie = hx.authenticatedWithParam(innerDeleteZombie);
// handleZombieTelemetry (2 path params) — must stay raw, spec §1.5.
// handlePutWorkspaceSkillSecret (3 path params) — must stay raw.

// After (M18_002) — routes declare middlewares; handlers are plain inner fns.
pub const zombie_routes: []const RouteSpec = &.{
    .{ .method = .GET,    .path = .list_or_create_zombies, .handler = innerListZombies,   .middlewares = &auth.bearer },
    .{ .method = .DELETE, .path = .delete_zombie,          .handler = innerDeleteZombie,  .middlewares = &auth.bearer },
    .{ .method = .GET,    .path = .zombie_telemetry,       .handler = innerZombieTelemetry, .middlewares = &auth.bearer },
    .{ .method = .PUT,    .path = .skill_secret,           .handler = innerPutSkillSecret,  .middlewares = &auth.bearer },
};

// Handler body — path-param arity is handled by the router; auth is guaranteed.
fn innerZombieTelemetry(hx: *Hx, req: *httpz.Request, ws: []const u8, zid: []const u8) void {
    // hx.principal is populated by bearer_oidc middleware; zero auth ceremony here.
    const conn = hx.db() catch return hx.fail("UZ-DB-001", "db unavailable");
    defer hx.releaseDb(conn);
    // ...
}
```

---

## Depends on

- **M11_002** introduced `hx.zig` with the comptime `authenticated()` /
  `authenticatedWithParam()` wrappers. This workstream removes those wrappers
  and replaces them with a runtime middleware chain. `Hx` (the struct) survives
  as a context helper with `ok`, `fail`, `db`, `releaseDb`, `redis` methods; the
  prologue (arena + req_id + authenticate) moves into middleware.
- httpz's `Middleware(H)` type is referenced for API shape, but this workstream
  does **not** migrate to httpz's router. Path dispatch stays in `src/http/router.zig`
  — the middleware chain is invoked by our own `dispatch()` in `src/http/server.zig`
  before the matched handler runs.

---

## Surface Area Checklist

- [ ] **OpenAPI spec update** — no: external request/response shapes are unchanged; this is an internal plumbing refactor.
- [ ] **`zombiectl` CLI changes** — no.
- [ ] **User-facing doc changes** — no.
- [x] **Release notes** — patch bump: `0.9.1` → `0.9.2` (internal refactor, no API change).
- [ ] **Schema changes** — no.
- [ ] **Schema teardown** — N/A (no SQL changes).
- [x] **Spec-vs-rules conflict check** —
  - RULE ZIG §init/deinit: each middleware struct follows `init(config) !Self` + `deinit(*Self) void`. ✓
  - RULE FLL: `src/auth/middleware/*.zig` each ≤250 lines; `middleware_chain.zig` ≤120 lines.
  - RULE FLS (PgQuery everywhere): no DB access from middleware; principal lookups happen in handlers, not middleware.

---

## Section 1: `src/auth/` structure and portability contract

### 1.1 — Directory layout

```
src/auth/
├── mod.zig                        # re-exports — stable public surface for zombie-auth
├── oidc.zig                       # (existing) JWT verifier
├── sessions.zig                   # (existing) session store
├── rbac.zig                       # (existing) role enum + helpers
├── principal.zig                  # NEW: `AuthPrincipal` moved here from common.zig
└── middleware/
    ├── mod.zig                    # re-exports every middleware
    ├── chain.zig                  # runtime chain runner + Middleware(H) interface
    ├── bearer_oidc.zig            # Bearer JWT via OIDC
    ├── admin_api_key.zig          # X-API-Key header
    ├── bearer_or_api_key.zig      # either (current common.authenticate behaviour)
    ├── require_role.zig           # composable: require admin/operator
    ├── webhook_hmac.zig           # HMAC-SHA256 body signature (approval_http, generic)
    ├── webhook_url_secret.zig     # URL-embedded secret from vault
    ├── slack_signature.zig        # Slack x-slack-signature + freshness
    └── oauth_state.zig            # OAuth state nonce + HMAC (github, slack OAuth)
```

### 1.2 — Portability contract

`src/auth/**` may import from: `std`, `httpz`, `pg` (for optional `*pg.Conn`
lookup of principal metadata), plus other files inside `src/auth/`.
**Forbidden imports:** `src/http/handlers/`, `src/state/`, `src/db/pool.zig`,
`src/observability/`, `src/errors/` (except a re-exported error-code table).

A CI check (`make check-auth-portable`) greps `src/auth/**` for any relative
import that escapes the folder. Failure = test fail.

### 1.3 — Unit test: `auth/mod.zig` compiles standalone

A dedicated test binary target `auth-only-tests` links **only** `src/auth/**`
and verifies every middleware compiles and its tests pass without the rest of
the project. Proves the extraction constraint.

### 1.4 — `AuthPrincipal` ownership lives in `src/auth/principal.zig`

Move the struct out of `src/http/handlers/common.zig`. `common.zig` re-exports
it for backward compatibility during migration; final state: one definition,
owned by `src/auth/`.

---

## Section 2: Middleware interface + chain runner

### 2.1 — `src/auth/middleware/chain.zig` defines the Middleware interface

```zig
pub const Outcome = enum { next, short_circuit };

pub const Middleware = struct {
    ptr: *anyopaque,
    execute_fn: *const fn (ptr: *anyopaque, hx: *Hx, req: *httpz.Request) anyerror!Outcome,

    pub fn execute(self: Middleware, hx: *Hx, req: *httpz.Request) !Outcome {
        return self.execute_fn(self.ptr, hx, req);
    }
};

/// Run the chain. Stops early on short_circuit (middleware wrote the response).
pub fn run(chain: []const Middleware, hx: *Hx, req: *httpz.Request) !Outcome {
    for (chain) |m| {
        switch (try m.execute(hx, req)) {
            .next => continue,
            .short_circuit => return .short_circuit,
        }
    }
    return .next;
}
```

`Hx` is passed as a **mutable pointer** because middleware populates
`hx.principal`, `hx.req_id`, etc. Shape matches httpz's
`execute(self, req, res, executor)` but simpler — no opaque executor handle,
no error-as-control-flow.

### 2.2 — `src/http/server.zig` dispatch invokes the chain

```zig
fn dispatchMatchedRoute(ctx, req, res, path) bool {
    const route = router.match(path) orelse return false;
    const spec = route.spec();                 // NEW: RouteSpec with middlewares
    var hx = Hx.initEmpty(ctx, res);
    defer hx.deinit();
    const outcome = chain.run(spec.middlewares, &hx, req) catch |e| {
        common.internalError(res, @errorName(e));
        return true;
    };
    if (outcome == .short_circuit) return true;
    spec.invoke(&hx, req, route.params);       // dispatches with 0..3 path params
    return true;
}
```

### 2.3 — Unit test: chain short-circuits on 401 middleware

Build a no-op middleware + a failing auth middleware + a sentinel middleware.
Run the chain; assert sentinel is NEVER called when auth short-circuits.

### 2.4 — Unit test: chain propagates Outcome.next through all middlewares

All three middlewares return `.next`. Assert the handler is invoked once.

---

## Section 3: Concrete middleware implementations

### 3.1 — `bearer_oidc.zig` + `admin_api_key.zig` + `bearer_or_api_key.zig`

Three independent middlewares covering the current `common.authenticate`
behaviour:
- `bearer_oidc`: requires `Authorization: Bearer <jwt>`, verifies via `oidc.Verifier`, populates `hx.principal`.
- `admin_api_key`: requires `X-API-Key`, compares against configured admin key (const-time).
- `bearer_or_api_key`: tries both; `.next` on success of either, `.short_circuit` with 401 otherwise.

Each returns `.short_circuit` and writes a problem+json 401/503 on failure.

### 3.2 — `require_role.zig` — composable role gate

```zig
pub fn require(comptime required: rbac.AuthRole) Middleware { ... }
```

Runs **after** a bearer/api_key middleware. Reads `hx.principal.role`, writes
403 + short_circuits on mismatch. Used for `/internal/v1/*` admin endpoints and
`operator`-gated endpoints. Composes with `bearer_or_api_key` as
`&.{ auth.bearer_or_api_key, auth.require_role(.admin) }`.

### 3.3 — Webhook / Slack / OAuth middlewares

- `webhook_hmac`: HMAC-SHA256 body signature (config: shared secret source).
- `webhook_url_secret`: extracts secret from `/v1/webhooks/{id}/{secret}`, validates via vault lookup.
- `slack_signature`: verifies `x-slack-signature` + `x-slack-request-timestamp` freshness.
- `oauth_state`: parses `?state=`, validates nonce + HMAC for OAuth callback CSRF.

Each is self-contained: one struct, one config, one `execute` method.

### 3.4 — Unit tests per middleware (tier: negative + happy path)

Each middleware ships with ≥4 test cases:
- Happy path: valid credentials → `.next` + principal populated where applicable.
- Missing credential: no header → `.short_circuit` + 401.
- Invalid credential: malformed/expired → `.short_circuit` + 401 or 403.
- Downstream failure: verifier/vault unavailable → `.short_circuit` + 503.

---

## Section 4: Route registration with middleware chains

### 4.1 — `RouteSpec` replaces the bare `Route` enum at dispatch

The existing `src/http/router.zig` match table returns `Route` + params. A new
sibling module `src/http/route_table.zig` owns the full `RouteSpec` for each
route variant: method, middlewares, inner handler fn. `dispatchMatchedRoute`
looks up the spec via the `Route` tag.

No change to `router.match()` — the pattern-matching surface stays identical.

### 4.2 — Pre-defined auth policies in `src/auth/middleware/mod.zig`

```zig
pub const policies = struct {
    pub const none:           []const Middleware = &.{};
    pub const bearer:         []const Middleware = &.{ bearer_or_api_key.instance };
    pub const admin:          []const Middleware = &.{ bearer_or_api_key.instance, require_role.admin };
    pub const operator:       []const Middleware = &.{ bearer_or_api_key.instance, require_role.operator };
    pub const webhook_hmac:   []const Middleware = &.{ webhook_hmac.instance };
    pub const webhook_secret: []const Middleware = &.{ webhook_url_secret.instance };
    pub const slack:          []const Middleware = &.{ slack_signature.instance };
    pub const oauth_callback: []const Middleware = &.{ oauth_state.instance };
};
```

Callers compose: `auth.policies.admin`, `auth.policies.bearer ++ auth.policies.workspace_scoped` (if new gates are added).

### 4.3 — Unit test: route_table coverage is total

Compile-time test: every `Route` enum variant has a matching `RouteSpec` in
`route_table.zig`. Missing entries fail with `@compileError` listing the tag.

### 4.4 — Integration test: 401 written by middleware; handler not invoked

Hit `/v1/zombies` with no Authorization header. Assert 401, assert the handler
body is never entered (via instrumented counter).

---

## Section 5: Handler conversion + hx.zig shrink

### 5.1 — Convert all converted M11_002 handlers to `innerHandle*`

Files previously using `hx.authenticated` / `hx.authenticatedWithParam`:
- `zombie_api.zig`, `zombie_activity_api.zig`, `zombie_telemetry.zig`
- `workspaces_*.zig` (4 files), `workspace_credentials_http.zig`
- `admin_platform_keys_http.zig`, `auth_sessions_http.zig` (mixed)

Each handler becomes `fn innerHandleX(hx: *Hx, req: *httpz.Request, ...) void`;
no `common.authenticate` call; role checks replaced with `require_role` in the
middleware chain.

### 5.2 — Convert previously-raw handlers to middleware-driven

Files that stayed raw in M11_002 because of arity or auth-scheme mismatch:
- `webhooks.zig` → `auth.policies.webhook_secret` / `auth.policies.webhook_hmac`.
- `approval_http.zig` → `auth.policies.webhook_hmac`.
- `slack_events.zig`, `slack_interactions.zig` → `auth.policies.slack`.
- `slack_oauth.zig`, `github_callback.zig` → `auth.policies.oauth_callback`.
- `skill_secrets_http.zig` (3 path params) → `auth.policies.bearer` — path-param
  arity is handled by `route_table`, not the auth layer.
- `zombie_telemetry.zig::handleZombieTelemetry` (2 path params) → same.
- `agent_relay.zig` (streaming) and `auth_sessions_http.zig::handleCreateAuthSession`
  (login) remain handler-raw but now explicitly register with `auth.policies.none`.

### 5.3 — Remove `hx.authenticated` and `hx.authenticatedWithParam`

`hx.zig` shrinks to just:
- `pub const Hx = struct { ... }` (data + methods: `ok`, `fail`, `db`, `releaseDb`, `redis`).
- No comptime wrappers.

Final line count of `hx.zig`: ≤80 LoC. Delete the wrapper tests from `hx_test.zig`.

### 5.4 — Grep gates in CI

```bash
# No handler calls common.authenticate directly — auth lives in middleware.
grep -rn "common.authenticate(" src/http/handlers/ --include="*.zig" --exclude="*_test.zig"
# Expected: 0 matches.

# No handler uses hx.authenticated* — wrappers gone.
grep -rn "hx.authenticated\|hx_mod.authenticated" src/http/ --include="*.zig"
# Expected: 0 matches.

# Portability — src/auth/ does not reach into http/state/db.
grep -rn '@import("../http\|@import("../state\|@import("../db' src/auth/ --include="*.zig"
# Expected: 0 matches.
```

---

## Section 6: Guardrails

### 6.1 — `src/auth/middleware/chain.zig` ≤120 lines

The chain runner and Middleware interface must stay minimal. No routing, no
handler dispatch, no HTTP-specific logic beyond the `*httpz.Request` parameter.

### 6.2 — Each middleware file ≤250 lines

Includes tests. If a middleware needs more, split into submodules (e.g.
`webhook_hmac/` directory with body reader + signer helpers).

### 6.3 — `hx.zig` ≤80 lines after the wrapper removal

M11_002 cap was 150; post-migration, ≤80.

### 6.4 — `src/auth/` has zero cross-module dependencies

`make check-auth-portable` enforces. Also asserted by the standalone
`auth-only-tests` build target (§1.3).

---

## Acceptance Criteria

1. `src/auth/middleware/` contains the 8 middleware files listed in §1.1 + `chain.zig` + `mod.zig`.
2. Every handler in `src/http/handlers/` is `fn innerHandleX(*Hx, *httpz.Request, ...) void` — zero `common.authenticate` call sites outside `src/auth/middleware/bearer_*.zig`.
3. `hx.authenticated` and `hx.authenticatedWithParam` are removed; grep gate passes.
4. `src/auth/` builds + tests pass in isolation via the `auth-only-tests` target.
5. `make check-auth-portable` passes (no imports escape the folder).
6. `make test-integration` passes — all existing HTTP integration tests (RBAC, BYOK, M18 telemetry) stay green.
7. The 4 test cases per middleware from §3.4 are all present and pass.
8. Route-table coverage test (§4.3) compiles — every `Route` tag has a spec.

---

## Interfaces

### New: `src/auth/middleware/chain.zig`

```zig
pub const Outcome = enum { next, short_circuit };

pub const Middleware = struct {
    ptr: *anyopaque,
    execute_fn: *const fn (ptr: *anyopaque, hx: *Hx, req: *httpz.Request) anyerror!Outcome,

    pub fn execute(self: Middleware, hx: *Hx, req: *httpz.Request) !Outcome;
};

pub fn run(chain: []const Middleware, hx: *Hx, req: *httpz.Request) !Outcome;
```

### New: `src/auth/middleware/mod.zig`

```zig
pub const chain = @import("chain.zig");
pub const bearer_oidc       = @import("bearer_oidc.zig");
pub const admin_api_key     = @import("admin_api_key.zig");
pub const bearer_or_api_key = @import("bearer_or_api_key.zig");
pub const require_role      = @import("require_role.zig");
pub const webhook_hmac      = @import("webhook_hmac.zig");
pub const webhook_url_secret = @import("webhook_url_secret.zig");
pub const slack_signature   = @import("slack_signature.zig");
pub const oauth_state       = @import("oauth_state.zig");

pub const policies = struct {
    pub const none:           []const chain.Middleware = &.{};
    pub const bearer:         []const chain.Middleware;
    pub const admin:          []const chain.Middleware;
    pub const operator:       []const chain.Middleware;
    pub const webhook_hmac_p: []const chain.Middleware;
    pub const webhook_secret: []const chain.Middleware;
    pub const slack:          []const chain.Middleware;
    pub const oauth_callback: []const chain.Middleware;
};
```

### New: `src/http/route_table.zig`

```zig
pub const RouteSpec = struct {
    method: std.http.Method,
    middlewares: []const auth.chain.Middleware,
    handler: *const anyopaque,   // inner fn, arity determined by Route tag
    invoke: *const fn (*Hx, *httpz.Request, params: RouteParams) void,
};

pub fn specFor(route: router.Route) RouteSpec;
```

### Changed: `src/http/handlers/hx.zig`

```zig
pub const Hx = struct {
    alloc:     std.mem.Allocator,
    principal: ?auth.AuthPrincipal,  // nullable — middleware may not populate
    req_id:    []const u8,
    ctx:       *common.Context,
    res:       *httpz.Response,

    pub fn ok(self: *Hx, status: std.http.Status, body: anytype) void;
    pub fn fail(self: *Hx, code: []const u8, detail: []const u8) void;
    pub fn db(self: *Hx) !*pg.Conn;
    pub fn releaseDb(self: *Hx, conn: *pg.Conn) void;
    pub fn redis(self: *Hx) *queue_redis.Client;
};
// authenticated() and authenticatedWithParam() REMOVED.
```

---

## Error Contracts

| Error | Middleware | Caller sees |
|-------|------------|-------------|
| Missing Authorization header | `bearer_oidc` / `bearer_or_api_key` | 401 + `UZ-AUTH-001` |
| Invalid/expired JWT | `bearer_oidc` | 401 + `UZ-AUTH-002` |
| JWKS fetch failed | `bearer_oidc` | 503 + `UZ-AUTH-003` |
| Missing X-API-Key | `admin_api_key` | 401 + `UZ-AUTH-010` |
| Wrong X-API-Key | `admin_api_key` | 401 + `UZ-AUTH-011` |
| Role mismatch | `require_role` | 403 + `UZ-AUTH-020` |
| Invalid HMAC | `webhook_hmac` | 401 + `UZ-WH-010` |
| Stale Slack timestamp | `slack_signature` | 401 + `UZ-WH-011` |
| Invalid OAuth state | `oauth_state` | 400 + `UZ-AUTH-030` |
| Vault unavailable (webhook secret lookup) | `webhook_url_secret` | 503 + `UZ-WH-012` |

All error codes already exist in `src/errors/error_entries.zig` — no new codes
required; middleware reuses the existing catalog.

---

## Failure Modes

| Failure | Trigger | Behavior |
|---------|---------|----------|
| Middleware panics | Programmer bug (e.g. deref null in HMAC calc) | `dispatch()` catch block writes 500 + `UZ-INTERNAL-001`; panic surfaces in logs |
| Middleware leaks allocation | Missing `defer alloc.free` in middleware impl | `std.testing.allocator` fails each middleware's own tests before release |
| Two middlewares both write response | Programmer bug | Policy: second write is a no-op; first `short_circuit` wins. Chain runner returns after first `.short_circuit`. |
| Handler invoked despite middleware short_circuit | Dispatcher bug | Integration test §4.4 asserts handler counter stays at 0 when middleware short-circuits |
| Middleware leaks connection | Middleware acquires `*pg.Conn` then short-circuits without release | Policy: **middleware MUST NOT acquire DB connections.** Principal lookups use cached JWKS or pre-loaded config. DB access is handler-only. |

---

## Implementation Constraints

| Constraint | Verify |
|------------|--------|
| `src/auth/**` has zero imports outside `src/auth/` (+ std, httpz, pg) | `make check-auth-portable` |
| Middleware never holds `*pg.Conn` across `.next` → handler | grep for `pool.acquire` in `src/auth/middleware/` — expected: 0 |
| Cross-compiles on x86_64-linux and aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| `auth-only-tests` target compiles + runs | `zig build auth-only-tests` |
| Every `Route` tag has `RouteSpec` | `@compileError` in §4.3 test |

---

## Spec-Claim Tracing

| Claim | Test | Status |
|-------|------|--------|
| Chain runs all middlewares in order | §2.4 unit | DONE (Batch A) |
| Chain short-circuits on first failure | §2.3 unit | DONE (Batch A) |
| Handler not invoked on short-circuit | §4.4 integration | PENDING (Batch D) |
| Every route has a spec | §4.3 comptime | PENDING (Batch D — empty table in C.2) |
| Each middleware has 4+ test cases | §3.4 per-file | DONE (Batches B.1–B.3, C.1, C.2) |
| src/auth is portable | §1.3 standalone target + grep gate | DONE (200/200 auth-only tests) |
| hx.zig ≤80 LoC after removal | §5.3 + §6.3 wc gate | PENDING (Batch E) |

---

## Migration Sequencing (within this workstream, committed as separate PRs or a stacked series)

1. **Batch A (foundation):** §1 scaffolding + §2 chain runner + `AuthPrincipal` move. ✅ DONE
2. **Batch B (middleware implementations):** §3 concrete middlewares with unit tests. ✅ DONE (B.1, B.2, B.3)
3. **Batch C (route table + dispatcher):** §4 route_table + dispatch wiring. ✅ DONE (C.1=oauth_state, C.2=route_table+dispatcher). Route table empty; dispatcher fast-path compiled but dead until Batch D.
   - **C.2 design note:** `MiddlewareRegistry` lives in `src/auth/middleware/mod.zig` and is held by `App` (server.zig) rather than `handler.Context`, to avoid modifying `common.zig` (782 lines, RULE FLL). `webhookUrlSecret.lookup_fn` is a stub returning null until Batch D wires the real vault lookup.
4. **Batch D (handler conversion):** §5.1 + §5.2 in 2-3 PRs grouped by file family (zombies / workspaces / webhooks / slack / oauth). PENDING.
5. **Batch E (cleanup):** §5.3 wrapper removal + §5.4 grep gates flip from "warn" to "fail". PENDING.

Batch A must merge before Batch B. Batches B–D may parallelize. Batch E closes the workstream.

---

## Verification Plan

```bash
# Build gates
make build
zig build -Dtarget=x86_64-linux
zig build -Dtarget=aarch64-linux

# Portability gates
zig build auth-only-tests                            # §1.3
make check-auth-portable                              # §6.4

# Grep gates (should all return 0 lines after Batch E)
grep -rn "common.authenticate(" src/http/handlers/ --include="*.zig" --exclude="*_test*"
grep -rn "hx.authenticated" src/http/ --include="*.zig"
grep -rn 'pool.acquire' src/auth/middleware/ --include="*.zig"

# Size gates
wc -l src/auth/middleware/chain.zig   # ≤120
wc -l src/http/handlers/hx.zig        # ≤80
for f in src/auth/middleware/*.zig; do wc -l "$f"; done  # each ≤250

# Test gates
make lint
make test
make test-integration
```
