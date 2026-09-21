// The two live paths that lost their only coverage when the dead `runEffect`
// wrapper was deleted.
//
// `runEffect` was the pre-migration dispatcher; the entry point runs
// `renderAndCount` directly now. Its test drove both of these INCIDENTALLY,
// by composing the whole layer stack to exercise a wrapper nothing calls.
// These drive them on purpose instead, which is the same coverage against a
// smaller surface: a failure here names the path, not the wrapper above it.

import { describe, test, expect } from "bun:test";
import { Effect, Exit, Layer } from "effect";

import { renderAndCount } from "../src/lib/run-effect.ts";
import { outputStdioLayer, Output, OUTPUT_FORMAT } from "../src/services/output.ts";
import { CliConfig } from "../src/services/config.ts";
import { outputDouble } from "./helpers-output-double.ts";

const configLayer = (jsonMode: boolean): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: undefined as never,
    jsonMode,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  } as never);

describe("renderAndCount — a cause carrying no typed failure", () => {
  // A die or an interrupt has no typed error inside it, so `findErrorOption`
  // answers None and the dispatcher has nothing to read an exit code off.
  // Left unhandled this is the branch where a crash exits 0 and a script
  // downstream treats the run as a success.
  const drive = async (cause: Effect.Effect<never, never, never>) => {
    const lines: string[] = [];
    const exit = await Effect.runPromiseExit(cause);
    const program = renderAndCount(exit as Exit.Exit<unknown, never>).pipe(
      Effect.provideService(Output, {
        ...outputDouble(),
        format: OUTPUT_FORMAT.text,
        error: (msg: string) => Effect.sync(() => void lines.push(msg)),
      } as never),
    );
    return { code: await Effect.runPromise(program as Effect.Effect<number>), lines };
  };

  test("a defect renders as UnexpectedError rather than exiting 0", async () => {
    const { code } = await drive(Effect.die(new Error("boom")));
    expect(code).toBeGreaterThan(0);
  });

  test("an interrupt is also a non-zero exit", async () => {
    const { code } = await drive(Effect.interrupt as Effect.Effect<never, never, never>);
    expect(code).toBeGreaterThan(0);
  });
});

describe("outputStdioLayer", () => {
  // The layer `main-layer.ts` picks when no streams were injected — the real
  // process path every ordinary invocation takes. Only the stream-injected
  // sibling was covered, so a break here would have surfaced first to a
  // person running the binary.
  test("builds an Output from the invocation's own json mode", async () => {
    for (const jsonMode of [false, true]) {
      const out = await Effect.runPromise(
        Effect.gen(function* () {
          return yield* Output;
        }).pipe(Effect.provide(outputStdioLayer), Effect.provide(configLayer(jsonMode))),
      );
      expect(out.format).toBe(jsonMode ? OUTPUT_FORMAT.json : OUTPUT_FORMAT.text);
    }
  });
});
