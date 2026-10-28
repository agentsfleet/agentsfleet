# TypeScript

This standard covers TypeScript and JavaScript run by Bun or Next.js: modules, types, errors, tests, runtime resources, React and the design system.
A coding agent reads it before editing such a file, and a reviewer judges a diff against it.
Generated and vendored code, such as `node_modules/`, `.next/` and `dist/`, is out of scope.

## Module shape

### TS-SHAPE — A module takes the shape its state calls for: a class, a frozen factory, a functions module, or a type-only module.
- **Applies to:** `**/*.{ts,tsx}`
- **Checked by:** review
- **Bad:** `let entries = new Map(); export function lookup(id: ProjectId) { … }`, mutable state at module scope
- **Good:** `export class ProjectCache { #entries = new Map<ProjectId, Project>(); lookup(id: ProjectId) { … } }`
- **Why:** state hidden at module scope has no owner, no lifecycle and no reset between tests.

A class with `#private` fields owns mutable state that has a lifecycle, and a factory returns `Object.freeze({ … })` for state fixed after construction.
Validators, parsers and formatters over passive values form a functions module with named exports.
A module holding only types and interfaces is type-only.

### TS-RESHAPE — A functions module that gains its first stateful operation becomes a class in the same change.
- **Applies to:** `**/*.{ts,tsx}`
- **Checked by:** review
- **Bad:** adding `let cachedToken: string | undefined;` at the top of a module of pure functions
- **Good:** `export class TokenSource { #cached?: string; … }`, replacing the module in the same diff
- **Why:** module-scope state added to dodge the conversion leaks between callers and between tests.

### TS-ORDER — A module reads in one order: type imports, value imports, side-effect imports, local types, module constants, the main export, then private helpers.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** private helpers above the exported class, and a constant declared between two functions
- **Good:** `import type` lines, `import` lines, local types, `const TIMEOUT_MS = 5_000;`, `export class ProjectStore`, then `function parseRow`
- **Why:** the public surface sits at the same depth in every file, so a reader finds it without searching.

### TS-NAME — Each construct has one case: camelCase values and functions, PascalCase types, classes and enum members, SCREAMING_SNAKE_CASE module tunables, and `#name` private fields.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `const maxRetries = 3;` as a module tunable, and `class projectStore { _db: Database }`
- **Good:** `const MAX_RETRIES = 3;` and `class ProjectStore { #db: Database }`
- **Why:** the case tells a reader what kind of thing a name is before they look it up.

A file is kebab-case (`project-store.ts`), except a React component file, which takes its component's name (`DataTable.tsx`).
A constant composed at runtime, such as `const config = { … }`, stays camelCase.

### TS-EXPORT — Application modules use named exports, and a default export appears only where a framework requires one.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `export default function ProjectTable() { … }` in `components/ProjectTable.tsx`
- **Good:** `export function ProjectTable() { … }`, imported as `import { ProjectTable } from "@/components/ProjectTable";`
- **Why:** a default export lets each importer choose a name, so renames stop cascading and searches miss callers.

Next.js file conventions such as `page.tsx`, `layout.tsx` and `not-found.tsx`, and tool configuration files such as `next.config.ts`, keep their default export.

### TS-BARREL — An `index.ts` re-exports the package's public surface by name, never with `export *`.
- **Applies to:** `**/index.{ts,tsx,js}`
- **Checked by:** review
- **Bad:** `export * from "./internal/row-parser";`
- **Good:** `export { ProjectStore } from "./project-store";`
- **Why:** a wildcard barrel hides the dependency graph and defeats tree-shaking.

### TS-ALIAS — An import that climbs more than one directory goes through a configured path alias.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `import { countProjects } from "../../../lib/project-rollup";`
- **Good:** `import { countProjects } from "@/lib/project-rollup";`, with the `@/*` alias declared in `tsconfig.json`
- **Why:** counted dots break on every move, and a deep climb usually means the file sits in the wrong directory.

### TS-TYPEIMPORT — A type-only import uses `import type`.
- **Applies to:** `**/*.{ts,tsx}`
- **Checked by:** linter: oxlint typescript/consistent-type-imports
- **Bad:** `import { Project } from "./types";` where `Project` is only a type
- **Good:** `import type { Project } from "./types";`
- **Why:** the transpiler drops a type import without analysis, so the bundle never carries a module it does not use.

## Bindings and types

### TS-CONST — A binding is `const` unless its scope reassigns it, and `var` never appears.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** linter: oxlint prefer-const; oxlint no-var
- **Bad:** `let limit = 50;` that nothing reassigns, or `var count = 0;`
- **Good:** `const limit = 50;`, and `let count = 0;` only where a loop reassigns it
- **Why:** a `let` waiting for a reassignment that never comes misleads every reader, and `var` hoisting breeds scope bugs.

### TS-ANY — No value is typed `any`, and unknown input is typed `unknown` and narrowed before use.
- **Applies to:** `**/*.{ts,tsx}`
- **Checked by:** linter: oxlint typescript/no-explicit-any
- **Bad:** `function readStatus(body: any) { return body.status; }`
- **Good:** `function readStatus(body: unknown) { return StatusBody.parse(body).status; }`
- **Why:** `any` switches the checker off for every value it touches, and that check exists to find exactly this bug.

### TS-NONNULL — A value that may be absent is handled with `??`, a guard or an early return, never asserted away with `!`.
- **Applies to:** `**/*.{ts,tsx}`
- **Checked by:** linter: oxlint typescript/no-non-null-assertion
- **Bad:** `const first = rows[0]!;`
- **Good:** `const first = rows[0]; if (first === undefined) return EMPTY_PAGE;`
- **Why:** under `strict` and `noUncheckedIndexedAccess` a possible `undefined` is a found bug, and `!` hides it.

### TS-CAST — An `as` cast appears only as `as const`, or to narrow after a runtime check the compiler cannot follow.
- **Applies to:** `**/*.{ts,tsx}`
- **Checked by:** review
- **Bad:** `(value as Project).name`, or `value as unknown as Project`
- **Good:** `isProject(value) ? value.name : undefined`, where `isProject` checks the shape at runtime
- **Why:** a cast tells the compiler to stop checking, so the bug it would have caught ships.

Narrowing after a check is allowed, such as `items as NonEmpty<Item>` after asserting `items.length > 0`.

### TS-TSIGNORE — A `@ts-ignore` or `@ts-expect-error` comment states its reason and links the issue that will remove it.
- **Applies to:** `**/*.{ts,tsx}`
- **Checked by:** review
- **Bad:** `// @ts-ignore` above a failing call
- **Good:** `// @ts-expect-error pdf-lib types omit the signal option: <issue URL>`
- **Why:** a suppression with no issue to close it outlives its bug and hides the next one on that line.

### TS-SCHEMA — Data that crosses a network or storage boundary is validated against a runtime schema before code uses it.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `const project = (await response.json()) as Project;`
- **Good:** `const project = ProjectSchema.parse(await response.json());`, with a Zod, Valibot or TypeBox schema
- **Why:** a type annotation on wire data is a guess the compiler cannot check, so bad input travels deep before it fails.

### TS-BRAND — Identifiers of different entities carry branded types, so one cannot stand in for another.
- **Applies to:** `**/*.{ts,tsx}`
- **Checked by:** review
- **Bad:** `function archive(projectId: string, workspaceId: string)`, called with the arguments swapped
- **Good:** `type ProjectId = string & { readonly __brand: "ProjectId" };` and `function archive(projectId: ProjectId, workspaceId: WorkspaceId)`
- **Why:** swapped string identifiers compile and fail at runtime, while branded ones fail at compile time.

### PJV — A function that must see a later change receives an object, a closure or an `AbortSignal`, never a copied boolean or number.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `poll(aborted)`, where `aborted` is a boolean the caller flips later
- **Good:** `poll(controller.signal)`, with the loop reading `signal.aborted`
- **Why:** primitives pass by value, so the callee holds a frozen snapshot and never sees the flip.

### PTK — A membership test on a wire-derived key uses `Object.hasOwn`, a `Map` or a null-prototype object, never `in`.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `if (project.status in byStatus) byStatus[project.status] += 1;`
- **Good:** `if (Object.hasOwn(byStatus, project.status)) byStatus[project.status] += 1;`
- **Why:** `in` walks the prototype chain, so a status named `constructor` counts as known and the totals stop adding up.

`in` stays correct for a key written as a literal, or for a deliberate probe of a class prototype.
The test feeds `constructor`, `toString`, `hasOwnProperty` and `__proto__`, not only an invented unknown value.

### TS-SPREAD — A shallow merge builds a new object with spread, never with `Object.assign` onto an existing one.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `Object.assign(defaults, overrides)`
- **Good:** `const options = { ...defaults, ...overrides };`
- **Why:** `Object.assign` mutates its first argument, so a shared defaults object changes under every other caller.

## Runtime and configuration

### TS-BUN — Code that runs on Bun uses Bun's primitives over Node compatibility modules and packages.
- **Applies to:** `**/*.{ts,js}` in packages whose runtime is Bun
- **Checked by:** review
- **Bad:** `await fs.promises.writeFile(path, data)`, or the `glob` package
- **Good:** `await Bun.write(path, data)`, and `new Bun.Glob("**/*.md").scan(root)`
- **Why:** Bun is the runtime, and its primitives skip a compatibility layer that brings its own edge cases.

The primitives are `Bun.serve`, `Bun.file`, `Bun.write`, `bun:sqlite`, `Bun.spawn`, `Bun.CryptoHasher`, `Bun.password`, `Bun.Glob` and `Bun.env`.
Modules with no Bun counterpart stay, such as `node:path` and synchronous `node:fs` reads.
A dependency that needs Node compatibility keeps it, with a comment naming the dependency.

### TS-ENV — Environment variables are read once, in one configuration module, into a frozen typed object.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `fetch(process.env.API_URL + "/projects")` inside a request handler
- **Good:** `export const CONFIG = Object.freeze({ apiUrl: required(process.env.API_URL, "API_URL") });`, read as `CONFIG.apiUrl`
- **Why:** scattered reads hide the configuration surface, and a missing variable fails mid-request instead of at startup.

Next.js inlines `process.env.NEXT_PUBLIC_*` only when the name is spelled in full, so the configuration module names each variable literally.

## Errors

### TS-ERRSTYLE — A module either throws or returns a `Result` union, never both.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `function loadConfig(path: string): Result<Config> { if (!path) throw new Error("no path"); … }`
- **Good:** `if (!path) return { ok: false, error: "no path" };` inside the `Result` module
- **Why:** a mixed module makes every caller learn which mode each function uses.

A module throws when callers cannot reasonably recover, and returns `{ ok: true; value } | { ok: false; error }` when callers handle the failure routinely.

### TS-ERRNAME — An `Error` subclass is named for what is wrong, not for when or where it happened.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `class LegacyWorkspaceLookupError extends Error {}`
- **Good:** `class WorkspaceNotFound extends Error {}`
- **Why:** a caller matches on the condition, and a name built from history stops describing it once the history moves on.

## Tests

### TS-BUNTEST — A package that runs on Bun writes its tests with `bun:test`, and no package mixes test runners.
- **Applies to:** `**/*.test.{ts,tsx}`
- **Checked by:** review
- **Bad:** `import { test, expect } from "vitest";` in a Bun package
- **Good:** `import { describe, expect, mock, test } from "bun:test";`
- **Why:** a second runner brings its own configuration and drifts in matcher and mock behaviour, even when the API looks the same.

A package whose `test` script names another runner keeps it, such as the Next.js packages that run Vitest with jsdom.

### TS-TESTFILE — A test file is named after the unit it covers, and an integration test ends in `.integration.test.ts`.
- **Applies to:** `**/*.test.{ts,tsx}`
- **Checked by:** review
- **Bad:** `tests.ts` covering three modules, or a live-database suite named `project-store.test.ts`
- **Good:** `project-store.test.ts` for `project-store.ts`, and `project-store.integration.test.ts` for the live suite
- **Why:** the name leads a reader to a file's tests and lets a runner filter out the slow lane.

### TS-SNAPSHOT — A test asserts behaviour directly, and a snapshot lands only where no direct assertion can express the check.
- **Applies to:** `**/*.test.{ts,tsx}`
- **Checked by:** review
- **Bad:** `expect(render(<ProjectTable rows={rows} />).container).toMatchSnapshot();`
- **Good:** `expect(screen.getAllByRole("row")).toHaveLength(rows.length + 1);`
- **Why:** snapshots drift, their diffs are opaque, and approving one asserts that output looks familiar, not that it is right.

## Runtime resources

### TS-FANOUT — A fan-out over data from users, files, the network, a database or command output has a named concurrency bound.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `await Promise.all(paths.map(hashFile));` over a directory listing
- **Good:** `await mapLimit(paths, MAX_PARALLEL_FILES, hashFile);`
- **Why:** an unbounded fan-out opens every file or socket at once and fails on the input size nobody tested.

### TS-RETRY — A retry loop has a maximum attempt count, a backoff delay, jitter when many clients retry, and an abort path.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `while (!(await tryConnect())) {}`
- **Good:** `for (let attempt = 1; attempt <= MAX_ATTEMPTS && !signal.aborted; attempt += 1)`, sleeping `backoffWithJitter(attempt)` between tries
- **Why:** an unbounded retry turns one outage into a request storm that never stops.

### TS-TIMEOUT — Every network request, subprocess, watcher, long poll and queue wait has a timeout or a cancellation owner.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `await Promise.race([fetch(url), sleep(TIMEOUT_MS)]);`
- **Good:** `await fetch(url, { signal: AbortSignal.timeout(TIMEOUT_MS) });`
- **Why:** a timer-only race returns on time but leaves the losing request running and holding its socket.

Long-running async work accepts an `AbortSignal` from its caller.
A helper that clears its timer in `finally` and returns a `Response` leaves the body read unbounded, unless the caller sets its own timeout.

### TS-BOUNDED — Input of unknown size is streamed, chunked or refused at a named limit, never read whole into memory.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `const text = await Bun.file(uploadPath).text();`
- **Good:** `for await (const chunk of Bun.file(uploadPath).stream()) { … }`, or a size check against `MAX_UPLOAD_BYTES` first
- **Why:** one large input exhausts memory for every request the process serves.

The same holds for `response.text()`, `response.json()`, `Array.from` over a stream, and a large `JSON.parse` or `JSON.stringify` on a request path.

### TS-FINALLY — Timers, intervals, subprocesses, file handles and temporary listeners are released in `finally`.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `const id = setInterval(poll, POLL_MS); await work(); clearInterval(id);`
- **Good:** `const id = setInterval(poll, POLL_MS); try { await work(); } finally { clearInterval(id); }`
- **Why:** a thrown error skips a cleanup line written after the work, and the timer keeps the process alive.

### TS-SPAWN — Every `Bun.spawn` reads or redirects its output pipes and is awaited, killed on timeout, or released by its owner.
- **Applies to:** `**/*.{ts,js}`
- **Checked by:** review
- **Bad:** `Bun.spawn(["git", "log"]);`, with stdout never read and the process never awaited
- **Good:** `const proc = Bun.spawn({ cmd: ["git", "log"], stdout: "pipe", timeout: GIT_TIMEOUT_MS });`, then its stdout read and `await proc.exited`
- **Why:** an unread pipe fills and blocks the child, and an unawaited child outlives the work it served.

A wrapper's tests cover its timeout and kill path, not only a clean exit.

### TS-CPU — Heavy synchronous work stays out of request handlers unless a named limit bounds it, and configuration is parsed once at startup.
- **Applies to:** `**/*.{ts,tsx,js,jsx}`
- **Checked by:** review
- **Bad:** `JSON.parse(readFileSync(CATALOGUE_PATH, "utf8"))` inside every request
- **Good:** `const CATALOGUE = loadCatalogue();` at module start, read by the handler
- **Why:** synchronous work in a handler blocks every other request on the event loop.

Hashing loops, compression, directory walks and bulk formatting count as heavy.
A worker declares its input size, concurrency, shutdown and error propagation, so it cannot fail silently.

## React and Next.js

### DID — A React component takes every `id` it renders or references from `React.useId()`, never from a hardcoded string.
- **Applies to:** `**/*.{tsx,jsx}`
- **Checked by:** review
- **Bad:** `<DialogDescription id="confirm-dialog-desc">` with `aria-describedby="confirm-dialog-desc"`
- **Good:** `const descId = React.useId();`, then `id={descId}` and `aria-describedby={descId}`
- **Why:** two mounted instances share a hardcoded id, and a screen reader announces only the first description.

Page-level singletons keep a static id, such as the root element or a global skip-link target.

### ASE — An async event handler that awaits a caller's promise catches the rejection, and does not rely on `finally` alone.
- **Applies to:** `**/*.{tsx,jsx}`
- **Checked by:** review
- **Bad:** `try { await onConfirm(); } finally { setPending(false); }`
- **Good:** `try { await onConfirm(); } catch (error) { onError?.(error); } finally { setPending(false); }`
- **Why:** React has no boundary for a rejected async handler, so the error vanishes in production.

The component exposes `onError?: (error: unknown) => void`, so its caller can show the failure.

### TS-ACTIONS — A client never fans out Server Actions with `Promise.all`, and one action reads the whole set instead.
- **Applies to:** client components in a Next.js app, `**/*.{ts,tsx}`
- **Checked by:** review
- **Bad:** `await Promise.all(ids.map(readOne));`, where `readOne` is a Server Action
- **Good:** `await readMany(ids);` as one Server Action, or a Route Handler read through `fetch`
- **Why:** Next runs a tab's Server Actions one at a time, so the calls queue and one slow action holds back the rest.

## Components and the design system

### TS-PRIMITIVE — A component renders the design system's primitive wherever one exists, never the raw HTML element it wraps.
- **Applies to:** `**/*.{tsx,jsx}` in packages that consume the design system, except tests and end-to-end specs
- **Checked by:** review
- **Bad:** `<button className="rounded-md bg-primary px-3">Save</button>`
- **Good:** `<Button>Save</Button>`, and `<Section asChild><section aria-label="Projects">…</section></Section>` where the element carries meaning
- **Why:** raw elements drift from the system's tokens silently, one surface at a time.

Primitives come from the consuming repository's own design-system package, never another product's.
Marketing heroes and section heads use `<DisplayXL>` and `<DisplayLG>`, and dashboard titles use `<PageTitle>`.
A raw element stays only where the design system exports no primitive for it.

### TS-DATES — A timestamp is labelled with the event it records and rendered by the shared `Time` component.
- **Applies to:** `ui/packages/app/**/*.tsx`
- **Checked by:** review
- **Bad:** a `Date` column that renders `new Date(row.createdAt).toLocaleString()`
- **Good:** a `Created` column that renders `<Time value={row.createdAt} />`
- **Why:** one label per event and one renderer keep dates consistent across tables and detail views.

Other events read `Updated`, `Last used`, `Occurred`, `Expires`, `Joined` or `Invited`, never a bare `Time` or `Date`.

### TS-LAYOUT — A layout change is proven by measuring the running build, and padding never subtracts a fixed scrollbar width.
- **Applies to:** `**/*.{tsx,css}`
- **Checked by:** review
- **Bad:** `padding-right: calc(13px - 6px)` to offset a scrollbar, approved from the CSS diff
- **Good:** equal padding on both edges with `scrollbar-gutter: stable both-edges`, confirmed by `getBoundingClientRect()` in the running app
- **Why:** overlay scrollbars and routes that do not scroll reserve no width, so the subtraction misaligns exactly where nobody looked.

A backgrounded tab suspends `requestAnimationFrame` and `ResizeObserver`, so behaviour that depends on them is checked by calling the arithmetic directly.

### TS-TOKEN — A Tailwind class uses the design system's token utility wherever one exists, and an arbitrary value only where none does.
- **Applies to:** `**/*.{tsx,jsx}` in packages that consume the design system, except tests and end-to-end specs
- **Checked by:** review
- **Bad:** `className="text-[14px] leading-[1.6] max-w-[640px]"`
- **Good:** `className="text-body leading-body max-w-narrow"`
- **Why:** one arbitrary value invites copies, and the published scale dilutes one surface at a time.

A value within a pixel or two of a token takes the token.
Arbitrary values stay for state selectors such as `data-[active=true]:bg-accent`, `content-[…]`, bespoke `grid-cols-[…]`, and `calc(…)` or shadows built on a token variable.

### TS-TOKENADD — A design value with no token joins the design system in the same change as its first use.
- **Applies to:** `**/*.{tsx,jsx}`, and the design system's `tokens.css` and `theme.css`
- **Checked by:** review
- **Bad:** `max-w-[46rem]` repeated across pages because no token matches
- **Good:** `--max-w-reading: 46rem` in `tokens.css`, `--container-reading: var(--max-w-reading)` in `theme.css`, then `max-w-reading`
- **Why:** the system grows without dilution only when each new value enters it once, by name.

### TWS — A Tailwind v4 `@theme inline` entry forwards to a differently named source token, never to itself.
- **Applies to:** `**/*.css`
- **Checked by:** review
- **Bad:** `@theme inline { --spacing-xs: var(--spacing-xs); }`, with `--spacing-xs` also set in `:root`
- **Good:** `:root { --sp-xs: 0.125rem; }` and `@theme inline { --spacing-xs: var(--sp-xs); }`
- **Why:** the self-loop works only through Tailwind's current layer order, and breaks silently when that order changes.

Source tokens use a prefix outside Tailwind namespaces, such as `--ff-*`, `--sp-*`, `--r-*` and `--easing-*`, and a new axis edits both files together.

### NCC — A CSS comment never contains another comment's opener or closer.
- **Applies to:** `**/*.css`
- **Checked by:** review
- **Bad:** `/* Type scale (px in /* */ comments) */`
- **Good:** `/* Type scale, in px. */`
- **Why:** CSS comments do not nest, so the first `*/` closes the comment and the rest breaks the stylesheet far from the cause.

Prose that must name the markers spells them as words: slash-star and star-slash.
