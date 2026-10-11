import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer } from "effect";

import { apiKeyListEffectFromArgs } from "../src/commands/api_key.ts";
import type { CliError } from "../src/errors/index.ts";
import { UnexpectedError } from "../src/errors/index.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import {
  configLayer,
  credentialsLayer,
  failureOf,
  httpLayerReturning,
  newCapture,
  outputLayer,
  type CapturedOutput,
} from "./helpers-memory-layers.ts";

const runApiKey = <E extends CliError>(
  effect: Effect.Effect<void, E, CliConfig | Output | HttpClient | Credentials>,
  cap: CapturedOutput,
  http: Layer.Layer<HttpClient>,
): Promise<Exit.Exit<void, E>> =>
  Effect.runPromiseExit(
    effect.pipe(
      Effect.provide(configLayer(false)),
      Effect.provide(outputLayer(cap)),
      Effect.provide(http),
      Effect.provide(credentialsLayer()),
    ),
  );

describe("api-key effect validation", () => {
  test("refuses a runaway next_cursor instead of walking forever", async () => {
    // A server that never returns a null cursor must trip the walk bound,
    // not spin the CLI indefinitely.
    const cap = newCapture();
    const paths: string[] = [];
    const exit = await runApiKey(
      apiKeyListEffectFromArgs({ sort: undefined }),
      cap,
      httpLayerReturning({ items: [], total: null, next_cursor: "same" }, paths),
    );

    expect(Exit.isFailure(exit)).toBe(true);
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(UnexpectedError);
    expect(err?.detail).toContain("did not end");
    expect(paths.length).toBeGreaterThan(1);
  });
});
