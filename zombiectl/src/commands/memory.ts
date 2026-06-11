// `zombiectl memory list|search` — read-only window into a zombie's durable
// memory over GET /v1/workspaces/{ws}/zombies/{zid}/memories.
//
// zombiectl memory list   --zombie <id> [--category <name>] [--limit <n>] [--workspace <id>]
// zombiectl memory search --zombie <id> <query> [--limit <n>] [--workspace <id>]
//
// Output as a service (7 Pillars): a real terminal gets an aligned table;
// `--json` or a piped/redirected stdout gets the published response envelope
// verbatim (auto-JSON when piped — the bind site reads stdout.isTTY and
// threads it here, so the handler never touches the process). Empty results
// are an answer, not an error: friendly line + docs pointer, exit 0.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { resolveAuthToken } from "./workspace-guards.ts";
import { wsZombieMemoriesPath } from "../lib/api-paths.ts";
import { ui } from "../output/index.ts";
import {
  ConfigError,
  ServerError,
  ValidationError,
  type CliError,
  type NetworkError,
  type UnexpectedError,
} from "../errors/index.ts";

// Mirrors of the server's published limit constants — same identifiers as
// src/zombied/http/handlers/memory/helpers.zig and the OpenAPI bounds on
// list_zombie_memories (RULE UFS: cross-runtime constants share a name).
// The client validates against the cap and documents the defaults in help
// text; it never invents its own caps, and it only forwards `limit` when
// the operator passed one (the server applies its defaults otherwise).
export const MAX_RECALL_LIMIT = 100;
export const DEFAULT_RECALL_LIMIT = 20;
export const DEFAULT_LIST_LIMIT = 100;

// Table preview cap in Unicode code points. Full content is never lost —
// JSON mode carries it verbatim.
const PREVIEW_MAX = 80;

const MS_PER_SECOND = 1000;
const TYPE_NUMBER = "number" as const;
const TYPE_STRING = "string" as const;
const LITERAL_DASH = "—" as const;
const SERVER_ERROR_TAG = "ServerError" as const;

// Server error codes this command remaps to actionable suggestions — same
// identifiers as src/zombied/errors/error_registry.zig (RULE UFS).
const ERR_MEM_ZOMBIE_NOT_FOUND = "UZ-MEM-002";
const ERR_MEM_UNAVAILABLE = "UZ-MEM-003";

const MEMORY_HYGIENE_DOCS_URL = "https://docs.usezombie.com/memory";
const SUGGEST_ZOMBIE_NOT_FOUND =
  "run `zombiectl list` to see the zombies in this workspace (or pass --workspace <id>)";
const SUGGEST_MEM_UNAVAILABLE =
  "retry shortly — the memory backend is temporarily unavailable";

const USAGE_LIST =
  "usage: zombiectl memory list --zombie <id> [--category <name>] [--limit <n>] [--workspace <id>]";
const USAGE_SEARCH =
  "usage: zombiectl memory search --zombie <id> <query> [--limit <n>] [--workspace <id>]";

const EMPTY_LIST_MESSAGE = "No memories stored for this zombie yet.";

const FIELD_KEY = "key" as const;
const FIELD_CATEGORY = "category" as const;
const FIELD_UPDATED = "updated" as const;
const FIELD_PREVIEW = "preview" as const;

const isNumber = (value: unknown): value is number => typeof value === TYPE_NUMBER;
const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

interface MemoryRow {
  readonly key?: string | null;
  readonly content?: string | null;
  readonly category?: string | null;
  readonly updated_at?: number | string | null;
}

interface MemoryListResponse {
  readonly items?: ReadonlyArray<MemoryRow>;
  readonly total?: number;
  readonly request_id?: string;
}

export interface MemoryReadFlags {
  readonly zombieId?: string | undefined;
  readonly category?: string | undefined;
  readonly limit?: string | undefined;
  readonly workspaceId?: string | undefined;
  // Bind-site stdout.isTTY read: `false` means piped/redirected → emit the
  // JSON envelope (7 Pillars auto-JSON). `undefined` (direct Effect callers,
  // unit tests) behaves like a terminal.
  readonly stdoutIsTty?: boolean | undefined;
}

// The only spot that interprets the wire timestamp. Today the wire carries
// epoch seconds as a decimal string (schema/013 TEXT, NullClaw format); the
// retention-schema work flips it to numeric epoch milliseconds. Both render
// here; JSON mode passes the raw value through untouched. When the numeric
// wire lands, delete the string branch and flip the fixtures — nothing
// else moves.
export const renderUpdatedAt = (value: number | string | null | undefined): string => {
  if (isNumber(value) && Number.isFinite(value)) {
    return new Date(value).toISOString();
  }
  if (isString(value) && /^\d+$/.test(value)) {
    return new Date(Number.parseInt(value, 10) * MS_PER_SECOND).toISOString();
  }
  return LITERAL_DASH;
};

// Collapse whitespace, then cut at PREVIEW_MAX code points. Slicing by code
// point (Array.from) can never split a surrogate pair, so the preview always
// re-encodes as valid UTF-8 even mid-emoji at the boundary.
export const previewText = (text: string | null | undefined): string => {
  if (!isString(text) || text.length === 0) return "";
  const oneline = text.replace(/\s+/g, " ").trim();
  const points = Array.from(oneline);
  if (points.length <= PREVIEW_MAX) return oneline;
  return `${points.slice(0, PREVIEW_MAX - 1).join("")}…`;
};

const requireZombieId = (
  value: string | undefined,
  usage: string,
): Effect.Effect<string, ValidationError> =>
  isString(value) && value.length > 0
    ? Effect.succeed(value)
    : Effect.fail(
        new ValidationError({ detail: "--zombie <id> is required", suggestion: usage }),
      );

const resolveWorkspace = (
  override: string | undefined,
): Effect.Effect<string, ConfigError | UnexpectedError, Workspaces> =>
  Effect.gen(function* () {
    if (isString(override) && override.length > 0) return override;
    const workspaces = yield* Workspaces;
    const state = yield* workspaces.load;
    if (!state.current_workspace_id) {
      return yield* Effect.fail(
        new ConfigError({
          detail: "no workspace selected",
          suggestion: "run `zombiectl workspace use <id>` or pass --workspace <id>",
        }),
      );
    }
    return state.current_workspace_id;
  });

interface MemoryQueryParams {
  readonly query: string | undefined;
  readonly category: string | undefined;
  readonly limit: string | undefined;
}

const buildPath = (wsId: string, zombieId: string, params: MemoryQueryParams): string => {
  const qs = new URLSearchParams();
  if (isString(params.query) && params.query.length > 0) qs.set("query", params.query);
  // the wire param shares the table-field name by design — one const serves both
  if (isString(params.category) && params.category.length > 0) qs.set(FIELD_CATEGORY, params.category);
  if (isString(params.limit) && params.limit.length > 0) qs.set("limit", params.limit);
  const q = qs.toString();
  const base = wsZombieMemoriesPath(wsId, zombieId);
  return q ? `${base}?${q}` : base;
};

// The transport's generic 4xx suggestion ("verify the request payload") is
// useless for the two memory-specific failures — remap to the next action
// the operator can actually take. Detail, code, status, request_id pass
// through so support workflows keep their grep keys.
const withMemorySuggestions = (err: NetworkError | ServerError): NetworkError | ServerError => {
  if (err._tag !== SERVER_ERROR_TAG) return err;
  if (err.code === ERR_MEM_ZOMBIE_NOT_FOUND) {
    return new ServerError({
      detail: err.detail,
      suggestion: SUGGEST_ZOMBIE_NOT_FOUND,
      code: err.code,
      status: err.status,
      requestId: err.requestId,
    });
  }
  if (err.code === ERR_MEM_UNAVAILABLE) {
    return new ServerError({
      detail: err.detail,
      suggestion: SUGGEST_MEM_UNAVAILABLE,
      code: err.code,
      status: err.status,
      requestId: err.requestId,
    });
  }
  return err;
};

interface MemoryRequestSpec extends MemoryQueryParams {
  readonly zombieId: string | undefined;
  readonly workspaceId: string | undefined;
  readonly stdoutIsTty: boolean | undefined;
  readonly usage: string;
  readonly emptyMessage: string;
}

const memoryReadEffect = (
  req: MemoryRequestSpec,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  Effect.gen(function* () {
    const config = yield* CliConfig;
    const output = yield* Output;
    const http = yield* HttpClient;

    const zombieId = yield* requireZombieId(req.zombieId, req.usage);
    const wsId = yield* resolveWorkspace(req.workspaceId);
    const token = yield* resolveAuthToken;

    const res = yield* http
      .request<MemoryListResponse>({ path: buildPath(wsId, zombieId, req), token })
      .pipe(Effect.mapError(withMemorySuggestions));

    // Machine context — explicit --json, or stdout is not a terminal —
    // gets the published envelope verbatim: full content, raw updated_at.
    if (config.jsonMode || req.stdoutIsTty === false) {
      yield* output.printJson(res);
      return;
    }

    const items = res.items ?? [];
    if (items.length === 0) {
      yield* output.info(req.emptyMessage);
      yield* output.info(ui.dim(`Memory hygiene guide: ${MEMORY_HYGIENE_DOCS_URL}`));
      return;
    }

    yield* output.printTable(
      [
        { key: FIELD_KEY, label: "KEY" },
        { key: FIELD_CATEGORY, label: "CATEGORY" },
        { key: FIELD_UPDATED, label: "UPDATED" },
        { key: FIELD_PREVIEW, label: "PREVIEW" },
      ],
      items.map((m) => ({
        [FIELD_KEY]: String(m.key ?? ""),
        [FIELD_CATEGORY]: String(m.category ?? ""),
        [FIELD_UPDATED]: renderUpdatedAt(m.updated_at),
        [FIELD_PREVIEW]: previewText(m.content),
      })),
    );
  });

export const memoryListEffectFromFlags = (
  flags: MemoryReadFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  memoryReadEffect({
    zombieId: flags.zombieId,
    workspaceId: flags.workspaceId,
    query: undefined,
    category: flags.category,
    limit: flags.limit,
    stdoutIsTty: flags.stdoutIsTty,
    usage: USAGE_LIST,
    emptyMessage: EMPTY_LIST_MESSAGE,
  });

export const memorySearchEffectFromArgs = (
  query: string | undefined,
  flags: MemoryReadFlags,
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> =>
  isString(query) && query.trim().length > 0
    ? memoryReadEffect({
        zombieId: flags.zombieId,
        workspaceId: flags.workspaceId,
        query,
        category: undefined,
        limit: flags.limit,
        stdoutIsTty: flags.stdoutIsTty,
        usage: USAGE_SEARCH,
        emptyMessage: `No memories matched "${query}".`,
      })
    : Effect.fail(
        new ValidationError({ detail: "search query is required", suggestion: USAGE_SEARCH }),
      );
