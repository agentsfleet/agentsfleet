// ZombieHelp — commander.Help subclass that preserves zombiectl's
// existing color scheme when commander renders --help. Wired by
// cli-tree.js via `program.configureHelp({ helpFactory: ... })`.
// Capability resolution (NO_COLOR, isTTY, ColorMode) flows through
// formatHelpHeading + palette.subtle, both of which accept a
// `{stream, env}` opts object so tests can drive each mode without
// touching globals.

import { Help } from "commander";
import { formatHelpHeading, palette } from "../output/index.js";

export class ZombieHelp extends Help {
  constructor({ stream, env } = {}) {
    super();
    this.styleOpts = {
      stream: stream ?? process.stdout,
      env: env ?? process.env,
    };
  }

  // Bold pulse-cyan section headers. Commander 14 calls this hook once
  // per section ("Usage:", "Options:", "Commands:", program description).
  styleTitle(title) {
    return formatHelpHeading(title, this.styleOpts);
  }
}

// Tagline helper — the dim "autonomous agent platform" grey under
// the version banner. cli-tree.js passes it to `program.description()`
// pre-styled, since commander renders the description through
// styleDescriptionText (default identity) before printing.
export function styleTagline(text, { stream, env } = {}) {
  return palette.subtle(text, {
    stream: stream ?? process.stdout,
    env: env ?? process.env,
  });
}
