// Terminal color-capability detection. Independent of every other output
// helper so tests can exercise it in isolation. Decision order is fixed:
// --json → NO_COLOR → !isTTY → FORCE_COLOR → TERM/COLORTERM.
//
// Returns one of: "none" (plain ASCII), "basic16" (8/16-color ANSI),
// "xterm256" (256-color). Helpers in palette.js gate every escape on this.

const MODE_NONE = "none";
const MODE_BASIC16 = "basic16";
const MODE_XTERM256 = "xterm256";

// One-shot guard so the basic16 fallback notice fires once per process,
// not on every styled call. Tests can reset via resetCapabilityWarning().
let warnedBasic16 = false;

export function resetCapabilityWarning() {
  warnedBasic16 = false;
}

export function detectColorMode(env = process.env, stream = process.stdout) {
  if (env.NO_COLOR && env.NO_COLOR.length > 0) return MODE_NONE;
  if (stream && stream.isTTY !== true) return MODE_NONE;

  const force = env.FORCE_COLOR;
  if (force === "0") return MODE_NONE;
  if (force === "1") return MODE_BASIC16;
  if (force === "2" || force === "3") return MODE_XTERM256;

  if (env.COLORTERM === "truecolor" || env.COLORTERM === "24bit") return MODE_XTERM256;

  const term = env.TERM ?? "";
  if (term === "" || term === "dumb") return MODE_NONE;
  if (term.includes("256color")) return MODE_XTERM256;
  if (term.includes("xterm") || term.includes("screen") || term.includes("vt100")) {
    return MODE_BASIC16;
  }
  return MODE_BASIC16;
}

export function isTty(stream = process.stdout) {
  return Boolean(stream && stream.isTTY === true);
}

// Emit a one-shot stderr notice when the resolved mode is basic16 — engineers
// on legacy terminals see a single line, not a stream. Silent in xterm256
// and none modes. Callers route through palette.js, not directly.
export function noteBasic16IfFirst(stream = process.stderr) {
  if (warnedBasic16) return;
  warnedBasic16 = true;
  if (!stream || stream.isTTY !== true) return;
  stream.write("note: terminal advertises <256 colors; using basic palette\n");
}

export const ColorMode = {
  NONE: MODE_NONE,
  BASIC16: MODE_BASIC16,
  XTERM256: MODE_XTERM256,
};
