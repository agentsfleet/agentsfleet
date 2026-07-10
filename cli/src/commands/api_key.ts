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
import { INTEGER_RE, validateRequiredId } from "../program/validators.ts";
import { ValidationError, type CliError } from "../errors/index.ts";
import {
  API_KEY_CREATED_AT,
  API_KEY_KEY_NAME,
  API_KEY_SORTS,
  API_KEY_SORT_CREATED_AT_DESC,
  DEFAULT_API_KEY_PAGE,
  DEFAULT_API_KEY_PAGE_SIZE,
  MAX_API_KEY_PAGE_SIZE,
} from "../constants/api-key.ts";

export interface ApiKeyCreateArgs {
  readonly name: string | undefined;
  readonly description: string | undefined;
}

export interface ApiKeyListArgs {
  readonly page: string | undefined;
  readonly pageSize: string | undefined;
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
  readonly total?: number;
  readonly page?: number;
  readonly page_size?: number;
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
const PAGE_FIELD = "page" as const;
const PAGE_SIZE_FIELD = "page_size" as const;
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

const parseBoundedInt = (
  raw: string | undefined,
  fallback: number,
  fieldName: string,
  min: number,
  max: number,
): Effect.Effect<number, ValidationError> => {
  if (raw === undefined) return Effect.succeed(fallback);
  if (!INTEGER_RE.test(raw)) {
    return Effect.fail(
      new ValidationError({
        detail: `${fieldName} must be an integer between ${min} and ${max}`,
        suggestion: `pass --${fieldName.replace("_", "-")} <${min}..${max}>`,
      }),
    );
  }
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < min || parsed > max) {
    return Effect.fail(
      new ValidationError({
        detail: `${fieldName} must be an integer between ${min} and ${max}`,
        suggestion: `pass --${fieldName.replace("_", "-")} <${min}..${max}>`,
      }),
    );
  }
  return Effect.succeed(parsed);
};

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

const queryForList = (page: number, pageSize: number, sort: string): string => {
  const query = new URLSearchParams();
  query.set(PAGE_FIELD, String(page));
  query.set(PAGE_SIZE_FIELD, String(pageSize));
  query.set("sort", sort);
  return `${TENANT_API_KEYS_PATH}?${query.toString()}`;
};

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
    const page = yield* parseBoundedInt(
      args.page,
      DEFAULT_API_KEY_PAGE,
      PAGE_FIELD,
      1,
      Number.MAX_SAFE_INTEGER,
    );
    const pageSize = yield* parseBoundedInt(
      args.pageSize,
      DEFAULT_API_KEY_PAGE_SIZE,
      PAGE_SIZE_FIELD,
      1,
      MAX_API_KEY_PAGE_SIZE,
    );
    const sort = yield* parseSort(args.sort);

    const res = yield* http.request<ApiKeyListResponse>({
      path: queryForList(page, pageSize, sort),
      token,
    });
    const keys = res.items ?? [];

    if (config.jsonMode) {
      yield* output.printJson(res);
      return;
    }
    if (keys.length === 0) {
      yield* output.info("no API keys found");
      return;
    }
    yield* output.printTable(
      [
        { key: KEY_NAME, label: "NAME" },
        { key: "status", label: "STATUS" },
        { key: "last_used_at", label: "LAST_USED" },
        { key: CREATED_AT, label: "CREATED" },
        { key: API_KEY_ID, label: "API_KEY_ID" },
      ],
      keys.map((key) => ({
        key_name: key.key_name ?? "",
        status: key.active === false ? STATUS_REVOKED : STATUS_ACTIVE,
        last_used_at: formatTime(key.last_used_at, TIME_NEVER),
        created_at: formatTime(key.created_at, TIME_MISSING),
        api_key_id: key.id ?? "",
      })),
    );
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
