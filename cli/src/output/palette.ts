// Single source of ANSI escape codes mapped to design-system tokens.
// Every other module composes through these helpers; nothing builds an
// escape sequence inline. Mapping per docs/DESIGN_SYSTEM.md "CLI /
// agentsfleet rendering" section.

import {
  ColorMode,
  detectColorMode,
  noteBasic16IfFirst,
  type ColorModeValue,
  type IsTtyStream,
  type WritableStreamLike,
} from "./capability.ts";

export type OrbitColor = keyof typeof ORBIT_COLORS;
type Pixel = OrbitColor | null;

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

// The approved Orbit sprite uses pale blues, dark eyes, blush and gold sparks.
const ORBIT_COLORS = {
  B: { rgb: [100, 151, 182], xterm: 110 }, M: { rgb: [160, 210, 223], xterm: 152 },
  L: { rgb: [200, 233, 236], xterm: 195 }, W: { rgb: [237, 250, 245], xterm: 231 },
  D: { rgb: [32, 62, 82], xterm: 23 }, P: { rgb: [232, 173, 195], xterm: 181 },
  G: { rgb: [232, 188, 118], xterm: 222 }, T: { rgb: [193, 189, 229], xterm: 183 },
} as const;
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
};

export function orbitRow(upper: readonly Pixel[], lower: readonly Pixel[], opts: StyleOpts): string {
  const env = opts.env ?? process.env;
  const mode = opts.mode ?? detectColorMode(env, opts.stream ?? process.stdout);
  const trueColor = env.FORCE_COLOR === "3" || (env.FORCE_COLOR !== "2" &&
    (env.COLORTERM === "truecolor" || env.COLORTERM === "24bit"));
  return upper.map((top, x) => {
    const bottom = lower[x] ?? null;
    const foreground = top ?? bottom;
    if (foreground === null) return " ";
    if (mode !== ColorMode.XTERM256) return top === null ? "_" : bottom === null ? "^" : "#";
    const front = orbitColor(foreground, false, trueColor);
    const back = top !== null && bottom !== null ? orbitColor(bottom, true, trueColor) : `${ESC}49m`;
    return `${front}${back}${top === null ? "▄" : "▀"}${RESET}`;
  }).join("");
}

/** Each redraw starts and ends at the reserved block's bottom, at column zero. */
export const terminal = {
  reserveRows: (rows: number): string => "\r\n".repeat(rows),
  repaintRows: (lines: readonly string[]): string => `${ESC}${lines.length}A\r${lines.map(line => `${ESC}2K${line}`).join("\r\n")}\r\n`,
  clearRows: (rows: number): string => `${ESC}${rows}A\r${Array.from({ length: rows }, () => `${ESC}2K`).join("\r\n")}${ESC}${rows - 1}A\r`,
};

function orbitColor(color: OrbitColor, background: boolean, trueColor: boolean): string {
  const value = ORBIT_COLORS[color];
  return trueColor ? `${ESC}${background ? 48 : 38};2;${value.rgb.join(";")}m`
    : `${ESC}${background ? 48 : 38};5;${value.xterm}m`;
}
