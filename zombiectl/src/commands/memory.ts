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

// Table preview cap in Unicode code points. Full content is never lost —
// JSON mode carries it verbatim. The server limit mirrors live in
// src/constants/memory-limits.ts (consumed by the command tree).
const PREVIEW_MAX = 80;

const TYPE_NUMBER = "number" as const;
const TYPE_STRING = "string" as const;
const LITERAL_DASH = "—" as const;
const SERVER_ERROR_TAG = "ServerError" as const;

// Server error codes this command remaps to actionable suggestions — same
// identifiers as src/zombied/errors/error_registry.zig (RULE UFS).
const ERR_MEM_ZOMBIE_NOT_FOUND = "UZ-MEM-002";
const ERR_MEM_UNAVAILABLE = "UZ-MEM-003";

const MEMORY_HYGIENE_DOCS_URL = "https://docs.agentsfleet.net/memory";
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
  readonly updated_at?: number | null;
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

// Server content is untrusted input to the operator's terminal: strip C0 and
// C1 control bytes (ESC, BEL, CSI, …) so stored memory can't smuggle ANSI/OSC
// sequences (clipboard hijack, screen rewriting, forged rows) into the table.
// JSON mode stays verbatim — machine consumers get the raw bytes.
const CONTROL_BYTES_RE = /[\u0000-\u001F\u007F-\u009F]/g;
export const cleanCell = (value: unknown): string =>
  String(value ?? "").replace(CONTROL_BYTES_RE, "");

// The only spot that interprets the wire timestamp: numeric epoch
// milliseconds (schema/013 BIGINT; OpenAPI integer/int64). JSON mode passes
// the raw value through untouched. The runtime guard keeps a malformed
// envelope rendering as the dash, and the try/catch keeps an out-of-range
// value (Date throws RangeError past ±8.64e15 ms) from killing the table.
export const renderUpdatedAt = (value: number | null | undefined): string => {
  try {
    if (isNumber(value) && Number.isFinite(value)) {
      return new Date(value).toISOString();
    }
  } catch {
    return LITERAL_DASH;
  }
  return LITERAL_DASH;
};

// Collapse whitespace FIRST (newlines/tabs become spaces), then strip the
// remaining control bytes, then cut at PREVIEW_MAX code points. Order
// matters: cleanCell deletes \n/\t outright, so running it first would
// concatenate words across line breaks. Slicing by code point (Array.from)
// can never split a surrogate pair, so the preview always re-encodes as
// valid UTF-8 even mid-emoji.
export const previewText = (text: string | null | undefined): string => {
  if (!isString(text) || text.length === 0) return "";
  const oneline = cleanCell(text.replace(/\s+/g, " ")).trim();
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
// useless for the memory-specific failures — remap to the next action the
// operator can actually take. Detail, code, status, request_id pass through
// so support workflows keep their grep keys.
const MEMORY_SUGGESTIONS: Record<string, string> = {
  [ERR_MEM_ZOMBIE_NOT_FOUND]: SUGGEST_ZOMBIE_NOT_FOUND,
  [ERR_MEM_UNAVAILABLE]: SUGGEST_MEM_UNAVAILABLE,
};

const withMemorySuggestions = (err: NetworkError | ServerError): NetworkError | ServerError => {
  if (err._tag !== SERVER_ERROR_TAG) return err;
  const suggestion = MEMORY_SUGGESTIONS[err.code];
  if (suggestion === undefined) return err;
  return new ServerError({
    detail: err.detail,
    suggestion,
    code: err.code,
    status: err.status,
    requestId: err.requestId,
  });
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

    // Runtime shape guard — the compile-time type can't vouch for server
    // bytes; a malformed envelope renders as empty rather than crashing.
    const items = Array.isArray(res.items) ? res.items : [];
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
        [FIELD_KEY]: cleanCell(m.key),
        [FIELD_CATEGORY]: cleanCell(m.category),
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
): Effect.Effect<void, CliError, CliConfig | Credentials | HttpClient | Output | Workspaces> => {
  // Trim at the boundary — shell quoting padding ("  acme  ") is operator
  // noise, not search intent; the no-match message shows what was searched.
  const trimmed = isString(query) ? query.trim() : "";
  return trimmed.length > 0
    ? memoryReadEffect({
        zombieId: flags.zombieId,
        workspaceId: flags.workspaceId,
        query: trimmed,
        category: undefined,
        limit: flags.limit,
        stdoutIsTty: flags.stdoutIsTty,
        usage: USAGE_SEARCH,
        emptyMessage: `No memories matched "${trimmed}".`,
      })
    : Effect.fail(
        new ValidationError({ detail: "search query is required", suggestion: USAGE_SEARCH }),
      );
};
