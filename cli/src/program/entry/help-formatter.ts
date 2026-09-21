// Help, formatted the way this CLI has always formatted it.
//
// The library's own formatter is close but loses two things the previous
// renderer guaranteed, and both are load-bearing:
//
//   - **A hard 80-column ceiling.** Help is the highest-frequency, lowest-
//     stakes surface this product has, and it must not assume a wide terminal.
//     The library's GLOBAL FLAGS block pads descriptions into a column that
//     lands around 120 characters, which wraps into a mess in a default
//     terminal.
//   - **The configuration pointer.** The full environment-variable matrix
//     lives in the docs so the help stays terse and the list keeps one source
//     of truth. Losing the pointer strands anyone looking for it.
//
// Wrapping is applied to the finished document rather than reimplementing the
// layout: the library owns which sections exist and in what order, and this
// only reflows what it produced. A rewrite here would be a second help
// renderer to keep in step with the tree.

import { CliError, CliOutput } from "effect/unstable/cli";

const MAX_WIDTH = 80;
const NEWLINE = "\n";
const CONFIG_DOCS_URL = "https://docs.agentsfleet.net/cli/configuration";
/**
 * The one-line pointer under every help document.
 *
 * Exported so the acceptance suite asserts the same string the CLI prints
 * rather than a copy: the full environment-variable matrix lives in the docs,
 * and an inline copy here would be a second list to keep in step.
 */
export const helpTail = (): string => `\nEnvironment variables: ${CONFIG_DOCS_URL}`;
const SUGGESTION_MARKER = "Did you mean";
const HELP_POINTER = "\n\n  Run `agentsfleet --help` to see the available commands.";

// Help is laid out in two columns — a term (a flag, a subcommand) and its
// description, separated by padding. Reflowing such a line as plain prose
// collapses that padding and destroys the column, so the two halves are
// wrapped separately and the description's continuations line up under where
// the description began.
const ENTRY_PATTERN = /^(\s+)(\S(?:.*?\S)?)(\s{2,})(\S.*)$/;
const INDENT_PATTERN = /^\s*/;
const ANSI_PATTERN = /\u001B\[[0-9;]*m/g;

// Past this, aligning the description would leave it a sliver of the line, so
// the entry breaks after its term instead and the description starts fresh.
const MAX_TERM_COLUMN = 34;
const HANGING_INDENT = "    ";

const visibleLength = (text: string): number => text.replace(ANSI_PATTERN, "").length;

/**
 * Greedy word wrap at a fixed left margin.
 *
 * Width is measured on visible characters: a colour escape costs terminal
 * columns nowhere but in `String.length`, and counting it would wrap styled
 * help earlier than plain help for no reason a reader could see.
 */
const wrapAt = (text: string, margin: string, width: number): ReadonlyArray<string> => {
  const lines: string[] = [];
  let current = "";
  for (const word of text.split(/\s+/)) {
    const candidate = current.length === 0 ? word : `${current} ${word}`;
    if (visibleLength(candidate) > width && current.length > 0) {
      lines.push(current);
      current = word;
      continue;
    }
    current = candidate;
  }
  if (current.length > 0) lines.push(current);
  return lines.map((line, index) => (index === 0 ? line : `${margin}${line}`));
};

const wrapEntry = (
  indent: string,
  term: string,
  gap: string,
  description: string,
): ReadonlyArray<string> => {
  const termColumn = indent.length + term.length;
  // A term too long to align against takes the whole line, and its
  // description follows underneath rather than in a two-character gutter.
  if (termColumn + gap.length > MAX_TERM_COLUMN) {
    const margin = `${indent}${HANGING_INDENT}`;
    const wrapped = wrapAt(description, margin, MAX_WIDTH - margin.length);
    return [`${indent}${term}`, ...wrapped.map((line, i) => (i === 0 ? `${margin}${line}` : line))];
  }
  const margin = " ".repeat(termColumn + gap.length);
  const wrapped = wrapAt(description, margin, MAX_WIDTH - margin.length);
  const [first = "", ...rest] = wrapped;
  return [`${indent}${term}${gap}${first}`, ...rest];
};

const wrapLine = (line: string): ReadonlyArray<string> => {
  if (visibleLength(line) <= MAX_WIDTH) return [line];
  const entry = ENTRY_PATTERN.exec(line);
  if (entry) {
    const [, indent = "", term = "", gap = "", description = ""] = entry;
    return wrapEntry(indent, term, gap, description);
  }
  const indent = INDENT_PATTERN.exec(line)?.[0] ?? "";
  const margin = `${indent}  `;
  return wrapAt(line.trim(), margin, MAX_WIDTH - indent.length).map((l, i) =>
    i === 0 ? `${indent}${l}` : l,
  );
};

const wrapDocument = (doc: string): string =>
  doc.split(NEWLINE).flatMap(wrapLine).join(NEWLINE);

/**
 * The flag this repository does not advertise.
 *
 * `--wizard` is the command-line library's own builder, which this repository
 * never designed: it walks flags in declaration order, so the first thing it
 * asks a newcomer is whether to set an API base URL. Deleting it is not ours
 * to do — it belongs to the library — but listing it in the help WE render is,
 * and a flag in the help is a promise that someone thought about it.
 *
 * It keeps working for anyone who names it. This only stops offering it.
 */
const UNADVERTISED_FLAG = "--wizard";
const TERM_INDENT = 2;

const indentOf = (line: string): number => line.length - line.trimStart().length;

const withoutUnadvertisedFlag = (doc: string): string => {
  const lines = doc.split(NEWLINE);
  const kept: string[] = [];
  let skipping = false;
  for (const line of lines) {
    const indent = indentOf(line);
    if (skipping) {
      // The entry's own wrapped description sits further in than its term.
      if (line.trim().length > 0 && indent > TERM_INDENT) continue;
      skipping = false;
    }
    if (indent === TERM_INDENT && line.trim().startsWith(UNADVERTISED_FLAG)) {
      skipping = true;
      continue;
    }
    kept.push(line);
  }
  return kept.join(NEWLINE);
};

/**
 * A mistyped command that resembles nothing gets a way forward.
 *
 * The library suggests a near match and says nothing when there is none, which
 * leaves the worst case — someone who has no idea what the commands are —
 * with a dead end. The previous renderer always pointed at `--help`, so the
 * pointer is restored for exactly the case that lacks a suggestion; adding it
 * underneath one would be telling someone to go looking for an answer already
 * printed above.
 */
const withHelpPointer = (rendered: string): string =>
  rendered.includes(SUGGESTION_MARKER) ? rendered : `${rendered}${HELP_POINTER}`;

const isUnknownSubcommand = (error: CliError.CliError): boolean =>
  error instanceof CliError.UnknownSubcommand;

/**
 * The library's formatter, reflowed to 80 columns, carrying the configuration
 * pointer, and never leaving an unknown command without a next step.
 *
 * Everything else is the library's. A second opinion about how to word
 * `Unrecognized flag` would drift from the one the parser actually uses.
 */
export const helpFormatter = (): CliOutput.Formatter => {
  const base = CliOutput.defaultFormatter();
  const pointerFor = (
    format: (error: CliError.CliError) => string,
  ): ((error: CliError.CliError) => string) =>
    (error) =>
      isUnknownSubcommand(error) ? withHelpPointer(format(error)) : format(error);

  return {
    ...base,
    formatHelpDoc: (doc) =>
      `${wrapDocument(withoutUnadvertisedFlag(base.formatHelpDoc(doc)))}${helpTail()}`,
    formatCliError: pointerFor(base.formatCliError),
    formatError: pointerFor(base.formatError),
    // The parser renders a parse failure through the PLURAL form, so this is
    // the one that actually fires for a mistyped command; the singular pair
    // above is overridden too because a caller reaching for either should get
    // the same sentence.
    formatErrors: (errors) =>
      errors.some(isUnknownSubcommand)
        ? withHelpPointer(base.formatErrors(errors))
        : base.formatErrors(errors),
  };
};
