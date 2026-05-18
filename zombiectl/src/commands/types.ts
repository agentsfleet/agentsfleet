// commands/types — shared shapes consumed by every command handler in
// src/commands/**. Created in §14 D39 to give the 14 command files a
// single source of truth for `(ctx, parsed, workspaces, deps)` instead
// of redeclaring at every call site.
//
// The shape mirrors what `cli.ts`'s buildDeps() and the lifecycle wrap
// in handlers-bind.js actually produce. Fields tighten as commands
// migrate; the open index signatures on CommandCtx / ParsedArgs /
// Workspaces accept any extra fields handlers read so command-specific
// reads don't force a churn through this file.

import type { ApiRequestOptions, RetryConfig } from "../lib/http.ts";
import type { StreamGetCallback, StreamGetOptions } from "../lib/sse.ts";
import type { Credentials, Workspaces, WorkspaceItem } from "../lib/state.ts";
import type { AnalyticsClient } from "../lib/analytics.ts";
import type {
  UiTheme,
  WriteStream,
  TableColumn,
  TableRow,
  KeyValueRows,
} from "../output/index.ts";

// Ctx shape commands read directly. Previously inherited from
// lib/run-command.ts::HandlerCtx; inlined here when the legacy
// wrapper was retired and the dispatcher (lib/run-effect.ts) became
// the only command boundary. Fields are a strict superset of what
// cli.ts buildDeps() emits + what handlers reach into.
export interface RunCommandCtx {
  stderr?: NodeJS.WritableStream | null;
  jsonMode?: boolean;
  apiUrl?: string;
  analyticsClient?: AnalyticsClient | null;
  distinctId?: string;
  analyticsContext?: Record<string, unknown> | null;
  retryConfig?: RetryConfig | null;
  // CLI telemetry session/device identity. Local camelCase; the
  // wire-format snake_case rename happens at emit/append sites.
  cliSessionId?: string | null;
  cliDeviceId?: string | null;
  [key: string]: unknown;
}

// On-disk shapes re-exported from lib/state.ts. Single source of truth for
// the worktree (~/.config/zombiectl/credentials.json, workspaces.json) —
// handlers, cli.ts, and the lifecycle all reference the same interfaces.
// Previously commands/types.ts declared its own Workspaces + CredentialFile
// with overlapping-but-inconsistent shapes, which surfaced as cross-file
// assignability errors the moment cli.ts became typed (D41).
export type { Credentials, Workspaces, WorkspaceItem };

export type { ApiRequestOptions };

export type StreamGetFn = (
  url: string,
  headers: Record<string, string>,
  onEvent: StreamGetCallback,
  options?: StreamGetOptions,
) => Promise<void>;

// The ctx that command handlers receive. Extends RunCommandCtx with
// the streams and credential fields commands read directly. The
// retryConfig is now set by the dispatcher (or by callers that need
// it) rather than mutated by a wrapper.
export interface CommandCtx extends RunCommandCtx {
  stdout?: NodeJS.WritableStream | null;
  stderr?: NodeJS.WritableStream | null;
  stdin?: NodeJS.ReadableStream | string | null;
  token?: string | null;
  apiKey?: string | null;
  // apiUrl is required — cli.ts sets it before any handler runs, and
  // http-client.ts:HttpRequestContext requires it. Optional here meant
  // the commands ↔ http-client seam couldn't typecheck (TS exact-optional
  // rejects the contravariance).
  apiUrl: string;
  jsonMode?: boolean;
  noOpen?: boolean;
  noInput?: boolean;
  env?: NodeJS.ProcessEnv | Record<string, string | undefined>;
}

// Parsed CLI invocation (Commander/cli-tree frame.parsed). `options`
// is a free-form record of flags — each command narrows by destructure.
export interface ParsedArgs {
  options: Record<
    string,
    string | boolean | number | string[] | undefined | null
  >;
  positionals: string[];
  args?: string[];
  [key: string]: unknown;
}

// Body returned by deps.request — commands narrow per-endpoint.
export type ApiResponse = unknown;

export interface SpinnerOptions {
  enabled?: boolean | undefined;
  stream?: NodeJS.WritableStream | null | undefined;
  label?: string | undefined;
  style?: string | undefined;
}

export interface SpinnerHandle {
  start: () => void;
  stop: (final?: string) => void;
  succeed: (message?: string) => void;
  fail: (message?: string) => void;
}

// The deps bag passed to every command. Shape matches buildDeps() in
// src/cli.ts verbatim. writeError is widened to accept any subset of
// CommandDeps that the call site assembles (some commands call it with
// just { printJson, writeLine, ui }, others pass the full deps).
export interface CommandDeps {
  apiHeaders: (ctx: CommandCtx) => Record<string, string>;
  clearCredentials: () => Promise<void> | void;
  createSpinner: (options: SpinnerOptions) => SpinnerHandle;
  loadCredentials: () => Promise<Credentials> | Credentials;
  newIdempotencyKey: () => string;
  openUrl: (
    url: string,
    opts?: {
      env?: NodeJS.ProcessEnv | Record<string, string | undefined> | undefined;
    },
  ) => Promise<boolean | void> | boolean | void;
  printJson: (
    stream: WriteStream | NodeJS.WritableStream,
    value: unknown,
  ) => void;
  printKeyValue: (stream: WriteStream, rows: KeyValueRows) => void;
  printSection: (stream: WriteStream, title: string) => void;
  printTable: (
    stream: WriteStream,
    columns: ReadonlyArray<TableColumn>,
    rows: ReadonlyArray<TableRow>,
  ) => void;
  request: (
    ctx: CommandCtx,
    path: string,
    opts?: ApiRequestOptions,
  ) => Promise<ApiResponse>;
  saveCredentials: (cred: Credentials) => Promise<void> | void;
  saveWorkspaces: (workspaces: Workspaces) => Promise<void> | void;
  // Optional SSE injector — zombie_steer uses this for the live event
  // tail; tests override it with a fake to assert frame handling. The
  // wrapper in handlers-bind.js omits it; commands fall back to the
  // real `streamGet` from `lib/sse.ts` when absent.
  streamGet?: StreamGetFn;
  ui: UiTheme;
  writeLine: (
    stream: WriteStream | NodeJS.WritableStream,
    line?: string,
  ) => void;
  writeError: (
    ctx: CommandCtx,
    code: string,
    message: string,
    opts?:
      | CommandDeps
      | Partial<
          Pick<CommandDeps, "printJson" | "writeLine" | "ui">
        >,
  ) => void;
}

// Canonical command handler signature. Every src/commands/*.ts export
// `command<Verb>` conforms to this.
export type CommandHandler = (
  ctx: CommandCtx,
  parsed: ParsedArgs,
  workspaces: Workspaces,
  deps: CommandDeps,
) => Promise<number>;

// ─────────────────────────────────────────────────────────────────────
// Option readers — narrow the union value of `parsed.options[key]` to
// the concrete primitive the command actually uses. Encapsulates the
// "non-empty string or null" pattern so commands stop redeclaring
// `typeof x === "string" && x.length > 0 ? x : null` at every site.
// TypeScript has no built-in `isString`/`isNumber`; these are the
// project-specific narrowing helpers worth standardising on.
// ─────────────────────────────────────────────────────────────────────

export function readString(
  options: ParsedArgs["options"],
  key: string,
): string | null {
  const v = options[key];
  return typeof v === "string" && v.length > 0 ? v : null;
}

export function readBoolean(
  options: ParsedArgs["options"],
  key: string,
): boolean {
  return Boolean(options[key]);
}

export function readNumber(
  options: ParsedArgs["options"],
  key: string,
): number | null {
  const v = options[key];
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (typeof v === "string" && v.length > 0) {
    const n = Number.parseInt(v, 10);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

// Sister to `readString` for callers that prefer `undefined`-shaped
// optional reads (Effect-side commands, `??`-chained flag fallbacks).
// Also coerces parseIntOption's numeric results to a string so query-
// string + flag-passthrough plumbing doesn't have to special-case both.
export function readStringOpt(
  options: ParsedArgs["options"],
  key: string,
): string | undefined {
  const v = options[key];
  if (typeof v === "string" && v.length > 0) return v;
  if (typeof v === "number" && Number.isFinite(v)) return String(v);
  return undefined;
}
