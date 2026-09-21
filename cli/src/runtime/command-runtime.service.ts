// CommandRuntime — per-invocation command identity. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/runtime/command-runtime.service.ts.
//
// Populated once per CLI invocation by the entry point, from the command path
// it walks off the tree before the parser runs. Reads:
//   - commandPath: the resolved command name(s), e.g. ["workspace", "create"]
//   - commandRunId: a fresh UUID per invocation (correlates analytics
//     events, spans, and log lines emitted during this run)
//
// The path is resolved rather than parsed because the layer carrying it has
// to be built before `Command.runWith` executes — see
// program/tree/resolve-path.ts.

import { Context, Layer } from "effect";

interface CommandRuntimeShape {
  readonly commandPath: ReadonlyArray<string>;
  readonly commandRunId: string;
}

export type CommandRuntime = CommandRuntimeShape;
export const CommandRuntime = Context.Service<CommandRuntime>(
  "agentsfleet/runtime/CommandRuntime",
);

export function getCommandRuntimeCommand(rt: CommandRuntimeShape): string {
  return rt.commandPath.join(" ");
}

export function getCommandRuntimeSpanName(rt: CommandRuntimeShape): string {
  return `cli.${rt.commandPath.join(".")}`;
}

export const commandRuntimeFromValuesLayer = (
  values: CommandRuntimeShape,
): Layer.Layer<CommandRuntime> =>
  Layer.succeed(CommandRuntime, CommandRuntime.of(values));
