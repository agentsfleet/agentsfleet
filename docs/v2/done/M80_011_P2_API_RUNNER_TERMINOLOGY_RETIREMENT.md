# M80_011 — Executor → runner terminology retirement + dead-metrics deletion

**Status:** DONE
**Branch:** feat/m80-011-executor-name-retirement
**Priority:** P2 (hygiene / naming debt)
**Milestone:** M80 (runner fleet)

## Goal

The M80_002 cutover folded the `zombied-executor` sidecar into the host-resident
`zombie-runner` (the parent control loop + NullClaw execution linked in directly).
The *binary* is gone, but the **name** `executor` lingered across the codebase —
identifiers, log scopes, a failure-class, an env var, a cgroup path, comments,
playbooks, architecture docs, and schema comments. Execution now *is* the runner,
so this retires the term in favour of `runner` (or `execution` where that is the
literal meaning), and deletes telemetry that the cutover orphaned.

## Problem

1. `executor`-named symbols imply a component that no longer exists, misleading
   readers about the architecture (one binary, two process roles — parent + forked
   sandboxed child — not a separate sidecar).
2. `runner_metrics.zig` (the old per-execution counter module) is **dead**: written
   by the runner but read only by tests. The runner binary exposes no `/metrics`
   endpoint, and the `zombie_executor_*` series were dropped from `zombied`'s render
   at the cutover, so the counters never leave the process. Real telemetry rides
   `ExecutionResult` → `report`, independent of this module.
3. `build_runner.zig` carried two always-false, unconsumed build options
   (`executor_harness`, `executor_provider_stub`) — vestigial since the M42
   redaction-harness path (`src/executor/`) was deleted.

## Files Changed (blast radius)

~67 files: `src/runner/engine/*`, `src/lib/contract/*`, `src/zombied/{errors,observability,state,zombie}/*`, `build.zig`, `build_runner.zig`, `deploy/baremetal/*`, `playbooks/*`, `samples/platform-ops/*`, `docs/architecture/*`, `schema/*.sql` (comments only), `ui/packages/*`, `zombiectl/src/constants/event-status.ts`, `tests/bench/micro.zig`. Deletes `src/runner/engine/runner_metrics.zig`.

## Sections

### §1 — Terminology rename — **DONE**

Every live `executor` reference → `runner`/`execution`. Contract-bearing renames
(pre-v2, RULE NLG — no compat shim, values/semantics unchanged):

| From | To | Surface |
|---|---|---|
| `.executor_crash` (FailureClass) | `.runner_crash` | durable `core.zombie_events.failure_label` + `/metrics reason=` + error map + tests |
| `EXECUTOR_NETWORK_POLICY` | `RUNNER_NETWORK_POLICY` | runner sandbox network env (read at `sandbox_args.zig`; no in-repo setter) |
| `/sys/fs/cgroup/zombie.executor` | `/sys/fs/cgroup/zombie.runner` | runner cgroup base |
| log scopes `.executor_*` | `.runner_*` | structured-log scope names |

`UZ-EXEC-*` / `ERR_EXEC_*` error codes are **kept** (`EXEC` = execution; a wire
contract) — only their grouping comments were relabelled.

### §2 — Dead `runner_metrics` deletion (RULE NLR/NLG) — **DONE**

Deleted `src/runner/engine/runner_metrics.zig` and its call sites in `runner.zig`
and `cgroup.zig`; `incFailureMetric` (only fed the counters) went with it. The two
vestigial `build_runner.zig` options (`executor_harness`, `executor_provider_stub`)
were removed. Real telemetry (`token_count`, `memory_peak_bytes`, `cpu_throttled_ms`)
is untouched — it rides `ExecutionResult` → `daemon/loop.zig` → `report`.

## Acceptance Criteria

- `git grep -i executor` returns zero outside `vendor/`, `docs/v2/done/`, and `changelog`.
- `zig build` (zombied + runner) succeeds.
- `zig build -Dtarget=x86_64-linux` and `-Dtarget=aarch64-linux` succeed for both binaries.
- `zig build test` (zombied + runner unit suites) passes.
- No `/metrics` regression: `runner_metrics` had no production reader/emitter.

## Out of Scope

- Reviving runner-scoped metrics (owned by M80_007 via a Postgres path).
- The `worker` → `runner` rename of `zombied worker` (separate, M80_008).
- Public docs site (`~/Projects/docs`) — swept on its own branch/PR.
