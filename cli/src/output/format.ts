// Tabular + structured-text rendering. Width-aware (reads
// process.stdout.columns at call time, defaults to 100). Pulse-currency
// rule: only formatHelpHeading consumes palette.pulse here. Section
// titles and table headers are intentionally non-pulse — they're not
// live signals, they're chrome.

import { palette, type StyleOpts } from "./palette.ts";

const COLUMN_GAP = "  ";

const NARROW_THRESHOLD = 80;
const HORIZONTAL_RULE = "─";

export interface FormatOpts extends StyleOpts {
  readonly widthHint?: number;
}

export interface TableColumn {
  readonly key: string;
  readonly label: string;
  readonly align?: typeof ALIGN_LEFT | typeof ALIGN_RIGHT;
}

export type TableRow = Record<string, unknown>;
export type KeyValueRows = Record<string, unknown> | ReadonlyArray<readonly [string, unknown]>;

function resolveWidth(opts: FormatOpts = {}): number {
  if (opts.widthHint !== undefined && Number.isFinite(opts.widthHint)) return opts.widthHint;
  const cols = process.stdout && (process.stdout as { columns?: number }).columns;
  return Number.isFinite(cols) && cols !== undefined && cols > 0 ? cols : 100;
}

function isAllNumeric(values: ReadonlyArray<unknown>): boolean {
  if (values.length === 0) return false;
  return values.every((v) => v !== "" && v != null && Number.isFinite(Number(v)));
}

// Section titles render in bold default text — they're chrome, not
// live signals. Pulse is reserved for help headings, the version dot,
// and live-glyph dots.
export function formatSection(title: string, opts?: FormatOpts): string {
  const head = palette.bold(title, opts);
  const rule = palette.subtle(HORIZONTAL_RULE.repeat(title.length), opts);
  return `\n${head}\n${rule}\n`;
}

export function formatHelpHeading(title: string, opts?: FormatOpts): string {
  return palette.pulseBold(title, opts);
}

// EVIDENCE label in evidence-amber, source ref in default text, "— "<quote>""
// in muted-grey. Mockup C reference:
//   EVIDENCE cd_logs:281–294 — "npm ERR! ENOSPC: no space left on device"
export function formatEvidence(ref: string, quote: string, opts?: FormatOpts): string {
  const label = palette.evidence("EVIDENCE", opts);
  const source = palette.text(ref);
  const quoted = palette.muted(`— "${quote}"`, opts);
  return `${label} ${source} ${quoted}`;
}

export function formatKeyValue(rows: KeyValueRows, opts?: FormatOpts): string {
  const entries: ReadonlyArray<readonly [string, unknown]> = Array.isArray(rows)
    ? (rows as ReadonlyArray<readonly [string, unknown]>)
    : Object.entries(rows as Record<string, unknown>);
  if (entries.length === 0) return "";
  const width = Math.max(...entries.map(([k]) => String(k).length), 0);
  const sep = palette.subtle("  ·  ", opts);
  const lines = entries.map(([key, value]) => {
    const label = palette.subtle(String(key).padEnd(width), opts);
    return `  ${label}${sep}${value}`;
  });
  return `${lines.join(LITERAL)  }\n`;
}

function renderHeader(columns: ReadonlyArray<TableColumn>, widths: ReadonlyArray<number>, opts: FormatOpts | undefined): string {
  const cells = columns.map((c, i) => c.label.padEnd(widths[i] ?? 0)).join(COLUMN_GAP);
  // Table headers are chrome, not currency — bold default, not pulse.
  return palette.bold(cells, opts);
}

function renderRule(widths: ReadonlyArray<number>, opts: FormatOpts | undefined): string {
  const rule = widths.map((w) => HORIZONTAL_RULE.repeat(w)).join(COLUMN_GAP);
  return palette.subtle(rule, opts);
}

function renderRow(
  columns: ReadonlyArray<TableColumn>,
  widths: ReadonlyArray<number>,
  row: TableRow,
  alignments: ReadonlyArray<typeof ALIGN_LEFT | typeof ALIGN_RIGHT>,
): string {
  return columns.map((c, i) => {
    const cell = String(row[c.key] ?? "");
    return alignments[i] === ALIGN_RIGHT ? cell.padStart(widths[i] ?? 0) : cell.padEnd(widths[i] ?? 0);
  }).join(COLUMN_GAP);
}

function renderHorizontal(
  columns: ReadonlyArray<TableColumn>,
  rows: ReadonlyArray<TableRow>,
  opts: FormatOpts | undefined,
): string {
  const alignments: Array<typeof ALIGN_LEFT | typeof ALIGN_RIGHT> = columns.map((c) => {
    if (c.align) return c.align;
    return isAllNumeric(rows.map((r) => r[c.key] ?? "")) ? ALIGN_RIGHT : ALIGN_LEFT;
  });
  const widths = columns.map((c) =>
    Math.max(c.label.length, ...rows.map((r) => String(r[c.key] ?? "").length))
  );
  const lines = [renderHeader(columns, widths, opts), renderRule(widths, opts)];
  for (const row of rows) lines.push(renderRow(columns, widths, row, alignments));
  return `${lines.join(LITERAL)  }\n`;
}

// Below NARROW_THRESHOLD columns, fall back to a vertical key:value
// layout — one block per record, blank line between blocks. Wider
// terminals get the tabular form.
function renderVertical(
  columns: ReadonlyArray<TableColumn>,
  rows: ReadonlyArray<TableRow>,
  opts: FormatOpts | undefined,
): string {
  const labelWidth = Math.max(...columns.map((c) => c.label.length));
  const blocks = rows.map((row) => {
    const lines = columns.map((c) => {
      const label = palette.subtle(c.label.padEnd(labelWidth), opts);
      const value = String(row[c.key] ?? "");
      return `  ${label}  ${value}`;
    });
    return lines.join(LITERAL);
  });
  return `${blocks.join("\n\n")  }\n`;
}

export function formatTable(
  columns: ReadonlyArray<TableColumn>,
  rows: ReadonlyArray<TableRow>,
  opts?: FormatOpts,
): string {
  if (rows.length === 0) return `${palette.subtle("(none)", opts)  }\n`;
  return resolveWidth(opts) < NARROW_THRESHOLD
    ? renderVertical(columns, rows, opts)
    : renderHorizontal(columns, rows, opts);
}
const LITERAL = "\n" as const;
const ALIGN_LEFT = "left" as const;
const ALIGN_RIGHT = "right" as const;
