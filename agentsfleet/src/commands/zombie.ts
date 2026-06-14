// Zombie CLI top-level command Effects — status / stop / resume / kill /
// delete. Install + update live in zombie_install.ts; list/logs/events/
// steer/credential leaves live in sibling files (zombie_list.ts,
// zombie_logs.ts, zombie_events.ts, zombie_steer.ts, zombie_credential.ts).
//
// Each command yields services from the MainLayer (CliConfig, Output,
// HttpClient, Credentials, Workspaces, Analytics) and emits one of the
// CliError variants on failure. The dispatcher's renderError prints the
// detail + suggestion + (for ServerError) request_id.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsZombiesPath, wsZombiePath } from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import {
  ZOMBIE_STATUS,
  type ZombieMutationStatus,
} from "../constants/zombie-status.ts";
import {
  ValidationError,
  type CliError,
} from "../errors/index.ts";

const STATUS_PAST_TENSE: Record<ZombieMutationStatus, string> = {
  [ZOMBIE_STATUS.STOPPED]: "stopped",
  [ZOMBIE_STATUS.ACTIVE]: "resumed",
  [ZOMBIE_STATUS.KILLED]: "killed",
};

const STATUS_VERB: Record<ZombieMutationStatus, string> = {
  [ZOMBIE_STATUS.STOPPED]: "stop",
  [ZOMBIE_STATUS.ACTIVE]: "resume",
  [ZOMBIE_STATUS.KILLED]: "kill",
};

interface ZombieListItem {
  readonly name?: string;
  readonly status?: string;
  readonly events_processed?: number;
  readonly budget_used_dollars?: number | null;
}

interface ZombieListResponse {
  readonly items?: ReadonlyArray<ZombieListItem>;
}

const requireZombieId = (
  zombieId: string | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> =>
  Effect.gen(function* () {
    if (!zombieId) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "zombie_id is required",
          suggestion: `usage: ${usage}`,
        }),
      );
    }
    const check = validateRequiredId(zombieId, "zombie_id");
    if (!check.ok) {
      return yield* Effect.fail(
        new ValidationError({ detail: check.message, suggestion: `usage: ${usage}` }),
      );
    }
    return zombieId;
  });

export const statusEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;
  const wsId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;

  const res = yield* http.request<ZombieListResponse>({
    path: wsZombiesPath(wsId),
    token,
  });

  if (config.jsonMode) {
    yield* output.printJson(res);
    return;
  }

  const zombies = res.items ?? [];
  if (zombies.length === 0) {
    yield* output.info(
      "No zombies running. Install one with: agentsfleet install --from <path>",
    );
    return;
  }

  yield* output.printSection("Zombies");
  for (const z of zombies) {
    const budget =
      typeof z.budget_used_dollars === "number"
        ? `$${z.budget_used_dollars.toFixed(2)}`
        : "—";
    yield* output.printKeyValue({
      Name: z.name ?? "",
      Status: z.status ?? "",
      Events: String(z.events_processed ?? 0),
      Budget: budget,
    });
  }
});

const setStatusEffect = (
  zombieId: string | undefined,
  status: ZombieMutationStatus,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const verb = STATUS_VERB[status];
    const wsId = yield* requireWorkspaceId;
    const id = yield* requireZombieId(zombieId, `agentsfleet ${verb} <zombie_id>`);
    const token = yield* resolveAuthToken;

    const res = yield* http.request<unknown>({
      path: wsZombiePath(wsId, id),
      method: "PATCH",
      body: { status },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
    } else {
      yield* output.success(`${id} ${STATUS_PAST_TENSE[status]}.`);
    }
  });

export const stopEffectFromId = (
  zombieId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(zombieId, ZOMBIE_STATUS.STOPPED);

export const resumeEffectFromId = (
  zombieId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(zombieId, ZOMBIE_STATUS.ACTIVE);

export const killEffectFromId = (
  zombieId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> => setStatusEffect(zombieId, ZOMBIE_STATUS.KILLED);

export const deleteEffectFromId = (
  zombieId: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const wsId = yield* requireWorkspaceId;
    const id = yield* requireZombieId(zombieId, "agentsfleet delete <zombie_id>");
    const token = yield* resolveAuthToken;

    yield* http.request<unknown>({
      path: wsZombiePath(wsId, id),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ zombie_id: id, deleted: true });
    } else {
      yield* output.success(`${id} deleted.`);
    }
  });
