# 🚧 Pub Surface & Struct-Shape Gate

**Family:** Zig discipline (sub-gate of ZIG GATE). **Source:** `docs/ZIG_RULES.md`. **Parent:** `docs/gates/zig.md`.

**Triggers — gate fires when:**

1. Creating any new `*.zig` file under `src/` (out-of-scope: `*_test.zig`, `vendor/`, `third_party/`, `node_modules/`, `.zig-cache/`).
2. **Threshold-cross on an existing file:**
   - An Edit adds the file's first `pub` type, OR
   - Adds a `pub fn ... self ...` method to a file currently dominated by `pub` free fns, OR
   - Removes the last pub free fn from a multi-pub-fn file.
3. Any user message saying "rethink the layout of `<file>`" or equivalent.

For all other `*.zig` edits, the full block is skipped with a one-liner: `PUB GATE: skipped — <one-line reason>`. Never produce a `pub` change without either the full gate block OR the skip warning preceding the edit.

**Override:** `PUB GATE: SKIPPED per user override (reason: ...)` immediately preceding the edit (sub-gate override). The umbrella `FILE SHAPE DECISION` override requires user's explicit ask **this turn** — auto-mode does NOT cover it.

## What this gate enforces

Two `docs/ZIG_RULES.md` rules:

1. **`pub` only when an external file imports the symbol** — default private; strip stale `pub`s when touching a file.
2. **File-as-struct shape is the default for new behavior-bound-to-state files.** A file *is* a struct in Zig — modeling that shape exposes ownership and testability cheaply. Conventional layout (multi-type modules, parsers/DSLs, constants modules, pub-free-fn-dominant modules, tagged-union dispatch tables, "operations over a passive value") **must be justified at PLAN**, not chosen by inertia.

**Tie-break:** behavior-bound-to-state → file-as-struct; operations-over-value → conventional. **Escape clause:** "I can articulate in one sentence why this is operations-over-value." If you can't, file-as-struct wins.

## File-as-struct layout

```zig
const Foo = @This();

// fields
allocator: std.mem.Allocator,
state: State,

// constructors
pub fn init(...) Foo { ... }
pub fn deinit(self: *Foo) void { ... }

// queries
pub fn isReady(self: Foo) bool { ... }

// mutators
pub fn step(self: *Foo) !void { ... }

// imports at end
const std = @import("std");
const State = @import("state.zig");
```

## FILE SHAPE DECISION (mandatory at PLAN, before first Write/Edit creating or reshaping)

```
FILE SHAPE DECISION: <intended-path>
  Trigger: <new file | existing file crossing threshold: <which>>
  Purpose (one sentence): <what this file's job is>
  Primary type (if any): <Name | none>
  Methods bound to that type: <N> | Pub free fns: <M>
  Verdict: <file-as-struct | conventional>
  Why not the other: <one sentence — required when verdict is conventional>
```

Skipping this block is a PLAN violation, not just an EXECUTE one.

## Pre-edit check (every `*.zig` Edit/Write)

1. Grep new-bytes for new pub surface — any match in *new* bytes (not the existing file) → gate fires:
   - `^pub` — new top-level pub declaration
   - `^\s+pub fn` — new pub method on an existing struct
   - new variant on a pub error union: `ErrorUnion{… NewVariant,`
   - new variant on a pub enum: `= enum { … NewVariant,`
2. Count primary types in the file (struct/union/enum the file is "about"); choose layout: file-as-struct (count = 1, all pub fns take `self`) or conventional (otherwise).
3. List every new `pub` symbol the edit introduces (top-level + variant additions); for each, grep external consumer:

   ```
   grep -rn "<symbol>" src/ tests/ --include="*.zig"
   ```

   → file:line, or `NONE`. Strip `pub` from any with `NONE`.
4. Progressive cleanup on touch: `grep -n "^pub " <file>` and audit existing `pub`s in the same diff.

## Required output (when fires)

```
PUB GATE: <file> | types=<0|1|>1> | layout=<file-as-struct|conventional> (<why>)
  New: <sym> consumer=<file:line>|NONE→strip; <sym> consumer=...
  Audited: <K> kept · <M> stripped
```

If no new `pub` symbols and the file is not new, the gate is a no-op — skip the printable block.

## Self-audit (end-of-turn)

```bash
git diff -U0 HEAD -- '*.zig' \
  | grep -E '^\+pub |^\+\s+pub fn |^\+\s+[A-Z][a-zA-Z]+,$' \
  | head
```

Non-empty = new pub surface this turn; verify a PUB GATE block was printed before each corresponding Edit/Write.

For new-file FILE SHAPE coverage:

```bash
git diff --diff-filter=A --name-only origin/<base> -- 'src/**/*.zig' \
  | grep -v -E '_test\.zig$|^vendor/|^third_party/'
```

Each path must have had a `FILE SHAPE DECISION` block printed before its first Write.
