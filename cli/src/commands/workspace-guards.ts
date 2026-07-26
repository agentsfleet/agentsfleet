// Shared Effect-shaped guards every workspace-scoped, auth-required
// command runs at the top of its gen block: require a current
// workspace, resolve a bearer token (env API key over stored login).
//
// `requireWorkspaceId` fails with ConfigError (EXIT_CODE.ConfigError = 5)
// when no workspace is selected. `resolveAuthToken` fails the same way
// when neither stored credentials nor AGENTSFLEET_API_KEY env yield a token.
// Both can also fail with UnexpectedError from the underlying state
// store (disk read failure); commands widen their error channel to
// `ConfigError | UnexpectedError` or just `CliError`.

import { Effect, Option, type Redacted } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveToken } from "../services/http-client.ts";
import {
  ConfigError,
  ValidationError,
  type UnexpectedError,
} from "../errors/index.ts";

export const WORKSPACE_CREATE_USAGE =
  "agentsfleet workspace create <name>" as const;
const WORKSPACE_NAME_MAX_CODEPOINTS = 128;
const ASCII_EDGE_WHITESPACE_PATTERN =
  /^[\u0009-\u000d\u0020]+|[\u0009-\u000d\u0020]+$/gu;
const UNICODE_WHITESPACE_ONLY_PATTERN =
  /^[\u00a0\u1680\u2000-\u200a\u202f\u205f\u3000]*$/u;
const WORKSPACE_NAME_UNSAFE_PATTERN =
  /[\u0000-\u001f\u007f-\u009f\u061c\u200e-\u200f\u2028-\u202e\u2066-\u2069]/u;
const WORKSPACE_NAME_UNSAFE_DETAIL =
  "workspace name contains unsupported control or directional formatting characters";

export const requireCreateName = (
  name: string | undefined,
): Effect.Effect<string, ValidationError> => {
  const trimmed = name?.replace(ASCII_EDGE_WHITESPACE_PATTERN, "");
  if (!trimmed || UNICODE_WHITESPACE_ONLY_PATTERN.test(trimmed)) {
    return Effect.fail(
      new ValidationError({
        detail: "workspace create requires <name>",
        suggestion: `usage: ${WORKSPACE_CREATE_USAGE}`,
      }),
    );
  }
  if ([...trimmed].length > WORKSPACE_NAME_MAX_CODEPOINTS) {
    return Effect.fail(
      new ValidationError({
        detail: `workspace name must be ${WORKSPACE_NAME_MAX_CODEPOINTS} characters or fewer`,
        suggestion: `usage: ${WORKSPACE_CREATE_USAGE}`,
      }),
    );
  }
  if (WORKSPACE_NAME_UNSAFE_PATTERN.test(trimmed)) {
    return Effect.fail(
      new ValidationError({
        detail: WORKSPACE_NAME_UNSAFE_DETAIL,
        suggestion: `usage: ${WORKSPACE_CREATE_USAGE}`,
      }),
    );
  }
  return Effect.succeed(trimmed);
};

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
        suggestion: `run \`${WORKSPACE_CREATE_USAGE}\` or \`agentsfleet workspace use <id>\``,
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
