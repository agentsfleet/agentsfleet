# M10_002: Reconcile And Ops Deep Module Refactor

**Prototype:** v1.0.0
**Milestone:** M10
**Workstream:** 2
**Date:** Mar 15, 2026
**Status:** DONE
**Priority:** P0 — maintainability and correctness hardening for runtime control surfaces
**Batch:** B2 — execute after M10_001 completion
**Depends on:** M10_001

---

## 1.0 Scope

**Status:** DONE

Refactor oversized runtime orchestration files into deep modules with explicit allocator ownership and unchanged runtime behavior.

**Dimensions:**
- 1.1 DONE Target files locked: `src/cmd/reconcile.zig`, `src/git/ops.zig`.
- 1.2 DONE Preserve all existing public API and CLI/runtime behavior.
- 1.3 DONE Extract cohesive submodules with clear ownership contracts at module boundaries.
- 1.4 DONE Move and expand tests per `skills/write-unit-test/SKILL.md` tier requirements.

---

## 2.0 Reconcile Refactor

**Status:** DONE

**Dimensions:**
- 2.1 DONE Split reconcile orchestration, idempotency logic, and side-effect boundaries into focused modules.
  - `src/cmd/reconcile/args.zig` — CLI/env parsing, ReconcileArgError, ReconcileMode, ReconcileArgs
  - `src/cmd/reconcile/state.zig` — DaemonState, g_daemon_state global, daemonHealthy
  - `src/cmd/reconcile/daemon.zig` — daemon lifecycle, signal handlers, leader lock, runDaemon
  - `src/cmd/reconcile/tick.zig` — reconcileTick, runOnce
  - `src/cmd/reconcile/emit.zig` — emitResult, pushOtelMetrics
  - `src/cmd/reconcile/metrics.zig` — HTTP /healthz + /metrics server, renderDaemonMetrics
  - `src/cmd/reconcile.zig` — thin orchestrator (run, openDbOrExit, integration tests)
- 2.2 DONE Preserve retry/state semantics and existing command contract.
- 2.3 DONE Added robust tests: T2 boundary staleness, T7 field regression parity, T3 unknown flag, plus all existing tests moved.
- 2.4 DONE Allocator/deinit pairing verified: env vars freed with defer in args, otel config freed with defer in emit, page_allocator sub-allocations freed in tick, DaemonState stack-allocated with g_daemon_state cleared via defer.

---

## 3.0 Ops Refactor

**Status:** DONE

**Dimensions:**
- 3.1 DONE Split git ops execution planning, state tracking, and error normalization into focused modules.
  - `src/git/errors.zig` — shared GitError error set (breaks import cycle)
  - `src/git/validate.zig` — isSafeIdentifierSegment, isSafeGitRef, isSafeRelativePath, isSafeWorktreeDirName
  - `src/git/command.zig` — CommandResources, sanitizedChildEnv, copyEnvIfPresent, run
  - `src/git/repo.zig` — WorktreeHandle, RuntimeCleanupStats, ensureBareClone, createWorktree, removeWorktree, cleanupRuntimeArtifacts, commitFile, push, remoteBranchExists
  - `src/git/pr.zig` — HttpResponseParts, splitHttpResponse, parseHttpStatus, parseGitHubOwnerRepo, createPullRequest, findOpenPullRequestByHead
  - `src/git/ops.zig` — thin re-export facade (explicit pub const re-exports for all public symbols)
- 3.2 DONE Preserve existing command behavior and output contracts.
- 3.3 DONE Added T2 single-char ref, T2 single-segment path, T3 commitFile invalid path, T3 push invalid ref, T3 createPullRequest invalid base_branch tests.
- 3.4 DONE Allocator/deinit pairing verified: stdout/stderr owned by CommandResources freed in deinit, takeStdout transfers ownership, caller frees run() return slice.

---

## 4.0 Verification Standard

**Status:** DONE

Follow `skills/write-unit-test/SKILL.md` with mandatory coverage tiers on touched surfaces:

- T1 Happy path
- T2 Edge cases
- T3 Negative/error paths
- T6 Integration coverage (where supported)
- T7 Regression behavior parity

**Dimensions:**
- 4.1 DONE Baseline-first: build and test confirmed clean before edits, then after.
- 4.2 DONE Targeted tests added for reconcile (T2 boundary, T7 parity, T3 unknown flag) and ops (T2 single-char ref, T2 single-segment path, T3 three new error-path tests).
- 4.3 DONE `zig build test --summary all` → 200 passed, 67 skipped (DB integration tests require HANDLER_DB_TEST_URL); `make test-depth` → unit=463, integration=99, gate passed.
- 4.4 DONE Allocator boundary verification: all defer-free patterns confirmed, no retained slices across module boundaries.

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 `reconcile.zig` refactored into deep modules with behavior parity.
- [x] 5.2 `ops.zig` refactored into deep modules with behavior parity.
- [x] 5.3 Allocator ownership boundaries explicit and verified in touched paths.
- [x] 5.4 Tests are robust per `skills/write-unit-test/SKILL.md` and gate is green.

---

## 6.0 Refactor workspace_billing

- `src/state/workspace_billing.zig`
