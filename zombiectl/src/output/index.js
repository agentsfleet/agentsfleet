// Public entry for zombiectl rendering. Commands import from here;
// the modules below this directory are implementation detail.

import { palette } from "./palette.js";
import { glyph, withGlyph } from "./glyph.js";
import {
  formatTable,
  formatKeyValue,
  formatSection,
  formatHelpHeading,
  formatEvidence,
} from "./format.js";
import { detectColorMode, isTty, resetCapabilityWarning, ColorMode } from "./capability.js";

// Legacy `ui` proxy — preserves the shape command modules already
// import. Each member resolves capability at call time so tests can
// override process.stdout.isTTY / NO_COLOR per test without re-importing.
export const ui = {
  ok:    (s) => withGlyph(glyph.ok, s),
  info:  (s) => palette.muted(s),
  warn:  (s) => `${glyph.warn().render()} ${palette.warn(s)}`,
  err:   (s) => `${glyph.error().render()} ${palette.error(s)}`,
  head:  (s) => formatHelpHeading(s),
  dim:   (s) => palette.subtle(s),
  label: (s) => palette.subtle(s),
};

// Stream-writing helpers. Mirror the existing printSection / printKeyValue /
// printTable signatures in src/ui-theme.js so call sites only update their
// imports, not their argument shape.
export function printSection(stream, title) {
  stream.write(formatSection(title));
}

export function printKeyValue(stream, rows) {
  stream.write(formatKeyValue(rows));
}

export function printTable(stream, columns, rows) {
  stream.write(formatTable(columns, rows));
}

export {
  palette,
  glyph,
  withGlyph,
  formatTable,
  formatKeyValue,
  formatSection,
  formatHelpHeading,
  formatEvidence,
  detectColorMode,
  isTty,
  resetCapabilityWarning,
  ColorMode,
};
