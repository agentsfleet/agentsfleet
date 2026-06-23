// `agentsfleet credential add|show|list|delete` — workspace-scoped opaque
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
import { wsCredentialsPath, wsCredentialPath } from "../lib/api-paths.ts";
import { ui } from "../output/index.ts";
import {
  ConfigError,
  ValidationError,
  type CliError,
} from "../errors/index.ts";
import {
  resolveCredentialBody,
  type CredentialAddFlags,
} from "./fleet_credential_body.ts";

const TYPE_STRING = "string" as const;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

interface CredentialRow {
  readonly name?: string;
  readonly created_at?: string | number | null;
}

interface CredentialsListResponse {
  readonly credentials?: ReadonlyArray<CredentialRow>;
}

const findCredentialByName = (
  wsId: string,
  name: string,
): Effect.Effect<
  CredentialRow | null,
  CliError,
  CliConfig | Credentials | HttpClient
> =>
  Effect.gen(function* () {
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const res = yield* http.request<CredentialsListResponse>({
      path: wsCredentialsPath(wsId),
      token,
    });
    const list = Array.isArray(res.credentials) ? res.credentials : [];
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
          detail: "credential name is required",
          suggestion: `usage: ${usage}`,
        }),
      );

export const credentialAddEffectFromFlags = (
  flags: CredentialAddFlags,
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
      "agentsfleet credential add <name> --data='<json-object>' [--force]",
    );
    const data = yield* resolveCredentialBody(flags);

    if (flags.force !== true) {
      const existing = yield* findCredentialByName(wsId, name);
      if (existing) {
        if (config.jsonMode) {
          yield* output.printJson({ status: "skipped", name, reason: "already_exists" });
        } else {
          yield* output.info(
            `Credential '${name}' already exists — skipped. Pass --force to overwrite.`,
          );
        }
        return;
      }
    }

    const token = yield* resolveAuthToken;
    yield* http.request<unknown>({
      path: wsCredentialsPath(wsId),
      method: "POST",
      body: { name, data },
      token,
    });

    const status = flags.force === true ? "overwritten" : "stored";
    if (config.jsonMode) {
      yield* output.printJson({ status, name });
    } else {
      yield* output.success(`Credential '${name}' ${status} in vault.`);
    }
  });

export const credentialShowEffectFromName = (
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
    const name = yield* requireName(rawName, "agentsfleet credential show <name>");
    const found = yield* findCredentialByName(wsId, name);
    if (!found) {
      if (config.jsonMode) {
        yield* output.printJson({ name, exists: false });
      } else {
        yield* output.error(`Credential '${name}' not found in vault.`);
      }
      return yield* Effect.fail(
        new ConfigError({
          detail: `credential '${name}' not found`,
          suggestion: `list available with: agentsfleet credential list`,
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
    yield* output.success(`Credential '${found.name}' exists.`);
    if (found.created_at) {
      yield* output.info(ui.dim(`  created_at: ${found.created_at}`));
    }
  });

export const credentialListEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;

  const wsId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;
  const res = yield* http.request<CredentialsListResponse>({
    path: wsCredentialsPath(wsId),
    token,
  });

  if (config.jsonMode) {
    yield* output.printJson(res);
    return;
  }
  const creds = res.credentials ?? [];
  if (creds.length === 0) {
    yield* output.info(
      "No credentials stored. Add one with: agentsfleet credential add <name> --data=@- (pipe JSON on stdin)",
    );
    return;
  }
  for (const c of creds) {
    yield* output.info(`  ${c.name ?? ""}  ${ui.dim(String(c.created_at ?? ""))}`);
  }
});

export const credentialDeleteEffectFromName = (
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
    const name = yield* requireName(rawName, "agentsfleet credential delete <name>");
    const token = yield* resolveAuthToken;
    yield* http.request<unknown>({
      path: wsCredentialPath(wsId, name),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ status: "deleted", name });
    } else {
      yield* output.success(`Credential '${name}' removed from vault.`);
    }
  });
