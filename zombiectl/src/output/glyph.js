// Status glyphs paired with their colors. Commands consume named exports
// (glyph.live, glyph.parked, etc.); they never hard-code the char + color
// pair. Per docs/DESIGN_SYSTEM.md "Status glyphs" section:
//
//   Live      -> ● in pulse-cyan
//   Parked    -> ○ in subtle-grey
//   Degraded  -> ● in warn-amber
//   Failed    -> ✕ in error-red
//
// Plus operational glyphs the spec carves out as functional (not
// decorative): ✓ for ok, ✕ for err, ⚠ for warn lines, ⠋..⠏ for spinners.

import { palette } from "./palette.js";

const CHAR_DOT_FILLED = "●";
const CHAR_DOT_OUTLINE = "○";
const CHAR_X = "✕";
const CHAR_CHECK = "✓";
const CHAR_WARN = "⚠";

function bind(char, paint) {
  return (opts) => ({
    char,
    render: () => paint(char, opts),
  });
}

export const glyph = {
  live:     bind(CHAR_DOT_FILLED, palette.pulse),
  parked:   bind(CHAR_DOT_OUTLINE, palette.subtle),
  degraded: bind(CHAR_DOT_FILLED, palette.warn),
  failed:   bind(CHAR_X, palette.error),
  ok:       bind(CHAR_CHECK, palette.success),
  error:    bind(CHAR_X, palette.error),
  warn:     bind(CHAR_WARN, palette.warn),
};

// Shorthands — the common "render a glyph + space + message" pattern.
// Used by the legacy `ui` proxy and a few commands. Same color rule.
export function withGlyph(g, message, opts) {
  return `${g(opts).render()} ${message}`;
}
