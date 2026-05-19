// `zombiectl credential add|show|list|delete` — workspace-scoped opaque
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

const STDIN_DATA_SENTINEL = "@-";
const MISSING_DATA_HINT =
  "missing --data flag. Pipe JSON on stdin with --data=@- or pass --data='{...}'. Stdin form keeps secrets out of shell history.";

interface CredentialRow {
  readonly name?: string;
  readonly created_at?: string | number | null;
}

interface CredentialsListResponse {
  readonly credentials?: ReadonlyArray<CredentialRow>;
}

type ParsedData =
  | { readonly ok: true; readonly value: Record<string, unknown> }
  | { readonly ok: false; readonly message: string };

const parseDataObject = (raw: string): ParsedData => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, message: `--data is not valid JSON: ${message}` };
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    return { ok: false, message: "--data must be a JSON object (not a string, array, or scalar)" };
  }
  const obj = parsed as Record<string, unknown>;
  if (Object.keys(obj).length === 0) {
    return {
      ok: false,
      message: "--data must be a non-empty JSON object — at least one field is required",
    };
  }
  return { ok: true, value: obj };
};

const readStdinJson: Effect.Effect<string, ConfigError> = Effect.tryPromise({
  try: () => Bun.stdin.text(),
  catch: (err) =>
    new ConfigError({
      detail: `failed to read stdin: ${err instanceof Error ? err.message : String(err)}`,
      suggestion: "ensure stdin is not closed and re-pipe the JSON payload",
    }),
});

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

export interface CredentialAddFlags {
  readonly name?: string | undefined;
  readonly data?: string | undefined;
  readonly force?: boolean | undefined;
}

const requireName = (
  name: string | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> =>
  typeof name === "string" && name.length > 0
    ? Effect.succeed(name)
    : Effect.fail(
        new ValidationError({
          detail: "credential name is required",
          suggestion: `usage: ${usage}`,
        }),
      );

const resolveDataSource = (
  data: string | undefined,
): Effect.Effect<string, CliError> =>
  Effect.gen(function* () {
    if (typeof data !== "string" || data.length === 0) {
      return yield* Effect.fail(
        new ValidationError({
          detail: MISSING_DATA_HINT,
          suggestion: "pass --data='{...}' or --data=@- for stdin",
        }),
      );
    }
    if (data !== STDIN_DATA_SENTINEL) return data;
    const raw = yield* readStdinJson;
    if (!raw || raw.trim().length === 0) {
      return yield* Effect.fail(
        new ValidationError({
          detail: "--data=@- but stdin was empty",
          suggestion: "pipe JSON on stdin: cat creds.json | zombiectl credential add <name> --data=@-",
        }),
      );
    }
    return raw;
  });

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
      "zombiectl credential add <name> --data='<json-object>' [--force]",
    );
    const raw = yield* resolveDataSource(flags.data);
    const validated = parseDataObject(raw);
    if (!validated.ok) {
      return yield* Effect.fail(
        new ValidationError({
          detail: validated.message,
          suggestion: "fix the --data payload and retry",
        }),
      );
    }

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
      body: { name, data: validated.value },
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
    const name = yield* requireName(rawName, "zombiectl credential show <name>");
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
          suggestion: `list available with: zombiectl credential list`,
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
      "No credentials stored. Add one with: zombiectl credential add <name> --data=@- (pipe JSON on stdin)",
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
    const name = yield* requireName(rawName, "zombiectl credential delete <name>");
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
