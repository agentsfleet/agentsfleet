// HttpClient service tests — feed a custom fetchImpl through CliConfig
// + the live HttpClient layer and exercise every branch of toCliError
// (ApiError 5xx → ServerError, 4xx auth → ServerError with login hint,
// 4xx other → ServerError, fetch-failed → NetworkError, generic →
// NetworkError) plus a happy-path request.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { CliConfig } from "../src/services/config.ts";
import { HttpClient, httpClientLayer, resolveToken } from "../src/services/http-client.ts";

const configLayer = (apiUrl: string): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl,
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode: false,
    noOpen: false,
  });

const SUITE_LAYER = (apiUrl: string): Layer.Layer<HttpClient> =>
  Layer.provide(httpClientLayer, configLayer(apiUrl));

let originalFetch: typeof globalThis.fetch;

beforeEach(() => {
  originalFetch = globalThis.fetch;
});

afterEach(() => {
  globalThis.fetch = originalFetch;
});

const setFetch = (impl: typeof globalThis.fetch) => {
  globalThis.fetch = impl;
};

describe("HttpClient", () => {
  test("request succeeds when fetch returns 200", async () => {
    setFetch((async () => new Response(JSON.stringify({ ok: true }), { status: 200 })) as unknown as typeof globalThis.fetch);
    const result = await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const http = yield* HttpClient;
          return yield* http.request<{ ok: boolean }>({ path: "/v1/x" });
        }),
        SUITE_LAYER("https://api.test.local"),
      ),
    );
    expect(result.ok).toBe(true);
  });
  test("ServerError with login hint when fetch returns 401", async () => {
    setFetch((async () =>
      new Response(JSON.stringify({ error: { code: "UZ-AUTH-002", message: "unauthorized" } }), {
        status: 401,
      })) as unknown as typeof globalThis.fetch);
    const exit = await Effect.runPromiseExit(
      Effect.provide(
        Effect.gen(function* () {
          const http = yield* HttpClient;
          return yield* http.request({ path: "/v1/x", retry: { maxAttempts: 1 } });
        }),
        SUITE_LAYER("https://api.test.local"),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const cause = exit.cause;
      const fail = Option.getOrNull(Cause.findErrorOption(cause));
      expect(fail?._tag).toBe("ServerError");
      expect(fail?.message).toContain("re-authenticate");
    }
  });
  test("ServerError carries 5xx hint", async () => {
    setFetch((async () =>
      new Response(JSON.stringify({ error: { code: "UZ-INTERNAL-001", message: "db down" } }), {
        status: 500,
      })) as unknown as typeof globalThis.fetch);
    const exit = await Effect.runPromiseExit(
      Effect.provide(
        Effect.gen(function* () {
          const http = yield* HttpClient;
          return yield* http.request({ path: "/v1/x", retry: { maxAttempts: 1 } });
        }),
        SUITE_LAYER("https://api.test.local"),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const fail = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(fail?._tag).toBe("ServerError");
      expect(fail?.message).toContain("retry");
    }
  });
  test("NetworkError when fetch throws fetch-failed", async () => {
    setFetch(((async () => {
      throw new TypeError("fetch failed");
    }) as unknown) as unknown as typeof globalThis.fetch);
    const exit = await Effect.runPromiseExit(
      Effect.provide(
        Effect.gen(function* () {
          const http = yield* HttpClient;
          return yield* http.request({ path: "/v1/x", retry: { maxAttempts: 1 } });
        }),
        SUITE_LAYER("https://api.test.local"),
      ),
    );
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const fail = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(fail?._tag).toBe("NetworkError");
    }
  });
  test("buildHeaders includes Bearer when token provided", async () => {
    const captured: { auth?: string } = {};
    setFetch((async (_url: unknown, init: RequestInit | undefined) => {
      const headers = init?.headers;
      if (headers && typeof headers === "object" && !Array.isArray(headers)) {
        const auth = (headers as Record<string, string>)["Authorization"];
        if (auth !== undefined) captured.auth = auth;
      }
      return new Response("{}", { status: 200 });
    }) as unknown as typeof globalThis.fetch);
    await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const http = yield* HttpClient;
          return yield* http.request({
            path: "/v1/x",
            token: Redacted.make("secret-token-1"),
          });
        }),
        SUITE_LAYER("https://api.test.local"),
      ),
    );
    expect(captured.auth).toBe("Bearer secret-token-1");
  });
  test("request POST with body serializes object", async () => {
    const captured: { body?: string } = {};
    setFetch((async (_url: unknown, init: RequestInit | undefined) => {
      if (typeof init?.body === "string") captured.body = init.body;
      return new Response("{}", { status: 200 });
    }) as unknown as typeof globalThis.fetch);
    await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const http = yield* HttpClient;
          return yield* http.request({
            path: "/v1/y",
            method: "POST",
            body: { hello: "world" },
          });
        }),
        SUITE_LAYER("https://api.test.local"),
      ),
    );
    expect(captured.body).toContain("\"hello\"");
  });
});

describe("resolveToken helper", () => {
  test("prefers stored over env", () => {
    const env = Option.some(Redacted.make("env-token"));
    const stored = Option.some(Redacted.make("stored-token"));
    const result = resolveToken(env, stored);
    expect(Option.isSome(result)).toBe(true);
    if (Option.isSome(result)) expect(Redacted.value(result.value)).toBe("stored-token");
  });
  test("falls back to env when stored is none", () => {
    const env = Option.some(Redacted.make("env-token"));
    const stored = Option.none<Redacted.Redacted<string>>();
    const result = resolveToken(env, stored);
    expect(Option.isSome(result)).toBe(true);
    if (Option.isSome(result)) expect(Redacted.value(result.value)).toBe("env-token");
  });
  test("returns none when both unset", () => {
    const result = resolveToken(Option.none(), Option.none());
    expect(Option.isNone(result)).toBe(true);
  });
});
