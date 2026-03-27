# M7_005: Migrate HTTP Server from Zap (C FFI) to httpz (Pure Zig)

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 005
**Date:** Mar 26, 2026
**Status:** PENDING
**Priority:** P1 — Eliminates C FFI boundary; simplifies build, IPv6, and TLS
**Batch:** B3 — after M7_001 (DEV Acceptance) and M7_004 (Config Alignment)
**Depends on:** M7_001 (DEV Acceptance Gate passing)

---

## 1.0 Dependency Swap

**Status:** PENDING

Replace `zap` (facilio C wrapper) with `httpz` (pure Zig, karlseguin — same author as pg.zig) in `build.zig.zon`.

**Dimensions:**
- 1.1 PENDING Add `httpz` dependency to `build.zig.zon`, wire module in `build.zig`
- 1.2 PENDING Remove `zap` dependency from `build.zig.zon` and `build.zig`
- 1.3 PENDING Verify `zig build` compiles without facilio C compilation step

---

## 2.0 Server Lifecycle Migration

**Status:** PENDING

Replace `src/http/server.zig` — listener init, bind, start, stop.

**Dimensions:**
- 2.1 PENDING Replace `zap.HttpListener.init` + `zap.start` with `httpz.Server().init` + `server.listen()`
- 2.2 PENDING Remove `[:0]const u8` interface workaround — httpz accepts native Zig `[]const u8`
- 2.3 PENDING Verify dual-stack `"::"` binding works natively (no `IPV6_V6ONLY` C-layer concern)
- 2.4 PENDING Verify graceful shutdown (`server.stop()` replaces `zap.stop()`)

---

## 3.0 Handler Migration

**Status:** PENDING

Replace `zap.Request` with `httpz.Request`/`httpz.Response` across all 22 handler files. The API surface is similar — `.path`, status codes, body writes.

**Dimensions:**
- 3.1 PENDING Migrate `src/http/handlers/common.zig` (14 zap refs — auth, CORS, trace context)
- 3.2 PENDING Migrate `src/http/handlers/agents.zig` + `agents/*.zig` (12 refs)
- 3.3 PENDING Migrate auth-path handlers: `auth_sessions.zig`, `github_callback.zig`, `skill_secrets.zig` (12 zap refs)
- 3.4 PENDING Migrate resource handlers: `health.zig`, `runs.zig`, `workspaces.zig`, `billing.zig`, `harness.zig`, `specs.zig` (20+ zap refs)
- 3.5 PENDING Migrate `src/http/workspace_guards.zig` (3 refs)

---

## 4.0 Router Migration

**Status:** PENDING

Replace manual path matching in `src/http/router.zig` with httpz's built-in router or keep manual matching (httpz supports both).

**Dimensions:**
- 4.1 PENDING Evaluate httpz router vs current manual `router.match()` — decide which to use
- 4.2 PENDING Migrate route definitions
- 4.3 PENDING Verify all existing route tests pass (`router.zig` has 4 test blocks)

---

## 5.0 Reconcile Daemon

**Status:** PENDING

`src/cmd/reconcile/daemon.zig` and `metrics.zig` use zap for a lightweight metrics HTTP endpoint.

**Dimensions:**
- 5.1 PENDING Migrate reconcile daemon HTTP to httpz
- 5.2 PENDING Verify metrics endpoint still serves Prometheus format

---

## 6.0 Verification

**Status:** PENDING

Full gate pass after migration.

**Dimensions:**
- 6.1 PENDING `make lint` — 0 errors
- 6.2 PENDING `make test` — all unit tests pass, no regressions
- 6.3 PENDING `make test-integration` — DB + Redis integration green
- 6.4 PENDING `make build` — production container builds (no facilio C step)
- 6.5 PENDING Deploy to DEV, verify `https://api-dev.usezombie.com/healthz` returns 200

---

## 7.0 Acceptance Criteria

**Status:** PENDING

- [ ] 7.1 Zero `zap` imports remain in codebase
- [ ] 7.2 `build.zig.zon` has no `zap` or `facilio` dependency
- [ ] 7.3 `make build` wall-clock time is lower than the pre-migration baseline (captured in §6.4)
- [ ] 7.4 All existing HTTP tests and integration tests pass
- [ ] 7.5 DEV API responds on dual-stack without C FFI workarounds

---

## 8.0 Out of Scope

- New HTTP features or endpoints
- TLS termination at the Zig level (Cloudflare Tunnel handles TLS)
- Framework-level features (templating, sessions, ORM) — httpz is a server, not a framework
