// Shared Effect-shaped guards every workspace-scoped, auth-required
// command runs at the top of its gen block: require a current
// workspace, resolve a bearer token from credentials → env.
//
// `requireWorkspaceId` fails with ConfigError (EXIT_CODE.ConfigError = 5)
// when no workspace is selected. `resolveAuthToken` fails the same way
// when neither stored credentials nor AGENTSFLEET_TOKEN env yield a token.
// Both can also fail with UnexpectedError from the underlying state
// store (disk read failure); commands widen their error channel to
// `ConfigError | UnexpectedError` or just `CliError`.

import { Effect, Option, type Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveToken } from "../services/http-client.ts";
import { ConfigError, type UnexpectedError } from "../errors/index.ts";

export const requireWorkspaceId: Effect.Effect<
  string,
  ConfigError | UnexpectedError,
  Workspaces
> = Effect.gen(function* () {
  const workspaces = yield* Workspaces;
  const state = yield* workspaces.load;
  if (!state.current_workspace_id) {
    return yield* Effect.fail(
      new ConfigError({
        detail: "no workspace selected",
        suggestion: "run `agentsfleet workspace add` or `workspace use <id>`",
      }),
    );
  }
  return state.current_workspace_id;
});

export const resolveAuthToken: Effect.Effect<
  Redacted.Redacted<string>,
  ConfigError | UnexpectedError,
  CliConfig | Credentials
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const credentials = yield* Credentials;
  const stored = yield* credentials.getAccessToken;
  const merged = resolveToken(config.accessToken, stored);
  if (Option.isNone(merged)) {
    return yield* Effect.fail(
      new ConfigError({
        detail: "not authenticated",
        suggestion: "run `agentsfleet login`",
      }),
    );
  }
  return merged.value;
});
