<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
-->

# M64_007: Apply Operational Restraint to docs.usezombie.com

**Prototype:** v2.0.0
**Milestone:** M64
**Workstream:** 007 (this spec) — sequenced as design-system **W4** (W1 token swap → W2 website apply → W3 app apply → **W4 docs apply** → W5 zombiectl already shipped)
**Date:** May 08, 2026
**Status:** PENDING
**Priority:** P2 — docs are read-often but lower-stakes-per-edit than the app or marketing site. Quality matters; timing flexibility is higher than W3. P2 not P3 because the docs and the dashboard share a top-nav workspace switcher conceptually — visual divergence between the two surfaces is jarring once a developer signs in.
**Categories:** DOCS · UI
**Batch:** B5 — depends on M64_002 (W1) merged. Independent of W2/W3; consumes only W1.
**Branch:** `chore/m64-007-design-w4` (in `~/Projects/docs/`, branched from `main`)
**Depends on:** M64_002 (W1) DONE. Mintlify framework constraints documented (current `docs.json` ships an orange primary palette — `#d96b2b`/`#e78a3c`/`#c45a1f` — and Mintlify's font config requires self-hosted woff2 references rather than design-system imports).

**Canonical architecture:** `docs/DESIGN_SYSTEM.md` — the spec's "Layout · Docs" section is the North Star: single column, ~68ch measure, Commit Mono headings, Instrument Sans body. The wake-pulse appears EXACTLY ONCE on the docs site — the brand-mark in the top nav.

---

## Implementing agent — read these first

1. `docs/DESIGN_SYSTEM.md` — full spec. Sections of particular weight for W4: §Typography (specimen reference), §Color (the `--pulse` currency rule + the *one-pulse-only* rule for docs), §Layout · Docs (single column, 68ch, mono headers, sans body, generous vertical rhythm), §Motion (`prefers-reduced-motion` is a hard requirement; the docs are read, not animated).
2. `AGENTS.md` — operating model. The DOC READ GATE makes `DESIGN_SYSTEM.md` mandatory on every `ui/packages/**` edit; for the docs repo this spec is the explicit equivalent.
3. `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html` — the Typography specimen is the North Star for the docs body. Open it side-by-side with the docs preview during the implementation.
4. `~/Projects/docs/docs.json` — Mintlify config. Today's `colors` block ships a heritage orange primary; W4 maps every theme variable to the design-system tokens. Mintlify's theme keys: `colors.primary`, `colors.light`, `colors.dark`, `colors.background`, `colors.text` (where supported by the current Mintlify version).
5. `~/Projects/docs/` repo top-level — Mintlify static-site layout. Pages live under `index.mdx`, `quickstart.mdx`, `concepts/**`, `cli/**`, `api-reference/**`, `billing/**`, `workspaces/**`, `zombies/**`, `contributing/**`, `memory.mdx`. Markdown content is OUT OF SCOPE — this spec is theme + layout only.
6. `ui/packages/design-system/src/index.ts` (in `~/Projects/usezombie/`) — the shared tokens. Mintlify can't `import` these directly, so W4 self-hosts the woff2 files in `public/` and references them via `docs.json`'s font config — copying the same Commit Mono + Instrument Sans subset the design-system ships.
7. `ui/packages/website/` (in `~/Projects/usezombie/`, post-W2) — the precedent for Tailwind utility composition with the new tokens. Docs is Mintlify (its own theming layer) but the colour anchor is the same.
8. The W3 commit (`c036b48d`) — for the dashboard's pulse-cap shape; the docs site does NOT replicate this, but understanding why it exists informs the *one-pulse-only* discipline applied to docs.

---

## Applicable Rules

- `docs/greptile-learnings/RULES.md` — universal. Specifically **NLG** (no "legacy" framing while pre-v2.0; the orange primary is *previous direction*, not "legacy heritage"), **TST-NAM** (no milestone IDs in source), **NLR** (touch-it-fix-it on Mintlify config or self-hosted CSS).
- `docs/DESIGN_SYSTEM.md` — doc-read gate fires on every theme/CSS edit in the docs repo.
- File & Function Length Gate — n/a beyond the standard cap (most edits are config/CSS, not source).
- Standard set otherwise.

---

## Anti-Patterns to Avoid

N/A — spec authoring complete; the implementing agent reads sections below as goal contract, not pseudocode.

---

## Overview

**Goal (testable):** `docs.usezombie.com` renders against `docs/DESIGN_SYSTEM.md` Operational Restraint end to end — Geist replaced by Commit Mono + Instrument Sans across every page, the orange primary palette swapped for the design-system tokens (`--pulse` cyan as the single accent on the brand-mark only, evidence amber on explicit cited-evidence callouts only, semantic info/warn/error tokens on Mintlify callouts), single-column long-form prose at ~68ch measure, Commit Mono headers (h1–h6), Instrument Sans body at 15-16px / 1.55 line-height, calm code-block syntax theme (no purple gradient), light-mode parity at WCAG AA, Lighthouse Performance ≥ 90 / Accessibility ≥ 95 on the deploy preview.

**Problem:** The docs site today ships an orange primary palette inherited from the pre-Operational-Restraint direction (`docs.json` anchors `#d96b2b`/`#e78a3c`/`#c45a1f`). It still loads Geist via whatever default Mintlify font path the Mint theme resolves. Customers who land on `/docs` after seeing the new dashboard chrome experience a visual handoff break — cyan-mint pulse on the app, orange chrome on docs. The brand promise ("It wakes") is invisible here because the brand-mark in the docs nav doesn't pulse and the colour anchor doesn't match.

**Solution summary:** Map every Mintlify theme variable in `docs.json` to a design-system token. Self-host Commit Mono + Instrument Sans woff2 files in `public/` (subsetted to Latin + Latin Extended for the docs body — the perf budget cannot ship the full character set). Override Mintlify's default code-block syntax theme with a calm palette anchored on usezombie tokens (Vesper-style, no purple gradients). Add the brand-mark `<WakePulse live>` in the top nav as the single pulse on the entire site. Verify dark + light mode each pass WCAG AA at body text (`#1A1D1E` on `#F8F6F1` ≥ 7:1 ratio; `#E5E7EB` on `#0B0D0E` similar). Lighthouse green on the deploy preview.

---

## Files Changed (blast radius)

(All paths are inside `~/Projects/docs/` unless prefixed.)

| File | Action | Why |
|------|--------|-----|
| `docs.json` | EDIT | Re-map `colors.primary`/`light`/`dark` from heritage orange to design-system tokens (`--pulse` cyan as primary; surface tokens as background; semantic tokens for callout colours). Configure custom CSS path for the bits Mintlify can't theme directly. |
| `public/fonts/CommitMono-Regular.woff2` | NEW | Self-hosted Commit Mono regular weight, Latin + Latin Extended subset only. ≤80KB. |
| `public/fonts/CommitMono-Medium.woff2` | NEW | Commit Mono medium weight (used on h1/h2 of long-form prose). ≤80KB. |
| `public/fonts/InstrumentSans-Regular.woff2` | NEW | Instrument Sans regular for body. Latin + Latin Extended subset. ≤80KB. |
| `public/fonts/InstrumentSans-Medium.woff2` | NEW | Instrument Sans medium for emphasis. ≤80KB. |
| `public/styles/usezombie-tokens.css` | NEW | Design-system tokens transcribed for the docs surface (every `--surface-*`, `--text-*`, `--pulse`, `--evidence`, `--info`, `--warn`, `--error`, `--border`, plus `prefers-reduced-motion` overrides). Imported via `docs.json` `customCss` (or Mintlify's equivalent) so every page picks up tokens without per-page overrides. |
| `public/styles/usezombie-overrides.css` | NEW | Layer over Mintlify's base theme: `@font-face` for the four self-hosted woff2 files, `body { font-family: "Instrument Sans", system-ui, sans-serif; }`, `h1,h2,h3,h4,h5,h6 { font-family: "Commit Mono", monospace; }`, code-block syntax theme override (custom palette anchored on tokens, no purple), inline `<code>` styling (Commit Mono on `--surface-1`, no border, `--r-sm`), table tabular-nums on numeric columns, search-box focus ring `--pulse-glow` (no animation), and the dot-grid background at 8% opacity for the `index.mdx` cover page only (inner pages stay clean). |
| `public/components/BrandMark.tsx` (or whatever Mintlify's component-extension path is — check Mintlify docs for the current version's MDX-component override syntax) | NEW | Inserts a `<WakePulse live>` element in the top-nav alongside the wordmark. Pulse is the *only* animated element on the site. Configure in `docs.json` under `navigation.brand` or the Mintlify equivalent. |
| `index.mdx` | EDIT | Cover page only: opt-in dot-grid background utility, display-xl headline (per spec: editorial license on the cover). Inner pages do NOT get the dot-grid. |
| `404.mdx` | EDIT | If exists, theme-sweep. If not, leave Mintlify's default. |
| `images/og-image.png` | EDIT (if it exists) | Re-render OG image on the new palette so social previews match. |

**Out of scope for this milestone:** every `*.mdx` content file under `concepts/`, `cli/`, `api-reference/`, `billing/`, `workspaces/`, `zombies/`, `contributing/`. Markdown content stays untouched — this PR is theme + layout only. If content bugs are found, file a follow-up issue, do not bundle.

---

## Workstreams

### Workstream A — Mintlify config + token map

`docs.json` `colors.*` keys re-anchored on design-system tokens. Verify against Mintlify's current theming docs (the schema may have moved between minor versions).

**Invariant:** every theme variable in `docs.json` traces to a single design-system token, not a hard-coded hex. The only hex literals in the diff live in the woff2-related `@font-face` rules (filenames, not colors).

### Workstream B — self-hosted fonts

Subset Commit Mono + Instrument Sans to Latin + Latin Extended, output 4 woff2 files under `public/fonts/`. Each ≤80KB. Reference via `@font-face` in `usezombie-overrides.css`. Subsetting tool: `pyftsubset` (fonttools) — the same toolchain `ui/packages/design-system` uses.

**Invariant:** zero Google Fonts requests in the deployed bundle. Confirm via `curl -s https://docs-preview-url/_app/immutable/...` grep after deploy preview. (For Mintlify: confirm in DevTools Network tab that no `fonts.googleapis.com` request fires.)

### Workstream C — calm code-block syntax theme

Mintlify defaults to a Prism-derived theme with purple keywords + green strings + blue functions. Override with a palette anchored on usezombie tokens: keywords `--text`, strings `--info`, comments `--text-subtle`, numbers `--evidence` (the *only* place evidence-amber appears anywhere on the docs site outside cited-evidence callouts). Reference: the Vesper VS Code theme is the closest stylistic precedent.

### Workstream D — brand-mark wake-pulse

Insert `<WakePulse live size={12}>` next to the wordmark in the top nav. Mintlify's brand component override path varies by version; check `docs.json` `navigation.brand` first, fall back to `_meta.tsx` or component-overrides under `public/components/`. The pulse is the ONLY animated element — `prefers-reduced-motion` falls back to a static dot in `--pulse`.

### Workstream E — light-mode parity + accessibility

Light mode is FIRST-CLASS on docs (more so than the app). Engineers read docs in both modes. Verify:
- Body text contrast ≥ 7:1 (AAA for body) — `#1A1D1E` on `#F8F6F1`, `#E5E7EB` on `#0B0D0E`
- Inline `<code>` ≥ 4.5:1 (AA) — `--text` on `--surface-1` in both modes
- Active link underline visible in both modes
- Focus ring `--pulse-glow` visible against both backgrounds

Tools: axe-core via Playwright + manual mode toggle on every top-level page.

---

## Failure Modes & Invariants

| Mode | What goes wrong | How the spec catches it |
|------|-----------------|-------------------------|
| Geist re-introduction | Mintlify default font config silently re-loads Geist via the Mint theme | Check `bun pm ls` (or the docs-repo equivalent) for any `geist`/`@fontsource-variable/geist*` deps; grep deploy preview HTML for `font-family` declarations |
| Pulse-anywhere-but-the-brand | A future MDX edit drops `<WakePulse live>` into a marketing example or callout, violating the "exactly once" rule | Spec acceptance criterion is grep-checkable: `<WakePulse live>` appears EXACTLY ONCE in the docs repo after this milestone — in the brand-mark component |
| Aurora gradients | A Mintlify theme update or a custom callout slips a multi-stop gradient back into the chrome | Manual eyeball + grep `usezombie-overrides.css` for `linear-gradient` (only allowed: dot-grid background, never multi-stop) |
| Font perf regression | Full Commit Mono character set ships to production, blowing the perf budget | Each woff2 ≤ 80KB; subsetted to Latin + Latin Extended; verify via `wc -c public/fonts/*.woff2` in the PR diff |
| Light-mode break | Light mode renders but contrast fails AA somewhere (most likely on inline code or muted text) | axe-core run on the preview; document violations as blockers, not nits |
| Mintlify version drift | A Mintlify minor bumps and the theming keys in `docs.json` change shape | Pin the Mintlify CLI version in the implementing agent's notes; document the minor in the PR description so future updates know what shape to migrate from |

**Architectural invariant:** `--pulse` appears EXACTLY ONCE on the docs site — the brand-mark in the top nav. Every other accent uses muted/subtle/info/warn/error/evidence per the design-system contract. Anywhere else is a violation, even if "it would look nice."

---

## Test Specification

| Test | Asserts |
|------|---------|
| Build smoke | `bun run dev` (or `mintlify dev`) builds and serves locally without errors |
| Font self-hosting | Network tab on deploy preview: zero requests to `fonts.googleapis.com` or `fonts.gstatic.com` |
| Font weight | Each `public/fonts/*.woff2` ≤ 80KB |
| Geist absence | `grep -ri "geist" ~/Projects/docs/{docs.json,public,index.mdx}` returns no matches in deploy artefacts |
| Pulse exclusivity | `grep -r "WakePulse" ~/Projects/docs/` returns one match only — the brand-mark component |
| Lighthouse Performance | ≥ 90 on deploy preview, mobile profile |
| Lighthouse Accessibility | ≥ 95 on deploy preview |
| Lighthouse Best Practices | ≥ 95 on deploy preview |
| axe-core / pa11y | Zero AA violations on every top-level page (index, quickstart, concepts, cli, api-reference, billing, workspaces, zombies, contributing, memory, 404) |
| Light + dark mode | Every top-level page renders without contrast violations in BOTH modes |
| Reduced-motion respect | `prefers-reduced-motion: reduce` toggled in DevTools — `<WakePulse>` falls back to static dot, no other animation present |
| Type specimen match | Side-by-side comparison of `~/.gstack/projects/usezombie/designs/design-system-20260508-0831/preview.html` and the docs deploy preview — Display / Body Large / Body / Code specimens match within visual taste tolerance |
| Slow-3G subset | Throttle to slow 3G, reload — body text renders before fonts load (system fallback chain holds), then swaps to Instrument Sans + Commit Mono once subsetted woff2 files arrive |

---

## Acceptance Criteria

- `docs.json` re-themed; zero hex literals beyond brand neutrals.
- `public/fonts/` carries exactly four self-hosted woff2 files, each ≤ 80KB, Latin + Latin Extended subset.
- `public/styles/usezombie-tokens.css` + `usezombie-overrides.css` land as new files; imported via `docs.json` custom CSS hook.
- Brand-mark `<WakePulse live>` renders in the top nav of every page; `--pulse` appears exactly once across the entire site.
- No Geist anywhere in the deployed bundle (Network tab + filesystem grep).
- No aurora / multi-stop gradients in the diff (filesystem grep).
- Lighthouse Performance ≥ 90, Accessibility ≥ 95, Best Practices ≥ 95 on the deploy preview.
- axe-core run produces zero AA violations on every top-level page in both light + dark mode.
- Playwright screenshots attached to the PR: docs landing (light + dark), a concepts page (long-form prose, light + dark), a reference page (API tables, light + dark), a guide page with code blocks (light + dark), the search modal/box open, the 404 page.
- Markdown content (`*.mdx` under `concepts/`, `cli/`, `api-reference/`, etc.) untouched in the diff.
- Spec moved from `docs/v2/active/` → `docs/v2/done/` in the usezombie repo (separate commit on whichever feature branch is open at close time).

---

## Out of Scope

- Markdown content rewrites — this PR is theme + layout only.
- New page additions, navigation reshuffles, or sidebar restructuring.
- Search UX changes beyond the focus-ring colour swap.
- API reference auto-generation tooling — separate concern.
- Backfilling older changelog entries to the new visual system.
- Welcome-email integration (handled by M64_005 bonus or M65 marketing-ops).

---

## Discovery (out-of-scope but adjacent observations)

- Mintlify's Mint theme version may have shifted between minor releases since the existing `docs.json` was last touched. Pin the CLI version in the implementing agent's notes; if a minor update is required, do it as a separate commit in the same PR.
- The `<WakePulse>` primitive is currently exported from `@usezombie/design-system` (the React/Next package). Mintlify's component override layer may or may not be able to import from a workspace package; if not, transcribe the WakePulse SVG + animation manually into a Mintlify-friendly component. Flag the transcription as drift-prone in the PR description.
- The `images/` directory may carry pre-Operational-Restraint screenshots referenced from `*.mdx` content. Screenshots are out of scope for this milestone, but flag any that look obviously stale for a future content milestone.
- The `mise.toml` in the docs repo controls toolchain pinning (Mintlify CLI, Node version). If the implementing agent pins the Mintlify version per the workstream A note, update `mise.toml` in the same commit.

---

## Implementation Notes

- This spec lives in `~/Projects/usezombie/docs/v2/pending/` BUT the implementation lands in `~/Projects/docs/` (separate repo). Per AGENTS.md "Docs-repo edits on own branch": commit on `chore/m64-007-design-w4`, push origin master only after the PR merges.
- The Mintlify deploy preview bot fires on every PR push. Use the preview URL as ground truth for Lighthouse + axe-core runs; do not rely on local `mintlify dev` for accessibility verification.
- Self-hosting woff2 files means the docs repo gains a binary asset path. Add `public/fonts/*.woff2` to LFS if the repo carries an LFS config; otherwise commit directly (woff2 is already binary-compressed; LFS adds little for files this small).
- Acquire Commit Mono + Instrument Sans license confirmation from the design-system package's font headers — the same licensing that lets `ui/packages/design-system` self-host applies to the docs repo as long as the woff2 files carry the same vendor headers.
