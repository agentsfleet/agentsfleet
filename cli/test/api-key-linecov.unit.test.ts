import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer } from "effect";

import { apiKeyListEffectFromArgs } from "../src/commands/api_key.ts";
import type { CliError } from "../src/errors/index.ts";
import { ValidationError } from "../src/errors/index.ts";
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
  test("rejects malformed integer strings before the API request", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const exit = await runApiKey(
      apiKeyListEffectFromArgs({ page: "1abc", pageSize: undefined, sort: undefined }),
      cap,
      httpLayerReturning({ items: [] }, paths),
    );

    expect(Exit.isFailure(exit)).toBe(true);
    const err = failureOf(exit);
    expect(err).toBeInstanceOf(ValidationError);
    expect(err?.detail).toContain("page must be an integer");
    expect(paths).toEqual([]);
  });
});
