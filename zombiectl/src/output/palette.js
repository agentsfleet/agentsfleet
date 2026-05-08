// Single source of ANSI escape codes mapped to design-system tokens.
// Every other module composes through these helpers; nothing builds an
// escape sequence inline. Mapping per docs/DESIGN_SYSTEM.md "CLI /
// zombiectl rendering" section.

import { ColorMode, detectColorMode, noteBasic16IfFirst } from "./capability.js";

// xterm256 codes — the canonical mapping. Indexed by token name.
const XTERM_256 = {
  pulse:     79,   // cyan2 — closest to --pulse #5EEAD4
  evidence:  220,  // gold1 — closest to --evidence #FBBF24
  success:   78,
  warn:      214,
  error:     210,
  muted:     102,  // grey53 — closest to --text-muted
  subtle:    240,  // closest to --text-subtle
};

// basic16 fallback — used when terminal advertises <256 colors. SGR
// codes per ECMA-48: 30-37 dim, 90-97 bright. We pick the closest hue
// match, accepting that 256→16 always loses fidelity.
const BASIC_16 = {
  pulse:    "36",  // cyan
  evidence: "33",  // yellow
  success:  "32",  // green
  warn:     "33",  // yellow (warn lands on the same bin as evidence in 16-color terms)
  error:    "31",  // red
  muted:    "2",   // dim
  subtle:   "2",   // dim (no separate subtle in 16-color)
};

const RESET = "[0m";
const ESC = "[";

// Render a styled string using the given token. mode/stream injection lets
// tests exercise specific capability paths without setting global env state.
function styled(token, text, opts = {}) {
  const env = opts.env ?? process.env;
  const stream = opts.stream ?? process.stdout;
  const mode = opts.mode ?? detectColorMode(env, stream);

  if (mode === ColorMode.NONE) return String(text);
  if (mode === ColorMode.BASIC16) {
    noteBasic16IfFirst(opts.warnStream ?? process.stderr);
    return `${ESC}${BASIC_16[token]}m${text}${RESET}`;
  }
  return `${ESC}38;5;${XTERM_256[token]}m${text}${RESET}`;
}

// Bold variant — used by helpHeading + version banner brand-mark. Bold
// sequence layered before the color so basic16 mode still bolds.
function styledBold(token, text, opts = {}) {
  const env = opts.env ?? process.env;
  const stream = opts.stream ?? process.stdout;
  const mode = opts.mode ?? detectColorMode(env, stream);

  if (mode === ColorMode.NONE) return String(text);
  if (mode === ColorMode.BASIC16) {
    noteBasic16IfFirst(opts.warnStream ?? process.stderr);
    return `${ESC}1;${BASIC_16[token]}m${text}${RESET}`;
  }
  return `${ESC}1;38;5;${XTERM_256[token]}m${text}${RESET}`;
}

// Bold-only (no color) variant. Used for chrome that needs weight but
// not currency — section titles, table headers. Capability-aware so
// NO_COLOR / !isTTY mode emits plain text.
function bold(text, opts = {}) {
  const env = opts.env ?? process.env;
  const stream = opts.stream ?? process.stdout;
  const mode = opts.mode ?? detectColorMode(env, stream);
  if (mode === ColorMode.NONE) return String(text);
  return `${ESC}1m${text}${RESET}`;
}

// Token helpers — every site that emits color goes through one of these.
// Add a new token here, not at the call site.
export const palette = {
  pulse:    (text, opts) => styled("pulse", text, opts),
  evidence: (text, opts) => styled("evidence", text, opts),
  success:  (text, opts) => styled("success", text, opts),
  warn:     (text, opts) => styled("warn", text, opts),
  error:    (text, opts) => styled("error", text, opts),
  muted:    (text, opts) => styled("muted", text, opts),
  subtle:   (text, opts) => styled("subtle", text, opts),
  text:     (text)       => String(text),
  bold,

  pulseBold: (text, opts) => styledBold("pulse", text, opts),
};

// Test/inspection surface. Production callers should not import these;
// they exist so unit tests can assert the exact mapping without redoing
// the lookup logic.
export const PALETTE_INTERNALS = {
  XTERM_256,
  BASIC_16,
  RESET,
  ESC,
};
