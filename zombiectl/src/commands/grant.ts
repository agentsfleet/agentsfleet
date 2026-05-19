// Integration Grant CLI commands — Effect-shaped.
//
// zombiectl grant list   --zombie <id>              → list grants for a zombie
// zombiectl grant delete --zombie <id> <grant_id>   → revoke a grant immediately

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import {
  requireWorkspaceId,
  resolveAuthToken,
} from "./workspace-guards.ts";
import { wsGrantsListPath, wsGrantPath } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import { ValidationError, type CliError } from "../errors/index.ts";

interface GrantRow {
  readonly service?: string | null;
  readonly status?: string | null;
  readonly requested_at?: number | string | null;
  readonly approved_at?: number | string | null;
  readonly grant_id?: string | null;
}

interface GrantListResponse {
  readonly items?: ReadonlyArray<GrantRow>;
}

const requireFlag = (
  value: string | undefined,
  detail: string,
  suggestion: string,
): Effect.Effect<string, ValidationError> =>
  value
    ? Effect.succeed(value)
    : Effect.fail(new ValidationError({ detail, suggestion }));

const requireValidId = (
  value: string,
  fieldName: string,
): Effect.Effect<string, ValidationError> => {
  const check = validateRequiredId(value, fieldName);
  if (!check.ok) {
    return Effect.fail(
      new ValidationError({
        detail: check.message,
        suggestion: "pass a valid uuidv7",
      }),
    );
  }
  return Effect.succeed(value);
};

export const grantListEffectFromArgs = (
  zombieIdPositional: string | undefined,
  zombieIdFlag: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const workspaceId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const zombieId = yield* requireFlag(
      zombieIdFlag ?? zombieIdPositional,
      "grant list requires --zombie <id>",
      "pass --zombie <zombie_id>",
    );

    const res = yield* http.request<GrantListResponse>({
      path: wsGrantsListPath(workspaceId, zombieId),
      token,
    });
    const grants = res.items ?? [];

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    if (grants.length === 0) {
      yield* output.info("no integration grants found");
      return;
    }
    yield* output.printTable(
      [
        { key: "service", label: "SERVICE" },
        { key: "status", label: "STATUS" },
        { key: "requested_at", label: "REQUESTED_AT" },
        { key: "approved_at", label: "APPROVED_AT" },
        { key: "grant_id", label: "GRANT_ID" },
      ],
      grants.map((g) => ({
        service: g.service ?? "",
        status: g.status ?? "",
        requested_at: g.requested_at
          ? new Date(g.requested_at).toISOString()
          : "-",
        approved_at: g.approved_at
          ? new Date(g.approved_at).toISOString()
          : "-",
        grant_id: g.grant_id ?? "",
      })),
    );
  });

export const grantDeleteEffectFromArgs = (
  zombieIdFlag: string | undefined,
  grantIdPositional: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const workspaceId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;
    const zombieIdRaw = yield* requireFlag(
      zombieIdFlag,
      "grant delete requires --zombie <id> <grant_id>",
      "pass --zombie <zombie_id> <grant_id>",
    );
    const zombieId = yield* requireValidId(zombieIdRaw, "zombie_id");
    const grantIdRaw = yield* requireFlag(
      grantIdPositional,
      "grant delete requires --zombie <id> <grant_id>",
      "pass <grant_id> as positional",
    );
    const grantId = yield* requireValidId(grantIdRaw, "grant_id");

    yield* http.request<unknown>({
      path: wsGrantPath(workspaceId, zombieId, grantId),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ deleted: true, grant_id: grantId });
    } else {
      yield* output.success(
        `Grant ${grantId} deleted. The zombie can no longer use this integration; further attempts will be denied.`,
      );
    }
  });
