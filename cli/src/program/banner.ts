// CLI version line. Per docs/DESIGN_SYSTEM.md "no decorative ASCII art":
// no emoji, no box-drawing border, no banner. The version line is one
// line — a pulse-cyan dot, the name, the version.

import { palette, glyph } from "../output/index.ts";
import type { WritableStreamLike } from "../output/capability.ts";

export interface PrintVersionOptions {
  jsonMode?: boolean | undefined;
  env?: NodeJS.ProcessEnv | undefined;
  noColor?: boolean | undefined;
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
    stream.write(`agentsfleet v${version}\n`);
    return;
  }

  const styleOpts = { stream, env };
  const dot = glyph.live(styleOpts).render();
  stream.write(`${dot} ${palette.text("agentsfleet")} ${palette.subtle(`v${version}`, styleOpts)}\n`);
}
