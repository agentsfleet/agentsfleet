// One shape for every failure the CLI prints in text mode.
//
//   ✕ error: unknown subcommand "lsit" for "agentsfleet"
//     code          UNKNOWN_COMMAND
//     did you mean  list
//     suggestion    run `agentsfleet --help` for the command list
//
// The label column is always as wide as its longest label, `did you mean`, so
// a value starts in the same column whichever labels a block carries, and two
// blocks printed one after the other line up. A value longer than the line
// wraps back to that column rather than to the margin.
//
// `renderErrorBlock` is pure: it takes the width and the style, so the layout
// can be tested at any width without a terminal. `writeErrorBlock` is the
// stream-facing wrapper that picks both.

import type { WritableStreamLike } from "./capability.ts";
import { ui } from "./index.ts";
import { visibleLength, wrapAt } from "./text.ts";

export const BLOCK_LABEL = {
  code: "code",
  suggestion: "suggestion",
  usage: "usage",
  didYouMean: "did you mean",
  request: "request",
} as const;

export type BlockLabel = (typeof BLOCK_LABEL)[keyof typeof BLOCK_LABEL];

// Order a reader scans in: what it is, how to fix it, then the trace id.
const LABEL_ORDER: ReadonlyArray<BlockLabel> = [
  BLOCK_LABEL.code,
  BLOCK_LABEL.usage,
  BLOCK_LABEL.didYouMean,
  BLOCK_LABEL.suggestion,
  BLOCK_LABEL.request,
];

const LABEL_WIDTH = Math.max(...LABEL_ORDER.map((label) => label.length));
const INDENT = "  " as const;
const GAP = "  " as const;
const VALUE_COLUMN = INDENT.length + LABEL_WIDTH + GAP.length;
export const DEFAULT_BLOCK_WIDTH = 80;
// Below this a wrapped value would be a column of single words; stop wrapping.
const MIN_VALUE_WIDTH = 20;
/** Where every label's value begins, counted from column 1. */
export const BLOCK_VALUE_COLUMN = VALUE_COLUMN + 1;
// Off a terminal, a field stays on one line however long it is: logs and
// pipes are read with grep, and a wrapped sentence no longer matches itself.
const UNWRAPPED = Number.POSITIVE_INFINITY;
// A one-line refusal often carries its own fix after an em dash: `not
// authenticated — run \`agentsfleet login\` first`. The fix belongs on the
// `suggestion` line, so split it there; a message without one stays whole.
const FIX_SEPARATOR = " — " as const;

export const BLOCK_SEVERITY = { error: "error", warning: "warning" } as const;
export type BlockSeverity = (typeof BLOCK_SEVERITY)[keyof typeof BLOCK_SEVERITY];

export interface ErrorBlock {
  readonly severity: BlockSeverity;
  readonly message: string;
  // An absent or empty value omits its line.
  readonly fields: Partial<Record<BlockLabel, string | undefined>>;
}

export interface BlockStyle {
  // Renders the whole first line, glyph included, e.g. `ui.err` / `ui.warn`.
  readonly head: (severity: BlockSeverity, text: string) => string;
  readonly label: (text: string) => string;
}

export const PLAIN_STYLE: BlockStyle = {
  head: (severity, text) => `${severity === BLOCK_SEVERITY.error ? "✕" : "!"} ${text}`,
  label: (text) => text,
};

const fieldLines = (label: BlockLabel, value: string, width: number, style: BlockStyle): ReadonlyArray<string> => {
  const margin = " ".repeat(VALUE_COLUMN);
  const room = Math.max(MIN_VALUE_WIDTH, width - VALUE_COLUMN);
  const pad = " ".repeat(LABEL_WIDTH - label.length);
  const wrapped = visibleLength(value) <= room ? [value] : wrapAt(value, margin, room);
  const [first = "", ...rest] = wrapped;
  return [`${INDENT}${style.label(label)}${pad}${GAP}${first}`, ...rest];
};

/** The block's lines, without a trailing newline. */
export const renderErrorBlock = (
  block: ErrorBlock,
  width: number = DEFAULT_BLOCK_WIDTH,
  style: BlockStyle = PLAIN_STYLE,
): string => {
  const head = style.head(block.severity, `${block.severity}: ${block.message}`);
  const fields = LABEL_ORDER.flatMap((label) => {
    const value = block.fields[label];
    return value === undefined || value.length === 0 ? [] : fieldLines(label, value, width, style);
  });
  return [head, ...fields].join("\n");
};

/** The house style: the glyph and colour every other line of output uses. */
export const HOUSE_STYLE: BlockStyle = {
  head: (severity, text) => (severity === BLOCK_SEVERITY.error ? ui.err(text) : ui.warn(text)),
  label: (text) => ui.dim(text),
};

/** Writes a block to `stream`, wrapped only on a terminal that reports its width. */
export const writeErrorBlock = (stream: WritableStreamLike, block: ErrorBlock): void => {
  const width = stream.isTTY === true && stream.columns !== undefined ? stream.columns : UNWRAPPED;
  stream.write(`${renderErrorBlock(block, width, HOUSE_STYLE)}\n`);
};

export const splitFix = (message: string): { readonly message: string; readonly fix: string | undefined } => {
  const at = message.indexOf(FIX_SEPARATOR);
  return at === -1
    ? { message, fix: undefined }
    : { message: message.slice(0, at), fix: message.slice(at + FIX_SEPARATOR.length) };
};
