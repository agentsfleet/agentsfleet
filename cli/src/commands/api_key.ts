// Tenant API-key commands. The raw key is printed only on create; list,
// revoke, and delete never receive or render key material.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import {
  TENANT_API_KEYS_PATH,
  tenantApiKeyPath,
} from "../lib/api-paths.ts";
import { validateRequiredId } from "../program/validators.ts";
import { UnexpectedError, ValidationError, type CliError } from "../errors/index.ts";
import {
  API_KEY_CREATED_AT,
  API_KEY_KEY_NAME,
  API_KEY_SORTS,
  API_KEY_SORT_CREATED_AT_DESC,
} from "../constants/api-key.ts";

export interface ApiKeyCreateArgs {
  readonly name: string | undefined;
  readonly description: string | undefined;
}

export interface ApiKeyListArgs {
  readonly sort: string | undefined;
}

interface CreatedApiKey {
  readonly id?: string;
  readonly key_name?: string;
  readonly key?: string;
  readonly created_at?: number | string | null;
}

interface ApiKeyRow {
  readonly id?: string;
  readonly key_name?: string;
  readonly active?: boolean;
  readonly created_at?: number | string | null;
  readonly last_used_at?: number | string | null;
  readonly revoked_at?: number | string | null;
}

interface ApiKeyListResponse {
  readonly items?: ReadonlyArray<ApiKeyRow>;
  readonly total?: number | null;
  readonly next_cursor?: string | null;
}

interface RevokedApiKey {
  readonly id?: string;
  readonly active?: boolean;
  readonly revoked_at?: number | string | null;
}

const KEY_NAME = API_KEY_KEY_NAME;
const CREATED_AT = API_KEY_CREATED_AT;
const DEFAULT_SORT = API_KEY_SORT_CREATED_AT_DESC;
const API_KEY_ID = "api_key_id" as const;
// Mirrors the daemon's QUERY_STARTING_AFTER (http/pagination.zig).
const QUERY_STARTING_AFTER = "starting_after" as const;
const QUERY_SORT = "sort" as const;
// The list has no paging controls; the client follows next_cursor until the
// server reports the end. The bound exists so a server that never returns a
// null cursor cannot spin the walk forever — at the server's default page of
// 50 it covers 2,000 keys, far past any real tenant.
const MAX_LIST_WALK_REQUESTS = 40;
const STATUS_ACTIVE = "active" as const;
const STATUS_REVOKED = "revoked" as const;
const TIME_NEVER = "never" as const;
const TIME_MISSING = "-" as const;
const SORTS: ReadonlySet<string> = new Set(API_KEY_SORTS);

const requireValue = (
  value: string | undefined,
  detail: string,
  suggestion: string,
): Effect.Effect<string, ValidationError> =>
  value
    ? Effect.succeed(value)
    : Effect.fail(new ValidationError({ detail, suggestion }));

const requireValidId = (
  value: string | undefined,
  fieldName: string,
): Effect.Effect<string, ValidationError> =>
  Effect.gen(function* () {
    const raw = yield* requireValue(
      value,
      `${fieldName} is required`,
      `pass <${fieldName}> as a positional argument`,
    );
    const check = validateRequiredId(raw, fieldName);
    if (!check.ok) {
      return yield* Effect.fail(
        new ValidationError({
          detail: check.message,
          suggestion: "pass a valid uuidv7",
        }),
      );
    }
    return raw;
  });

const parseSort = (raw: string | undefined): Effect.Effect<string, ValidationError> => {
  if (raw === undefined) return Effect.succeed(DEFAULT_SORT);
  if (SORTS.has(raw)) return Effect.succeed(raw);
  return Effect.fail(
    new ValidationError({
      detail: "sort must be one of created_at, -created_at, key_name, -key_name",
      suggestion: "pass --sort -created_at",
    }),
  );
};

const formatTime = (
  value: number | string | null | undefined,
  missing: string,
): string => (value ? new Date(value).toISOString() : missing);

const queryForList = (sort: string, startingAfter: string | undefined): string => {
  const query = new URLSearchParams();
  query.set(QUERY_SORT, sort);
  if (startingAfter !== undefined) query.set(QUERY_STARTING_AFTER, startingAfter);
  return `${TENANT_API_KEYS_PATH}?${query.toString()}`;
};

const LIST_TABLE_COLUMNS = [
  { key: KEY_NAME, label: "NAME" },
  { key: "status", label: "STATUS" },
  { key: "last_used_at", label: "LAST_USED" },
  { key: CREATED_AT, label: "CREATED" },
  { key: API_KEY_ID, label: "API_KEY_ID" },
];

const listTableRow = (key: ApiKeyRow) => ({
  key_name: key.key_name ?? "",
  status: key.active === false ? STATUS_REVOKED : STATUS_ACTIVE,
  last_used_at: formatTime(key.last_used_at, TIME_NEVER),
  created_at: formatTime(key.created_at, TIME_MISSING),
  api_key_id: key.id ?? "",
});

export const apiKeyCreateEffectFromArgs = (
  args: ApiKeyCreateArgs,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const name = yield* requireValue(
      args.name,
      "api-key create requires --name <name>",
      "pass --name <key_name>",
    );

    const res = yield* http.request<CreatedApiKey>({
      path: TENANT_API_KEYS_PATH,
      method: "POST",
      body: { key_name: name, description: args.description ?? "" },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }

    yield* output.success(`API key created: ${res.id ?? ""}`);
    yield* output.info("");
    yield* output.info("API key (shown once - store securely):");
    yield* output.info(`  ${res.key ?? ""}`);
    yield* output.info("");
    yield* output.printTable(
      [
        { key: "label", label: "" },
        { key: "value", label: "" },
      ],
      [
        { label: API_KEY_ID, value: res.id ?? "" },
        { label: KEY_NAME, value: res.key_name ?? name },
        { label: CREATED_AT, value: formatTime(res.created_at, TIME_MISSING) },
      ],
    );
  });

export const apiKeyListEffectFromArgs = (
  args: ApiKeyListArgs,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const sort = yield* parseSort(args.sort);

    // The list is complete by construction: follow next_cursor until the
    // server reports the end. Each page arrives in the requested sort, so
    // the concatenation preserves the global order.
    const keys: ApiKeyRow[] = [];
    let total: number | null = null;
    let cursor: string | undefined;
    let completed = false;
    for (let requests = 0; requests < MAX_LIST_WALK_REQUESTS && !completed; requests += 1) {
      const res = yield* http.request<ApiKeyListResponse>({
        path: queryForList(sort, cursor),
        token,
      });
      keys.push(...(res.items ?? []));
      total = res.total ?? total;
      const next = res.next_cursor ?? null;
      if (next === null) completed = true;
      else cursor = next;
    }
    if (!completed) {
      return yield* Effect.fail(
        new UnexpectedError({
          detail: `the API key list did not end after ${MAX_LIST_WALK_REQUESTS} pages`,
          suggestion: "retry; if it persists, report the server's runaway next_cursor",
        }),
      );
    }

    if (config.jsonMode) {
      yield* output.printJson({ items: keys, total: total ?? keys.length, next_cursor: null });
      return;
    }
    if (keys.length === 0) {
      yield* output.info("no API keys found");
      return;
    }
    yield* output.printTable(LIST_TABLE_COLUMNS, keys.map(listTableRow));
  });

export const apiKeyRevokeEffectFromId = (
  rawId: string | undefined,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const id = yield* requireValidId(rawId, API_KEY_ID);

    const res = yield* http.request<RevokedApiKey>({
      path: tenantApiKeyPath(id),
      method: "PATCH",
      body: { active: false },
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    yield* output.success(`API key ${res.id ?? id} revoked. It can no longer authenticate.`);
  });

export const apiKeyDeleteEffectFromId = (
  rawId: string | undefined,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;
    const token = yield* resolveAuthToken;
    const id = yield* requireValidId(rawId, API_KEY_ID);

    yield* http.request<unknown>({
      path: tenantApiKeyPath(id),
      method: "DELETE",
      token,
    });

    if (config.jsonMode) {
      yield* output.printJson({ deleted: true, id });
      return;
    }
    yield* output.success(`API key ${id} deleted.`);
  });
