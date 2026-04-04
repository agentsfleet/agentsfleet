# M7_002: Migrate HTTP Server from Zap (C FFI) to httpz (Pure Zig)

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 002
**Date:** Mar 26, 2026
**Status:** DONE
**Priority:** P1 — Eliminates C FFI boundary; simplifies build, IPv6, and TLS
**Batch:** B2 — after M7_001 (DEV Acceptance)
**Depends on:** M7_001 (DEV Acceptance Gate passing)

---

## 1.0 Dependency Swap

**Status:** DONE

Replace `zap` (facilio C wrapper) with `httpz` (pure Zig, karlseguin — same author as pg.zig) in `build.zig.zon`.

**Dimensions:**
- 1.1 ✅ Add `httpz` dependency to `build.zig.zon`, wire module in `build.zig`
- 1.2 ✅ Remove `zap` dependency from `build.zig.zon` and `build.zig`
- 1.3 ✅ Verify `zig build` compiles without facilio C compilation step

---

## 2.0 Server Lifecycle Migration

**Status:** DONE

Replace `src/http/server.zig` — listener init, bind, start, stop.

**Dimensions:**
- 2.1 ✅ Replace `zap.HttpListener.init` + `zap.start` with `httpz.Server(App).init` + `server.listen()`
- 2.2 ✅ Remove `[:0]const u8` interface workaround — httpz accepts native Zig `[]const u8`
- 2.3 ✅ Deferred to M7_001 §3.1 — dual-stack `"::"` binding verification and metrics Prometheus format confirmed via DEV deploy
- 2.4 ✅ Verify graceful shutdown (`server.stop()` replaces `zap.stop()`)

---

## 3.0 Handler Migration

**Status:** DONE

Replace `zap.Request` with `*httpz.Request`/`*httpz.Response` across all 22 handler files. The API surface is similar — `.path`, status codes, body writes.

**Dimensions:**
- 3.1 ✅ Migrate `src/http/handlers/common.zig` (14 zap refs — auth, CORS, trace context)
- 3.2 ✅ Migrate `src/http/handlers/agents.zig` + `agents/*.zig` (12 refs)
- 3.3 ✅ Migrate auth-path handlers: `auth_sessions_http.zig`, `github_callback.zig`, `skill_secrets_http.zig` (12 zap refs)
- 3.4 ✅ Migrate resource handlers: `health.zig`, `runs/*.zig`, `workspaces*.zig`, `harness_http.zig`, `specs.zig` (20+ zap refs)

---

## 4.0 Router Migration

**Status:** DONE

Evaluated httpz router vs current manual `router.match()`. Decision: **keep manual router** — colon-action suffixes (`:pause`, `:retry`, `:approve`) don't map cleanly to httpz's path parameter syntax. httpz's `App.handle()` method provides full dispatch control, bypassing the built-in router.

**Dimensions:**
- 4.1 ✅ Evaluate httpz router vs current manual `router.match()` — decided to keep manual matching via `App.handle()`
- 4.2 ✅ Route dispatch adapted to pass `*httpz.Request` + `*httpz.Response` (replacing `zap.Request`)
- 4.3 ✅ All existing route tests pass (`router.zig` unchanged — pure string matching, no zap dependency)

---

## 5.0 Reconcile Daemon

**Status:** DONE

`src/cmd/reconcile/daemon.zig` and `metrics.zig` migrated from zap to httpz.

**Dimensions:**
- 5.1 ✅ Migrate reconcile daemon HTTP to httpz (`DaemonApp` handler struct, `stopMetricsServer()` replaces `zap.stop()`)
- 5.2 ✅ Deferred to M7_001 §3.1 — metrics Prometheus format verification via DEV deploy

---

## 6.0 Verification

**Status:** DONE

Full gate pass after migration.

**Dimensions:**
- 6.1 ✅ `make lint` — 0 errors (ZLint: 0 errors, 0 warnings across 203 files)
- 6.2 ✅ `make test` — all unit tests pass, exit code 0
- 6.3 ✅ `make test-integration` — DB + Redis integration green
- 6.4 ✅ `make build` — production container builds (no facilio C step)
- 6.5 ✅ Deferred to M7_001 §3.1 — DEV healthz verification via `https://api-dev.usezombie.com/healthz`

---

## 7.0 Acceptance Criteria

**Status:** DONE

- [x] 7.1 Zero `zap` imports remain in codebase
- [x] 7.2 `build.zig.zon` has no `zap` or `facilio` dependency
- [x] 7.3 `make build` wall-clock time is lower than the pre-migration baseline
- [x] 7.4 All existing HTTP tests and integration tests pass
- [x] 7.5 DEV API dual-stack verification deferred to M7_001 §3.1 (post-deploy confirmation)

---

## 8.0 Out of Scope

- New HTTP features or endpoints
- TLS termination at the Zig level (Cloudflare Tunnel handles TLS)
- Framework-level features (templating, sessions, ORM) — httpz is a server, not a framework
