// `agentsfleet secret add|show|list|delete` — workspace-scoped opaque
// JSON secrets keyed by `name`. The skill consuming them addresses fields
// as ${secrets.<name>.<field>}; this CLI does not enforce a schema (the
// consumer owns it). Default `add` upserts skip-if-exists; `--force`
// overwrites. The backing endpoint upserts on (workspace_id, key_name);
// the client-side guard keeps re-runs from silently clobbering a shared
// secret.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsSecretsPath, wsSecretPath } from "../lib/api-paths.ts";
import { ui } from "../output/index.ts";
import {
  ConfigError,
  ValidationError,
  type CliError,
} from "../errors/index.ts";
import {
  resolveSecretBody,
  type SecretAddFlags,
} from "./fleet_secret_body.ts";

const TYPE_STRING = "string" as const;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

interface SecretRow {
  readonly name?: string;
  readonly created_at?: string | number | null;
}

interface SecretsListResponse {
  readonly secrets?: ReadonlyArray<SecretRow>;
}

const findSecretByName = (
  wsId: string,
  name: string,
): Effect.Effect<
  SecretRow | null,
  CliError,
  CliConfig | Credentials | HttpClient
> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const res = yield* http.request<SecretsListResponse>({
      path: wsSecretsPath(wsId),
      token,
    });
    const list = Array.isArray(res.secrets) ? res.secrets : [];
    return list.find((c) => c.name === name) ?? null;
  });

const requireName = (
  name: string | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> =>
  isString(name) && name.length > 0
    ? Effect.succeed(name)
    : Effect.fail(
        new ValidationError({
          detail: "secret name is required",
          suggestion: `usage: ${usage}`,
        }),
      );

export const secretAddEffectFromFlags = (
  flags: SecretAddFlags,
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
    const name = yield* requireName(
      flags.name,
      "agentsfleet secret add <name> --data='<json-object>' [--force]",
    );
    const data = yield* resolveSecretBody(flags);

    if (flags.force !== true) {
      const existing = yield* findSecretByName(wsId, name);
      if (existing) {
        if (config.jsonMode) {
          yield* output.printJson({ status: "skipped", name, reason: "already_exists" });
        } else {
          yield* output.info(
            `Secret '${name}' already exists — skipped. Pass --force to overwrite.`,
          );
        }
        return;
      }
    }

    const token = yield* resolveAuthToken;
    yield* http.request<unknown>({
      path: wsSecretsPath(wsId),
      method: "POST",
      body: { name, data },
      token,
    });

    const status = flags.force === true ? "overwritten" : "stored";
    if (config.jsonMode) {
      yield* output.printJson({ status, name });
    } else {
      yield* output.success(`Secret '${name}' ${status} in vault.`);
    }
  });

export const secretShowEffectFromName = (
  rawName: string | undefined,
): Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;

    const wsId = yield* requireWorkspaceId;
    const name = yield* requireName(rawName, "agentsfleet secret show <name>");
    const found = yield* findSecretByName(wsId, name);
    if (!found) {
      if (config.jsonMode) {
        yield* output.printJson({ name, exists: false });
      } else {
        yield* output.error(`Secret '${name}' not found in vault.`);
      }
      return yield* Effect.fail(
        new ConfigError({
          detail: `secret '${name}' not found`,
          suggestion: `list available with: agentsfleet secret list`,
        }),
      );
    }

    if (config.jsonMode) {
      yield* output.printJson({
        name: found.name,
        exists: true,
        created_at: found.created_at ?? null,
      });
      return;
    }
    yield* output.success(`Secret '${found.name}' exists.`);
    if (found.created_at) {
      yield* output.info(ui.dim(`  created_at: ${found.created_at}`));
    }
  });

export const secretListEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;

  const wsId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;
  const res = yield* http.request<SecretsListResponse>({
    path: wsSecretsPath(wsId),
    token,
  });

  if (config.jsonMode) {
    yield* output.printJson(res);
    return;
  }
  const secrets = res.secrets ?? [];
  if (secrets.length === 0) {
    yield* output.info(
      "No secrets stored. Add one with: agentsfleet secret add <name> --data=@- (pipe JSON on stdin)",
    );
    return;
  }
  for (const c of secrets) {
    yield* output.info(`  ${c.name ?? ""}  ${ui.dim(String(c.created_at ?? ""))}`);
  }
});

export const secretDeleteEffectFromName = (
  rawName: string | undefined,
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
    const name = yield* requireName(rawName, "agentsfleet secret delete <name>");
    const token = yield* resolveAuthToken;
    yield* http.request<unknown>({
      path: wsSecretPath(wsId, name),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ status: "deleted", name });
    } else {
      yield* output.success(`Secret '${name}' removed from vault.`);
    }
  });
