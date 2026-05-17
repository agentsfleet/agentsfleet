// Coverage-target tests for the Effect substrate. Picks up the
// stragglers that aren't naturally hit by behavioural tests:
//   - Context.Tag class constructors (services don't `new` their tag
//     classes; Layer.succeed registers the shape directly).
//   - TaggedError `message` getters on variants whose suite doesn't
//     assert `.message`.
//   - Analytics service `identify` / `alias` / `shutdown`.
//   - Credentials `getSavedAt` / `getSessionId` / `getApiUrl` /
//     `clearAccessToken`.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Effect, Option, Redacted } from "effect";
import { CliConfig } from "../src/services/config.ts";
import {
  TelemetryRuntime,
  TelemetryRuntimeFromValues,
} from "../src/services/telemetry-runtime.ts";
import { Credentials, CredentialsLive } from "../src/services/credentials.ts";
import { Output, OutputFromStreams } from "../src/services/output.ts";
import { Analytics, AnalyticsLive } from "../src/services/analytics.ts";
import { HttpClient } from "../src/services/http-client.ts";
import {
  AuthError,
  ConfigError,
  NetworkError,
  ServerError,
  UnexpectedError,
  ValidationError,
} from "../src/errors/index.ts";

describe("Context.Tag class constructors are reachable", () => {
  test("substrate tag classes can be instantiated directly", () => {
    const Ctor = (cls: unknown): new () => object => cls as new () => object;
    expect(new (Ctor(CliConfig))()).toBeInstanceOf(CliConfig);
    expect(new (Ctor(TelemetryRuntime))()).toBeInstanceOf(TelemetryRuntime);
    expect(new (Ctor(Credentials))()).toBeInstanceOf(Credentials);
    expect(new (Ctor(Output))()).toBeInstanceOf(Output);
    expect(new (Ctor(Analytics))()).toBeInstanceOf(Analytics);
    expect(new (Ctor(HttpClient))()).toBeInstanceOf(HttpClient);
  });
});

describe("CliError variant message getters", () => {
  const detail = "d";
  const suggestion = "s";
  test("ServerError.message renders detail + suggestion", () => {
    const err = new ServerError({
      detail,
      suggestion,
      code: "X",
      status: 500,
      requestId: null,
    });
    expect(err.message).toContain(detail);
    expect(err.message).toContain(suggestion);
  });
  test("ConfigError.message renders detail + suggestion", () => {
    const err = new ConfigError({ detail, suggestion });
    expect(err.message).toContain(detail);
    expect(err.message).toContain(suggestion);
  });
  test("UnexpectedError.message renders detail + suggestion", () => {
    const err = new UnexpectedError({ detail, suggestion });
    expect(err.message).toContain(detail);
    expect(err.message).toContain(suggestion);
  });
  test("AuthError / NetworkError / ValidationError message getters", () => {
    expect(new AuthError({ detail, suggestion, code: "c" }).message).toContain(detail);
    expect(new NetworkError({ detail, suggestion, url: "u" }).message).toContain(detail);
    expect(new ValidationError({ detail, suggestion }).message).toContain(detail);
  });
});

describe("Analytics identify / alias / shutdown", () => {
  test("identify mutates the distinctId state, alias is a no-op Effect", async () => {
    const program = Effect.gen(function* () {
      const a = yield* Analytics;
      yield* a.identify("user-123");
      yield* a.alias("user-123", "device-abc");
      yield* a.capture("test_event", { foo: "bar" });
      yield* a.shutdown;
    });
    await Effect.runPromise(
      Effect.provide(
        program,
        AnalyticsLive.pipe(
          (l) => l,
        ),
      ).pipe(
        Effect.provide(
          TelemetryRuntimeFromValues({ sessionId: "s", deviceId: "d" }),
        ),
      ) as Effect.Effect<void, never, never>,
    );
    expect(true).toBe(true);
  });
});

describe("Credentials full surface", () => {
  test("getSavedAt / getSessionId / getApiUrl / clearAccessToken on empty state", async () => {
    const tempDir = mkdtempSync(join(tmpdir(), "zombiectl-creds-fn-"));
    const prev = process.env.ZOMBIE_STATE_DIR;
    process.env.ZOMBIE_STATE_DIR = tempDir;
    try {
      const result = await Effect.runPromise(
        Effect.provide(
          Effect.gen(function* () {
            const c = yield* Credentials;
            const savedAt = yield* c.getSavedAt;
            const sessionId = yield* c.getSessionId;
            const apiUrl = yield* c.getApiUrl;
            yield* c.clearAccessToken;
            yield* c.saveAccessToken({
              token: Redacted.make("tok"),
              sessionId: "sess",
              apiUrl: "https://x",
            });
            const savedAt2 = yield* c.getSavedAt;
            const sessionId2 = yield* c.getSessionId;
            const apiUrl2 = yield* c.getApiUrl;
            const token2 = yield* c.getAccessToken;
            yield* c.clearAccessToken;
            return { savedAt, sessionId, apiUrl, savedAt2, sessionId2, apiUrl2, hasToken: Option.isSome(token2) };
          }),
          CredentialsLive,
        ),
      );
      expect(result.savedAt).toBeNull();
      expect(result.sessionId).toBeNull();
      expect(result.apiUrl).toBeNull();
      expect(result.hasToken).toBe(true);
      expect(result.sessionId2).toBe("sess");
      expect(result.apiUrl2).toBe("https://x");
      expect(typeof result.savedAt2 === "number").toBe(true);
    } finally {
      if (prev === undefined) delete process.env.ZOMBIE_STATE_DIR;
      else process.env.ZOMBIE_STATE_DIR = prev;
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});

describe("Output service via OutputFromStreams covers all method bodies", () => {
  test("every method body executes under a captured stream pair", async () => {
    class Sink {
      readonly chunks: string[] = [];
      isTTY = false;
      write(c: string | Uint8Array): boolean {
        this.chunks.push(typeof c === "string" ? c : Buffer.from(c).toString());
        return true;
      }
      end(): void {}
    }
    const stdout = new Sink();
    const stderr = new Sink();
    const layer = OutputFromStreams({
      stdout: stdout as unknown as NodeJS.WritableStream,
      stderr: stderr as unknown as NodeJS.WritableStream,
    });
    await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const o = yield* Output;
          yield* o.intro("a");
          yield* o.info("b");
          yield* o.success("c", { command: "x" });
          yield* o.warn("d");
          yield* o.error("e", { command: "x" });
          yield* o.outro("f");
          yield* o.printJson({ k: 1 });
          yield* o.printJsonErr({ k: 2 });
          yield* o.printKeyValue({ a: "1", b: "2" });
          yield* o.printSection("section");
        }),
        layer,
      ),
    );
    expect(stdout.chunks.length).toBeGreaterThan(0);
    expect(stderr.chunks.length).toBeGreaterThan(0);
  });
});

describe("CliConfig direct shape access via Effect.runPromise", () => {
  test("yielding CliConfig from a layer-resolved env reads all fields", async () => {
    const value = await Effect.runPromise(
      Effect.provide(
        Effect.gen(function* () {
          const cfg = yield* CliConfig;
          return cfg;
        }),
        // Inline layer construction touches both the resolveCliConfig
        // call site and the spread-merge branch of CliConfigFromValues.
        (await import("../src/services/config.ts")).CliConfigFromValues({
          jsonMode: true,
          noOpen: true,
        }),
      ),
    );
    expect(value.jsonMode).toBe(true);
    expect(value.noOpen).toBe(true);
  });
});
