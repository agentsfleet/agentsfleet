// Public entry for agentsfleet rendering. Commands import from here;
// the modules below this directory are implementation detail.

import { palette, type StyleOpts } from "./palette.ts";
import { glyph, withGlyph } from "./glyph.ts";
import {
  EMPTY_CELL,
  cell,
  instantCell,
  ago,
  entityColumns,
  entityTable,
  AGE_KEY,
  formatTable,
  formatKeyValue,
  formatSection,
  formatEvidence,
  type FormatOpts,
  type FieldTones,
  type ValueTones,
  type TableColumn,
  type TableRow,
  type KeyValueRows,
  type EntityTableSpec,
} from "./format.ts";
import { detectColorMode, isTty, resetCapabilityWarning, ColorMode } from "./capability.ts";

export interface UiTheme {
  readonly ok: (s: string) => string;
  readonly warn: (s: string) => string;
  readonly err: (s: string) => string;
  readonly dim: (s: string) => string;
}

// `ui` proxy — call-time capability resolution. Each member resolves
// process.stdout.isTTY / NO_COLOR on call so tests can override these
// per test without re-importing the module.
export const ui: UiTheme = {
  ok:    (s) => withGlyph(glyph.ok, s),
  warn:  (s) => `${glyph.warn().render()} ${palette.warn(s)}`,
  err:   (s) => `${glyph.error().render()} ${palette.error(s)}`,
  dim:   (s) => palette.subtle(s),
};

export interface WriteStream {
  write(chunk: string): unknown;
}

// Stream-writing helpers. Mirror the existing printSection / printKeyValue /
// printTable signatures in src/ui-theme.js so call sites only update their
// imports, not their argument shape.
export function printSection(stream: WriteStream, title: string, opts?: FormatOpts): void {
  stream.write(formatSection(title, opts));
}

export function printKeyValue(stream: WriteStream, rows: KeyValueRows, opts?: FormatOpts, tones?: FieldTones): void {
  stream.write(formatKeyValue(rows, opts, tones));
}

export function printTable(stream: WriteStream, columns: ReadonlyArray<TableColumn>, rows: ReadonlyArray<TableRow>, opts?: FormatOpts): void {
  stream.write(formatTable(columns, rows, opts));
}

export function printEntityTable(stream: WriteStream, spec: EntityTableSpec, rows: ReadonlyArray<TableRow>, opts?: FormatOpts): void {
  stream.write(entityTable(spec, rows, opts));
}

export {
  palette,
  glyph,
  EMPTY_CELL,
  cell,
  instantCell,
  ago,
  withGlyph,
  formatTable,
  entityColumns,
  entityTable,
  AGE_KEY,
  formatKeyValue,
  formatSection,
  formatEvidence,
  detectColorMode,
  isTty,
  resetCapabilityWarning,
  ColorMode,
  type StyleOpts,
  type FormatOpts,
  type FieldTones,
  type ValueTones,
  type TableColumn,
  type TableRow,
  type KeyValueRows,
  type EntityTableSpec,
};
