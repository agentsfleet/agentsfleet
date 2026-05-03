# BUN_RULES — TypeScript / Bun discipline

Modelled on `ZIG_RULES.md`. Pre-design rules, decisive defaults, and the
anti-patterns each rule exists to prevent. Every rule has a one-line
"why" so you can judge edge cases.

## Scope

Triggers on every `Edit`/`Write` to:

- `*.ts`, `*.tsx`, `*.js`, `*.jsx` outside `vendor/`, `node_modules/`, `.next/`, `dist/`, `build/`.
- Any Bun-runtime entrypoint (`bun.lock`, `bunfig.toml`, files invoked via `bun run`).

Out of scope: framework-generated files (Next.js generated types, Astro `.astro`).

The **UI Component Substitution Gate** (`docs/gates/ui-substitution.md`) and **GREPTILE GATE** (which carries RULE UFS, RULE TGU, RULE PRI for TypeScript) sit on top of this file — they fire in addition to these rules, not instead of them.

## §1 · TS FILE SHAPE DECISION (mandatory at PLAN)

Before writing the first line of any new `*.ts`/`*.tsx` file under `src/`, `app/`, or `lib/`, print:

```
TS FILE SHAPE DECISION: <intended-path>
  Trigger: <new file | threshold-cross: <which>>
  Purpose: <one sentence — what this file's job is>
  Primary export: <ClassName | factory function | const | type-only module>
  Behaviour bound to state: <yes/no>
  Verdict: <class | factory | functions-module | type-only>
  Why not the other: <one sentence — required when verdict is conventional>
```

**Decision matrix:**

| Behaviour | State | Verdict |
|---|---|---|
| Operations bound to a single piece of state | yes (mutable, lifecycle) | **class** with `#private` fields |
| Operations on a passive value (validators, parsers, formatters) | no | **functions-module** (named exports) |
| One-shot construction returning a stateful holder | yes (frozen after init) | **factory function** returning `Object.freeze({...})` |
| Only types/interfaces/enums | n/a | **type-only module** |

**Threshold-cross binding** — same as Zig: an existing functions-module that's about to gain its first stateful operation rearchitects to `class` in the same diff. Don't grow a `let cachedX = …` at module scope to dodge the conversion.

**Override:** `TS FILE SHAPE DECISION: SKIPPED per user override (reason: ...)` — user-only, in this turn. Auto-mode does NOT cover it (mirrors Zig FILE SHAPE rule).

## §2 · `const` discipline

- **`const` by default. `let` only when the variable is reassigned in the same scope.** Never `let` "because I might change it later". A future-tense `let` is dead-on-arrival.
- **No `var`. Ever.** Hoisting is a bug nursery; the lint rule should be a hard error in `tsconfig`/`eslint.config.ts`.
- **Module-level constants are `SCREAMING_SNAKE_CASE`** when they're tunables (timeouts, limits, magic numbers). `camelCase` when they're configuration values composed at runtime (`const config = {...}`).
- **String literals used in ≥2 sites become a named const** (RULE UFS — applies to all source). The "it's just a label" exception is not an exception.

## §3 · Import discipline

- **`import type { X } from "..."`** for type-only imports. Bun's TypeScript transpile drops type-only imports without analysis — using value imports for types adds bundle weight and complicates tree-shaking.
- **No default exports** for application code. Default exports break renaming, hide rebinding, and confuse `import` autocomplete. Exception: framework conventions that require it (Next.js page/layout, Astro components).
- **Side-effect imports last** (`import "./bootstrap"`). Distinguish `import "./reset.css"` from value imports by physical position — eyeballs need that signal.
- **Path aliases over `../../../`.** Configure in `tsconfig.json`/`bunfig.toml`. If you find yourself counting dots, the file is in the wrong directory.

## §4 · Bun-native primitives over Node compat

| Task | Use | Avoid (unless explicit reason) |
|---|---|---|
| HTTP server | `Bun.serve({ fetch, port })` | `express`, `fastify`, `hono` (unless a specific middleware is needed) |
| Filesystem read | `await Bun.file(path).text()` / `.json()` / `.bytes()` | `fs.promises.readFile` |
| Filesystem write | `await Bun.write(path, data)` | `fs.promises.writeFile` |
| SQLite | `import { Database } from "bun:sqlite"` | `better-sqlite3`, `sqlite3` |
| Test runner | `import { test, expect } from "bun:test"` | `vitest`, `jest`, `mocha` |
| Process spawn | `Bun.spawn(["cmd", "arg"])` | `child_process.spawn` |
| Hashing | `Bun.password.hash` / `Bun.CryptoHasher` | `crypto.createHash` for new code |
| Globbing | `new Bun.Glob(pattern).scan(...)` | `glob` package |
| Env | `Bun.env` (immutable view of `process.env`) | reading `process.env` directly when Bun-native works |

The Bun runtime is the constraint. If a dependency forces Node-compat, document the reason in a comment (`// using fs.promises because pdf-lib expects Node Buffer`).

## §5 · Type discipline at parse boundaries

- **Never `any`.** Use `unknown` and narrow.
- **Narrow at parse boundaries** — when JSON crosses a network/storage boundary, validate with a real schema (Zod / Valibot / TypeBox). Trust nothing from outside the process.
- **Branded types for IDs** (`type WorkspaceId = string & { readonly _brand: "WorkspaceId" }`). Prevents passing a `UserId` where a `WorkspaceId` is expected — a compile-time bug class that's expensive at runtime.
- **No `as` casts** except: (a) const assertions (`as const`), (b) narrowing after a runtime check the compiler can't see (`x as NonEmpty<T>` after asserting `x.length > 0`).
- **Tagged unions over optional-field structs** (RULE TGU, applies to TypeScript too):
  ```ts
  // ❌ optional-field
  type Result = { ok: boolean; value?: T; error?: string };
  // ✅ tagged union
  type Result<T> = { ok: true; value: T } | { ok: false; error: string };
  ```

## §6 · File ordering

```ts
// 1. Type-only imports
import type { Workspace, User } from "./types";

// 2. Value imports
import { z } from "zod";
import { Database } from "bun:sqlite";

// 3. Module-local types
type Internal = { … };

// 4. Module-level consts
const TIMEOUT_MS = 5_000;
const STATEMENTS = { listAll: "SELECT * FROM ws" } as const;

// 5. Main export (class | factory | functions-module)
export class WorkspaceStore { … }

// 6. Helpers (private to module)
function parseRow(row: unknown): Workspace { … }
```

Side-effect imports land **after** value imports if order matters; otherwise group with value imports.

## §7 · Naming

| Construct | Convention | Example |
|---|---|---|
| Variable / function | camelCase | `parseWorkspace`, `cachedTokens` |
| Class / type / interface | PascalCase | `WorkspaceStore`, `RouteHandler` |
| Enum & enum members | PascalCase / PascalCase | `Status.Pending`, `Status.Active` |
| Module-level tunable constants | SCREAMING_SNAKE | `MAX_RETRIES`, `TIMEOUT_MS` |
| Filename | kebab-case matching default-ish export | `workspace-store.ts` |
| Test file | `<unit>.test.ts` (Bun convention) | `workspace-store.test.ts` |
| Private class field | `#name` (real private, not `_name`) | `#db`, `#cache` |

## §8 · `bun:test` discipline

- **One test file per source file**, sibling-named (`workspace-store.test.ts` next to `workspace-store.ts`). Mirrors RULE TST-NAM.
- **Use `bun:test` exports** — `test`, `expect`, `describe`, `beforeEach`, `afterEach`, `mock`. Never import from `vitest`/`jest` even if the API looks identical.
- **No milestone IDs in test names** (RULE TST-NAM). Tests describe behaviour, not provenance.
- **Integration tests opt out via filename**: `workspace-store.integration.test.ts` — Bun pattern for filtered runs (`bun test --test-name-pattern '\.integration\.'`).
- **Snapshot tests are forbidden by default.** They drift, the diff is opaque, and "looks right" isn't a behaviour assertion. Only land a snapshot when you genuinely have no other way to assert (rare).

## §9 · Error handling

Pick one style per module — never mix:

- **Throw** (`throw new TypeError(...)`) when the caller cannot reasonably handle the error and the program should die or unwind.
- **Result-style** (`type Result<T,E> = { ok: true; value: T } | { ok: false; error: E }`) when the caller routinely handles the failure and a thrown error would force every caller to wrap in `try/catch`.

In a Result-style module, **never throw**. In a throw-style module, **never return `Result<…>`**. Mixing forces every caller to know which mode each function is in — unmaintainable.

`Error` subclasses must be named after *what is wrong*, not *when it happened* (mirrors RULE NLG): `WorkspaceNotFound`, not `LegacyWorkspaceLookupError`.

## §10 · Anti-patterns (named, banned)

| Pattern | Why banned |
|---|---|
| `let` for "I might change it later" | Future-tense reassignment is a bug pretending to be flexibility. |
| `any` to silence the type-checker | The check exists to find this. |
| `// @ts-ignore` / `// @ts-expect-error` without an issue link | Comments rot; a permanent `@ts-ignore` is a permanent lie. |
| Default exports for application code | Renames cascade. Imports lie about what they import. |
| Re-exporting everything from `index.ts` | Hides dependency graph, defeats tree-shaking. Re-export only what's part of the public surface. |
| `Object.assign` for shallow merging | Use spread (`{ ...a, ...b }`). `Object.assign` mutates the first arg — bug class waiting to happen. |
| Top-level `console.log` left in source | Use a real logger, or remove before commit. Audit catches this in pre-commit if a logger gate exists. |
| `process.env.X` reads scattered through code | Read once at module init, freeze, export typed. Scattered reads make config opaque. |

## §11 · Override syntax

Per-rule override:

```
BUN RULE <§N>: SKIPPED per user override (reason: ...)
```

immediately preceding the edit. **User-invokable only.** Generic "scope creep" / "save for later" not valid — name a concrete external constraint (third-party-API shape, framework requirement, dependency mismatch).

## §12 · Family

- Universal rules (RULE UFS, RULE TGU, RULE PRI, RULE FLL, RULE ORP, RULE TST-NAM) live in `docs/greptile-learnings/RULES.md` and apply to TypeScript via the **GREPTILE GATE**.
- Frontend visual primitives live in `docs/gates/ui-substitution.md` (UI GATE).
- Length caps live in `docs/gates/file-length.md` (LENGTH GATE — file ≤ 350, fn ≤ 50, method ≤ 70 — same as every other source language).
- This file is the **TypeScript / Bun-specific** layer that those universal rules cannot express. Read it once at session start, re-read on sub-task shape change.
