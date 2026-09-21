// Tabular + structured-text rendering. Width-aware (reads
// process.stdout.columns at call time, defaults to 100). Pulse-currency
// rule: only formatHelpHeading consumes palette.pulse here. Section
// titles and table headers are intentionally non-pulse — they're not
// live signals, they're chrome.

import { palette, type StyleOpts } from "./palette.ts";

const COLUMN_GAP = "  ";

/**
 * What a rendered cell shows when there is no value.
 *
 * One declaration. This glyph was declared thirteen times across nine names —
 * `LITERAL`, `DASH`, `LITERAL_DASH`, `EMPTY_CELL`, `EMPTY_REQUIREMENT`,
 * `WAITING_UNKNOWN`, `NO_DISPLAY_NAME`, `UNPRICED` — because RULE UFS asks each
 * file to name a repeated literal and nothing asks whether the file next door
 * already named it. Nine names for one glyph is nine chances for the tables to
 * stop agreeing about what "nothing" looks like.
 */
export const EMPTY_CELL = "—" as const;

/** A cell's text, or [`EMPTY_CELL`] when there is nothing to show. */
export const cell = (value: string | null | undefined): string =>
  value !== null && value !== undefined && value.length > 0 ? value : EMPTY_CELL;

const MS_PER_SECOND = 1_000;
const SECONDS_PER_MINUTE = 60;
const MINUTES_PER_HOUR = 60;
const HOURS_PER_DAY = 24;
const DAYS_PER_YEAR = 365;
const SECONDS_PER_HOUR = SECONDS_PER_MINUTE * MINUTES_PER_HOUR;
const SECONDS_PER_DAY = SECONDS_PER_HOUR * HOURS_PER_DAY;
const SECONDS_PER_YEAR = SECONDS_PER_DAY * DAYS_PER_YEAR;

/** The coarse unit an age is reported in, one letter each. */
const AGE_UNIT = {
  second: "s", minute: "m", hour: "h", day: "d", year: "y",
} as const;

/**
 * How long ago an instant was, in the coarsest unit that still reads true.
 *
 * It renders [`EMPTY_CELL`] rather than a number whenever it cannot know: a
 * missing or unparseable timestamp, and an instant in the future — which is
 * clock disagreement between this machine and the server, not an age. A table
 * that printed `-1m` there would be stating something no one measured.
 */
export function ago(createdAtMs: unknown, nowMs: number = Date.now()): string {
  if (typeof createdAtMs !== "number" || !Number.isSafeInteger(createdAtMs))
    return EMPTY_CELL;
  const elapsed = Math.floor((nowMs - createdAtMs) / MS_PER_SECOND);
  if (elapsed < 0) return EMPTY_CELL;
  if (elapsed < SECONDS_PER_MINUTE) return `${elapsed}${AGE_UNIT.second}`;
  if (elapsed < SECONDS_PER_HOUR)
    return `${Math.floor(elapsed / SECONDS_PER_MINUTE)}${AGE_UNIT.minute}`;
  if (elapsed < SECONDS_PER_DAY)
    return `${Math.floor(elapsed / SECONDS_PER_HOUR)}${AGE_UNIT.hour}`;
  if (elapsed < SECONDS_PER_YEAR)
    return `${Math.floor(elapsed / SECONDS_PER_DAY)}${AGE_UNIT.day}`;
  return `${Math.floor(elapsed / SECONDS_PER_YEAR)}${AGE_UNIT.year}`;
}

/** The row field an age reads from, and the header it renders under. */
export const AGE_KEY = "created_at" as const;
const AGE_LABEL = "AGO" as const;
const AGE_COLUMN_IS_APPENDED =
  "the age column is appended by entityColumns — a domain column cannot claim it";

/**
 * One table shape: the thing's name, then its identifier, then what it is,
 * then how old it is.
 *
 * The groups are separate fields rather than one array because that is what
 * removes the choice: a caller has no position to place, so thirteen tables
 * cannot drift into six orders the way they did when each owned its own column
 * list. The age column is appended here, so a table cannot omit it by being
 * edited.
 */
export interface EntityTableSpec {
  /** Absent only where the entity genuinely has none — a schedule, say. */
  readonly name?: TableColumn;
  readonly id?: TableColumn;
  readonly domain: ReadonlyArray<TableColumn>;
  /** The row field the age reads, where it is not [`AGE_KEY`]. */
  readonly ageKey?: string;
}

const NARROW_THRESHOLD = 80;
const HORIZONTAL_RULE = "─";
const TERMINAL_CONTROL_CHARACTERS = /[\u0000-\u001f\u007f-\u009f]/gu;
const BIDIRECTIONAL_CONTROL_CHARACTERS =
  /[\u061c\u200e\u200f\u2028-\u202e\u2066-\u2069]/gu;

function safeCell(value: unknown): string {
  return String(value ?? "")
    .replace(TERMINAL_CONTROL_CHARACTERS, " ")
    .replace(BIDIRECTIONAL_CONTROL_CHARACTERS, " ");
}

export interface FormatOpts extends StyleOpts {
  readonly widthHint?: number;
}

export interface TableColumn {
  readonly key: string;
  readonly label: string;
  readonly align?: typeof ALIGN_LEFT | typeof ALIGN_RIGHT;
}

export type TableRow = Record<string, unknown>;
export type KeyValueRows =
  Record<string, unknown> | ReadonlyArray<readonly [string, unknown]>;

function resolveWidth(opts: FormatOpts = {}): number {
  if (opts.widthHint !== undefined && Number.isFinite(opts.widthHint))
    return opts.widthHint;
  const cols =
    process.stdout && (process.stdout as { columns?: number }).columns;
  return Number.isFinite(cols) && cols !== undefined && cols > 0 ? cols : 100;
}

function isAllNumeric(values: ReadonlyArray<unknown>): boolean {
  if (values.length === 0) return false;
  return values.every(
    (v) => v !== "" && v != null && Number.isFinite(Number(v)),
  );
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
export function formatEvidence(
  ref: string,
  quote: string,
  opts?: FormatOpts,
): string {
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
  const width = Math.max(...entries.map(([k]) => safeCell(k).length), 0);
  const sep = palette.subtle("  ·  ", opts);
  const lines = entries.map(([key, value]) => {
    const label = palette.subtle(safeCell(key).padEnd(width), opts);
    return `  ${label}${sep}${safeCell(value)}`;
  });
  return `${lines.join(LITERAL)}\n`;
}

function renderHeader(
  columns: ReadonlyArray<TableColumn>,
  widths: ReadonlyArray<number>,
  opts: FormatOpts | undefined,
): string {
  const cells = columns
    .map((c, i) => c.label.padEnd(widths[i] ?? 0))
    .join(COLUMN_GAP);
  // Table headers are chrome, not currency — bold default, not pulse.
  return palette.bold(cells, opts);
}

function renderRule(
  widths: ReadonlyArray<number>,
  opts: FormatOpts | undefined,
): string {
  const rule = widths.map((w) => HORIZONTAL_RULE.repeat(w)).join(COLUMN_GAP);
  return palette.subtle(rule, opts);
}

function renderRow(
  columns: ReadonlyArray<TableColumn>,
  widths: ReadonlyArray<number>,
  row: TableRow,
  alignments: ReadonlyArray<typeof ALIGN_LEFT | typeof ALIGN_RIGHT>,
): string {
  return columns
    .map((c, i) => {
      const cell = safeCell(row[c.key]);
      return alignments[i] === ALIGN_RIGHT
        ? cell.padStart(widths[i] ?? 0)
        : cell.padEnd(widths[i] ?? 0);
    })
    .join(COLUMN_GAP);
}

function renderHorizontal(
  columns: ReadonlyArray<TableColumn>,
  rows: ReadonlyArray<TableRow>,
  opts: FormatOpts | undefined,
): string {
  const alignments: Array<typeof ALIGN_LEFT | typeof ALIGN_RIGHT> = columns.map(
    (c) => {
      if (c.align) return c.align;
      return isAllNumeric(rows.map((r) => r[c.key] ?? ""))
        ? ALIGN_RIGHT
        : ALIGN_LEFT;
    },
  );
  const widths = columns.map((c) =>
    Math.max(c.label.length, ...rows.map((r) => safeCell(r[c.key]).length)),
  );
  const lines = [renderHeader(columns, widths, opts), renderRule(widths, opts)];
  for (const row of rows)
    lines.push(renderRow(columns, widths, row, alignments));
  return `${lines.join(LITERAL)}\n`;
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
      const value = safeCell(row[c.key]);
      return `  ${label}  ${value}`;
    });
    return lines.join(LITERAL);
  });
  return `${blocks.join("\n\n")}\n`;
}

export function formatTable(
  columns: ReadonlyArray<TableColumn>,
  rows: ReadonlyArray<TableRow>,
  opts?: FormatOpts,
): string {
  if (rows.length === 0) return `${palette.subtle("(none)", opts)}\n`;
  return resolveWidth(opts) < NARROW_THRESHOLD
    ? renderVertical(columns, rows, opts)
    : renderHorizontal(columns, rows, opts);
}
export function entityColumns(spec: EntityTableSpec): ReadonlyArray<TableColumn> {
  for (const column of spec.domain)
    if (column.key === AGE_KEY || column.label === AGE_LABEL)
      throw new Error(AGE_COLUMN_IS_APPENDED);
  return [
    ...(spec.name === undefined ? [] : [spec.name]),
    ...(spec.id === undefined ? [] : [spec.id]),
    ...spec.domain,
    { key: AGE_KEY, label: AGE_LABEL },
  ];
}

/** [`formatTable`] over [`entityColumns`], with the age rendered per row. */
export function entityTable(
  spec: EntityTableSpec,
  rows: ReadonlyArray<TableRow>,
  opts?: FormatOpts,
): string {
  const source = spec.ageKey ?? AGE_KEY;
  const aged = rows.map((row) => ({ ...row, [AGE_KEY]: ago(row[source]) }));
  return formatTable(entityColumns(spec), aged, opts);
}

const LITERAL = "\n" as const;
const ALIGN_LEFT = "left" as const;
const ALIGN_RIGHT = "right" as const;
