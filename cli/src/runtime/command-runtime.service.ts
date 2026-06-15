// CommandRuntime — per-invocation command identity. Mirrors
// ~/Projects/oss/cli/apps/cli/src/shared/runtime/command-runtime.service.ts.
//
// Populated once per CLI invocation by the commander → Effect bridge
// (src/lib/commander-bridge.ts). Reads:
//   - commandPath: the resolved command name(s), e.g. ["workspace", "add"]
//   - commandRunId: a fresh UUID per invocation (correlates analytics
//     events, spans, and log lines emitted during this run)
//
// Forward-looking — keeps option (c) close: when commander is replaced
// with effect/unstable/cli's Command.runWith, this service is populated
// natively by the Command primitive — the consumer code (analytics
// layer, command-instrumentation, spans) does not change. Only the
// adapter that fills CommandRuntime moves from commander-bridge.ts to
// "wherever Command.runWith puts it".

import { Context, Layer } from "effect";

interface CommandRuntimeShape {
  readonly commandPath: ReadonlyArray<string>;
  readonly commandRunId: string;
}

export class CommandRuntime extends Context.Service<
  CommandRuntime,
  CommandRuntimeShape
>()("agentsfleet/runtime/CommandRuntime") {}

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
