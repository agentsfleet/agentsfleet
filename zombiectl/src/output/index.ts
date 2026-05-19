// Public entry for zombiectl rendering. Commands import from here;
// the modules below this directory are implementation detail.

import { palette, type StyleOpts } from "./palette.ts";
import { glyph, withGlyph } from "./glyph.ts";
import {
  formatTable,
  formatKeyValue,
  formatSection,
  formatHelpHeading,
  formatEvidence,
  type FormatOpts,
  type TableColumn,
  type TableRow,
  type KeyValueRows,
} from "./format.ts";
import { detectColorMode, isTty, resetCapabilityWarning, ColorMode } from "./capability.ts";

export interface UiTheme {
  readonly ok: (s: string) => string;
  readonly info: (s: string) => string;
  readonly warn: (s: string) => string;
  readonly err: (s: string) => string;
  readonly head: (s: string) => string;
  readonly dim: (s: string) => string;
  readonly label: (s: string) => string;
}

// `ui` proxy — call-time capability resolution. Each member resolves
// process.stdout.isTTY / NO_COLOR on call so tests can override these
// per test without re-importing the module.
export const ui: UiTheme = {
  ok:    (s) => withGlyph(glyph.ok, s),
  info:  (s) => palette.muted(s),
  warn:  (s) => `${glyph.warn().render()} ${palette.warn(s)}`,
  err:   (s) => `${glyph.error().render()} ${palette.error(s)}`,
  head:  (s) => formatHelpHeading(s),
  dim:   (s) => palette.subtle(s),
  label: (s) => palette.subtle(s),
};

export interface WriteStream {
  write(chunk: string): unknown;
}

// Stream-writing helpers. Mirror the existing printSection / printKeyValue /
// printTable signatures in src/ui-theme.js so call sites only update their
// imports, not their argument shape.
export function printSection(stream: WriteStream, title: string): void {
  stream.write(formatSection(title));
}

export function printKeyValue(stream: WriteStream, rows: KeyValueRows): void {
  stream.write(formatKeyValue(rows));
}

export function printTable(stream: WriteStream, columns: ReadonlyArray<TableColumn>, rows: ReadonlyArray<TableRow>): void {
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
  type StyleOpts,
  type FormatOpts,
  type TableColumn,
  type TableRow,
  type KeyValueRows,
};
