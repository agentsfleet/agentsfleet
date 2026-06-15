# LIFECYCLE_PATTERNS — init/deinit/defer/errdefer convention for Zig

The contract for memory ownership and cleanup in Zig structs. Sister doc to `dispatch/write_zig.md` (which covers per-edit discipline) and `LOGGING_STANDARD.md` (which covers the cross-language wire format).

Pre-design rules, decisive defaults, and the anti-patterns each rule exists to prevent. Every rule has a one-line "why" so you can judge edge cases.

## §1 · Scope

Triggers on every `Edit`/`Write` that adds, removes, or reshapes lifecycle methods on a Zig struct:

- New `pub fn init(`
- New `pub fn deinit(`
- New `errdefer` or `defer` adjacent to an allocation
- New struct field that holds heap memory, an arena, or an opaque handle
- Refactor of an existing init/deinit pair
- Cross-thread allocator handoff

Out of scope:

- Stack-only structs with no allocation (zero-cost values; no lifecycle).
- Test-only mocks/stubs in `*_test.zig` that fake init/deinit for assertion purposes.

The **LIFECYCLE GATE** (`dispatch/write_zig.md`, lifecycle) sits on top of this file and audits init/deinit pairing + errdefer placement on every Zig edit. It is **not** a substitute for the **PUB GATE** (`dispatch/write_zig.md`, pub-surface) — see §11 for the carve-out.

## §2 · Today's de-facto convention (survey-derived)

Documented honestly, not aspirationally. As of this milestone:

- Init/deinit pairing is inconsistent. Some structs have both (`HttpServer.init` + `HttpServer.deinit`), some have init but no deinit (caller assumed-omniscient), a handful have deinit but no init (tail end of refactors).
- `errdefer` placement varies. Sometimes immediately after the alloc it protects (correct), sometimes batched at the bottom of init (fragile if init reorders), sometimes missing entirely (leaks on init failure).
- Allocator storage varies. Some structs store `allocator: std.mem.Allocator` (then `deinit(self: *Self) void`), others omit it (then `deinit(self: *Self, allocator: std.mem.Allocator) void`). Both work; the inconsistency makes call-site reading slow.
- Arena vs GPA boundaries are vague. A handful of subsystems mix per-allocation `defer` with arena `reset` in the same scope. Either pattern is fine alone; mixing is a leak class.
- Ownership transfer at the call site is undocumented. `MyStruct.init(alloc, slice)` may or may not take ownership of `slice`; readers must grep `init` body to find out.

The proposed convention below is what every new struct must conform to and what the fix-pass converges existing structs toward.

## §3 · Bun's convention (synthesized from `~/Projects/oss/bun/src`)

Reference patterns, with file:line citations:

**Heap-returned init with immediate errdefer** — `~/Projects/oss/bun/src/Watcher.zig:65-90`:

```zig
pub fn init(comptime T: type, ctx: *T, fs: *bun.fs.FileSystem, allocator: std.mem.Allocator) !*Watcher {
    const watcher = try allocator.create(Watcher);
    errdefer allocator.destroy(watcher);
    watcher.* = .{
        .fs = fs,
        .allocator = allocator,
        // ... more fields, some allocated
        .watch_events = try allocator.alloc(WatchEvent, max_count),
    };
    try Platform.init(&watcher.platform, fs.top_level_dir);
    return watcher;
}
```

Allocator stored in the struct. errdefer placed immediately after the first allocation. Subsequent allocations protected by the same errdefer (the destroy frees everything when init unwinds).

**Allocator-as-deinit-param (value type)** — `~/Projects/oss/bun/src/StaticHashMap.zig`:

```zig
pub fn deinit(self: *Self, gpa: std.mem.Allocator) void {
    gpa.free(self.slice());
}
```

Allocator NOT stored. Caller passes it at deinit time. Suitable when the struct is short-lived and the allocator is unambiguous in the caller's scope.

**Owned ownership wrapper** — `~/Projects/oss/bun/src/ptr/owned.zig:14-95`:

```zig
pub fn Owned(comptime Pointer: type) type { /* uses DefaultAllocator (ZST) */ }
pub fn Dynamic(comptime Pointer: type) type { /* stores std.mem.Allocator at runtime */ }
pub fn OwnedIn(comptime Pointer: type, comptime Allocator: type) type { /* generic */ }
```

When ownership semantics matter and a comment isn't enough, `Owned<T>` makes the ownership compile-time-explicit.

**`bun.sys.Error.deinit` — only after clone** — `~/Projects/oss/bun/src/sys/Error.zig:106-120`:

```zig
pub fn deinit(this: *Error) void {
    this.deinitWithAllocator(bun.default_allocator);
}

/// Only call this after it's been .clone()'d
pub fn deinitWithAllocator(this: *Error, allocator: std.mem.Allocator) void {
    if (this.path.len > 0) allocator.free(this.path);
    if (this.dest.len > 0) allocator.free(this.dest);
}
```

Idempotent (safe to call on a non-cloned Error — the `len > 0` check no-ops). Documentation calls out the pre-condition (`Only call this after it's been .clone()'d`).

## §4 · Our convention — `init` shape rules

**Decision matrix:**

| Struct lifetime / size | `init` signature | Reason |
|---|---|---|
| Heap-allocated, lives across function returns | `pub fn init(allocator: std.mem.Allocator) !*Self` | Caller stores the pointer; deinit knows where the struct lives. |
| Stack-allocated, fits on the caller's stack, no internal heap | `pub fn init() Self` (infallible) | No allocation, no failure mode. |
| Stack-allocated, internal heap (e.g. `ArrayList`) | `pub fn init(allocator: std.mem.Allocator) Self` (infallible) | Allocation deferred to first append; init cannot fail. |
| Stack-allocated, eager heap | `pub fn init(allocator: std.mem.Allocator) !Self` | Caller's stack holds the value; allocator stored in the struct or remembered by the caller. |

**Mandatory rules:**

1. **Allocator is the first parameter.** Always. Even when the struct is short-lived and won't store it.
2. **`errdefer` immediately after the first allocation.** Never batched. The Zig compiler reorders neither allocations nor cleanups; explicit-by-position is the convention.
3. **Every allocation that occurs after `errdefer` must be protected.** If init makes 3 allocations, you need either 1 errdefer that frees the wrapper struct (which then frees the children in deinit) OR 3 errdefers that free each individually as init unwinds.
4. **No partial state.** A struct that returns from init MUST be fully initialized — every field set, every invariant established. Half-initialized structs are a debugging hellmouth.
5. **Init cannot block on I/O for >100 ms unless the call is documented as blocking.** If init does network or filesystem work, the doc comment must say so. Synchronous init that secretly hits the network is a recurring incident class.

Example (heap-returned, allocator stored):

```zig
pub const Watcher = struct {
    allocator: std.mem.Allocator,
    fs: *FileSystem,
    events: []Event,

    pub fn init(allocator: std.mem.Allocator, fs: *FileSystem) !*Watcher {
        const self = try allocator.create(Watcher);
        errdefer allocator.destroy(self);

        const events = try allocator.alloc(Event, 64);
        errdefer allocator.free(events);

        self.* = .{
            .allocator = allocator,
            .fs = fs,
            .events = events,
        };
        return self;
    }
};
```

## §5 · Our convention — `deinit` shape rules

**Two valid signatures:**

| Signature | When | Notes |
|---|---|---|
| `pub fn deinit(self: *Self) void` | Allocator is stored in the struct (most common heap case). | Caller writes `var w = try Watcher.init(alloc, fs); defer w.deinit();` — clean. |
| `pub fn deinit(self: *Self, allocator: std.mem.Allocator) void` | Allocator NOT stored (short-lived value type, or struct is returned-by-value and the caller's scope owns the allocator). | Caller writes `defer thing.deinit(alloc);` — slightly noisier but explicit. |

**Mandatory rules:**

1. **`deinit` must free everything `init` allocated.** Lints catch the inverse (`init` allocates, `deinit` doesn't free) but not silently-correct cases (`init` allocates A and B, `deinit` frees only A — silent leak). Reviewer responsibility: walk the alloc list against the free list.
2. **`deinit` must be idempotent.** Calling it twice must not crash. Either set freed pointers to undefined (`self.events = &.{}` after free) or guard with `if (self.events.len > 0)`.
3. **`deinit` does not return errors.** Cleanup that *can* fail (closing a file, flushing a buffer) must be split into a separate explicit method (`pub fn close(self: *Self) !void`) called before `deinit`, or the failure must be logged and swallowed inside deinit.
4. **No partial deinit.** Once `deinit` runs, the struct is dead. Don't leave half-freed state for "the caller will free the rest" — that's a leak waiting to happen.
5. **Deinit order is reverse of init order.** Free children before freeing the parent that owns them. `errdefer` chains in init naturally produce the right order; mirror it in deinit.

Example matching the §4 init:

```zig
pub fn deinit(self: *Watcher) void {
    self.allocator.free(self.events);
    self.allocator.destroy(self);
}
```

## §6 · `defer` vs `errdefer` decision

**Rule:** `defer` for unconditional cleanup; `errdefer` for cleanup that should NOT run on success.

- `defer` runs on every exit from the scope (success or error).
- `errdefer` runs only when the scope exits via an `error` return.

**The most common bug** is `errdefer` left where `defer` belonged — produces a leak on the success path, because the cleanup never runs. Symptom: memleak gate flags it; trace shows a successful return path.

**The mirror bug** is `defer` left where `errdefer` belonged — produces a use-after-free, because the cleanup runs on success and frees memory the caller now owns.

**Decision tree:**

| Situation | Choice |
|---|---|
| Allocation that the function will own and dispose of regardless of outcome | `defer alloc.free(buf);` |
| Allocation that on success becomes the caller's responsibility (returned, stored in a struct field, transferred to a callback) | `errdefer alloc.free(buf);` — runs only if init unwinds before ownership transfers |
| Resource that must be released regardless (file descriptor, mutex, lock) | `defer file.close();` / `defer mutex.unlock();` |
| Resource only opened to facilitate init that becomes part of the returned struct | `errdefer file.close();` — and the struct's `deinit` closes it on success |

**Anti-pattern:** mixing `defer` and `errdefer` on the same allocation. If `defer` is correct, errdefer is dead code. If errdefer is correct, defer would double-free. Pick one.

## §7 · Ownership transfer at the boundary

When a function takes a slice/pointer and stores it in a struct, the call site must know who owns the memory after the call. Without convention this is grep-the-implementation; with convention it's call-site-readable.

**Convention:**

1. **`init`/`fromSlice`/`takeOwnership` semantics are explicit in the function name.** A function ending in `Borrowed` (`initBorrowed`) does not take ownership; the caller must outlive the struct.
2. **Doc-comment pattern.** When a function takes ownership, the doc comment leads with `/// Takes ownership of <param>. Caller MUST NOT free.`
3. **Call-site marker for ownership transfer.** When the call site transfers ownership, an inline comment marks it: `// owns: thing` immediately above the call. Forces a reader to confirm the transfer.

```zig
const events = try allocator.alloc(Event, 64);
// owns: events
const watcher = try Watcher.fromOwnedEvents(allocator, events);
// (no `defer allocator.free(events)` — watcher.deinit() does it)
```

**Anti-pattern:** silent transfers. `fn foo(slice: []u8) void` whose body squirrels `slice` into a global without warning the caller — bug magnet. If unavoidable (third-party API), wrap behind an `Owns<T>` newtype.

## §8 · Arena vs GPA boundaries

Two allocator kinds, two patterns. **Don't mix in the same scope.**

**Arena** (`std.heap.ArenaAllocator`):

- One allocator per request/task/document. Lifetime tied to the surrounding work.
- Per-allocation `defer alloc.free(...)` is **forbidden** inside an arena scope — it's wasted work; the arena `reset()` reclaims everything at scope exit.
- Cleanup is one `arena.reset()` or `arena.deinit()` at scope end.
- Use when: many small short-lived allocations whose total lifetime matches the scope.

**GPA / specific allocator** (`std.heap.GeneralPurposeAllocator`, `std.testing.allocator`, custom):

- Long-lived. May be shared across threads.
- Every `alloc` paired with `defer alloc.free(...)` (or `errdefer` per §6).
- Use when: allocations have varied lifetimes, or memory is shared across components with different scopes.

**Boundary rule:** when crossing from a GPA scope into a function expected to use an arena (e.g. parsing a request body), pass the arena explicitly:

```zig
pub fn handleRequest(gpa: std.mem.Allocator, req: Request) !Response {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();

    return parseAndDispatch(arena.allocator(), req); // child sees arena
}
```

Child functions receive the arena and don't know (or care) about the GPA above. They just allocate freely.

**Anti-pattern:** an arena leaking into a long-lived structure. Arena memory becomes invalid when the arena is destroyed; storing arena-allocated slices in a struct that outlives the arena is a use-after-free.

## §9 · RAII via struct method

The Zig idiom for owned resources at the call site:

```zig
var thing = try Thing.init(alloc);
defer thing.deinit();
// ... use thing ...
```

Anything else is a smell:

- `defer alloc.free(thing.buf)` — hides ownership in the caller; if `Thing` later grows a second allocation, the caller forgets to free it.
- Manual cleanup at function tail — works but unwinds wrong on early return / error.
- Ad-hoc cleanup in `errdefer` blocks at the call site — encodes init/deinit logic in two places.

The struct's `deinit` method is the single source of truth for cleanup. Call sites use `defer self.deinit()` (or `errdefer` if ownership might transfer mid-function) and never reach inside the struct.

## §10 · Anti-patterns flagged by `deinit-pairs.sh`

| Pattern | Severity | Fix |
|---|---|---|
| `pub fn init(` with no matching `pub fn deinit(` in the same struct | **blocking** | Add `deinit`, or remove `init` if the struct is value-type without resources. |
| `pub fn deinit(` with no matching `pub fn init(` | **blocking** | Likely tail of a refactor — restore init, or rename deinit to a non-lifecycle name (e.g. `close`, `release`). |
| `errdefer` at end of init scope (batched) | informational | Move adjacent to the allocation it protects. |
| `defer` and `errdefer` on the same allocation | **blocking** | Pick one. |
| `init` returning `*Self` without `errdefer alloc.destroy(self)` | **blocking** | The most common leak class on init failure. |
| `deinit` that calls allocator-storing struct's `allocator` field after some other field's `.deinit()` (use-after-free risk if children depend on parent's allocator) | informational | Reorder cleanup to free children first. |
| Stored allocator field never used in deinit | informational | Either remove the field or wire it. |

## §10A · Tightening clauses (closures of common skip rationalizations)

Failure modes the audit script and reviewer must close. These are **not aspirational** — each closes a specific way an agent could otherwise dodge the rule.

| # | Rationalization | Closure |
|---|---|---|
| LC1 | "Struct is small / one allocation, skip `deinit`" | Any heap allocation OR opaque handle (file descriptor, mutex, socket, GPU resource, lock) requires `deinit`. **No size threshold**. Audit script flags struct definitions with `allocator.alloc`/`alloc.create`/`std.fs.File.open` whose enclosing struct lacks `pub fn deinit`. |
| LC2 | "Renamed `deinit` to `close` / `release` to dodge pair audit" | Audit script treats `deinit`, `close`, `release`, `destroy`, `shutdown`, `free`, `dispose` as **lifecycle methods**. All require a paired `init` and trigger LIFECYCLE GATE. Rename does not bypass; intent does. |
| LC3 | "I'll batch `errdefer` at the bottom of init, easier to read" | The LAST `errdefer` line in init must lexically precede the LAST allocation it protects. Audit script enforces line-position. Batched-at-bottom errdefer is **blocking**. |
| LC4 | "Both `defer` and `errdefer` on the same allocation, belt-and-suspenders" | Audit greps for `defer X.free(Y)` + `errdefer X.free(Y)` (or the reverse) in the same scope. **Blocking violation** — pick one per §6. |
| LC5 | "`deinit` is idempotent in practice; no test needed" | Every struct with `deinit` must have a `*_test.zig` test that calls `s.deinit(); s.deinit();` and asserts no crash (sentinel-on-second-call is fine). Reviewer responsibility. Audit cannot fully grep for this; reviewer must verify. |
| LC6 | "Storing arena slice in long-lived struct works in my test" | Heuristic flag: struct fields of slice type AND a stored `*ArenaAllocator` field → audit raises informational warning. Reviewer must acknowledge or restructure. |
| LC7 | "Empty no-op `init` paired with empty no-op `deinit` to satisfy the gate" | Audit informational-flags when both `init` and `deinit` bodies are empty or single `_ = self;`. Pair exists for shape, not for state — likely should be a `const` value or removed. |
| LC8 | "Auto-mode is on" | **Auto-mode does NOT cover gate skips.** Skip without explicit user-given override = automatic violation. |
| LC9 | "Renamed-only file (no content change), gate doesn't fire" | Rename without content change does **not** fire LIFECYCLE GATE. Rename + content change does. Pure rename is a no-op for the gate. |
| LC10 | "Two docs disagree (e.g. write_zig.md says X, LIFECYCLE_PATTERNS says Y)" | Precedence: dispatch façade > standards doc > spec. The façade (`dispatch/write_zig.md`, lifecycle) is the canonical enforcement layer. |

These are enforced by `deinit-pairs.sh` (mechanical) and the dispatch façade (`dispatch/write_zig.md`, lifecycle — output discipline). When in conflict, the façade wins.

## §11 · Carve-out: PUB GATE vs LIFECYCLE GATE

Both gates touch the same `pub fn init` / `pub fn deinit` symbols, but ask different questions:

| Gate | Question |
|---|---|
| **PUB GATE** (`dispatch/write_zig.md`, pub-surface) | Should this symbol be `pub`? Does the file shape (file-as-struct vs functions-module) justify it? |
| **LIFECYCLE GATE** (`dispatch/write_zig.md`, lifecycle) | If `init` exists, does `deinit`? Is `errdefer` placed correctly? Is allocator ownership clear? Is the call-site contract documented? |

When a `pub fn init` is added, **both gates may fire**. Print both blocks. Neither gate skips deferring to the other; the redundancy on this critical surface is a feature.

## §12 · Override syntax

Per-rule override (rare, user-only):

```
LIFECYCLE RULE §<N>: SKIPPED per user override (reason: ...)
```

immediately preceding the edit. Generic "save for later" / "tactical decision" not valid — name a concrete external constraint (third-party library shape that prescribes lifecycle, vendored code we can't change). Auto-mode does NOT cover this override.

## §13 · Family

- `dispatch/write_zig.md` — Zig discipline umbrella (absorbs ZIG GATE + PUB + LIFECYCLE). This doc is the lifecycle-specific layer below it.
- `LOGGING_STANDARD.md` §7 — orthogonal: how lifecycle hooks (init/deinit) emit observability records.
- `dispatch/write_zig.md` (pub-surface) — PUB GATE; carve-out in §11.
- `dispatch/write_any.md` (File & Function Length) — file ≤ 350, fn ≤ 50, method ≤ 70 (deinit/init don't escape these limits).
- Universal rules (RULE UFS, RULE TGU, RULE ORP) live in `docs/greptile-learnings/RULES.md` and apply via the GREPTILE GATE.
- This file is the **lifecycle convention** that those universal rules cannot express. Read it once at session start; re-read on sub-task shape change.
