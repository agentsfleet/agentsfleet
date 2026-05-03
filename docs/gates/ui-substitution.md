# 🚧 UI Component Substitution Gate

**Family:** Frontend design-system discipline. **Source:** `AGENTS.md` (project-side guard). Authoritative primitive set: `ui/packages/design-system/src/index.ts`.

**Triggers** — every `Edit`/`Write` to a `*.tsx` / `*.jsx` under `ui/packages/app/`.

**Override:** `UI GATE: SKIPPED per user override (reason: ...)` immediately preceding the edit.

## What this gate enforces

The dashboard's design-system package (`ui/packages/design-system/src/index.ts` exports — `Section`, `Card`, `Badge`, `Button`, `Input`, `Dialog`, `Pagination`, `EmptyState`, `Tooltip`, etc.) is the source of truth for visual primitives. Raw HTML in dashboard files drifts from those tokens silently.

This gate enforces "use the primitive when one exists" without enumerating the primitives in this rule (so the rule scales as the design-system grows).

## Pre-edit check

1. Read (or recall) the design-system index. Treat its exports as the substitute set.
2. For each raw HTML element your edit adds (`<section>`, `<button>`, `<input>`, `<article>`, `<dialog>`, `<dl>`, `<dt>`, `<dd>`, `<ul>`, `<ol>`, `<time>`, `<table>`, `<nav>`, `<header>`, `<form>`, etc.), check the index for a matching primitive. If one exists, use it.
   - `<table>` → `DataTable` (declarative `columns` + `rows`; compose with `Pagination` for cursor/page nav).
   - `<div role="alert">` / `<p role="alert">` / `<span role="alert">` (or `role="status"`) → `<Alert variant>` with `info` / `success` / `warning` / `destructive`. Default role is `alert` for `destructive`/`warning` and `status` otherwise; override via the `role` prop only for a concrete reason.
   - `<time>` (and any inline `new Date(x).toLocaleString()` / `.toLocaleDateString()` for a user-visible timestamp) → `<Time value format="absolute|relative|datetime">`. Use `formatTimeAbsolute` when the formatted string must live in a `title=` attribute or other non-element context. The canonical `datetime` attribute is hydration-safe; the visible `relative` label opts into `suppressHydrationWarning`.
   - `<ul>` / `<ol>` (semantic listing of items) → `<List variant="unordered|ordered|plain" divided?>` + `<ListItem>`. Layout-only `<ul>` (top nav, dropdown menus) stay raw with a "Raw HTML kept" justification.
   - `<dl>` / `<dt>` / `<dd>` (label/value pairs) → `<DescriptionList layout="inline|stacked">` + `<DescriptionTerm>` + `<DescriptionDetails mono?>`.
3. Use `asChild` when you need the underlying HTML tag for semantics:

   ```tsx
   <Section asChild>
     <section aria-label="...">…</section>
   </Section>
   ```

## Required output (default — one line)

```
UI GATE: <file> | primitives:<list> | raw-kept:<list with one-word reason each | none>
```

Multi-line block fires only when raw-kept is non-empty:

```
UI GATE: <file>
  Primitives used: <list>
  Raw HTML kept:
    - <element>: <reason>  (e.g. "ul: no DS primitive")
    - ...
```

## Self-audit (end-of-turn)

```bash
git diff -U0 HEAD -- 'ui/packages/app/**/*.tsx' \
  | grep -E '^\+.*(<(section|button|input|dialog|article|nav|header|form|table|time|ul|ol|dl|dt|dd)\b|role="(alert|status)"|toLocaleString|toLocaleDateString)' \
  | head
```

Non-empty is a violation unless every match has a printed "Raw HTML kept" justification in the corresponding gate block.
