// `agentsfleet secret create|update|show|list|delete` — workspace-scoped
// opaque JSON secrets keyed by `name`. The skill consuming them addresses
// fields as ${secrets.<name>.<field>}; this CLI does not enforce a schema
// (the consumer owns it).
//
// `create` claims a name that is free and never overwrites: the endpoint
// answers `UZ-VAULT-005` on a name this workspace already holds, and a
// taken name is reported as a skip so re-running a provisioning script is
// quiet. `update` replaces a held name's whole body in one PUT — the name
// is never released, so fleets that require it keep resolving.

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
import { resolveCatalogueTarget } from "../lib/model-catalogue.ts";
import { SECRET_FIELD_MODEL, SECRET_FIELD_PROVIDER } from "../constants/custom-endpoint.ts";
import type { Redacted } from "effect/Redacted";

// Reject a provider this server's catalogue does not price, and normalise its
// spelling to the catalogue's. Cannot run at parse time: the accepted set is a
// property of the server, not of the binary.
//
// Runs AFTER the body composes, never before. Every pairing rule in
// fleet_secret_body.ts is local and free, so a caller who passed `--data`
// alongside the typed flags must hear that without a round-trip first — an
// error the CLI can raise offline should never cost a network call.
//
// Keyed off `flags.provider`, so only the typed form is checked. A `--data`
// blob is an opaque object that may not be a model credential at all, and
// stays unconstrained by design.
const withCatalogueProvider = (
  flags: SecretAddFlags,
  data: Record<string, unknown>,
  token: Redacted<string> | undefined,
): Effect.Effect<Record<string, unknown>, CliError, HttpClient> =>
  Effect.gen(function* () {
    const provider = flags.provider?.trim();
    if (!provider) return data;
    // One read resolves both identifiers. `--model` was checked nowhere, which
    // is the same defect `--provider` had one flag over: a typo stored a
    // credential that reported success and failed at the first event.
    const target = yield* resolveCatalogueTarget(provider, flags.model, token);
    const next: Record<string, unknown> = { ...data, [SECRET_FIELD_PROVIDER]: target.provider };
    if (target.model !== undefined) next[SECRET_FIELD_MODEL] = target.model;
    return next;
  });

const TYPE_STRING = "string" as const;
const METHOD_PUT = "PUT" as const;
const STATUS_UPDATED = "updated" as const;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

/** The workspace already holds this name. Matched on the code rather than the
 *  bare `409` so an unrelated future conflict on this route is not swallowed
 *  into a silent skip. */
const ERR_SECRET_NAME_TAKEN = "UZ-VAULT-005" as const;

const isNameTaken = (err: CliError): boolean =>
  err._tag === "ServerError" && err.code === ERR_SECRET_NAME_TAKEN;

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
      "agentsfleet secret create <name> --data='<json-object>'",
    );
    const body = yield* resolveSecretBody(flags);

    const token = yield* resolveAuthToken;
    const data = yield* withCatalogueProvider(flags, body, token);
    // The server decides whether the name was free, so there is no preflight
    // read: a check here would be a check-then-write over exactly the window
    // `UZ-VAULT-005` exists to close, and two concurrent creates would both
    // pass it. One round-trip, and Postgres picks the winner.
    const stored = yield* http
      .request<unknown>({
        path: wsSecretsPath(wsId),
        method: "POST",
        body: { name, data },
        token,
      })
      .pipe(
        Effect.as(true),
        // A taken name is an outcome, not a failure: re-running a provisioning
        // script must stay quiet and exit 0 rather than abort the run.
        Effect.catchIf(isNameTaken, () => Effect.succeed(false)),
      );

    if (!stored) {
      if (config.jsonMode) {
        yield* output.printJson({ status: "skipped", name, reason: "already_exists" });
      } else {
        yield* output.info(
          `Secret '${name}' already exists — skipped. Replace its value with: agentsfleet secret update ${name} --data='<json-object>'`,
        );
      }
      return;
    }

    if (config.jsonMode) {
      yield* output.printJson({ status: "stored", name });
    } else {
      yield* output.success(`Secret '${name}' stored in vault.`);
    }
  });

/**
 * `agentsfleet secret update <name> --data='<json-object>'` — replace a stored
 * secret's whole body without releasing the name.
 *
 * One PUT, no preflight read and no delete. `delete` then `create` also
 * replaces a value, but leaves the name unclaimed between the two calls, and
 * every fleet that requires it fails in that window. Replacement is total —
 * a field absent from the new body is absent from the stored secret — which
 * is the only honest write on a resource the client can never read back.
 * Accepts the same input forms as `create`: `--data` (or `--data=@-` stdin)
 * and the typed custom-endpoint flags.
 */
export const secretUpdateEffectFromFlags = (
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
      "agentsfleet secret update <name> --data='<json-object>'",
    );
    // Resolved before the token so a malformed invocation costs no round-trip
    // and never reaches the vault.
    const body = yield* resolveSecretBody(flags);

    const token = yield* resolveAuthToken;
    const data = yield* withCatalogueProvider(flags, body, token);
    yield* http.request<unknown>({
      path: wsSecretPath(wsId, name),
      method: METHOD_PUT,
      body: { data },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ status: STATUS_UPDATED, name });
    } else {
      yield* output.success(`Secret '${name}' updated. The name stayed claimed throughout.`);
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
      "No secrets stored. Create one with: agentsfleet secret create <name> --data=@- (pipe JSON on stdin)",
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
