// Direct Effect-layer tests for eventsEffectFromFlags — covers the
// ValidationError guard (missing agentId) that is unreachable via the
// normal CLI path because commander enforces <agent_id> as a required
// positional before the handler runs.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option } from "effect";

import { eventsEffectFromFlags } from "../src/commands/agent_events.ts";
import { CliConfig } from "../src/services/config.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { Credentials } from "../src/services/credentials.ts";
import { ValidationError, type CliError } from "../src/errors/index.ts";

// Minimal stub layers — only the services accessed before the
// guard under test fires need real implementations.

const configLayer = (): Layer.Layer<CliConfig> =>
  Layer.succeed(CliConfig, {
    apiUrl: "https://api.test.local",
    dashboardUrl: "https://dash.test.local",
    accessToken: Option.none(),
    jsonMode: false,
    noOpen: false,
    telemetryPosthogKey: "phc_test",
    telemetryPosthogHost: "https://us.i.posthog.com",
  });

const outputLayer = (): Layer.Layer<Output> =>
  Layer.succeed(Output, {
    intro: () => Effect.void,
    info: () => Effect.void,
    success: () => Effect.void,
    warn: () => Effect.void,
    error: () => Effect.void,
    outro: () => Effect.void,
    printJson: () => Effect.void,
    printJsonErr: () => Effect.void,
    printKeyValue: () => Effect.void,
    printSection: () => Effect.void,
    printTable: () => Effect.void,
  });

const httpClientLayer = (): Layer.Layer<HttpClient> =>
  Layer.succeed(HttpClient, {
    request: () => Effect.die("should not be called"),
  });

const unusedWorkspacesLayer = (): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.die("should not be called"),
    save: () => Effect.die("should not be called"),
  });

const unusedCredentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.die("should not be called"),
    getSavedAt: Effect.die("should not be called"),
    getSessionId: Effect.die("should not be called"),
    getApiUrl: Effect.die("should not be called"),
    saveAccessToken: () => Effect.die("should not be called"),
    clearAccessToken: Effect.die("should not be called"),
  });

const runWith = <E extends CliError>(
  effect: Effect.Effect<void, E, CliConfig | Output | HttpClient | Workspaces | Credentials>,
): Promise<Exit.Exit<void, E>> =>
  Effect.runPromiseExit(
    effect.pipe(
      Effect.provide(configLayer()),
      Effect.provide(outputLayer()),
      Effect.provide(httpClientLayer()),
      Effect.provide(unusedWorkspacesLayer()),
      Effect.provide(unusedCredentialsLayer()),
    ),
  );

describe("eventsEffectFromFlags — ValidationError guard", () => {
  test("fails with ValidationError when agentId is undefined", async () => {
    const effect = eventsEffectFromFlags({ agentId: undefined });
    const exit = await runWith(effect);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const failure = Cause.findErrorOption(exit.cause);
      const err = Option.getOrNull(failure);
      expect(err).toBeInstanceOf(ValidationError);
      if (err instanceof ValidationError) {
        expect(err.detail).toMatch(/agent_id is required/i);
        expect(err.suggestion).toMatch(/agentsfleet events/);
      }
    }
  });

  test("fails with ValidationError when agentId is empty string", async () => {
    const effect = eventsEffectFromFlags({ agentId: "" });
    const exit = await runWith(effect);
    expect(Exit.isFailure(exit)).toBe(true);
    if (Exit.isFailure(exit)) {
      const failure = Cause.findErrorOption(exit.cause);
      const err = Option.getOrNull(failure);
      expect(err).toBeInstanceOf(ValidationError);
    }
  });
});
