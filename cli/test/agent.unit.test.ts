// Direct Effect-layer test for the requireAgentId guard in agent.ts —
// covers the `!agentId` branch (missing id) that is unreachable via the
// normal CLI path because commander enforces <agent_id> as a required
// positional before the mutation handlers run. The workspace guard runs
// first, so its stub must succeed for execution to reach the id guard.

import { describe, test, expect } from "bun:test";
import { Cause, Effect, Exit, Layer, Option } from "effect";

import { deleteEffectFromId, stopEffectFromId } from "../src/commands/agent.ts";
import { CliConfig } from "../src/services/config.ts";
import { HttpClient } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { Credentials } from "../src/services/credentials.ts";
import { ValidationError, type CliError } from "../src/errors/index.ts";

const WS_ID = "01900000-0000-7000-8000-0000000000ws";

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
    request: () => Effect.die("should not be called — guard fails first"),
  });

// The workspace guard runs before the id guard, so load must succeed.
const workspacesLayer = (): Layer.Layer<Workspaces> =>
  Layer.succeed(Workspaces, {
    load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }),
    save: () => Effect.void,
  });

const unusedCredentialsLayer = (): Layer.Layer<Credentials> =>
  Layer.succeed(Credentials, {
    getAccessToken: Effect.die("should not be called — guard fails first"),
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
      Effect.provide(workspacesLayer()),
      Effect.provide(unusedCredentialsLayer()),
    ),
  );

const expectRequiredIdFailure = async (
  effect: Effect.Effect<void, CliError, CliConfig | Output | HttpClient | Workspaces | Credentials>,
): Promise<void> => {
  const exit = await runWith(effect);
  expect(Exit.isFailure(exit)).toBe(true);
  if (Exit.isFailure(exit)) {
    const err = Option.getOrNull(Cause.findErrorOption(exit.cause));
    expect(err).toBeInstanceOf(ValidationError);
    if (err instanceof ValidationError) {
      expect(err.detail).toMatch(/agent_id is required/i);
    }
  }
};

describe("agent mutation guards — requireAgentId", () => {
  test("deleteEffectFromId fails with ValidationError when id is undefined", async () => {
    await expectRequiredIdFailure(deleteEffectFromId(undefined));
  });

  test("deleteEffectFromId fails with ValidationError when id is empty string", async () => {
    await expectRequiredIdFailure(deleteEffectFromId(""));
  });

  test("stopEffectFromId fails with ValidationError when id is undefined", async () => {
    await expectRequiredIdFailure(stopEffectFromId(undefined));
  });
});
