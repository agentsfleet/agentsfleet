// The lease filter's query grammar, kept free of React so the parse and the
// format can be tested as the pure inverse pair they are.
//
// The shape is GitHub's issue search: space-separated `key:value` tokens, values
// quoted when they contain spaces. Only the keys below are understood; anything
// else is dropped rather than guessed at, because a filter that silently means
// something other than what was typed is worse than one that narrows nothing.

/** Leading characters shown for a workspace id; the full id rides the title. */
const WORKSPACE_ID_DISPLAY_CHARS = 8;
const TRUNCATION_ELLIPSIS = "…";

export const FILTER_KEY = {
  workspace: "workspace",
  fleet: "fleet",
} as const;

const KEY_VALUE_SEPARATOR = ":";
const QUOTE = '"';

// The one bare word read as a connective between pairs — accepted so the query
// can be typed the way the hint reads ("workspace:<id> and fleet:<name>"), and
// skipped so it never becomes a filter. `and:value` still carries a key, so it
// stays an unknown-key drop, never a connective.
const CONNECTIVE_AND = "and";

export type LeaseFilters = {
  /** The workspace id the feed is narrowed to, or null when unfiltered. */
  workspace: string | null;
  /** The fleet id or exact name the feed is narrowed to, or null. */
  fleet: string | null;
};

export const NO_LEASE_FILTERS: LeaseFilters = { workspace: null, fleet: null };

/**
 * Split on whitespace, honouring double quotes so a fleet name with spaces
 * survives as one token. An unterminated quote closes at end of input — the
 * operator is mid-type, and refusing to parse would blank their own filter back
 * at them on every keystroke.
 */
function tokenize(raw: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let quoted = false;
  for (const char of raw) {
    if (char === QUOTE) {
      quoted = !quoted;
      continue;
    }
    if (!quoted && /\s/.test(char)) {
      if (current.length > 0) tokens.push(current);
      current = "";
      continue;
    }
    current += char;
  }
  if (current.length > 0) tokens.push(current);
  return tokens;
}

/**
 * Read the filter query into its named parts. A repeated key keeps the LAST
 * occurrence, matching how a shell reads repeated flags and how GitHub resolves
 * a duplicated qualifier — the operator's most recent intent wins.
 */
export function parseLeaseFilterQuery(raw: string): LeaseFilters {
  let workspace: string | null = null;
  let fleet: string | null = null;
  for (const token of tokenize(raw)) {
    if (token.toLowerCase() === CONNECTIVE_AND) continue;
    const separator = token.indexOf(KEY_VALUE_SEPARATOR);
    if (separator <= 0) continue;
    const key = token.slice(0, separator).toLowerCase();
    const value = token.slice(separator + 1);
    if (value.length === 0) continue;
    if (key === FILTER_KEY.workspace) workspace = value;
    else if (key === FILTER_KEY.fleet) fleet = value;
  }
  return { workspace, fleet };
}

/** Quote a value only when it would otherwise tokenize into several. */
function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `${QUOTE}${value}${QUOTE}` : value;
}

/**
 * Render filters back into the query the operator would have typed, so the input
 * repopulates from the URL on load and a shared link opens with its filter
 * visible rather than merely applied.
 */
export function formatLeaseFilterQuery(filters: LeaseFilters): string {
  const parts: string[] = [];
  if (filters.workspace !== null) {
    parts.push(`${FILTER_KEY.workspace}${KEY_VALUE_SEPARATOR}${quoteIfNeeded(filters.workspace)}`);
  }
  if (filters.fleet !== null) {
    parts.push(`${FILTER_KEY.fleet}${KEY_VALUE_SEPARATOR}${quoteIfNeeded(filters.fleet)}`);
  }
  return parts.join(" ");
}

export function shortWorkspaceId(workspaceId: string): string {
  return workspaceId.length > WORKSPACE_ID_DISPLAY_CHARS
    ? `${workspaceId.slice(0, WORKSPACE_ID_DISPLAY_CHARS)}${TRUNCATION_ELLIPSIS}`
    : workspaceId;
}
