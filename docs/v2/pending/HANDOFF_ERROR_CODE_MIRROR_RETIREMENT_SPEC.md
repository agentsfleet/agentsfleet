# Handoff — Spec out: retire the `error-codes` mirror file (eliminate the audit allowlist exception)

You are picking up from another agent that hit a friction point during a
TypeScript migration: editing the canonical CLI-side error-code mirror file
forced an edit to a guardrail audit script. The Captain pushed back: the audit
exists for a reason, and the fact that we have to special-case a path inside
its allowlist is itself a smell. Your job is to **author a spec** for retiring
the mirror file (and the allowlist exception with it), not implement anything.

## Captain identifier

The Captain is Kishore. Address as Captain / Skipper / Boss.

## Prior-agent feedback the Captain wants you to know

The prior agent was migrating `zombiectl/src/constants/error-codes.js` to TypeScript along with nine other constants files. The `audit-error-codes.sh` script (in `~/Projects/dotfiles/scripts/`, symlinked into `scripts/`) has a hardcoded allowlist that names this file by extension:

```bash
'^src/errors/|/_?test_harness\.zig$|_test\.zig$|^src/executor/client_errors\.zig$|^src/zbench_fixtures\.zig$|^zombiectl/src/constants/error-codes\.js$'
```

When the file became `.ts`, the audit fired. The prior agent asked permission and patched the regex to accept `error-codes\.(js|ts)$`. The Captain accepted at the time but immediately reflected: **why is this file on the allowlist at all? The right fix isn't to widen the gate — it's to find a structural shape that doesn't need the gate widened.**

**Lesson the Captain wants encoded:**
- When a guardrail fires, the FIRST instinct should be "find the path that doesn't need a patch", not "ask for permission to patch."
- An allowlist exception is a smell. Every entry on it should justify its existence on its own. If a single file forces every consumer of an audit to special-case it, that's a hint the file's responsibilities are misplaced.
- "Fix the code, not the harness" applies even when a patch is technically narrow and contract-preserving.

That patch has since been reverted; the audit is back to its tight `.js$` allowlist. `error-codes.js` stayed `.js` in the M68 PR. This spec exists to retire the file (and the exception) properly.

## Current state — what exists today

### The audit and its allowlist

`scripts/audit-error-codes.sh` (real path: `~/Projects/dotfiles/scripts/audit-error-codes.sh`) walks every `*.zig` source file and every file under `zombiectl/src/`, looking for raw `UZ-<CAT>-<NNN>` string literals. If it finds any in a file NOT on the allowlist, it fails the commit.

Allowlist (verbatim from current regex):

| Path pattern | Why it's on the list today |
|---|---|
| `^src/errors/` | The canonical Zig registry — `error_entries.zig`, `error_registry.zig`. This is the source of truth. |
| `/_?test_harness\.zig$`, `_test\.zig$` | Zig test files — tests assert specific UZ codes. |
| `^src/executor/client_errors\.zig$` | Executor crate's own mirror (separate compilation unit, can't import from `src/errors/`). |
| `^src/zbench_fixtures\.zig$` | Benchmark fixtures. |
| `^zombiectl/src/constants/error-codes\.js$` | CLI-side mirror — the file this spec retires. |

The first four are language/compilation-unit boundaries. The fifth is the one Captain wants gone.

### How the CLI mirror is consumed

Two consumers in `zombiectl/src/`:

1. **`src/lib/error-map-presets.js`** — uses the named symbols (`ERR_UNAUTHORIZED`, `ERR_TOKEN_EXPIRED`, etc.) as **map keys**. The map translates an inbound `UZ-AUTH-002` from the API into a friendly tag (`"UNAUTHORIZED"`) plus a display-ready message. This is the file that fundamentally needs the UZ strings — they're the lookup keys.
2. **`src/commands/auth.js`** — uses three symbols in equality comparisons inside the new `zombiectl auth status` command (D31 in M68). Specifically: `code === ERR_FORBIDDEN || code === ERR_UNAUTHORIZED || code === ERR_TOKEN_EXPIRED`. This consumer could pivot to comparing the resolved `tag` string instead (e.g., `tag === "UNAUTHORIZED"`).

There may be other consumers; grep before assuming.

### The full file (~50 lines of declarations, mostly)

`zombiectl/src/constants/error-codes.js` defines roughly 25 `export const ERR_*` symbols, each mapped to its UZ-* string. The file's whole purpose is to be a typed lookup table for the JS side.

## The problem statement

The CLI mirror exists because **you cannot import from `.zig` files into JavaScript**. The Zig server defines codes in `src/errors/error_registry.zig`; the CLI needs to recognize those same codes when they come back over HTTP; the only mechanism today is to manually maintain a parallel set of constants in `.js`.

This creates three follow-on problems:

1. **Audit allowlist exception.** The file is the *one place* in the CLI tree allowed to contain raw UZ-* literals. Every audit-script edit (e.g., the TypeScript migration) risks touching this entry. The exception is a forever-friction point.
2. **Drift risk.** When a new error code lands in `src/errors/error_registry.zig`, a human has to remember to update `zombiectl/src/constants/error-codes.js`. There's no compile-time link. Future microservices (executor crate, hypothetical storage service, etc.) multiply this problem — each language needs its own mirror, and each mirror can drift independently.
3. **The same problem will hit `error-map-presets.js`.** Even if you delete `error-codes.js`, the preset map *also* contains UZ-* string literals (as map keys). That file would need an allowlist entry. You're not eliminating the exception; you're moving it.

## What you are spec'ing

A standalone spec — separate from M68 — that retires the `error-codes` mirror **and** the audit allowlist entry by introducing a structural mechanism that eliminates the need for a per-language hand-maintained copy.

## Two solution paths (you decide; surface trade-offs to Captain)

### Path 1: Build-time codegen

Add a script — likely `zombiectl/scripts/generate-error-codes.mjs` — that:

1. Reads `src/errors/error_registry.zig` (or a JSON sidecar emitted by Zig if parsing Zig from JS is awkward — a `zig build emit-error-registry-json` step is a reasonable interface)
2. Emits `zombiectl/src/constants/error-codes.generated.ts` with the same shape as today's hand-maintained file (`export const ERR_FOO = "UZ-X-NNN"`)
3. Runs as a `prepublish` / `build` / `prebuild` step
4. The emitted file is **gitignored**

Audit script change: drop the `error-codes\.js$` allowlist entry entirely. The emitted file isn't tracked, so the audit (which only walks tracked files) doesn't see it. Or, narrow the audit's walk to `git ls-files`-tracked paths.

Same approach applies to `src/executor/client_errors.zig` if it has the same generated-from-canonical shape.

For `error-map-presets.js` — this file is **hand-maintained** because it carries semantic content (friendly tags, display messages, suggested user actions). You can split it: the lookup-key portion is generated (no UZ literals in source) and the semantic-content portion is hand-edited (keyed by named symbols imported from the generated file). Allowlist entry retired.

**Pros:**
- Drift is structurally impossible. Server-side error registry changes → next build emits fresh CLI constants.
- Audit allowlist shrinks to language/compilation-unit boundaries only.
- Scales to N microservices: each gets its own emit target.

**Cons:**
- Build pipeline gets a new step; CI must run it before `bun test`/`bun build`.
- Editor experience: `error-codes.generated.ts` doesn't exist until first build, so cold-clone breaks `tsc --noEmit` until then. Solve via `postinstall`.
- Versioning: a stale checkout of `error-codes.generated.ts` (if anyone accidentally commits it) is a confusing-bug magnet. Add a CI check that the file is in `.gitignore`.

### Path 2: Fold into a single semantic-rich mirror

Make `error-map-presets.js` (or its TS replacement) the *single* canonical CLI-side mirror. It's already the only file that *needs* the UZ-* strings (as map keys). Re-export the named symbols (`ERR_UNAUTHORIZED`, etc.) from the same file so existing consumers keep working.

Audit script change: replace the `error-codes\.js$` allowlist entry with whatever the consolidated file path is (`error-map-presets.js` or its successor). One entry stays, but it's the file that legitimately needs the literals.

**Pros:**
- No build pipeline change. Pure refactor.
- Single source of truth for the CLI-side error knowledge — both lookup keys and display messages co-located.
- Minimal disruption to consumers; `import { ERR_FOO } from "../lib/error-map-presets.js"` is a one-character rename per import.

**Cons:**
- Doesn't eliminate the drift problem — humans still maintain the mirror by hand.
- Future microservices multiply the maintenance burden the same way today does.
- Allowlist exception persists, just relocated.

### Path 3: Don't have a CLI-side mirror at all

The CLI never compares incoming `err.code` to a named symbol. Instead, the runCommand wrapper parses the `UZ-<CAT>-<NNN>` shape and exposes only the `tag` (e.g., `"UNAUTHORIZED"`) + a generic `category` (e.g., `"AUTH"`). Consumers compare against tags only.

Audit script change: remove the `error-codes\.js$` allowlist entry. The file ceases to exist. The runCommand wrapper still receives UZ codes but never persists them as constants — they're transient.

**Pros:**
- No mirror, no audit exception, no drift.
- Minimal code.

**Cons:**
- Loses information at the boundary. If the server adds a new UZ-AUTH-011, the CLI's tag set is now stale — there's no compile-time signal that a new tag exists.
- Tag-string comparisons are less ergonomic than imported symbols; typos in `"UNAUTHORZIED"` aren't caught at type-check time without a stringly-typed enum.
- The semantic content (display messages, suggestions) lives somewhere — likely a server-supplied `message` field. That coupling is its own design decision.

## Inputs you must read before drafting

1. **`docs/TEMPLATE.md`** — the canonical spec template. Use `kishore-spec-new` to create the file (don't hand-roll the filename / frontmatter).
2. **`docs/AGENTS.md` / `CLAUDE.md`** — particularly the "Fix the code, not the harness" rule and the gate-flag triage rubric (🎯/🔧/🏆/⚠️).
3. **`scripts/audit-error-codes.sh`** end-to-end. Understand what the audit catches and what the allowlist promises.
4. **`src/errors/error_registry.zig`** and **`src/errors/error_entries.zig`** — the canonical source.
5. **`zombiectl/src/constants/error-codes.js`** — the file being retired.
6. **`zombiectl/src/lib/error-map-presets.js`** — the second consumer that also contains UZ-* literals (this is the under-discussed second exception even if you eliminate the first).
7. **`zombiectl/src/commands/auth.js`** + every other consumer that imports `ERR_*` from `constants/error-codes.js` (grep for confirmation).
8. **`src/executor/client_errors.zig`** — the executor's mirror; same shape, same problem, allowlist entry, possibly solvable by the same mechanism.

## What the spec must produce

Use `kishore-spec-new` to create the spec under `docs/v2/pending/`. The spec must include (per `docs/TEMPLATE.md`):

1. **Problem statement** — articulate the drift risk, the audit allowlist smell, and the multi-language-mirror multiplication risk. Use concrete numbers (how many UZ codes exist today, how many consumers, what the cost of a drift incident would be).
2. **Out-of-scope** — explicitly: Zig-side mirror retirement (`src/executor/client_errors.zig`) is a separate spec; this one focuses on the CLI surface. But surface that the same pattern likely applies and recommend a follow-up.
3. **Solution paths** — present all three above (codegen / fold-into-preset / no-mirror), with the pros / cons, and your recommendation. Captain decides.
4. **Acceptance criteria** — at minimum:
   - `scripts/audit-error-codes.sh` regex contains no path entry pointing inside `zombiectl/src/` (or at most one entry pointing at the consolidated file, depending on path choice)
   - `bun test` green
   - `bun run lint` green
   - A new test (or audit step) catches the case where someone adds a UZ-* literal to a non-allowlisted file
5. **Migration plan** — file-by-file:
   - For Path 1: the codegen script, the build-pipeline wiring, the `.gitignore` entry, the CI gate, the postinstall trigger, the consumer-side import update (no source change, just the import path becomes the generated file)
   - For Path 2: which file absorbs which responsibility, every consumer rewrite, the allowlist relocation
   - For Path 3: every consumer rewrite to use tags, the wrapper change in `runCommand`, the loss-of-information audit
6. **Failure-mode + invariant tables** per `docs/TEMPLATE.md`.
7. **Open questions you must surface** (do not silently resolve; ask Captain):
   - **Q1: Codegen interface.** Does Zig already emit error registry as a JSON artifact, or do we need to add `zig build emit-error-registry-json`? Read `build.zig` before answering.
   - **Q2: Same retirement for executor mirror?** `src/executor/client_errors.zig` has the same shape. Is that in-scope or a follow-up? (Recommendation: follow-up, but explicit.)
   - **Q3: What about `error-map-presets.js` itself?** If you go Path 1 (codegen), this file still holds UZ-* keys. Does the codegen also emit the map skeleton (no semantic content), or does the file remain hand-maintained with an allowlist entry? Captain should pick.
   - **Q4: TypeScript migration timing.** M68 is mid-TypeScript-migration. Does this spec ship its emit as `.ts` (post-M68) or `.js` (independent of migration)? Recommendation: `.ts`, but flag the timing dependency.

## What you do NOT do

- Do not write any code. This is a spec authoring task only.
- Do not commit anything other than the spec file. The spec lives in `docs/v2/pending/` and gets `Status: PENDING`.
- Do not touch `M68_001_*.md` (active M68 spec).
- Do not edit any harness/gate/hook. If you hit a gate while writing markdown, the harness is right and the markdown is wrong.
- Do not assume Path 1 wins. Surface the trade-offs honestly; Captain decides at plan-eng-review time.

## Branch / worktree

A new worktree off `main`:

```bash
cd /Users/kishore/Projects/usezombie
git checkout main
git pull --ff-only
git branch feat/m{N}-error-code-mirror-retirement main   # whichever Mxx Captain assigns
git worktree add ../usezombie-m{N}-error-code-mirror-retirement feat/m{N}-error-code-mirror-retirement
cd ../usezombie-m{N}-error-code-mirror-retirement
```

The M68 worktree (`feat/m68-trigger-dx-and-free-trial`) is mid-flight and must not be touched.

## Definition of done for *this handoff*

A single new spec file under `docs/v2/pending/M{N}_{NNN}_*.md`, committed on its own feature branch off main, no code changes, all open questions (Q1–Q4 above, plus any you find) listed unresolved, `Status: PENDING`, ready for Captain's plan-eng-review / plan-ceo-review pass.

When you finish: open a PR for the spec-only branch, title `spec(M{N}): retire error-code mirror + audit allowlist exception`. Captain reviews, then decides which path to ship.
