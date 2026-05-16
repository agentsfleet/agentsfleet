// commands/types — shared shapes consumed by every command handler in
// src/commands/**. Created in §14 D39 to give the 14 command files a
// single source of truth for `(ctx, parsed, workspaces, deps)` instead
// of redeclaring at every call site.
//
// The shape mirrors what `cli.js`'s buildDeps() and the lifecycle wrap
// in handlers-bind.js actually produce. Fields tighten as commands
// migrate; the open index signatures on CommandCtx / ParsedArgs /
// Workspaces accept any extra fields handlers read so command-specific
// reads don't force a churn through this file.

import type { HandlerCtx as RunCommandCtx } from "../lib/run-command.ts";
import type { ApiRequestOptions } from "../lib/http.ts";
import type { StreamGetCallback, StreamGetOptions } from "../lib/sse.ts";
import type {
  UiTheme,
  WriteStream,
  TableColumn,
  TableRow,
  KeyValueRows,
} from "../output/index.ts";

export type { ApiRequestOptions };

export type StreamGetFn = (
  url: string,
  headers: Record<string, string>,
  onEvent: StreamGetCallback,
  options?: StreamGetOptions,
) => Promise<void>;

// The ctx that command handlers receive. Extends runCommand's
// HandlerCtx (whose retryConfig the wrapper mutates) with the streams
// and credential fields commands read directly.
export interface CommandCtx extends RunCommandCtx {
  stdout?: NodeJS.WritableStream | null;
  stderr?: NodeJS.WritableStream | null;
  stdin?: NodeJS.ReadableStream | string | null;
  token?: string | null;
  apiKey?: string | null;
  apiUrl?: string;
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

export interface WorkspaceEntry {
  id: string;
  label?: string;
  role?: string;
  [key: string]: unknown;
}

export interface Workspaces {
  current_workspace_id?: string | null;
  current_workspace_label?: string | null;
  workspaces?: WorkspaceEntry[];
  [key: string]: unknown;
}

// Body returned by deps.request — commands narrow per-endpoint.
export type ApiResponse = unknown;

export interface SpinnerOptions {
  enabled?: boolean | undefined;
  stream?: NodeJS.WritableStream | null | undefined;
  label?: string | undefined;
}

export interface SpinnerHandle {
  start: () => void;
  stop: (final?: string) => void;
  succeed?: () => void;
  fail?: () => void;
}

export interface CredentialFile {
  token?: string | null;
  saved_at?: number | null;
  session_id?: string | null;
  [key: string]: unknown;
}

// The deps bag passed to every command. Shape matches buildDeps() in
// src/cli.js verbatim. writeError is widened to accept any subset of
// CommandDeps that the call site assembles (some commands call it with
// just { printJson, writeLine, ui }, others pass the full deps).
export interface CommandDeps {
  apiHeaders: (ctx: CommandCtx) => Record<string, string>;
  clearCredentials: () => Promise<void> | void;
  createSpinner: (options: SpinnerOptions | string) => SpinnerHandle;
  loadCredentials: () => Promise<CredentialFile | null> | CredentialFile | null;
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
  saveCredentials: (cred: CredentialFile) => Promise<void> | void;
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
