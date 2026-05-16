// ZombieHelp — commander.Help subclass that preserves zombiectl's
// existing color scheme when commander renders --help. Wired by
// cli-tree.ts via `program.createHelp = () => new ZombieHelp()`
// (configureHelp's `helpFactory` key is unsupported in commander 14).
// Capability resolution (NO_COLOR, isTTY, ColorMode) flows through
// formatHelpHeading + palette.subtle, both of which accept a
// `{stream, env}` opts object so tests can drive each mode without
// touching globals.

import { Help } from "commander";
import { formatHelpHeading, palette } from "../output/index.ts";
import type { WritableStreamLike } from "../output/capability.ts";

export interface ZombieHelpOptions {
  stream?: WritableStreamLike | undefined;
  env?: NodeJS.ProcessEnv | undefined;
}

interface ResolvedStyleOpts {
  stream: WritableStreamLike;
  env: NodeJS.ProcessEnv;
}

export class ZombieHelp extends Help {
  private readonly styleOpts: ResolvedStyleOpts;

  constructor({ stream, env }: ZombieHelpOptions = {}) {
    super();
    this.styleOpts = {
      stream: stream ?? process.stdout,
      env: env ?? process.env,
    };
  }

  // Bold pulse-cyan section headers. Commander 14 calls this hook once
  // per section ("Usage:", "Options:", "Commands:", program description).
  override styleTitle(title: string): string {
    return formatHelpHeading(title, this.styleOpts);
  }
}

// Tagline helper — the dim "usezombie cli" grey under the version
// banner. cli-tree.ts passes it to `program.description()` pre-styled,
// since commander renders the description through styleDescriptionText
// (default identity) before printing.
export function styleTagline(text: string, { stream, env }: ZombieHelpOptions = {}): string {
  return palette.subtle(text, {
    stream: stream ?? process.stdout,
    env: env ?? process.env,
  });
}
