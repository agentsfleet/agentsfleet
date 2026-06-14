// withCommandInstrumentation coverage. Adapted from Supabase's
// command-instrumentation.unit.test.ts — usezombie reads argv from
// process.argv.slice(2) instead of a Stdio service, so the tests stub
// process.argv in beforeEach. CommandRuntime is provided via the
// dedicated fixture layer (commandRuntimeFromValuesLayer).

import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { Effect, Exit, Layer, Option } from "effect";
import {
  CommandRuntime,
  commandRuntimeFromValuesLayer,
} from "../../src/runtime/command-runtime.service.ts";
import {
  CurrentAnalyticsContext,
  withAnalyticsContext,
} from "../../src/services/telemetry/analytics-context.ts";
import { Analytics } from "../../src/services/telemetry/analytics.service.ts";
import { withCommandInstrumentation } from "../../src/services/telemetry/command-instrumentation.ts";

interface CapturedEvent {
  event: string;
  properties: Record<string, unknown>;
}

function mockContextualAnalytics() {
  const captured: CapturedEvent[] = [];
  const layer = Layer.succeed(
    Analytics,
    Analytics.of({
      capture: (event: string, properties: Record<string, unknown> = {}) =>
        Effect.gen(function* () {
          const context = yield* CurrentAnalyticsContext;
          captured.push({
            event,
            properties: { ...context, ...properties },
          });
        }),
      identify: () => Effect.void,
      alias: () => Effect.void,
      groupIdentify: () => Effect.void,
    }),
  );
  return { layer, captured };
}

function commandLayer(commandPath: ReadonlyArray<string>) {
  return commandRuntimeFromValuesLayer({
    commandPath,
    commandRunId: "rid-fixture-1",
  });
}

let savedArgv: string[];

beforeEach(() => {
  savedArgv = process.argv;
});

afterEach(() => {
  process.argv = savedArgv;
});

function setArgv(args: string[]): void {
  process.argv = ["node", "agentsfleet", ...args];
}

describe("withCommandInstrumentation", () => {
  it("creates a command span and annotates it with command metadata (analytics off)", async () => {
    setArgv(["workspace", "list"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.gen(function* () {
      const span = yield* Effect.currentSpan;
      expect(span.name).toBe("cli.workspace.list");
      expect(span.attributes.get("command")).toBe("workspace list");
      expect(typeof span.attributes.get("command_run_id")).toBe("string");
    }).pipe(
      withCommandInstrumentation({ analytics: false }),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["workspace", "list"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured).toEqual([]);
  });

  it("emits cli_command_executed with command / flags_used / flag_values / exit_code / duration_ms", async () => {
    setArgv(["start", "--detach", "--exclude=auth"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.gen(function* () {
      yield* Effect.void;
    }).pipe(
      withCommandInstrumentation(),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["start"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured).toHaveLength(1);
    const evt = analytics.captured[0]!;
    expect(evt.event).toBe("cli_command_executed");
    expect(evt.properties.command).toBe("start");
    expect(evt.properties.flags_used).toEqual(["detach", "exclude"]);
    expect(evt.properties.flag_values).toEqual({});
    expect(evt.properties.exit_code).toBe(0);
    expect(typeof evt.properties.duration_ms).toBe("number");
    expect(typeof evt.properties.command_run_id).toBe("string");
  });

  it("shares command_run_id across user captures and the cli_command_executed event", async () => {
    setArgv(["start"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.gen(function* () {
      const svc = yield* Analytics;
      const ctx = yield* CurrentAnalyticsContext;
      yield* svc.capture("cli_stack_started", { command_run_id: ctx.command_run_id });
    }).pipe(
      withCommandInstrumentation(),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["start"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured).toHaveLength(2);
    const [milestone, command] = analytics.captured;
    expect(milestone?.event).toBe("cli_stack_started");
    expect(command?.event).toBe("cli_command_executed");
    expect(typeof milestone?.properties.command_run_id).toBe("string");
    expect(milestone?.properties.command_run_id).toBe(command?.properties.command_run_id);
  });

  it("captures failed commands with exit_code=1 and re-raises the cause", async () => {
    setArgv(["login"]);
    const analytics = mockContextualAnalytics();
    const program = withCommandInstrumentation()(Effect.fail(new Error("boom"))).pipe(
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["login"])),
      Effect.exit,
    );
    const exit = await Effect.runPromise(program);
    expect(Exit.isFailure(exit)).toBe(true);
    expect(analytics.captured).toHaveLength(1);
    expect(analytics.captured[0]?.event).toBe("cli_command_executed");
    expect(analytics.captured[0]?.properties.exit_code).toBe(1);
  });

  it("captures flag values only for keys in the allowedFlagValues allowlist", async () => {
    setArgv([
      "start",
      "--detach",
      "--mode=docker",
      "--exclude",
      "auth",
      "--exclude",
      "storage",
    ]);
    const analytics = mockContextualAnalytics();
    const program = Effect.void.pipe(
      withCommandInstrumentation({
        flags: {
          stack: "default",
          mode: "docker" as const,
          exclude: ["auth", "storage"],
          serviceVersion: [] as string[],
          detach: true,
        },
        allowedFlagValues: ["exclude", "mode", "stack"],
      }),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["start"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured).toHaveLength(1);
    const evt = analytics.captured[0]!;
    expect(evt.properties.flags_used).toEqual(["detach", "exclude", "mode"]);
    expect(evt.properties.flag_values).toEqual({
      exclude: ["auth", "storage"],
      mode: "docker",
    });
  });

  it("emits kebab-case keys for camelCase allowlist entries only when the flag is used", async () => {
    setArgv(["login", "--name", "my-machine", "--no-browser"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.void.pipe(
      withCommandInstrumentation({
        flags: {
          token: Option.none<string>(),
          name: "my-machine",
          noBrowser: true,
        },
        allowedFlagValues: ["token", "name", "noBrowser"],
      }),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["login"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured).toHaveLength(1);
    const evt = analytics.captured[0]!;
    expect(evt.properties.flags_used).toEqual(["name", "no-browser"]);
    expect(evt.properties.flag_values).toMatchObject({
      name: "my-machine",
      "no-browser": true,
    });
  });

  it("skips analytics capture when analytics: false", async () => {
    setArgv(["telemetry", "enable"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.sync(() => "ok").pipe(
      withCommandInstrumentation({ analytics: false }),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["telemetry", "enable"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured).toEqual([]);
  });

  it("ignores undefined flag values in flag_values output", async () => {
    setArgv(["start", "--mode=docker"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.void.pipe(
      withCommandInstrumentation({
        flags: { mode: "docker", absent: undefined as unknown as string },
        allowedFlagValues: ["mode", "absent"],
      }),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["start"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured[0]?.properties.flag_values).toEqual({ mode: "docker" });
  });

  it("collapses duplicate --flag occurrences in flags_used to a single sorted entry", async () => {
    setArgv(["start", "--exclude=a", "--exclude", "b", "--exclude=c"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.void.pipe(
      withCommandInstrumentation(),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["start"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured[0]?.properties.flags_used).toEqual(["exclude"]);
  });

  it("ignores positional arguments (only --flags count as flags_used)", async () => {
    setArgv(["workspace", "add", "my-ws"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.void.pipe(
      withCommandInstrumentation(),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["workspace", "add"])),
    );
    await Effect.runPromise(program);
    expect(analytics.captured[0]?.properties.flags_used).toEqual([]);
  });

  it("propagates outer withAnalyticsContext groups into the wrapped command body", async () => {
    setArgv(["start"]);
    const analytics = mockContextualAnalytics();
    const program = Effect.gen(function* () {
      const ctx = yield* CurrentAnalyticsContext;
      expect(ctx.groups).toEqual({ workspace: "ws-1" });
    }).pipe(
      withAnalyticsContext({ groups: { workspace: "ws-1" } }),
      withCommandInstrumentation({ analytics: false }),
      Effect.provide(analytics.layer),
      Effect.provide(commandLayer(["start"])),
    );
    await Effect.runPromise(program);
  });

  it("emits the configured CommandRuntime.commandRunId on the analytics event", async () => {
    setArgv(["start"]);
    const analytics = mockContextualAnalytics();
    const layer = commandRuntimeFromValuesLayer({
      commandPath: ["start"],
      commandRunId: "rid-custom-42",
    });
    const program = Effect.void.pipe(
      withCommandInstrumentation(),
      Effect.provide(analytics.layer),
      Effect.provide(layer),
    );
    await Effect.runPromise(program);
    expect(analytics.captured[0]?.properties.command_run_id).toBe("rid-custom-42");
  });

  it("touches CommandRuntime class constructor for coverage", async () => {
    const layer = commandRuntimeFromValuesLayer({
      commandPath: ["x"],
      commandRunId: "rid-2",
    });
    const out = await Effect.runPromise(
      Effect.gen(function* () {
        return yield* CommandRuntime;
      }).pipe(Effect.provide(layer)),
    );
    expect(out.commandRunId).toBe("rid-2");
  });
});
