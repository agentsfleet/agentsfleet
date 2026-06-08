// Single source of ANSI escape codes mapped to design-system tokens.
// Every other module composes through these helpers; nothing builds an
// escape sequence inline. Mapping per docs/DESIGN_SYSTEM.md "CLI /
// zombiectl rendering" section.

import {
  ColorMode,
  detectColorMode,
  noteBasic16IfFirst,
  type ColorModeValue,
  type IsTtyStream,
  type WritableStreamLike,
} from "./capability.ts";

const VALUE_33 = "33" as const;
const STATUS_ERROR = "error" as const;
const PALETTE_TOKEN_EVIDENCE = "evidence" as const;
const PALETTE_TOKEN_MUTED = "muted" as const;
const PALETTE_TOKEN_PULSE = "pulse" as const;
const PALETTE_TOKEN_SUBTLE = "subtle" as const;
const PALETTE_TOKEN_SUCCESS = "success" as const;
const PALETTE_TOKEN_WARN = "warn" as const;

export type Token = typeof PALETTE_TOKEN_PULSE | typeof PALETTE_TOKEN_EVIDENCE | typeof PALETTE_TOKEN_SUCCESS | typeof PALETTE_TOKEN_WARN | typeof STATUS_ERROR | typeof PALETTE_TOKEN_MUTED | typeof PALETTE_TOKEN_SUBTLE;

export interface StyleOpts {
  readonly env?: NodeJS.ProcessEnv;
  readonly stream?: IsTtyStream;
  readonly mode?: ColorModeValue;
  readonly warnStream?: WritableStreamLike;
}

// xterm256 codes — the canonical mapping. Indexed by token name.
const XTERM_256: Record<Token, number> = {
  pulse:     79,   // cyan2 — closest to --pulse #5EEAD4
  evidence:  220,  // gold1 — closest to --evidence #FBBF24
  success:   78,
  warn:      214,
  error:     210,
  muted:     102,  // grey53 — closest to --text-muted #8B9398
  // --text-subtle lifted from #5C6469 → #7A8085 for AA contrast.
  // xterm256 244 (#808080) is the new closest match (was 240 #585858).
  subtle:    244,
};

// basic16 fallback — used when terminal advertises <256 colors. SGR
// codes per ECMA-48: 30-37 dim, 90-97 bright. We pick the closest hue
// match, accepting that 256→16 always loses fidelity.
const BASIC_16: Record<Token, string> = {
  pulse:    "36",  // cyan
  evidence: VALUE_33,  // yellow
  success:  "32",  // green
  warn:     VALUE_33,  // yellow (warn lands on the same bin as evidence in 16-color terms)
  error:    "31",  // red
  muted:    "2",   // dim
  subtle:   "2",   // dim (no separate subtle in 16-color)
};

const RESET = "[0m";
const ESC = "[";

// Render a styled string using the given token. mode/stream injection lets
// tests exercise specific capability paths without setting global env state.
function styled(token: Token, text: unknown, opts: StyleOpts = {}): string {
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
function styledBold(token: Token, text: unknown, opts: StyleOpts = {}): string {
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
function bold(text: unknown, opts: StyleOpts = {}): string {
  const env = opts.env ?? process.env;
  const stream = opts.stream ?? process.stdout;
  const mode = opts.mode ?? detectColorMode(env, stream);
  if (mode === ColorMode.NONE) return String(text);
  return `${ESC}1m${text}${RESET}`;
}

export type Painter = (text: unknown, opts?: StyleOpts) => string;

export interface Palette {
  readonly pulse: Painter;
  readonly evidence: Painter;
  readonly success: Painter;
  readonly warn: Painter;
  readonly error: Painter;
  readonly muted: Painter;
  readonly subtle: Painter;
  readonly text: (text: unknown) => string;
  readonly bold: Painter;
  readonly pulseBold: Painter;
}

// Token helpers — every site that emits color goes through one of these.
// Add a new token here, not at the call site.
export const palette: Palette = {
  pulse:    (text, opts) => styled(PALETTE_TOKEN_PULSE, text, opts),
  evidence: (text, opts) => styled(PALETTE_TOKEN_EVIDENCE, text, opts),
  success:  (text, opts) => styled(PALETTE_TOKEN_SUCCESS, text, opts),
  warn:     (text, opts) => styled(PALETTE_TOKEN_WARN, text, opts),
  error:    (text, opts) => styled(STATUS_ERROR, text, opts),
  muted:    (text, opts) => styled(PALETTE_TOKEN_MUTED, text, opts),
  subtle:   (text, opts) => styled(PALETTE_TOKEN_SUBTLE, text, opts),
  text:     (text)       => String(text),
  bold,

  pulseBold: (text, opts) => styledBold(PALETTE_TOKEN_PULSE, text, opts),
};

// Test/inspection surface. Production callers should not import these;
// they exist so unit tests can assert the exact mapping without redoing
// the lookup logic.
export const PALETTE_INTERNALS = {
  XTERM_256,
  BASIC_16,
  RESET,
  ESC,
} as const;
