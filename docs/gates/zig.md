# 🚧 ZIG GATE

**Family:** Zig discipline (umbrella). **Source:** `docs/ZIG_RULES.md`. **Sub-gates:** Pub Surface & Struct-Shape (`docs/gates/pub-surface.md`), File & Function Length (`docs/gates/file-length.md`).

**Triggers** — every `Edit`/`Write` to a `*.zig` file outside `vendor/`, `third_party/`, `.zig-cache/`. Tests are in scope — drain/errdefer/ownership rules apply equally.

**Override:** `ZIG GATE: SKIPPED per user override (reason: ...)`. **User-invokable only.**

## What this gate covers

`docs/ZIG_RULES.md` codifies Zig discipline: drain/dupe, errdefer, ownership, sentinel, cross-compile, TLS, `pub` audit, file-as-struct shape, snake_case. Drift is silent until a leak/UAF/build break surfaces in production.

The **Pub Surface & Struct-Shape Gate** (see `docs/gates/pub-surface.md`) and **File & Function Length Gate** (see `docs/gates/file-length.md`) are sub-gates of this one — the broader Zig discipline is the umbrella.

## Pre-edit check

Recall the relevant `docs/ZIG_RULES.md` section for the change pattern:

| Pattern | Rule |
|---|---|
| DB code (`conn.query()` / `conn.exec()`) | Drain before deinit. `make check-pg-drain` verifies. |
| Allocator | Dupe before parent deinit; errdefer in reverse construction order. |
| New `pub` symbol | PUB GATE fires (see `docs/gates/pub-surface.md`). |
| File growth | LENGTH GATE (see `docs/gates/file-length.md`). |
| New file under `src/cmd/` | Cross-compile required: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`. |

Verify each rule applies or is N/A for this edit.

## Required output (default — one line)

```
ZIG GATE: <file> | drain:<ok|N/A> errdefer:<ok|N/A> dupe:<ok|N/A> pub:<see PUB|N/A> length:<see LENGTH|N/A>
```

Comment-only edit:

```
ZIG GATE: <file> | comment-only | N/A
```

Full multi-line block fires only when a sub-rule reports a violation:

```
ZIG GATE: <file>
  ZIG_RULES.md sections consulted: <e.g. drain, errdefer, pub, length, cross-compile>
  Drain discipline: <conn.query → .drain() before deinit ✓ | violation: <where>>
  Dupe before parent deinit: <ok | violation: <where>>
  errdefer ordering: <reverse-construction | violation: <where>>
  Sentinel/null handling: <ok | violation: <where>>
  Pub surface: <see PUB GATE block | violation>
  Length: <see LENGTH GATE block | violation>
```
