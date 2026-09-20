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
import { isString } from "../lib/guards.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveToken } from "../services/http-client.ts";
import { validateRequiredId } from "../program/validators.ts";
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

/**
 * A required identifier, validated, as an Effect.
 *
 * `validateRequiredId` returns a `{ ok, message }` record because it predates
 * the Effect layer and is called from both sides of the commander boundary.
 * This is the one place that lifts it, so a command never re-derives the
 * refusal: `fleet_schedule` had its own copy, and it was the only command that
 * validated an id at all.
 */
export const requireValidId = (
  value: string | undefined,
  fieldName: string,
  usage: string,
): Effect.Effect<string, ValidationError> => {
  const check = validateRequiredId(value, fieldName);
  if (check.ok) return Effect.succeed(value as string);
  // The two refusals point different ways on purpose. An absent id needs the
  // usage line, because the caller has not typed the flag yet. A malformed one
  // needs the SHAPE: they typed it, so repeating the usage tells them nothing
  // they did not already do.
  const typed = isString(value) && value.trim().length > 0;
  return Effect.fail(
    new ValidationError({
      detail: check.message,
      suggestion: typed ? UUIDV7_SUGGESTION : usage,
    }),
  );
};

/** The usage line a bad `--workspace` points at. */
const WORKSPACE_OVERRIDE_USAGE = "pass --workspace <workspace_id>" as const;
const WORKSPACE_ID_FIELD = "workspace_id" as const;
const UUIDV7_SUGGESTION = "pass a valid uuidv7" as const;

/**
 * The workspace a command acts on: the caller's `--workspace` when it named
 * one, otherwise the selected workspace.
 *
 * The override is VALIDATED, never merely trusted. Two commands carried
 * private copies of this resolver and had already drifted apart: `memory`
 * passed an unchecked `--workspace` straight into a URL path, while
 * `schedule` refused a malformed one — so the same typo produced a server
 * 404 from one command and a usage error from the other. A third spelling of
 * the "no workspace selected" suggestion lived in each copy.
 */
export const resolveWorkspaceId = (
  override: string | undefined,
): Effect.Effect<string, ConfigError | UnexpectedError | ValidationError, Workspaces> =>
  isString(override) && override.length > 0
    ? requireValidId(override, WORKSPACE_ID_FIELD, WORKSPACE_OVERRIDE_USAGE)
    : requireWorkspaceId;

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
