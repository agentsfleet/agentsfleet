// Width arithmetic for terminal text, shared by help and error output.

const ANSI_PATTERN = /\u001B\[[0-9;]*m/g;

/** Columns a string occupies, ignoring colour escapes. */
export const visibleLength = (text: string): number => text.replace(ANSI_PATTERN, "").length;

/**
 * Greedy word wrap at a fixed left margin.
 *
 * Width is measured on visible characters: a colour escape costs terminal
 * columns nowhere but in `String.length`, and counting it would wrap styled
 * help earlier than plain help for no reason a reader could see.
 */
export const wrapAt = (text: string, margin: string, width: number): ReadonlyArray<string> => {
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
