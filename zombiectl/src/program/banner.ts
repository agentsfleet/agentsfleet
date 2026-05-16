// CLI version line and pre-release notice. Per docs/DESIGN_SYSTEM.md
// "no decorative ASCII art": no emoji, no box-drawing border, no
// banner. The version line is one line — a pulse-cyan dot, the name,
// the version. Pre-release stays a one-liner with a functional ⚠ glyph.

import { palette, glyph } from "../output/index.ts";
import type { WritableStreamLike } from "../output/capability.ts";

export interface PrintVersionOptions {
  jsonMode?: boolean | undefined;
  env?: NodeJS.ProcessEnv | undefined;
  noColor?: boolean | undefined;
}

export interface PrintPreReleaseWarningOptions {
  jsonMode?: boolean | undefined;
  env?: NodeJS.ProcessEnv | undefined;
  noColor?: boolean | undefined;
  ttyOnly?: boolean | undefined;
}

function resolveEnv(opts: { env?: NodeJS.ProcessEnv | undefined }): NodeJS.ProcessEnv {
  if (opts.env) return opts.env;
  return typeof process !== "undefined" ? process.env : ({} as NodeJS.ProcessEnv);
}

function resolveNoColor(opts: { noColor?: boolean | undefined }, env: NodeJS.ProcessEnv): boolean {
  const envNoColor = typeof env.NO_COLOR === "string" && env.NO_COLOR.length > 0;
  return Boolean(opts.noColor) || envNoColor;
}

export function printVersion(
  stream: WritableStreamLike,
  version: string,
  opts: PrintVersionOptions = {},
): void {
  if (opts.jsonMode) return;

  const env = resolveEnv(opts);
  const noColor = resolveNoColor(opts, env);

  if (noColor) {
    stream.write(`zombiectl v${version}\n`);
    return;
  }

  const styleOpts = { stream, env };
  const dot = glyph.live(styleOpts).render();
  stream.write(`${dot} ${palette.text("zombiectl")} ${palette.subtle(`v${version}`, styleOpts)}\n`);
}

export function printPreReleaseWarning(
  stream: WritableStreamLike,
  opts: PrintPreReleaseWarningOptions = {},
): void {
  const env = resolveEnv(opts);
  const noColor = resolveNoColor(opts, env);
  const jsonMode = opts.jsonMode || false;
  const ttyOnly = opts.ttyOnly || false;

  if (jsonMode) return;
  if (ttyOnly) return;

  if (noColor) {
    stream.write(`\n[PRE-RELEASE] This is a pre-release build for early access testing.\n`);
    stream.write(`Contact nkishore@megam.io to get access.\n\n`);
    return;
  }

  const styleOpts = { stream, env };
  const warnGlyph = glyph.warn(styleOpts).render();
  const tag = `${warnGlyph} ${palette.warn("Pre-release build", styleOpts)}`;
  stream.write(`\n  ${tag} — not for production use.\n`);
  stream.write(`     Early access testing only. Contact ${palette.text("nkishore@megam.io")} to get access.\n\n`);
}
