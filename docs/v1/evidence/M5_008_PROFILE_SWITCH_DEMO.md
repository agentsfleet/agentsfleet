# M5_008 Profile Switch Demo Evidence

**Workstream:** M5_008
**Date:** Mar 10, 2026
**Status:** DONE

## 1.0 Closure Evidence For 1.3 / 2.3 / 2.4 / 4.1 / 4.2 / 4.4

### 1.1 Immutable Compile/Activate/Run Linkage (1.3)

`Mar 10, 2026: 09:17 PM` — Added immutable audit artifact schema:

- `schema/009_profile_linkage_audit.sql`
- `schema/embed.zig`
- `src/cmd/common.zig` (canonical migrations now include version 9)

Contract implemented as synchronous DB writes in request path:
- Compile writes `COMPILE` artifact: `src/http/handlers/harness_control_plane/compile.zig`
- Activate writes `ACTIVATE` artifact: `src/http/handlers/harness_control_plane/activate.zig`
- Start-run writes `RUN` artifact: `src/http/handlers/runs/start.zig`
- Run query exposes linkage chain: `src/audit/profile_linkage.zig`, `src/http/handlers/runs/get.zig`

Immutability guard:
- `profile_linkage_audit_artifacts` has append-only triggers rejecting `UPDATE` and `DELETE`.

### 1.2 Targeted Integration Verification For New Linkage Contract

`Mar 10, 2026: 09:18 PM` — Ran targeted linkage immutability/queryability tests:

```bash
ZIG_GLOBAL_CACHE_DIR=$PWD/.tmp/zig-global-cache \
ZIG_LOCAL_CACHE_DIR=$PWD/.tmp/zig-local-cache \
zig build test --summary all -- --test-filter "integration: linkage chain is queryable for run"
```

Output:

```text
Build Summary: 9/9 steps succeeded; 111/111 tests passed
test success
+- run test zombied-tests 111 passed 1s MaxRSS:2M
```

```bash
ZIG_GLOBAL_CACHE_DIR=$PWD/.tmp/zig-global-cache \
ZIG_LOCAL_CACHE_DIR=$PWD/.tmp/zig-local-cache \
zig build test --summary all -- --test-filter "integration: linkage artifacts are immutable and reject updates"
```

Output:

```text
Build Summary: 9/9 steps succeeded; 111/111 tests passed
test success
+- run test zombied-tests 111 passed 42ms MaxRSS:2M
```

### 1.3 Canonical Verification Gate

`Mar 10, 2026: 09:19 PM` — Ran canonical gate:

```bash
make test
```

Output excerpt:

```text
Build Summary: 9/9 steps succeeded; 111/111 tests passed
✓ [zombied] test depth gate passed (unit=253 integration=70)
(pass) harness lifecycle: activate deterministically changes subsequent run snapshot linkage
(pass) harness lifecycle contract: API and CLI JSON expose profile identity parity fields
15 pass
0 fail
✓ Full test suite passed
```

## 2.0 Oracle API Review Evidence (Required)

### 2.1 Review Prompt Used

```text
Code review this refactor+feature for correctness and risks. Context: Zig backend. Goals were (1) split handler modules without changing entrypoint behavior, (2) add immutable synchronous audit linkage for COMPILE->ACTIVATE->RUN profile binding, (3) defer uuidv7. Please review for: SQL schema correctness (constraints/RLS/triggers), linkage insert/query logic, transactional consistency edge cases, memory safety/leaks, and test gaps. Return findings ordered by severity with concrete file:line pointers and minimal fix suggestions.
```

### 2.2 Oracle Execution Result

`Mar 10, 2026: 09:22 PM` — Executed Oracle API review with `claude-4.6-sonnet` multiple times (local binary and `npx` fallback).

Command family:

```bash
oracle --engine api --model claude-4.6-sonnet ...
npx -y @indykish/oracle --engine api --model claude-4.6-sonnet ...
```

Result from Oracle session logs (all attempts):

```text
OpenAI streaming call ended with an unknown transport error.
ERROR: Unexpected token '<', "<!DOCTYPE "... is not valid JSON
Transport: unknown — unknown transport failure
```

No model findings were returned because all API attempts failed before response payload delivery.

## 3.0 UUID Strategy Decision

- Decision: **defer UUIDv7 adoption** for `run_id`, `profile_version_id`, `compile_job_id`.
- Rationale: M5_008 closure needed deterministic immutable linkage now; UUIDv7 migration has non-trivial compatibility/migration surface and no production DB pressure yet.
- Follow-up spec item created: `docs/spec/v1/M6_007_UUIDV7_ID_MIGRATION_PLAN.md`.

## 4.0 Artifact References

- `schema/009_profile_linkage_audit.sql`
- `schema/embed.zig`
- `src/cmd/common.zig`
- `src/audit/profile_linkage.zig`
- `src/http/handlers/runs/*.zig`
- `src/http/handlers/harness_control_plane/*.zig`
- `docs/spec/v1/M5_008_DYNAMIC_AGENT_PROFILE_END_TO_END_WORKFLOW.md`
- `docs/spec/v1/M6_007_UUIDV7_ID_MIGRATION_PLAN.md`
