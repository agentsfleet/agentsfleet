// Workspace command Effects — create / list / use / show / secrets / delete.
//
// Only `workspace create` hits the API (POST /v1/workspaces). The other five
// commands operate against the on-disk Workspaces store (services/workspaces.ts).
// `workspace create` is also the only command that does NOT call requireWorkspaceId
// — it CREATES the current workspace; gating it on an existing one would
// produce a chicken-and-egg failure on a fresh install.
//
// Errors map to CliError variants → dispatcher exit codes:
//   - missing positional / malformed UUID → ValidationError (exit 4)
//   - no active / unknown workspace       → ConfigError    (exit 5)
//   - API failure on `create`             → ServerError    (exit 3)
//   - state store IO failure              → UnexpectedError (exit 1)

import { Effect } from "effect";
import { Analytics } from "../services/telemetry/analytics.service.ts";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces, type WorkspaceItem, type WorkspacesValue } from "../services/workspaces.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { WORKSPACES_COLLECTION_PATH } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import {
  ConfigError,
  ValidationError,
  type CliError,
} from "../errors/index.ts";
import {
  EVT_WORKSPACE_ADD_COMPLETED,
  EVT_WORKSPACE_CREATED,
  EVT_WORKSPACE_LIST_VIEWED,
  EVT_WORKSPACE_USED,
  EVT_WORKSPACE_DELETED,
} from "../constants/analytics-events.ts";

const WORKSPACE_ID_FIELD = "workspace_id";
const WORKSPACE_LOCAL_REMOVAL_FIELD = "removed_from_local_state";
// The real, registered top-level command group (cli-tree-fleet.ts). One const
// so the JSON-mode and human-readable redirects can never re-diverge onto a
// phantom `agentsfleet agent secret` that has no CLI registration.
const SECRET_COMMAND = "agentsfleet secret" as const;

interface WorkspaceCreateResponse {
  readonly workspace_id: string;
  readonly name?: string | null;
}

const validateWorkspaceId = (
  workspaceId: string,
): Effect.Effect<string, ValidationError> => {
  const check = validateRequiredId(workspaceId, WORKSPACE_ID_FIELD);
  if (!check.ok) {
    return Effect.fail(
      new ValidationError({ detail: check.message, suggestion: "pass a valid uuidv7" }),
    );
  }
  return Effect.succeed(workspaceId);
};

export const workspaceAddEffect = (
  nameArg: string | undefined,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const analytics = yield* Analytics;
    const workspaces = yield* Workspaces;
    const token = yield* resolveAuthToken;

    const body = nameArg ? { name: nameArg } : {};
    const created = yield* http.request<WorkspaceCreateResponse>({
      path: WORKSPACES_COLLECTION_PATH,
      method: "POST",
      body,
      token,
    });
    const workspaceId = created.workspace_id;
    const resolvedName = created.name ?? nameArg ?? null;

    const state = yield* workspaces.load;
    const existing = state.items.find((x) => x.workspace_id === workspaceId);
    const items: WorkspaceItem[] = existing
      ? state.items
      : [
          ...state.items,
          { workspace_id: workspaceId, name: resolvedName, created_at: Date.now() },
        ];
    yield* workspaces.save({ current_workspace_id: workspaceId, items });

    yield* analytics.capture(EVT_WORKSPACE_ADD_COMPLETED, {
      workspace_id: workspaceId,
    });
    // workspace_created carries just the command tag so PostHog
    // dashboards can pivot on command name (telemetry is opt-OUT
    // default; AGENTSFLEET_TELEMETRY_DISABLED=1 or DO_NOT_TRACK=1 disables).
    yield* analytics.capture(EVT_WORKSPACE_CREATED, {
      command: "workspace.create",
    });

    if (config.jsonMode) {
      yield* output.printJson({ workspace_id: workspaceId, name: resolvedName });
      return;
    }
    yield* output.printSection("Workspace added");
    yield* output.printKeyValue({
      workspace_id: workspaceId,
      name: resolvedName ?? LITERAL,
    });
  });

export const workspaceListEffect: Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Output | Workspaces
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const analytics = yield* Analytics;
  const workspaces = yield* Workspaces;
  const state = yield* workspaces.load;

  yield* analytics.capture(EVT_WORKSPACE_LIST_VIEWED, {
    workspace_count: state.items.length,
  });

  if (config.jsonMode) {
    yield* output.printJson({
      current_workspace_id: state.current_workspace_id,
      workspaces: state.items,
    });
    return;
  }
  if (state.items.length === 0) {
    yield* output.info("no workspaces");
    return;
  }
  yield* output.printTable(
    [
      { key: "active", label: "ACTIVE" },
      { key: WORKSPACE_ID_FIELD, label: "WORKSPACE" },
      { key: "name", label: "NAME" },
    ],
    state.items.map((item) => ({
      active: item.workspace_id === state.current_workspace_id ? "*" : "",
      workspace_id: item.workspace_id,
      name: item.name ?? LITERAL,
    })),
  );
});

const requireUseId = (
  workspaceId: string | undefined,
): Effect.Effect<string, ValidationError> =>
  Effect.gen(function* () {
    if (!workspaceId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "workspace use requires <workspace_id>",
          suggestion: "usage: agentsfleet workspace use <workspace_id>",
        }),
      );
    }
    return yield* validateWorkspaceId(workspaceId);
  });

const requireDeleteId = (
  workspaceId: string | undefined,
): Effect.Effect<string, ValidationError> =>
  Effect.gen(function* () {
    if (!workspaceId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "workspace delete requires <workspace_id>",
          suggestion: "usage: agentsfleet workspace delete <workspace_id>",
        }),
      );
    }
    return yield* validateWorkspaceId(workspaceId);
  });

export const workspaceUseEffectFromArgs = (
  positional: string | undefined,
  fromOpt: string | undefined,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const analytics = yield* Analytics;
    const workspaces = yield* Workspaces;

    const workspaceId = yield* requireUseId(positional ?? fromOpt);
    const state = yield* workspaces.load;
    const known = state.items.find((x) => x.workspace_id === workspaceId);
    if (!known) {
      return yield* Effect.fail(
        new ConfigError({
          detail: `workspace ${workspaceId} is not in your local list`,
          suggestion: "run `agentsfleet workspace create` or `agentsfleet workspace list`",
        }),
      );
    }
    yield* workspaces.save({ ...state, current_workspace_id: workspaceId });
    yield* analytics.capture(EVT_WORKSPACE_USED, { workspace_id: workspaceId });

    if (config.jsonMode) {
      yield* output.printJson({ active: workspaceId });
    } else {
      yield* output.success(`active workspace: ${workspaceId}`);
    }
  });

export const workspaceShowEffectFromArgs = (
  positional: string | undefined,
  fromOpt: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const workspaces = yield* Workspaces;
    const state = yield* workspaces.load;

    const workspaceId =
      fromOpt ?? positional ?? state.current_workspace_id ?? undefined;
    if (!workspaceId) {
      return yield* Effect.fail(
        new ConfigError({
          detail: "no active workspace",
          suggestion: 'run `agentsfleet workspace use <id>` or pass --workspace-id',
        }),
      );
    }
    const known = state.items.find((x) => x.workspace_id === workspaceId) ?? null;
    const detail = {
      workspace_id: workspaceId,
      active: workspaceId === state.current_workspace_id,
      name: known?.name ?? null,
      created_at: known?.created_at ?? null,
    };
    if (config.jsonMode) {
      yield* output.printJson(detail);
      return;
    }
    yield* output.printSection("Workspace");
    yield* output.printKeyValue({
      workspace_id: detail.workspace_id,
      active: detail.active ? "yes" : "no",
      name: detail.name ?? LITERAL,
    });
  });

export const workspaceSecretsEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Output
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  if (config.jsonMode) {
    yield* output.printJson({
      status: "redirect",
      message: `use \`${SECRET_COMMAND}\` from the CLI, or manage workspace secrets at /secrets in the dashboard`,
    });
    return;
  }
  yield* output.printSection("Workspace secrets");
  yield* output.info(
    `Manage secrets at /secrets in the dashboard, or run: ${SECRET_COMMAND}`,
  );
});

export const workspaceDeleteEffectFromArgs = (
  positional: string | undefined,
  fromOpt: string | undefined,
): Effect.Effect<
  void,
  CliError,
  Analytics | CliConfig | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const analytics = yield* Analytics;
    const workspaces = yield* Workspaces;

    const workspaceId = yield* requireDeleteId(positional ?? fromOpt);
    const state = yield* workspaces.load;
    const next: WorkspacesValue = {
      current_workspace_id: state.current_workspace_id,
      items: state.items.filter((x) => x.workspace_id !== workspaceId),
    };
    if (next.current_workspace_id === workspaceId) {
      next.current_workspace_id = next.items[0]?.workspace_id ?? null;
    }
    yield* workspaces.save(next);
    yield* analytics.capture(EVT_WORKSPACE_DELETED, { workspace_id: workspaceId });

    if (config.jsonMode) {
      yield* output.printJson({ [WORKSPACE_LOCAL_REMOVAL_FIELD]: workspaceId });
    } else {
      yield* output.success(`workspace removed from local state: ${workspaceId}`);
    }
  });
const LITERAL = "—" as const;
