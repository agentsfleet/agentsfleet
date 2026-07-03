/*
 * EYEBROW_CLASS — the single source of truth for eyebrow typography: the
 * uppercase, letter-spaced monospace micro-label that marks a section, a
 * field group, a table column, or a nav group (e.g. "MANAGE WORKSPACE",
 * "BALANCE", "ACTIVE MODEL", "CONNECTORS", column headers, sidebar groups).
 *
 * Typography only — no color, no margin. Callers add the color token for
 * their context (`text-muted-foreground` is the default; `text-text-subtle`
 * for the quieter tier) and any spacing. `<SectionLabel>` is the ready-made
 * `<h2>` wrapper; use this constant directly on `<th>`, `<dt>`, `<span>`,
 * `<div>` where an `<h2>` would be wrong semantics.
 *
 * Before this, the same label was hand-typed as either
 * `text-eyebrow/tracking-eyebrow` or `text-label/tracking-label` across
 * dozens of sites, so two near-identical sizes (11px vs 12px) drifted apart.
 * One constant collapses them to the eyebrow scale.
 */
export const EYEBROW_CLASS =
  "font-mono text-eyebrow uppercase leading-eyebrow tracking-eyebrow";
