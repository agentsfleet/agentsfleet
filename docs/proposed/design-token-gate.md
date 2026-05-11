# 🚧 DESIGN TOKEN GATE (proposed — dotfiles edit)

**Status:** PROPOSED. This file lives in the repo so the diff is reviewable.
The eventual home is `~/Projects/dotfiles/docs/gates/design-token.md` plus
the matching rows in `AGENTS.md`, `AGENTS_INVARIANCE.md`, and the audit-
script registration in `scripts/audit-agents-md.sh`. Land as a single
dotfiles commit on `master` per the Invariance Suite Gate protocol.

**Family:** Frontend design-system discipline (paired with the existing
UI Component Substitution Gate). **Source:** `AGENTS.md` (project-side
guard). Authoritative token set: `ui/packages/design-system/src/theme.css`.

**Triggers** — every `Edit`/`Write` to `*.tsx`/`*.jsx` under
`ui/packages/app/` **or** `ui/packages/website/` (both consume
`@usezombie/design-system`). Tests (`*.test.tsx`) and Playwright specs
(`tests/e2e/**`) are exempt; they assert on rendered DOM and frequently
use raw selectors.

**Override:** `// DESIGN TOKEN: SKIPPED per user override (reason: ...)`
in a comment immediately preceding the line. Reasons must cite a
concrete constraint — "bespoke per-surface grid template," "tight UI
chrome width with no equivalent token," "external library prop expects
px-string," not "looks the same" / "shorter to write."

---

## What this gate enforces

The design-system package exposes named Tailwind utilities for every
typographic and layout primitive (`text-display-xl`, `text-eyebrow`,
`leading-prose`, `max-w-narrow`, `tracking-display-md`, `duration-snap`,
…). Raw arbitrary values bypass the published scale. Over time that
drift dilutes the system; the moment one surface ships
`text-[14px] leading-[1.6]`, others copy the pattern.

This gate blocks arbitrary classes **when an equivalent token utility
exists.** It does NOT block:

- Tailwind state selectors (`data-[active=true]:bg-accent`)
- Pseudo-element `content-[...]`
- Bespoke `grid-cols-[...]` / `grid-rows-[...]` (per-surface templates)
- `calc(...)` expressions that consume a token (`h-[calc(100vh-var(--header-height))]`)
- Token-using shadows (`shadow-[0_0_0_3px_var(--pulse-glow)]`)

## Pre-edit check (one-time per turn)

1. Read (or recall) the design-system theme bridge:
   `ui/packages/design-system/src/theme.css` (Layer 2 `@theme inline { ... }`).
2. The available utilities are exactly the bridged names:

   | Family | Utilities |
   |---|---|
   | Text size | `text-{label,eyebrow,body-sm,body,body-lg,heading,display-md,display-lg,display-xl}` |
   | Fluid text | `text-fluid-{display-md,display-lg,hero}` |
   | Tracking | `tracking-{display-xl,display-lg,display-md,eyebrow,label}` |
   | Line-height | `leading-{display-xl,display-lg,display-md,heading,eyebrow,body-lg,body,body-sm,label,mono,prose}` |
   | Max width | `max-w-{trim,narrow,measure,form,wide,content,tagline,prose}` |
   | Min width | `min-w-{trim,narrow,measure,form,wide,content,tagline,prose}` |
   | Spacing | `{p,m,gap,space-{x,y}}-{xs,sm,md,lg,xl,2xl,3xl,4xl,5xl,6xl}` |
   | Motion | `duration-snap`, `ease-snap` |
   | Radius | `rounded-{sm,md,lg}` |
   | Colors | `bg-*` / `text-*` / `border-*` against semantic tokens only |

3. For every arbitrary `*-[...]` class your edit adds, ask: **does a
   token utility exist?**
   - If yes, use it.
   - If no, the arbitrary is allowed.
   - If close but not exact, prefer the token. The system absorbs the
     ±1–2 px difference; over hundreds of callsites the consistency
     dominates the per-callsite shift.

## Required output (default — one line)

```
DESIGN TOKEN GATE: <file> | arbitraries-kept: <list with one-word reason each | none>
```

Multi-line block fires only when `arbitraries-kept` is non-empty:

```
DESIGN TOKEN GATE: <file>
  Arbitraries kept:
    - <class>: <reason>  (e.g. "grid-cols-[2fr_1fr_1fr_1fr]: bespoke footer grid")
    - ...
```

## Self-audit (end-of-turn)

```bash
scripts/audit-design-tokens.sh --diff
```

The script (lives in the repo at `scripts/audit-design-tokens.sh`)
prints `file:line` + suggested token for every blocking violation and
exits 1. Whole-worktree mode: `scripts/audit-design-tokens.sh --all`.

A clean run is required before HARNESS VERIFY can advance.

---

## Family rules

This gate composes with the existing UI Component Substitution Gate
(raw HTML → primitives). Order of operations on a fresh `.tsx` edit:

1. **UI GATE** — pick the primitive (`<Card>`, `<Section>`, `<Button>`)
2. **DESIGN TOKEN GATE** — pick the token utility (`text-body`, `leading-body`, `max-w-narrow`)
3. **UFS GATE** — pick a named const for any repeated string literal
4. **LENGTH GATE** — keep the file ≤350 lines

A `.tsx` that passes all four reads as system-conformant without
spelunking into the design-system source.

---

## Required changes elsewhere when this lands

Per the Invariance Suite Gate's rule-extension protocol, all four
land in the same dotfiles commit:

### 1. `AGENTS.md` → "Gate index" section

Insert after the **UI Component Substitution Gate** entry:

```
### DESIGN TOKEN GATE

**Triggers:** every Edit/Write to `*.tsx`/`*.jsx` under `ui/packages/app/` or `ui/packages/website/`. Tests + Playwright specs exempt.
**Override:** `// DESIGN TOKEN: SKIPPED per user override (reason: ...)` immediately preceding the line. Reasons must cite a concrete constraint.
**Body:** `docs/gates/design-token.md` — token utility table, pre-edit check, output format, audit `scripts/audit-design-tokens.sh`.
```

### 2. `AGENTS.md` → EXECUTE doc-reads table

Insert row near the existing `ui/packages/**/*.{tsx,jsx,css}` row:

```
| `*.tsx` / `*.jsx` under `ui/packages/{app,website}/`                       | `docs/gates/design-token.md` — token utility table; DESIGN TOKEN GATE fires per edit. |
```

### 3. `AGENTS.md` → HARNESS VERIFY required output

Insert row in the gate table:

```
| DESIGN TOKEN GATE    | ✅ pass | 🟡 N violations addressed | 🔴 N unresolved |
```

### 4. `AGENTS_INVARIANCE.md` → new question

Insert in the design-system section:

```
- For each Edit/Write to `*.tsx` under `ui/packages/{app,website}/`, did
  you print the DESIGN TOKEN GATE line OR run
  `scripts/audit-design-tokens.sh --diff` and confirm it exits 0?
  YES/NO
```

### 5. `scripts/audit-agents-md.sh` (audit script registry)

Add `scripts/audit-design-tokens.sh` to the `DOTFILES_RESIDENT` set so
the invariance audit knows about it.

### 6. Project-side wiring (`make/quality.mk` in this repo)

Add the audit to `_website_lint` and `_app_lint`:

```make
_website_lint:
	@echo "→ [website] Running Oxlint + TypeScript check..."
	@cd ui/packages/website && bun run lint
	@cd ui/packages/website && bun run typecheck
	@echo "→ [website] Running design-token audit..."
	@scripts/audit-design-tokens.sh --diff
	@echo "✓ [website] Lint passed"
```

(Symmetric change for `_app_lint`.)

---

## Why a gate, not a lint rule

Three reasons:

1. **Cross-package coupling.** The token set lives in
   `ui/packages/design-system`; the consumers live in
   `ui/packages/{app,website}`. A single oxlint config can't see across
   the boundary cleanly. The audit script reads the theme.css
   utilities and consumes the diff — it's the right shape.

2. **Inline override.** Some `-[...]` classes are intentional and
   irreplaceable (bespoke grids, `calc(...)`, status-dot sizes).
   The comment-based override fits the existing gate idiom and gives
   reviewers a paper trail of *why* each arbitrary stayed.

3. **HARNESS VERIFY integration.** Gates are the canonical
   end-of-turn audit point. Lint rules surface in editor noise but
   miss the agentic lifecycle. The gate's pre-edit one-liner +
   end-of-turn self-audit puts the discipline in the model's hot path.

---

## Pre-flight checklist before landing the dotfiles commit

- [ ] Run `bash scripts/audit-agents-md.sh` — must pass before commit
- [ ] Read `AGENTS_INVARIANCE.md` after edits and answer every question
- [ ] Emit tabulated invariance report
- [ ] After `git commit` in `~/Projects/dotfiles`, write `.agents-invariance-signoff`
- [ ] `cd ~/Projects/dotfiles && git push origin master`
