# M52_001: Bun Vendor Utilities — JSONLineBuffer, copy_file, object_pool, unbounded_queue

**Prototype:** v2.0.0
**Milestone:** M52
**Workstream:** 001
**Date:** Apr 26, 2026
**Status:** DEFERRED — investigated and rolled back on Apr 26, 2026. Speculative vendoring without concrete callsites violates this milestone's own "speculative vendoring rots" rule. See **Investigation outcome (Apr 26, 2026)** at the bottom of this spec for the codebase grep, the four findings, and the explicit re-trigger conditions.
**Priority:** P2 — secondary/tooling. Quality-of-implementation upgrade for hot paths (Redis stream parsing, artifact I/O, buffer reuse, worker control batching). Non-blocking for v2.0 launch; pulls forward correctness + perf wins that land cleanly in std-only Zig.
**Categories:** API
**Batch:** B1 — independent of all in-flight worker/auth/billing milestones; safe to land in parallel.
**Branch:** feat/m52-bun-vendor-utilities
**Depends on:** none. M40 (worker substrate) already vendored 5 Bun files at `src/sys/`, `src/util/strings/` — this milestone follows the same vendor pattern.

---

## Overview

**Goal (testable):** Four standalone utility modules from `oven-sh/bun` (MIT) are vendored into `src/util/` as snake-case files, each with a passing unit-test sibling, no remaining `bun.*` imports, and at least one production callsite migrated to demonstrate the win. `make lint`, `make test`, and cross-compile (x86_64-linux + aarch64-linux) all green.

**Problem:** usezombie has hand-rolled equivalents — or std-only patterns that lose efficiency — for four common substrate concerns:

1. **Newline-delimited stream parsing.** Redis RESP framing (`src/queue/redis_protocol.zig`) and OTEL line-export (`src/observability/otel_export.zig`) repeatedly memcopy/realloc their backing buffers. No std primitive offers one-pass scan + lazy compaction.
2. **File copy.** Artifact / log-spool moves use the read-write fallback path even on Linux ≥ 5.3 where `copy_file_range` is free; macOS lacks `clonefile` shortcut. No std primitive picks the fast path automatically.
3. **Object reuse.** Per-request response buffers, parsed-JSON scratch buffers, and DB row scratch slabs are freshly allocated per request. Std offers no generic object pool with reset hook.
4. **Lock-free MPMC queue.** Worker-watcher control-stream batching (`src/cmd/worker_watcher.zig`) takes per-message locks on the runtime map. A lock-free queue lets the dispatch path batch control messages and amortize lock acquisition. Std offers nothing in this shape.

**Solution summary:** Vendor four Bun files into `src/util/` with snake_case names, strip all `bun.*` imports to std equivalents, add `_test.zig` siblings, and migrate one concrete callsite per module to prove fit. Files stay under the 350-line / 50-fn-line gate. Each file carries an `// Vendored from oven-sh/bun <commit-sha> @ <path>` header comment for provenance.

**Non-goals:**
- No new abstractions, no convenience wrappers around the vendored APIs.
- No cross-callsite refactor sweep — one production migration per module proves fit; broader adoption is a follow-up workstream if usage justifies.
- No vendoring of `Progress.zig`, `pointer_info.zig`, `raw_ref_count.zig`, or `comptime_string_map.zig` — see Discovery / Out-of-Scope.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal repo discipline.
- `docs/ZIG_RULES.md` — every Zig touch: drain/dupe/errdefer chain, ownership encoding, sentinel collision, cross-compile, TLS, memory.
- File & Function Length Gate — 350 file / 50 fn / 70 method. `Progress.zig` exceeded this and was deferred for that reason.
- Milestone-ID Gate — no `M52_*` / `§*` references in `*.zig`. Vendor header comments cite the bun commit sha, never our milestone id.
- Verification Gate — `make lint`, `make test`, cross-compile both linux targets. Integration tests N/A — no schema or HTTP-handler touch in this workstream.
- Schema Table Removal Guard — N/A (no schema changes).
- License hygiene — confirm bun's `LICENSE` is MIT before vendoring; preserve attribution comment in each vendored file.

---

## Files Changed (blast radius)

| File | Action | Why |
|---|---|---|
| `src/util/json_line_buffer.zig` | NEW | Vendored from `bun/src/bun.js/JSONLineBuffer.zig`. One-pass scan + lazy compaction for newline-delimited buffers. std-only after strip. |
| `src/util/json_line_buffer_test.zig` | NEW | Unit tests: empty, partial line, multi-line, lazy compaction trigger, OOM unwind. |
| `src/util/copy_file.zig` | NEW | Vendored from `bun/src/copy_file.zig`. Cascading fast-path: ioctl_ficlone → copy_file_range → sendfile → read/write. Linux + macOS only; Windows path stripped. |
| `src/util/copy_file_test.zig` | NEW | Unit tests: small file, large file (≥ 1 MiB), empty file, target-exists overwrite, EXDEV cross-fs fallback. |
| `src/util/object_pool.zig` | NEW | Vendored from `bun/src/pool.zig`. Generic pool with reset hook, optional thread-safety, capacity bound. std-only after strip. |
| `src/util/object_pool_test.zig` | NEW | Unit tests: acquire/release cycle, capacity bound enforced, reset-hook called, single-thread + multi-thread paths. |
| `src/util/unbounded_queue.zig` | NEW | Vendored from `bun/src/threading/unbounded_queue.zig`. Lock-free MPMC queue for pointer-sized payloads + batching API. std-only after strip. |
| `src/util/unbounded_queue_test.zig` | NEW | Unit tests: push/pop one, batch push/pop, empty pop returns null, multi-producer multi-consumer stress (1k msgs, ≥ 4 threads). |
| `src/queue/redis_protocol.zig` | MODIFY | Replace ad-hoc line scanner with `JsonLineBuffer`. One callsite migration to prove fit. |
| `src/util/copy_file.zig` callsite | MODIFY (TBD) | Pick one of: artifact spool move, reconcile cache write, log rotation. Implementation step picks the cleanest; spec does not prescribe. |
| `src/cmd/worker_watcher.zig` | MODIFY | Replace per-message control-message lock+dispatch with batched drain through `UnboundedQueue`. Verify under `make memleak`. |
| `src/util/object_pool.zig` callsite | MODIFY (TBD) | Pick one of: HTTP response buffer pool, JSON encode scratch pool. Implementation step picks. |
| `THIRD_PARTY_NOTICES.md` (or equivalent) | NEW or MODIFY | Append bun MIT attribution + commit sha. Create if missing. |

---

## Sections

### §1 — Vendor + strip (no callsite migration yet)

Pull the four files in isolation. Each file lands with:

- Snake-case filename matching the table above.
- Header comment: `// Vendored from oven-sh/bun <commit-sha> @ <original/path.zig>` — first line of file, before `const std = @import("std");`.
- All `bun.*` imports stripped: `bun.assert` → `std.debug.assert`, `bun.ByteList` → `std.ArrayList(u8)`, `bun.Mutex` → `std.Thread.Mutex`, etc.
- Windows-specific code paths removed (Linux + macOS only).
- JSC / event-loop / mimalloc references removed entirely — if a function only exists for JSC interop, delete it.
- File ≤ 350 lines, every function ≤ 50 (≤ 70 if method).
- Each file has a sibling `_test.zig` with the test cases listed in the Test Specification.

DONE when: `zig build test` passes the new test files, `make lint` clean, cross-compile clean. No production callsites migrated yet.

### §2 — One production callsite per module

Each vendored module proves its fit by replacing one real callsite. Implementation picks the specific callsite from the candidates flagged in Files Changed. Constraints:

- The callsite must reduce LOC or eliminate a hand-rolled equivalent — not just rename one alloc to another.
- No behavior change observable from outside the module being modified — covered by existing tests.
- `make test-integration` passes if the callsite touches HTTP/DB/Redis code (worker_watcher migration certainly does).
- `make memleak` passes for the worker_watcher migration.

DONE when: four callsites migrated, all verification gates green.

### §3 — Attribution + provenance

- `THIRD_PARTY_NOTICES.md` lists the bun project, MIT license text (or link), commit sha vendored from, and the four file paths copied.
- Each vendored file's header comment matches the `THIRD_PARTY_NOTICES.md` sha.
- A short `docs/VENDORED.md` (or equivalent) describes the vendor policy: when to update from upstream, what counts as "vendored" vs "forked".

DONE when: an unfamiliar reader can trace any vendored line back to its bun origin in under 60 seconds.

---

## Interfaces (contract)

The four modules expose the signatures below. Names follow Bun's originals where unambiguous; rename to snake_case for consistency with the rest of `src/util/`. Field name `allocator` is canonical (not `alloc`, not `gpa`).

### `src/util/json_line_buffer.zig`

```zig
pub const JsonLineBuffer = struct {
    allocator: std.mem.Allocator,
    // ... internal: backing storage + scan/read offsets ...

    pub fn init(allocator: std.mem.Allocator) JsonLineBuffer;
    pub fn deinit(self: *JsonLineBuffer) void;

    pub fn write(self: *JsonLineBuffer, bytes: []const u8) !void;
    pub fn nextLine(self: *JsonLineBuffer) ?[]const u8; // borrow; valid until next write/compact
};
```

### `src/util/copy_file.zig`

```zig
pub const CopyFileError = error{ /* ... */ };
pub const CopyFileRangeResult = struct { bytes: u64, used_fast_path: bool };

pub fn copyFile(src_path: []const u8, dst_path: []const u8) CopyFileError!CopyFileRangeResult;
pub fn copyFileFd(src_fd: std.posix.fd_t, dst_fd: std.posix.fd_t) CopyFileError!CopyFileRangeResult;
```

No struct state; pure functions. Fast-path detection happens internally with a `std.atomic.Value(u8)` for kernel-support cache.

### `src/util/object_pool.zig`

```zig
pub fn ObjectPool(
    comptime T: type,
    comptime opts: struct {
        thread_safe: bool = false,
        max_capacity: ?usize = null,
        reset: ?*const fn (*T) void = null,
    },
) type {
    return struct {
        allocator: std.mem.Allocator,
        // ... internal free-list ...

        pub fn init(allocator: std.mem.Allocator) Self;
        pub fn deinit(self: *Self) void;

        pub fn acquire(self: *Self) !*T;
        pub fn release(self: *Self, item: *T) void;
    };
}
```

### `src/util/unbounded_queue.zig`

```zig
pub fn UnboundedQueue(comptime T: type) type {
    return struct {
        // T must be a pointer or pointer-shaped (verified at comptime).
        // Lock-free MPMC.

        pub fn init() Self; // no allocator — nodes are caller-owned
        pub fn deinit(self: *Self) void;

        pub fn push(self: *Self, item: T) void;
        pub fn pushBatch(self: *Self, head: T, tail: T, count: usize) void;
        pub fn pop(self: *Self) ?T;
        pub fn popBatch(self: *Self) ?Batch; // drains queue; iterate Batch
    };
}
```

---

## Invariants (must hold across this workstream)

1. **No `bun.*` imports in vendored files.** Self-audit grep before COMMIT: `git diff --name-only HEAD | xargs grep -nE '^\s*const\s+bun\s*=|@import\("bun"\)' | head` returns empty for paths under `src/util/`.
2. **Allocator field is named `allocator`.** Self-audit grep: `grep -nE '\b(alloc|gpa|arena)\s*:\s*(std\.mem\.Allocator|Allocator)' src/util/*.zig` returns empty.
3. **`init` takes allocator first, `deinit` takes `*Self`.** Both Bun and our existing convention.
4. **errdefer unwinds partial init.** Any `init` doing >1 allocation has matching errdefer per step. Verified by reading the diff and by an OOM-injection test in §1.
5. **File length gate satisfied.** Each vendored `.zig` ≤ 350 lines; each fn ≤ 50; method ≤ 70.
6. **License attribution present.** Header comment + `THIRD_PARTY_NOTICES.md` entry both reference the same upstream commit sha.

---

## Failure Modes (acknowledged, documented)

- **Bun upstream changes the file's API.** Vendor freeze: we own the snapshot, we update on demand. The header comment + `THIRD_PARTY_NOTICES.md` sha lets us diff later.
- **Strip introduces a subtle bug.** Mitigated by (a) ports the test cases bun ships when possible, (b) requires one production callsite migration per module, (c) `make memleak` for the worker_watcher migration.
- **Object pool capacity bound is wrong for our workload.** §2 callsite migration measures, not assumes; revisit cap after a soak window.
- **UnboundedQueue races on aarch64 / weak-memory.** Bun's tests cover this; our `_test.zig` ports the multi-producer stress; cross-compile aarch64 + run on dev host before declaring done.

---

## Test Specification

| ID | Module | Claim under test |
|---|---|---|
| T1 | json_line_buffer | Empty buffer → `nextLine` returns null. |
| T2 | json_line_buffer | Single full line written, `nextLine` returns the line minus terminator, second `nextLine` returns null. |
| T3 | json_line_buffer | Partial line written then completed by a second `write` — line surfaces only after terminator arrives. |
| T4 | json_line_buffer | After draining N lines, internal compaction triggers (asserted via offset reset). |
| T5 | json_line_buffer | `init` + immediate `deinit` leaks nothing under `std.testing.allocator`. |
| T6 | copy_file | Small file (< 4 KiB) copies byte-for-byte. |
| T7 | copy_file | Large file (1 MiB random bytes) copies byte-for-byte. |
| T8 | copy_file | Empty file copies and produces zero-byte target. |
| T9 | copy_file | Target exists → overwritten cleanly. |
| T10 | copy_file | Cross-fs fallback path (synthesized via tmpfs vs main fs when available; otherwise marked skip). |
| T11 | object_pool | Acquire then release returns same pointer on next acquire. |
| T12 | object_pool | `max_capacity` bound: pool refuses to retain beyond cap; over-cap items are freed on release. |
| T13 | object_pool | `reset` hook called on every release. |
| T14 | object_pool | `thread_safe=true` survives 4-thread × 1k-cycle stress without leaks. |
| T15 | unbounded_queue | Push one, pop one returns same pointer; second pop returns null. |
| T16 | unbounded_queue | `pushBatch` of N items drains in order via N `pop`s. |
| T17 | unbounded_queue | 4-producer × 4-consumer × 10k-msg stress: every pushed item is popped exactly once. |
| T18 | unbounded_queue | `popBatch` drains entire queue atomically; concurrent push during drain may or may not be included (document which). |
| T19 | worker_watcher migration | Existing M40 lifecycle tests still pass post-migration. |
| T20 | redis_protocol migration | Existing protocol tests still pass post-migration. |

T1–T18 are unit tests under `src/util/*_test.zig`. T19–T20 reuse existing integration tests; the spec asserts they still pass, no new test file needed.

---

## Verification Plan

```bash
# §1 gates
make lint
zig build test                                   # tier 1, includes new util tests
zig build -Dtarget=x86_64-linux                  # cross-compile gate
zig build -Dtarget=aarch64-linux

# §2 gates (touch worker_watcher and redis_protocol)
make test-integration                            # tier 2
make down && make up && make test-integration    # tier 3 fresh DB
make memleak                                     # required: worker_watcher allocator wiring touched
make check-pg-drain                              # required: any *.zig touched

# §3 gates
test -f THIRD_PARTY_NOTICES.md                   # attribution exists
grep -c 'oven-sh/bun' THIRD_PARTY_NOTICES.md     # > 0
```

Final "Verified:" block in the CHORE(close) message must show all four gates green plus cross-compile success.

---

## Discovery / Out-of-Scope (deferred)

These were considered during the audit and intentionally rejected for this workstream:

- **`Progress.zig`** — operator-facing CLI is `zombiectl` (Node), not `zombied`. Server-side progress is best served by structured log lines, not TTY bars. Defer indefinitely; revisit only if an interactive `zombied repair`/`zombied seed` ships.
- **`comptime_string_map.zig`** — std now ships `std.StaticStringMap` which covers most cases. The remaining ~5–10% routing win is real but not visible at current load. Park as v2.x perf workstream if HTTP routing ever profiles hot.
- **`ptr/meta.zig`** + **`ptr/raw_ref_count.zig`** — no current callsite. Vendor when a real consumer (shared zombie runtime, multi-publisher event listener) demands it. Speculative vendoring rots.
- **`Mutex.zig` / `Condition.zig` / `Futex.zig`** — `std.Thread.*` covers usezombie's lock contention profile. Optimization, not necessity.
- **`ThreadPool.zig` / `work_pool.zig`** — too large (>1k LOC), too entangled with Bun's Event primitive. usezombie's per-zombie thread-spawn model is adequate for v2.0.
- **`watcher/**`** — all variants (kqueue, inotify, ReadDirectoryChangesW) require event-loop coupling. usezombie reloads via Redis control messages, not FS events.
- **`allocators/**`** — mimalloc-coupled or JSC-coupled; not portable.

---

## Implementing-agent prologue (read before EXECUTE)

1. **Confirm bun checkout.** `cd /Users/kishore/Projects/oss/bun && git rev-parse HEAD` → record this sha; it goes in every header comment + `THIRD_PARTY_NOTICES.md`.
2. **Read prior vendor PR.** `git log --oneline --all -- src/sys/errno.zig src/util/strings/string_joiner.zig` → land on commit `1441c7f2`. Mirror its style (header comment shape, where `THIRD_PARTY_NOTICES` lives if it does, deinit pattern).
3. **Pattern to mirror for struct shape.** Read `src/util/strings/string_builder.zig` end-to-end. That is the canonical example of a vendored struct with `allocator` field + `init`/`deinit`. Match its header comment + import order + naming.
4. **Strip pass per file.** Open each bun source side-by-side with the target file. Walk top-down: replace `bun.X` with std equivalent or delete the function. Keep the order Bun used; do not reorganize.
5. **Test pass per file.** Port any `test "..."` blocks bun ships in the same file. Add the test cases listed in the Test Specification table. Run `zig build test` per file before moving to the next module.
6. **Migration pass.** Pick one callsite per module from the candidates table. Smallest-blast-radius wins. Verify the existing tests for that callsite still pass.
7. **CHORE(close).** Spec moves `pending/` → `done/`, `Status: DONE`. New `<Update>` block in `/Users/kishore/Projects/docs/changelog.mdx` tagged `["Internal", "API"]` describing the four utilities at user-perceptible level (or skip if truly internal-only — call that out in PR Session Notes).

---

## Session Notes (filled by implementing agent during CHORE(close))

- Bun source commit: `<sha>`
- `make lint`: ✓ / ✗
- `make test`: ✓ / ✗ — N passed, M skipped
- `make test-integration` (tier 2): ✓ / ✗
- `make test-integration` (tier 3, fresh DB): ✓ / ✗
- `make memleak` (last 3 lines): `<paste>`
- `make check-pg-drain`: ✓ / ✗
- Cross-compile linux x86_64 / aarch64: ✓ / ✗
- `/write-unit-test` outcome: <pass / iterations / skip+reason>
- `/review` outcome: <pass / findings dispositioned / skip+reason>
- Migrated callsites: <list four>

---

## Investigation outcome (Apr 26, 2026) — DEFERRED

This milestone was opened, the four utilities were vendored and tested
(§1 work landed cleanly: `src/util/json_line_buffer.zig`,
`src/util/copy_file.zig`, `src/util/object_pool.zig`,
`src/util/unbounded_queue.zig`, plus sibling `_test.zig` files —
1097 LOC across 9 files, all unit tests green, cross-compile
x86_64-linux + aarch64-linux clean, lint clean). The §1 commit was
then **reverted** when §2 (callsite migration) discovery showed no
real callsite exists for any of the four modules in the current
codebase.

### Why the rollback

The spec's own §2 constraint says: *"The callsite must reduce LOC
or eliminate a hand-rolled equivalent — not just rename one alloc
to another."* A full grep across `src/` produced these findings:

| Module | Spec-prescribed callsite | Actual fit |
|---|---|---|
| **JsonLineBuffer** | `src/queue/redis_protocol.zig` (RESP framing) | ✗ — RESP uses CRLF terminators, parses through `std.Io.Reader.takeByte()`, not a buffered byte accumulator. No LF-streaming consumer in the Zig codebase today. `nullclaw` (LLM client) is a vendored build dep that handles its own streaming; M42 SSE substrate emits, doesn't parse; M48 BYOK doesn't add a streaming consumer. |
| **UnboundedQueue** | `src/cmd/worker_watcher.zig` (control batching) | ✗ — `worker_watcher.map_lock:62` guards a `std.AutoHashMap` of running zombies during dispatch, not a control-message queue. The HashMap lock is structural; swapping to UnboundedQueue removes a queue we don't have. The closest near-fit is `src/events/bus.zig` — but that bus has exactly one emitter (`src/state/policy.zig:31` emitting `"policy_event"`) and one consumer that just `log.info`s every event. Migrating it would be technically correct but performance-invisible — there's no contention to remove. The migration would also require switching `BusEvent` (currently a 352-byte value type in a preallocated 1024-slot ringbuffer) to heap-allocated nodes, *adding* allocator overhead per `emit()` and trading bounded drop-on-overflow for unbounded heap growth. Net change is plausibly worse, not better. |
| **object_pool** | HTTP response buffer pool / JSON encode scratch (TBD) | ✗ — `httpz` (vendored at `vendor/httpz/`) manages its own request/response buffers; we don't allocate response bytes per-request on a hot path. Per-handler JSON encode goes through `std.json.Stringify` against a request-scoped arena (`src/http/handlers/common.zig` patterns), not a hot reuse target. `executor/session.zig:124` uses `std.heap.ArenaAllocator` per session — replacing arena with pool would be a regression (different reuse semantics). Nothing currently allocates often enough on a hot path to make pooling visible without first introducing the workload. |
| **copy_file** | Artifact spool move / log rotation (TBD) | ✗ — `grep -rn "std\.fs\..*\.copyFile\|fs\.rename"` returns nothing in `src/`. `src/git/repo.zig` delegates worktree creation to the `git` subprocess. `src/cmd/reconcile/` writes to stdout, not files. Logs go through `std.log` to stderr. There is genuinely no file-copy caller in the Zig codebase today. |

### Re-trigger conditions

This milestone should be **re-opened** (move back to `pending/`) when
any of the following conditions materialize. Each names a concrete
callsite that the corresponding utility will service:

- **JsonLineBuffer** — re-open when **either** of these lands:
  - An LLM streaming consumer in Zig (e.g. a Zig-side Anthropic /
    OpenAI SSE client; would happen if `nullclaw` is replaced or if
    BYOK provider work moves the streaming layer in-process).
  - An SSE *consumer* on the server (e.g. relaying or fan-in from an
    upstream event source). M42 substrate is a producer, not a fit.
- **UnboundedQueue** — re-open when **either** of these lands:
  - A high-emit telemetry / audit layer with multiple producer
    threads contending on a single consumer thread. Today's
    `src/events/bus.zig` has one emitter and a near-noop consumer;
    if it grows to dozens of emitters or ships persistence/replay,
    the UnboundedQueue migration becomes worthwhile.
  - A worker-control batching path with **its own queue** (not a
    HashMap lock) — i.e. a producer/consumer pair where draining N
    items can amortize an expensive consumer-side operation.
- **object_pool** — re-open when **either** of these lands:
  - A profile (`make bench`) that shows allocator overhead in the
    request hot path or in JSON encode/decode.
  - A handler pattern that allocates and frees a fixed-shape buffer
    per request without a request-scoped arena.
- **copy_file** — re-open when **either** of these lands:
  - An artifact / spool / log-rotation pipeline that copies files
    between paths in Zig (not via subprocess).
  - A reconcile cache write that benefits from `copy_file_range` /
    FICLONE on Linux.

### Investigation deliverables (preserved in git history)

- `feat/m52-bun-vendor-utilities` branch, commit `5a028fc7`
  ("feat(util): vendor four bun utilities to src/util") — full §1
  vendoring with sibling tests, then reverted by the next commit.
  Available for cherry-pick if any re-trigger condition fires before
  the bun upstream changes shape significantly. Bun source commit
  vendored from: `dc578b12eca413e16b6bbea117ff24b73b48187f`.
- This spec retains the original Files Changed table, Interfaces
  contract, and Test Specification — all still valid as a
  resurrection blueprint when a re-trigger fires.

### Downstream milestone impact

- **M53_001 (API Hygiene Sweep)** — *no impact*. M53 "coordinates
  with" M52 conditionally; with M52 deferred, M53 audits the existing
  mutex pattern in `src/cmd/worker_watcher.zig` as canonical state.
- **M54_001 (Pub Surface and Ownership)** — *positively impacted*.
  M54 was blocked behind M52's four new `pub` APIs. With M52
  deferred, M54 unblocks immediately — no extra `pub` surface to
  audit, premise simpler.
- **M55_001 (String Utility Adoption)** — *no impact*. M55 covers
  three string utilities (StringBuilder/Joiner/SmolStr); orthogonal
  to M52's ObjectPool.
