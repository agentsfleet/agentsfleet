# M11_003: Clerk Signup Webhook + Personal Account Bootstrap

**Prototype:** v2
**Milestone:** M11
**Workstream:** 003
**Date:** Apr 20, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — Blocks user acquisition; no signup path = no entry
**Batch:** B5
**Branch:** feat/m11-003-clerk-signup
**Depends on:** M28_001 (webhook auth middleware — `svix_signature` primitives), M28_002 (tenant API keys — referenced by M11_008 primer)

---

## Overview

**Goal (testable):** A Clerk `user.created` webhook delivered to `POST /v1/webhooks/clerk` with a valid Svix signature provisions a fresh personal account — tenant + user + membership + default workspace (Heroku-named, collision-safe) + credit state (0 cents) + audit row — atomically in a single SQL transaction, idempotent on `oidc_subject`, and returns 200 with `{workspace_id, workspace_name, created}`. Invalid signature returns 401 with `UZ-WH-010`.

**Problem:** UseZombie has no automated signup path today. Clerk manages identity but we have no webhook receiver to bootstrap the corresponding internal records. Without this endpoint, every new user requires manual DB provisioning, making a public `usezombie.com/sign-up` impossible.

**Solution summary:** One Zig HTTP handler at `/v1/webhooks/clerk` that (a) verifies Svix signatures via reusable primitives extracted from `src/auth/middleware/svix_signature.zig` into `src/crypto/svix_verify.zig`, (b) parses the Clerk `user.created` event, and (c) invokes `signup_bootstrap.bootstrapPersonalAccount(conn, alloc, params)` — a new transactional helper that inserts tenant + user + membership + workspace (with a Heroku-name generator that retries on uniqueness collision) + credit_state + audit row. Idempotency is enforced by a fast-path check on `core.users.oidc_subject` before the transaction opens. No invite system, no redemption, no downstream zombies, no AgentMail — those concerns live in later milestones (auto-credit in M11_011; Homelab zombie program in M11_012).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/http/router.zig` | MODIFY | Add `clerk_webhook` route variant + path match `/v1/webhooks/clerk` |
| `src/http/route_table.zig` | MODIFY | Register `clerk_webhook` with `registry.none()` — handler verifies Svix inline |
| `src/http/route_table_invoke.zig` | MODIFY | Add `invokeClerkWebhook` |
| `src/http/handlers/clerk_webhook.zig` | CREATE | Svix verify + parse `user.created` + `bootstrapPersonalAccount` call + 200 |
| `src/state/signup_bootstrap.zig` | CREATE | Transactional bootstrap facade (tenant + user + membership + workspace + credit state + audit) |
| `src/state/signup_bootstrap_store.zig` | CREATE | Store helpers: fast-path replay check, insert primitives |
| `src/state/heroku_names.zig` | CREATE | Heroku-style `{adjective}-{noun}-{number}` generator (no external deps) |
| `src/crypto/svix_verify.zig` | CREATE | Reusable Svix v1 crypto core (HMAC + space-separated multi-sig + timestamp freshness) — ≤120 lines |
| `src/auth/middleware/svix_signature.zig` | MODIFY | Refactor to call `svix_verify.zig`. No behavioral change; pure extraction. |
| `src/errors/error_entries.zig` | MODIFY | Update `UZ-WH-010` / `UZ-WH-011` hints to provider-neutral (reused for Clerk; no new codes) |
| `public/openapi.json` | MODIFY | Add `POST /v1/webhooks/clerk` path + 2 new error codes |
| `src/http/handlers/clerk_webhook_integration_test.zig` | CREATE | Integration tests — real schema, Svix-signed payloads |
| `src/state/signup_bootstrap_test.zig` | CREATE | Real-schema integration tests for bootstrap paths |
| `src/crypto/svix_verify_test.zig` | CREATE | Unit tests for Svix v1 verifier primitives |
| `docs/v2/pending/P1_UI_CLI_API_M11_003_INVITE_SIGNUP_ONBOARDING.md` | DELETE | Superseded by this spec (pivot: no invites) |

## Applicable Rules

- **RULE FLL** — every new `.zig` file ≤ 350 lines (test files exempt).
- **RULE FLS** — if any `.drain()` path is added, audit for zombie-pg-drain; handler uses `conn.exec()` for inserts (no rows needed) to avoid the drain obligation.
- **RULE XCC** — cross-compile `x86_64-linux` + `aarch64-linux` before commit.
- **RULE ORP** — orphan sweep: the delete of `docs/v2/pending/P1_UI_CLI_API_M11_003_INVITE_SIGNUP_ONBOARDING.md` must be followed by a grep for any references in other specs (future scrub handled by task #10).
- **RULE TNM** — test file naming: problem-oriented (`clerk_webhook_integration_test.zig`, not `m11_003_clerk_test.zig`).
- **RULE ITF** — integration tests seed real schema via fixture modules; no TEMP TABLE mocks.
- **RULE CTM** — constant-time compare for HMAC output (inherited from M28's svix middleware — preserved in the extraction).
- **Schema Table Removal Guard** — `VERSION=0.9.0 (<2.0.0)` — no schema migrations in this spec (only reads existing `core.tenants`/`core.users`/`core.memberships`/`core.workspaces` + `billing.workspace_credit_state`/`billing.workspace_credit_audit`).

---

## Sections (implementation slices)

### §1 — Svix Verifier Extraction

**Status:** PENDING

Extract the Svix v1 HMAC crypto core from `src/auth/middleware/svix_signature.zig` into `src/crypto/svix_verify.zig`. Middleware becomes a thin wrapper around the reusable fn. Enables `/v1/webhooks/clerk` handler to verify without the zombie-shaped middleware context.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/zombie/svix_verify.zig:verifySvix` | `{secret: "whsec_test", svix_id: "msg_1", svix_ts: <now>, svix_sig: "v1,<valid_hmac>", body: "{...}"}` | `true` | unit |
| 1.2 | PENDING | `src/zombie/svix_verify.zig:verifySvix` | Same payload but `svix_sig: "v1,<tampered>"` | `false` (constant-time) | unit |
| 1.3 | PENDING | `src/zombie/svix_verify.zig:verifySvix` | Valid sig but `svix_ts` 6 minutes in the past | `false` (stale) | unit |
| 1.4 | PENDING | `src/auth/middleware/svix_signature.zig:verifyRequest` | Post-refactor middleware still accepts a valid-signed request for `/v1/webhooks/svix/{zombie_id}` | No behavioral change from pre-refactor | integration |

### §2 — Clerk Webhook Handler

**Status:** PENDING

`POST /v1/webhooks/clerk`. Reads `Svix-Id`/`Svix-Timestamp`/`Svix-Signature` headers, verifies against `env.CLERK_WEBHOOK_SECRET`, parses `user.created` payload, extracts primary email + display name, calls `bootstrapPersonalAccount`, returns 200.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `src/http/handlers/clerk_webhook.zig:innerClerkWebhook` | Valid Svix-signed `user.created` event, fresh oidc_subject | 200 `{workspace_id, workspace_name, created:true}`; 6 rows inserted | integration |
| 2.2 | PENDING | `src/http/handlers/clerk_webhook.zig:innerClerkWebhook` | Invalid signature | 401 `UZ-WH-010`, zero DB writes | integration |
| 2.3 | PENDING | `src/http/handlers/clerk_webhook.zig:innerClerkWebhook` | Valid signature, timestamp 6 minutes old | 401 `UZ-WH-011`, zero DB writes | integration |
| 2.4 | PENDING | `src/http/handlers/clerk_webhook.zig:innerClerkWebhook` | Valid request but body missing primary email | 400 `UZ-REQ-001`, zero DB writes | integration |

### §3 — Transactional Bootstrap

**Status:** PENDING

`signup_bootstrap.bootstrapPersonalAccount(conn, alloc, params) !Bootstrap` — single SQL transaction inserting tenant + user + membership + workspace + credit_state + audit. Fast-path replay check before opening the transaction. Heroku-name collision retry up to 8 attempts.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `src/state/signup_bootstrap.zig:bootstrapPersonalAccount` | Fresh `oidc_subject = "user_abc"`, `email = "jane@acme.com"` | Bootstrap returns `{user_id, tenant_id, workspace_id, workspace_name (Heroku-style), created:true}`; tenant + user + membership + workspace + credit_state (0 cents) + audit row all present | integration |
| 3.2 | PENDING | `src/state/signup_bootstrap.zig:bootstrapPersonalAccount` | Replay: second call with same `oidc_subject` | Returns `{..., created:false}`; DB state unchanged from first call | integration |
| 3.3 | PENDING | `src/state/signup_bootstrap.zig:pickUniqueWorkspaceName` | Injected name generator that returns a colliding name twice, then a unique name | Third attempt succeeds; workspace row inserted | integration |
| 3.4 | PENDING | `src/state/signup_bootstrap.zig:bootstrapPersonalAccount` | Fault-inject: workspace INSERT succeeds, credit_state INSERT fails | Full rollback — zero rows committed; error propagates | integration |

### §4 — Error Registry + OpenAPI

**Status:** PENDING

Add two new error codes to the registry and the OpenAPI spec. Codes + handler line up with section 2 dimensions.

**Dimensions:**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `src/errors/error_entries.zig` | Compile-time registry validation | `UZ-WH-010` + `UZ-WH-011` hints are provider-neutral (mention "Webhook" not "Slack"); no duplicate codes | unit (comptime) |
| 4.2 | PENDING | `public/openapi.json` | `make check-openapi-errors` | Zero errors — `/v1/webhooks/clerk` documented with 200/400/401 responses mapping to `UZ-WH-010`/`UZ-WH-011`/`UZ-REQ-001` | contract |

---

## Interfaces

**Status:** PENDING

### Public Functions

```zig
// src/zombie/svix_verify.zig
pub fn verifySvix(
    secret: []const u8,       // e.g. "whsec_<base64>"
    svix_id: []const u8,      // Svix-Id header
    svix_ts: []const u8,      // Svix-Timestamp header (unix seconds)
    svix_sig: []const u8,     // Svix-Signature header — space-separated v1,<base64> entries
    body: []const u8,         // raw request body
    now_unix: i64,            // time injection for tests
    max_drift_seconds: i64,   // freshness window (default 300)
) bool;

// src/state/signup_bootstrap.zig
pub const BootstrapParams = struct {
    oidc_subject: []const u8,
    email: []const u8,
    display_name: ?[]const u8 = null,
};

pub const Bootstrap = struct {
    user_id: []u8,
    tenant_id: []u8,
    workspace_id: []u8,
    workspace_name: []u8,
    created: bool,

    pub fn deinit(self: *Bootstrap, alloc: std.mem.Allocator) void;
};

pub fn bootstrapPersonalAccount(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    params: BootstrapParams,
) !Bootstrap;

// src/state/heroku_names.zig
pub fn generate(alloc: std.mem.Allocator) ![]u8;  // "{adj}-{noun}-{3digit}" e.g. "jolly-harbor-482"
```

### Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| Svix-Id | string | matches `msg_[A-Za-z0-9]+` | `msg_2NBvO4nKQ3xYz8A` |
| Svix-Timestamp | string | unix seconds, within `[now - 300, now + 300]` | `1776700000` |
| Svix-Signature | string | space-separated `v1,<base64>` entries; at least one must verify | `v1,XyZ... v1,abc...` |
| body.data.id | string | Clerk user id, non-empty | `user_2aXy3zQ` |
| body.data.email_addresses | array | non-empty; resolves via `primary_email_address_id` | `[{id: ..., email_address: ...}]` |
| body.data.first_name | string? | optional, ≤ 64 chars when present | `"Jane"` |

### Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `workspace_id` | string (UUIDv7) | 200 OK | `"0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"` |
| `workspace_name` | string | 200 OK | `"jolly-harbor-482"` |
| `created` | bool | 200 OK | `true` on fresh bootstrap, `false` on replay |

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Svix signature invalid | Handler rejects pre-transaction | 401 `UZ-WH-010` — "Invalid Clerk webhook signature" |
| Svix timestamp stale (>5 min drift) | Handler rejects pre-transaction | 401 `UZ-WH-011` — "Clerk webhook timestamp out of window" |
| Malformed JSON body | Handler rejects pre-transaction | 400 `UZ-REQ-001` — "Malformed JSON body" |
| Missing primary email | Handler rejects pre-transaction | 400 `UZ-REQ-001` — "Primary email address required" |
| DB unavailable | Tx never opens | 500 `UZ-INTERNAL-001`; Clerk retries |
| Heroku-name collision exhaustion (8 retries) | Tx rolls back | 500 `UZ-INTERNAL-003`; Clerk retries |
| Unique-constraint race (two concurrent webhooks, same oidc_subject) | First wins; second rolls back | Second caller gets 500; Clerk retries; next replay returns `created:false` |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Svix sig invalid | Attacker POSTs forged payload | 401, zero DB writes, log warn with `req_id` | 401 `UZ-WH-010` returned to attacker |
| Svix ts stale | Replay attack OR clock skew > 5 min | 401, zero DB writes | 401 `UZ-WH-011` |
| DB pool exhausted | API under load | 500, Clerk retries | 500 `UZ-INTERNAL-001`; Clerk backs off and redelivers |
| Partial commit via crash | API process killed mid-transaction | Postgres rolls back on connection close | Next delivery retries cleanly |
| Platform LLM keys missing on first signup | Primer (M11_008) has not run | Bootstrap still commits (user workspace created with 0 credits); user sees empty workspace on first login | — (no LLM call attempted at signup; silent until first zombie run) |
| `CLERK_WEBHOOK_SECRET` env var missing | Misconfiguration | Handler fails closed with 500 | 500 `UZ-INTERNAL-003` with operator-only hint; Clerk retries |

**Platform constraints:**

- **M28 Svix middleware assumes zombie-scoped secret lookup via `core.zombies.webhook_secret_ref` → vault.** Our `/v1/webhooks/clerk` has no zombie — it reads `env.CLERK_WEBHOOK_SECRET` directly. This is why the extraction of `svix_verify.zig` is load-bearing.
- **Clerk webhook secret rotation requires a Fly machines restart** (env vars are baked in at boot). This is acceptable at the rotation cadence (~yearly).
- **Clerk retries on any non-2xx** with exponential backoff over ~24 hours. We must never return 5xx for logic errors that won't self-heal — only for transient infra failures.

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| `src/crypto/svix_verify.zig` ≤ 120 lines | `wc -l src/zombie/svix_verify.zig` ≤ 120 |
| `src/http/handlers/clerk_webhook.zig` ≤ 350 lines | `wc -l` ≤ 350 |
| `src/state/signup_bootstrap.zig` ≤ 350 lines | `wc -l` ≤ 350 |
| Constant-time HMAC compare (RULE CTM) | `grep -n "constantTimeEql" src/zombie/svix_verify.zig` — must be used for the HMAC output comparison |
| Bootstrap transaction commits 6 rows or zero rows | Dim 3.4 fault-injection test |
| Replay returns `created:false` with zero new writes | Dim 3.2 |
| No `attachAllPlatformDefaults` or `workspace_providers` references | `grep -rn "workspace_providers\|attachAllPlatformDefaults" src/` — 0 matches |
| Handler uses `conn.exec()` for INSERTs (avoids drain obligation) | `grep -n "conn.query" src/state/signup_bootstrap*.zig` — 0 matches OR `.drain()` present in same fn |
| Cross-compiles x86_64-linux + aarch64-linux | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | Every registered error code resolves in the REGISTRY | `src/errors/error_registry.zig` comptime loop — compile fails if `UZ-WH-010` or `UZ-WH-011` is declared but missing from ENTRIES |
| 2 | No duplicate error codes | Same comptime loop's pair-wise check |
| 3 | Svix signature compare is constant-time | `src/crypto/svix_verify.zig` uses `constantTimeEql` — reviewed in PR + grep enforced via Implementation Constraints |
| 4 | Bootstrap is transactional (BEGIN / COMMIT or ROLLBACK) | `signup_bootstrap.zig` wraps the multi-INSERT in `conn.exec("BEGIN")` / `conn.exec("COMMIT")`; `errdefer` calls ROLLBACK on any error |

---

## Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| svix: valid signature accepted | 1.1 | `svix_verify.verifySvix` | Generated HMAC over `{id}.{ts}.{body}` | `true` |
| svix: tampered body rejected | 1.2 | `svix_verify.verifySvix` | Valid sig computed over body A, called with body B | `false` |
| svix: stale timestamp rejected | 1.3 | `svix_verify.verifySvix` | `svix_ts` = now − 301s | `false` |
| svix: future timestamp rejected | 1.3 | `svix_verify.verifySvix` | `svix_ts` = now + 301s (pre-sign attack) | `false` |
| svix: multi-sig rotation — second entry verifies | 1.1 | `svix_verify.verifySvix` | `v1,<bad> v1,<good>` | `true` |
| error registry: UZ-WH-010 / UZ-WH-011 hints provider-neutral | 4.1 | `error_registry_test.zig` | `lookup("UZ-WH-010").hint` + `lookup("UZ-WH-011").hint` | Hint text does not contain "Slack"; does contain "Webhook" |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| clerk webhook: happy path bootstrap | 2.1 | Postgres | Svix-signed `user.created` body | 200; `core.tenants/users/memberships/workspaces` + `billing.workspace_credit_state/audit` all have one new row |
| clerk webhook: invalid signature 401 | 2.2 | Postgres | Same body, tampered sig | 401 `UZ-WH-010`; no DB writes |
| clerk webhook: stale timestamp 401 | 2.3 | Postgres | Svix-signed with ts 6 min old | 401 `UZ-WH-011`; no DB writes |
| clerk webhook: missing email 400 | 2.4 | Postgres | Body without primary email | 400 `UZ-REQ-001`; no DB writes |
| clerk webhook: svix middleware parity | 1.4 | Postgres | Valid-signed request to `/v1/webhooks/svix/{zombie_id}` | Same acceptance behavior as before extraction |
| bootstrap: fresh signup commits 6 rows | 3.1 | Postgres | Fresh `oidc_subject` | 6 rows across `core.*` and `billing.*` |
| bootstrap: replay returns existing | 3.2 | Postgres | Second call, same `oidc_subject` | `created:false`; no new rows |
| bootstrap: heroku name collision retries | 3.3 | Postgres + injected gen | Generator yields same name twice, then unique | Workspace inserted on 3rd attempt |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| clerk webhook: body size > 2MB | 2.4 | 3MB body | 413 `UZ-REQ-002` |
| bootstrap: exhaust name retries | 3.3 | Generator always returns same colliding name | `BootstrapError.WorkspaceNameCollisionExhausted`; 500 response |

### Edge Case Tests (boundary values)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| svix: timestamp exactly at drift boundary | 1.3 | `svix_ts = now - 300` | accepted (inclusive) |
| svix: timestamp one second past boundary | 1.3 | `svix_ts = now - 301` | rejected |
| bootstrap: email with no local part | 3.1 | `email = "@domain.com"` | Bootstrap succeeds with tenant name = "personal" |
| bootstrap: display_name null | 3.1 | `display_name = null` | Bootstrap succeeds; `core.users.display_name` is NULL |

### Regression Tests (pre-existing behavior that MUST NOT change)

| Test name | What it guards | File |
|-----------|---------------|------|
| svix middleware at `/v1/webhooks/svix/{zombie_id}` still accepts valid sigs | M28 behavior preserved after extraction | `src/auth/middleware/svix_signature_test.zig` (existing) |

### Leak Detection Tests

| Test name | Dim | What it proves |
|-----------|-----|---------------|
| bootstrap: Bootstrap struct deinit frees all allocations | 3.1 | `std.testing.allocator` detects zero leaks after `bootstrap.deinit(alloc)` |
| heroku_names: generate + free | 3.3 | `std.testing.allocator` detects zero leaks across 100 generate/free cycles |
| clerk webhook: handler arena reset | 2.1 | Request-scoped arena releases all allocations after handler returns |

### Spec-Claim Tracing

| Spec claim (from Overview/Goal) | Test that proves it | Test type |
|--------------------------------|-------------------|-----------|
| "atomically in a single SQL transaction" | Dim 3.4 fault-inject rolls back full state | integration |
| "idempotent on oidc_subject" | Dim 3.2 replay returns existing | integration |
| "Heroku-named, collision-safe" | Dim 3.3 injected-collision retry | integration |
| "invalid signature returns 401 with UZ-WH-010" | Dim 2.2 | integration |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Extract Svix primitives to `src/crypto/svix_verify.zig` + refactor middleware to call it | `zig build && zig build test -Dtest-filter="svix"` |
| 2 | Reword `UZ-WH-010` / `UZ-WH-011` hints to provider-neutral (reused for Clerk) | `zig build` (comptime validation passes) |
| 3 | Create `src/state/heroku_names.zig` | `zig build test -Dtest-filter="heroku"` |
| 4 | Create `src/state/signup_bootstrap_store.zig` + `signup_bootstrap.zig` | `zig build` |
| 5 | Create `src/state/signup_bootstrap_test.zig` (real-schema integration) | `make test-integration -Dtest-filter="signup_bootstrap"` |
| 6 | Add `clerk_webhook` route to router + route_table + invoke dispatch | `zig build && zig build test -Dtest-filter="router"` |
| 7 | Create `src/http/handlers/clerk_webhook.zig` | `zig build` |
| 8 | Create `src/http/handlers/clerk_webhook_integration_test.zig` | `make test-integration -Dtest-filter="clerk_webhook"` |
| 9 | Update `public/openapi.json` (new path + error codes) | `make check-openapi-errors` |
| 10 | Delete `docs/v2/pending/P1_UI_CLI_API_M11_003_INVITE_SIGNUP_ONBOARDING.md`; orphan sweep | `grep -rn "INVITE_SIGNUP_ONBOARDING" docs/ src/` returns 0 matches |
| 11 | Full gate: `make lint && make check-pg-drain && gitleaks detect && cross-compile both targets` | All pass |
| 12 | Tier-3 fresh DB: `make down && make up && make test-integration` | All green |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] `POST /v1/webhooks/clerk` with valid Svix signature creates tenant + user + membership + workspace + credit_state + audit rows — verify: Dim 2.1 integration test passes.
- [ ] Replay of the same `user.created` event returns `created:false` with no additional DB rows — verify: Dim 3.2.
- [ ] Invalid signature returns 401 `UZ-WH-010` with zero DB writes — verify: Dim 2.2.
- [ ] Stale timestamp (>5 min drift) returns 401 `UZ-WH-011` — verify: Dim 2.3.
- [ ] Heroku name collision retries up to 8 times — verify: Dim 3.3.
- [ ] Fault-injected credit_state write rolls back full bootstrap — verify: Dim 3.4.
- [ ] OpenAPI documents `/v1/webhooks/clerk` + both new error codes — verify: `make check-openapi-errors` passes.
- [ ] `src/auth/middleware/svix_signature.zig` still verifies `/v1/webhooks/svix/{zombie_id}` correctly after refactor — verify: Dim 1.4 regression test.
- [ ] All touched `.zig` files (non-test) ≤ 350 lines; `svix_verify.zig` ≤ 120 — verify: `wc -l`.
- [ ] Zero references to `workspace_providers` or `attachAllPlatformDefaults` — verify: `grep -rn "workspace_providers\|attachAllPlatformDefaults" src/`.
- [ ] Tier-3 fresh DB integration passes — verify: `make down && make up && make test-integration`.

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

```bash
# E1: Build
zig build 2>&1 | head -5; echo "build=$?"

# E2: Unit tests
zig build test 2>&1 | tail -10; echo "test=$?"

# E3: Tier-3 fresh-DB integration
make down && make up && make test-integration 2>&1 | tail -20

# E4: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E5: Drain + gitleaks
make check-pg-drain 2>&1 | tail -3
gitleaks detect 2>&1 | tail -3

# E6: Lint
make lint 2>&1 | tail -20

# E7: OpenAPI error contracts
make check-openapi-errors 2>&1 | tail -10

# E8: 350-line gate (exempts .md, tests, vendor/)
git diff --name-only origin/main \
  | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' 2>/dev/null \
  | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'

# E9: Dead code sweep — no references to deleted spec's symbols
grep -rn "INVITE_SIGNUP_ONBOARDING\|workspace_providers\|attachAllPlatformDefaults" src/ docs/v2/ 2>/dev/null
echo "E9: expected 0 non-historical hits"

# E10: Leak detection
zig build test 2>&1 | grep -i "leak" | head -5
echo "E10: (empty = pass)"
```

---

## Dead Code Sweep

**Status:** PENDING

**1. Files to delete.**

| File to delete | Verify deleted |
|---------------|----------------|
| `docs/v2/pending/P1_UI_CLI_API_M11_003_INVITE_SIGNUP_ONBOARDING.md` | `test ! -f docs/v2/pending/P1_UI_CLI_API_M11_003_INVITE_SIGNUP_ONBOARDING.md` |

**2. Orphaned references.**

| Deleted symbol or spec name | Grep command | Expected |
|----------------------------|--------------|----------|
| `INVITE_SIGNUP_ONBOARDING` | `grep -rn "INVITE_SIGNUP_ONBOARDING" docs/ src/` | 0 matches |
| `workspace_providers` (never existed; guard against re-introduction from m11 worktree draft) | `grep -rn "workspace_providers" src/` | 0 matches |
| `attachAllPlatformDefaults` (never existed; same guard) | `grep -rn "attachAllPlatformDefaults" src/` | 0 matches |

**3. `main.zig` test discovery.**

Add `_ = @import("state/signup_bootstrap_test.zig");`, `_ = @import("http/handlers/clerk_webhook_integration_test.zig");`, `_ = @import("zombie/svix_verify_test.zig");` where the test discovery lives (typically `src/main.zig` or a test index file — confirmed at EXECUTE time).

---

## Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit tests | `make test` | | |
| Integration tests | `make test-integration` | | |
| Tier-3 fresh DB | `make down && make up && make test-integration` | | |
| Leak detection | `zig build test \| grep leak` | | |
| Cross-compile x86 | `zig build -Dtarget=x86_64-linux` | | |
| Cross-compile arm | `zig build -Dtarget=aarch64-linux` | | |
| Lint | `make lint` | | |
| Drain | `make check-pg-drain` | | |
| Gitleaks | `gitleaks detect` | | |
| OpenAPI errors | `make check-openapi-errors` | | |
| 350L gate | `wc -l` (exempts .md, tests — RULE FLL) | | |
| Dead code sweep | `grep -rn "INVITE_SIGNUP_ONBOARDING\|workspace_providers"` | | |

---

## Out of Scope

- **Invites, access codes, redemption.** The original M11_003 spec's entire invite surface is deleted by this pivot. Superseded. No `billing.invites` schema, no `POST /v1/invites`, no `POST /v1/workspaces/{ws}/credits/redeem`, no code-validity endpoint.
- **Signup-automation zombies.** No `lead_capturer`, `concierge`, or `bootstrap_notifier`. No AgentMail webhook. No zombie chain runtime. No Resend call from the Clerk handler.
- **CLI signup flows.** No `zombiectl auth signup`, no `zombiectl credits redeem`. Terminal-first signup happens via the Clerk hosted sign-in page in a browser; CLI auth is a separate concern.
- **Web UI signup page.** The Clerk hosted sign-in page is sufficient for this milestone. `usezombie.com/sign-up` (if it becomes a custom-styled page) is a future UI milestone.
- **Auto-credit on signup.** Credit state is initialized to 0 cents. Granting N starter credits is **M11_011** (separate milestone).
- **Primer playbook.** Admin user + operations workspace + Fireworks platform_llm_keys provisioning is **M11_008** (separate milestone, depends on this spec landing).
- **Homelab zombie + samples restructure.** The v1→v2 Homelab pivot is **M11_012** (separate multi-milestone program with its own planning cycle).
- **Rate limiting / Clerk bot protection.** Deferred to v2+.
- **Signup notification to admin.** No Slack/email ping on new signups. If needed later, it's a log-tap on the handler, not a new system.
- **Clerk webhook secret rotation automation.** Rotation flow is op→Fly pipeline + restart — operational concern, not code.
- **Dashboard UI for new signups.** Lives in M12_001 / M27_001.
