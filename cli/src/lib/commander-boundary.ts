// Everything that happens where commander's parse stage meets the house
// error surface: the output wiring, the rejection re-render, and the exit
// mapping.
//
// Why the render is deferred rather than done in `outputError`: commander
// writes its message BEFORE it throws, so the hook that sees the text has no
// access to the `commander.*` code, and the catch site that has the code has
// no access to the command that raised it. `captureRejection` records the
// command-side half, `renderRejection` joins it with the code at the catch
// site, and exactly one line reaches stderr.

import type { Command, CommanderError } from "commander";
import {
  CLI_NAME,
  COMMANDER_CODE_TO_REJECTION,
  COMMANDER_ERROR_PREFIX,
  COMMANDER_HELP_HINT,
  HELP_FLAG,
  HELP_HINT_PREFIX,
  HELP_HINT_SUFFIX,
  SUGGESTION_PREFIX,
  USAGE_PREFIX,
  type RejectionCode,
} from "../constants/rejection.ts";

const HELP_CODES: ReadonlySet<string> = new Set([
  "commander.help",
  "commander.helpDisplayed",
]);

// commander.* codes that mean "the invocation was wrong". Every member maps
// to the validation exit code, so exit 2 is left to mean transport failure
// alone — see EXIT_CODE in errors/index.ts.
const COMMANDER_USAGE_CODES: ReadonlySet<string> = new Set(
  Object.keys(COMMANDER_CODE_TO_REJECTION),
);

export interface WritableStreamLike {
  write(chunk: string): unknown;
}

export interface PendingRejection {
  readonly detail: string;
  readonly usageLine: string;
}

export interface ProgramExitState {
  exitCode: number;
}

export function commandPath(cmd: Command): string {
  const segments: string[] = [];
  for (let node: Command | null = cmd; node; node = node.parent) {
    const name = node.name();
    if (name && name !== CLI_NAME) segments.unshift(name);
  }
  return [CLI_NAME, ...segments].join(" ");
}

// A leaf gets `usage: agentsfleet events [options] <fleet_id>` — runnable as
// printed. A group or the root gets pointed at its command list instead,
// because "usage: agentsfleet [options] [command]" names no command the
// operator could actually run next.
export function commandUsageLine(cmd: Command): string {
  const path = commandPath(cmd);
  if (cmd.commands.length > 0) return `${HELP_HINT_PREFIX}${path} ${HELP_FLAG}${HELP_HINT_SUFFIX}`;
  const usage = cmd.usage();
  return `${USAGE_PREFIX}${path}${usage ? ` ${usage}` : ""}`;
}

// Returns null for commander's showHelpAfterError hint, which the Suggestion
// line replaces, and for anything already recorded.
export function captureRejection(
  text: string,
  cmd: Command,
  existing: PendingRejection | null,
): PendingRejection | null {
  if (existing) return existing;
  const trimmed = text.trim();
  if (!trimmed || trimmed === COMMANDER_HELP_HINT) return null;
  const detail = trimmed.startsWith(COMMANDER_ERROR_PREFIX)
    ? trimmed.slice(COMMANDER_ERROR_PREFIX.length)
    : trimmed;
  return { detail, usageLine: commandUsageLine(cmd) };
}

export function rejectionCodeFor(commanderCode: string): RejectionCode | null {
  return COMMANDER_CODE_TO_REJECTION[commanderCode] ?? null;
}

// Human mode returns the body the `error: ` stem and ✕ glyph wrap around;
// JSON mode returns the envelope RULE JCL pins, so a machine consumer parses
// the same failure it would get from any other command.
export function renderRejection(
  pending: PendingRejection,
  code: RejectionCode | null,
  jsonMode: boolean,
): string {
  if (jsonMode) {
    return JSON.stringify(
      { error: { code: code ?? null, message: pending.detail } },
      null,
      2,
    );
  }
  return `${pending.detail}${SUGGESTION_PREFIX}${pending.usageLine}`;
}

export function isUsageRejection(commanderCode: string): boolean {
  return COMMANDER_USAGE_CODES.has(commanderCode);
}

export function exitFromCommanderError(
  err: CommanderError,
  state: ProgramExitState,
  usageExitCode: number,
): number {
  if (HELP_CODES.has(err.code)) return 0;
  if (state.exitCode !== 0) return state.exitCode;
  return isUsageRejection(err.code) ? usageExitCode : err.exitCode;
}

// commander 14 scopes exitOverride and configureOutput to the command they
// are called on, so a subcommand's parse rejection would otherwise call
// process.exit and write to the real stderr, bypassing both the Effect
// bridge and the injected test streams. Walk the whole tree.
export function applyOutputToTree(
  cmd: Command,
  stdout: WritableStreamLike,
  stderr: WritableStreamLike,
  onRejection: (text: string, cmd: Command) => void,
): void {
  cmd.exitOverride();
  cmd.configureOutput({
    writeOut: (s: string) => {
      stdout.write(s);
    },
    writeErr: (s: string) => {
      stderr.write(s);
    },
    outputError: (s: string) => {
      onRejection(s, cmd);
    },
  });
  for (const sub of cmd.commands) applyOutputToTree(sub, stdout, stderr, onRejection);
}

// A group node carries subcommands but no action of its own, so commander
// answers a bare `agentsfleet workspace` with `help({ error: true })`: the
// body lands on stderr while the process exits 0, so `| less` reads nothing.
//
// Resolving the invocation here, before parse, is what fixes it. Inferring it
// from the write stream instead does not work — commander emits the help body
// and any addHelpText tail as separate writes, so a body-matching filter
// splits the tail onto the other stream. Attaching an action to the group
// does not work either: commander then treats an unknown subcommand as an
// excess argument and stops reporting it as an unknown command.
//
// Only a bare path qualifies. Anything carrying a flag stays with commander.
export function resolveBareGroup(root: Command, argv: readonly string[]): Command | null {
  if (argv.length === 0 || argv.some((token) => token.startsWith("-"))) return null;
  let cmd: Command = root;
  for (const token of argv) {
    const next: Command | undefined = cmd.commands.find(
      (c) => c.name() === token || c.aliases().includes(token),
    );
    if (!next) return null;
    cmd = next;
  }
  const hasAction = typeof (cmd as { _actionHandler?: unknown })._actionHandler === "function";
  return cmd.commands.length > 0 && !hasAction ? cmd : null;
}
