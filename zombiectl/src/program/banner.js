// CLI version line and pre-release notice. Per docs/DESIGN_SYSTEM.md
// "no decorative ASCII art": no emoji, no box-drawing border, no
// banner. The version line is one line — a pulse-cyan dot, the name,
// the version. Pre-release stays a one-liner with a functional ⚠ glyph.

import { palette, glyph } from "../output/index.js";

export function printVersion(stream, version, opts = {}) {
  if (opts.jsonMode) return;

  const envNoColor = typeof process !== "undefined" && process.env && process.env.NO_COLOR
    ? process.env.NO_COLOR.length > 0
    : false;
  const noColor = Boolean(opts.noColor) || envNoColor;

  if (noColor) {
    stream.write(`zombiectl v${version}\n`);
    return;
  }

  const dot = glyph.live({ stream }).render();
  stream.write(`${dot} ${palette.text("zombiectl")} ${palette.subtle(`v${version}`, { stream })}\n`);
}

export function printPreReleaseWarning(stream, opts = {}) {
  const noColor = opts.noColor || false;
  const jsonMode = opts.jsonMode || false;
  const ttyOnly = opts.ttyOnly || false;

  if (jsonMode) return;
  if (ttyOnly) return;

  if (noColor) {
    stream.write(`\n[PRE-RELEASE] This is a pre-release build for early access testing.\n`);
    stream.write(`Contact nkishore@megam.io to get access.\n\n`);
    return;
  }

  const warnGlyph = glyph.warn({ stream }).render();
  const tag = `${warnGlyph} ${palette.warn("Pre-release build", { stream })}`;
  stream.write(`\n  ${tag} — not for production use.\n`);
  stream.write(`     Early access testing only. Contact ${palette.text("nkishore@megam.io")} to get access.\n\n`);
}
