import { describe, expect, test } from "bun:test";
import { Effect, Exit, Layer } from "effect";

import { apiKeyListEffectFromArgs } from "../src/commands/api-key/api-key.ts";
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

// The walk bound this list once carried; a longer list is now read whole.
const RETIRED_WALK_PAGES = 40;

describe("api-key list walk", () => {
  test("refuses a cursor the walk already followed instead of walking forever", async () => {
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
    expect(err?.detail).toBe("the API key list repeated page cursor same");
    expect(paths).toHaveLength(2);
  });

  test("walks past the retired page bound until the server reports the end", async () => {
    const cap = newCapture();
    const paths: string[] = [];
    const pages = RETIRED_WALK_PAGES + 5;
    const http = Layer.succeed(HttpClient, {
      request: <T,>(input: { path: string }) => Effect.sync(() => {
        paths.push(input.path);
        const page = paths.length;
        return {
          items: [{ id: `key-${page}`, key_name: `key ${page}`, active: true }],
          total: pages,
          next_cursor: page < pages ? `cursor-${page}` : null,
        } as T;
      }),
    });
    const exit = await runApiKey(apiKeyListEffectFromArgs({ sort: undefined }), cap, http);

    expect(Exit.isSuccess(exit)).toBe(true);
    expect(paths).toHaveLength(pages);
    expect(paths[pages - 1]).toContain(`starting_after=cursor-${pages - 1}`);
  });
});
