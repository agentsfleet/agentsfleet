/*
 * tokenizeBash — pure, framework-free bash tokenizer for the animated
 * terminal demo. Classifies each word into a semantic bucket that the
 * renderer turns into a colored <span>. "Bash" is a loose descriptor:
 * we don't parse a grammar, we recognize enough surface patterns to
 * highlight the canonical zombiectl demo (spec §5.8.4).
 *
 * Ordering matters: comments, strings, and variables are tried first
 * because they can contain characters that would otherwise look like
 * flags, paths, or operators.
 */

export type TokenType =
  | "command"
  | "flag"
  | "string"
  | "number"
  | "operator"
  | "path"
  | "variable"
  | "comment"
  | "default";

export interface Token {
  readonly type: TokenType;
  readonly text: string;
}

const COMMAND_SET = new Set([
  "zombiectl",
  "curl",
  "npx",
  "bun",
  "npm",
  "git",
  "cd",
  "ls",
  "cat",
  "echo",
  "make",
  "docker",
]);

const OPERATOR_TOKENS = new Set(["|", "||", "&&", ">", ">>", "<", "&", ";"]);

function classify(word: string, isFirstSignificant: boolean): TokenType {
  if (word.length === 0) return "default";
  if (word.startsWith("#")) return "comment";
  if (word.startsWith("--") || (word.startsWith("-") && word.length > 1 && /[a-zA-Z]/.test(word[1]!))) {
    return "flag";
  }
  if (word.startsWith("$")) return "variable";
  if ((word.startsWith('"') && word.endsWith('"')) || (word.startsWith("'") && word.endsWith("'"))) {
    return "string";
  }
  if (/^-?\d+(\.\d+)?$/.test(word)) return "number";
  if (OPERATOR_TOKENS.has(word)) return "operator";
  if (word.startsWith("/") || word.startsWith("./") || word.startsWith("../") || word.startsWith("~")) {
    return "path";
  }
  if (word.startsWith("https://") || word.startsWith("http://")) return "path";
  if (isFirstSignificant && COMMAND_SET.has(word)) return "command";
  return "default";
}

export function tokenizeBash(line: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  let firstSignificantSeen = false;

  while (i < line.length) {
    const ch = line[i]!;

    // Whitespace run — preserved as default so spacing renders.
    if (ch === " " || ch === "\t") {
      let end = i;
      while (end < line.length && (line[end] === " " || line[end] === "\t")) end += 1;
      tokens.push({ type: "default", text: line.slice(i, end) });
      i = end;
      continue;
    }

    // Comment — consumes to end of line.
    if (ch === "#") {
      tokens.push({ type: "comment", text: line.slice(i) });
      i = line.length;
      continue;
    }

    // Quoted string — single or double, no embedded escape handling.
    if (ch === '"' || ch === "'") {
      const quote = ch;
      let end = i + 1;
      while (end < line.length && line[end] !== quote) end += 1;
      end = Math.min(end + 1, line.length);
      tokens.push({ type: "string", text: line.slice(i, end) });
      i = end;
      continue;
    }

    // Word — runs to whitespace.
    let end = i;
    while (end < line.length && line[end] !== " " && line[end] !== "\t") end += 1;
    const word = line.slice(i, end);
    const type = classify(word, !firstSignificantSeen);
    if (type === "command") firstSignificantSeen = true;
    if (type !== "default") firstSignificantSeen = true;
    tokens.push({ type, text: word });
    i = end;
  }

  return tokens;
}
