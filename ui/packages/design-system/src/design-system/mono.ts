/*
 * MONO_CLASS — the single source of truth for technical typography: the
 * monospace treatment for identifiers, hashes, host ids, timestamps, model
 * names, secret keys, token counts and other values a reader scans rather
 * than reads (e.g. "1c59e4131FF7", "runner-7f2a", "2026-09-13T18:07:04Z").
 *
 * Typography only — no color, no margin, no truncation. Callers add the color
 * token for their context and any layout.
 *
 * Before this, the family was spelled with a size that was not its own. An
 * audit measured every Commit Mono node on ten app routes and found 50 at
 * 12px and 25 at 14px and NONE at 13px — while `--fs-mono` (13px) sat unused
 * behind the `text-mono` token, which only seven call sites named. The
 * spellings in the wild were `font-mono text-xs`, `font-mono text-sm`,
 * `font-mono text-body-sm`, and `font-mono` with no size at all, inheriting
 * whatever the parent happened to be.
 *
 * The visible cost was two sizes for one role in one table: on `/admin/models`
 * the model id rendered at 14px mono and the context and rate columns at 12px
 * mono, one row apart. This is the same two-spellings drift `EYEBROW_CLASS`
 * was written to end, and it gets the same answer — one constant to grep.
 *
 * NOT for mono that sits inside a deliberately sized block. A value rendered
 * beside its own label — `heartbeat <Time/>`, `$0.18 spent`, a timestamp in a
 * 12px metadata column — takes that block's size, because a technical value
 * set larger than the word next to it reads as a mistake. The rule the design
 * system states is "apply mono to the technical value itself, never its
 * surrounding"; the family is what changes there, not the size. This constant
 * is for a value that stands on its own.
 */
export const MONO_CLASS = "font-mono text-mono leading-mono";
