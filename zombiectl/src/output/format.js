// Tabular + structured-text rendering. Width-aware (reads
// process.stdout.columns at call time, defaults to 100). Pulse-currency
// rule: only formatHelpHeading consumes palette.pulse here. Section
// titles and table headers are intentionally non-pulse — they're not
// live signals, they're chrome.

import { palette } from "./palette.js";

const NARROW_THRESHOLD = 80;
const HORIZONTAL_RULE = "─";

function resolveWidth(opts = {}) {
  if (opts.widthHint && Number.isFinite(opts.widthHint)) return opts.widthHint;
  const cols = process.stdout && process.stdout.columns;
  return Number.isFinite(cols) && cols > 0 ? cols : 100;
}

function isAllNumeric(values) {
  if (values.length === 0) return false;
  return values.every((v) => v !== "" && v != null && Number.isFinite(Number(v)));
}

// Drop-in for the legacy printSection shape. Section titles render in
// bold default text — they're chrome, not live signals. Pulse is
// reserved for help headings, the version dot, and live-glyph dots.
export function formatSection(title, opts) {
  const head = palette.bold(title, opts);
  const rule = palette.subtle(HORIZONTAL_RULE.repeat(title.length), opts);
  return `\n${head}\n${rule}\n`;
}

export function formatHelpHeading(title, opts) {
  return palette.pulseBold(title, opts);
}

// EVIDENCE label in evidence-amber, source ref in default text, "— "<quote>""
// in muted-grey. Mockup C reference:
//   EVIDENCE cd_logs:281–294 — "npm ERR! ENOSPC: no space left on device"
export function formatEvidence(ref, quote, opts) {
  const label = palette.evidence("EVIDENCE", opts);
  const source = palette.text(ref);
  const quoted = palette.muted(`— "${quote}"`, opts);
  return `${label} ${source} ${quoted}`;
}

export function formatKeyValue(rows, opts) {
  const entries = Array.isArray(rows) ? rows : Object.entries(rows);
  if (entries.length === 0) return "";
  const width = Math.max(...entries.map(([k]) => String(k).length), 0);
  const sep = palette.subtle("  ·  ", opts);
  const lines = entries.map(([key, value]) => {
    const label = palette.subtle(String(key).padEnd(width), opts);
    return `  ${label}${sep}${value}`;
  });
  return lines.join("\n") + "\n";
}

function renderHeader(columns, widths, opts) {
  const cells = columns.map((c, i) => c.label.padEnd(widths[i])).join("  ");
  // Table headers are chrome, not currency — bold default, not pulse.
  return palette.bold(cells, opts);
}

function renderRule(widths, opts) {
  const rule = widths.map((w) => HORIZONTAL_RULE.repeat(w)).join("  ");
  return palette.subtle(rule, opts);
}

function renderRow(columns, widths, row, alignments) {
  return columns.map((c, i) => {
    const cell = String(row[c.key] ?? "");
    return alignments[i] === "right" ? cell.padStart(widths[i]) : cell.padEnd(widths[i]);
  }).join("  ");
}

function renderHorizontal(columns, rows, opts) {
  const alignments = columns.map((c) => {
    if (c.align) return c.align;
    return isAllNumeric(rows.map((r) => r[c.key] ?? "")) ? "right" : "left";
  });
  const widths = columns.map((c, i) =>
    Math.max(c.label.length, ...rows.map((r) => String(r[c.key] ?? "").length))
  );
  const lines = [renderHeader(columns, widths, opts), renderRule(widths, opts)];
  for (const row of rows) lines.push(renderRow(columns, widths, row, alignments));
  return lines.join("\n") + "\n";
}

// Below NARROW_THRESHOLD columns, fall back to a vertical key:value
// layout — one block per record, blank line between blocks. Wider
// terminals get the tabular form.
function renderVertical(columns, rows, opts) {
  const labelWidth = Math.max(...columns.map((c) => c.label.length));
  const blocks = rows.map((row) => {
    const lines = columns.map((c) => {
      const label = palette.subtle(c.label.padEnd(labelWidth), opts);
      const value = String(row[c.key] ?? "");
      return `  ${label}  ${value}`;
    });
    return lines.join("\n");
  });
  return blocks.join("\n\n") + "\n";
}

export function formatTable(columns, rows, opts) {
  if (rows.length === 0) return palette.subtle("(none)", opts) + "\n";
  return resolveWidth(opts) < NARROW_THRESHOLD
    ? renderVertical(columns, rows, opts)
    : renderHorizontal(columns, rows, opts);
}
