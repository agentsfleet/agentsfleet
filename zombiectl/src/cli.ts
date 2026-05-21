import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { type Command, CommanderError, InvalidArgumentError } from "commander";

import { openUrl } from "./lib/browser.ts";
import {
  clearCredentials,
  loadCredentials,
  loadWorkspaces,
  newIdempotencyKey,
  saveCredentials,
  saveWorkspaces,
  type Credentials,
  type Workspaces,
} from "./lib/state.ts";
import { Effect } from "effect";
import { runCommanderParse } from "./lib/commander-bridge.ts";
import { extractRoleFromToken, resolveAuthTokenForCli } from "./program/auth-token.ts";
import { printJson, writeError, writeLine } from "./program/io.ts";
import { printVersion, printPreReleaseWarning } from "./program/banner.ts";
import { requireAuth, AUTH_FAIL_MESSAGE } from "./program/auth-guard.ts";
import { ui, printKeyValue, printSection, printTable } from "./output/index.ts";
import { createSpinner } from "./ui-progress.ts";
import { DEFAULT_API_URL, normalizeApiUrl } from "./util/url.ts";
import { buildProgram } from "./program/cli-tree.ts";
import { buildHandlers, type Lifecycle } from "./program/handlers-bind.ts";
import { detectTokenInArgv } from "./lib/argv-redact.ts";
import { ROLE_ADMIN } from "./constants/auth-roles.ts";
import type { ProgramState } from "./program/cli-tree-types.ts";
import type { CommandCtx, CommandDeps } from "./commands/types.ts";
import type { WritableStreamLike } from "./output/capability.ts";

// VERSION: source-of-truth package.json, read once. `make sync-version`
// writes package.json + build.zig.zon together; no manual edits here.
const PKG_JSON_PATH = join(dirname(fileURLToPath(import.meta.url)), "..", "package.json");
const pkgJson = JSON.parse(readFileSync(PKG_JSON_PATH, "utf8")) as { version: string };
export const VERSION: string = pkgJson.version;

// Only `login` skips the preAction auth-guard. Subcommands of every
// other root inherit the requirement.
const AUTH_EXEMPT: ReadonlySet<string> = new Set(["login"]);

export interface RunCliIo {
  stdout?: WritableStreamLike;
  stderr?: WritableStreamLike;
  stdin?: NodeJS.ReadableStream;
  env?: NodeJS.ProcessEnv;
  fetchImpl?: typeof fetch;
}

// Commander's built-in --version prints plain text and exits, which
// can't satisfy `--version --json` or `--help --version → --version
// wins`. Pre-scan argv so we render version ourselves.
function maybePrintVersion(
  argv: readonly string[],
  stdout: WritableStreamLike,
  jsonMode: boolean,
  env: NodeJS.ProcessEnv,
): boolean {
  for (const token of argv) {
    if (token === "--") break;
    if (token === "--version" || token === "-v") {
      if (jsonMode) {
        printJson(stdout, { version: VERSION });
      } else {
        printVersion(stdout, VERSION, {
          noColor: Boolean(env.NO_COLOR && env.NO_COLOR.length > 0),
          jsonMode: false,
        });
      }
      return true;
    }
  }
  return false;
}

function detectJsonMode(argv: readonly string[]): boolean {
  for (const token of argv) {
    if (token === "--") return false;
    if (token === "--json") return true;
  }
  return false;
}

function resolveGlobalApiUrl(argv: readonly string[], env: NodeJS.ProcessEnv): string | null {
  let api: string | null = null;
  for (let i = 0; i < argv.length; i += 1) {
    const t = argv[i];
    if (t === undefined) break;
    if (t === "--") break;
    if (t === "--api") { api = argv[i + 1] || null; break; }
    if (t.startsWith("--api=")) { api = t.slice("--api=".length); break; }
  }
  return api || env.ZOMBIE_API_URL || env.API_URL || null;
}

function buildDeps(): CommandDeps {
  return {
    clearCredentials,
    createSpinner,
    loadCredentials,
    newIdempotencyKey,
    openUrl,
    printJson,
    printKeyValue,
    printSection,
    printTable,
    saveCredentials,
    saveWorkspaces,
    ui,
    writeLine,
    writeError,
  };
}

function installPreAction(program: Command, ctx: CommandCtx, state: ProgramState): void {
  program.hook("preAction", (thisCommand, actionCommand) => {
    // Carry --no-open / --no-input / --json / --api from commander's
    // globals into ctx so handlers see the operator's intent. Commander
    // normalises --no-open to opts.open === false (similarly --no-input).
    const opts = thisCommand.optsWithGlobals() as Record<string, unknown>;
    ctx.jsonMode = ctx.jsonMode || Boolean(opts["json"]);
    ctx.noOpen = opts["open"] === false || opts["noOpen"] === true;
    ctx.noInput = opts["input"] === false || opts["noInput"] === true;
    const apiOverride = opts["api"];
    if (typeof apiOverride === "string" && apiOverride.length > 0) {
      ctx.apiUrl = normalizeApiUrl(apiOverride);
    }

    // Auth-guard: walk to the top-level command and exempt `login` only.
    let root: Command = actionCommand;
    while (root.parent && root.parent.name() !== "zombiectl") root = root.parent;
    if (AUTH_EXEMPT.has(root.name())) return;
    const auth = requireAuth(ctx);
    if (!auth.ok) {
      state.exitCode = 1;
      writeError(ctx, "AUTH_REQUIRED", AUTH_FAIL_MESSAGE, { printJson, writeLine, ui });
      throw new CommanderError(1, "auth.required", AUTH_FAIL_MESSAGE);
    }
  });
}

// commander.* error codes that map to POSIX "usage error" exit 2.
// Commander itself uses 1 for these; we override to 2 so the
// did-you-mean / unknown-subcommand contract matches POSIX.
const COMMANDER_USAGE_CODES: ReadonlySet<string> = new Set([
  "commander.unknownCommand",
  "commander.unknownOption",
  "commander.invalidArgument",
  "commander.missingArgument",
  "commander.missingMandatoryOptionValue",
  "commander.optionMissingArgument",
  "commander.excessArguments",
]);

function exitFromCommanderError(err: CommanderError, state: ProgramState): number {
  if (err.code === "commander.help" || err.code === "commander.helpDisplayed") return 0;
  if (state.exitCode !== 0) return state.exitCode;
  if (COMMANDER_USAGE_CODES.has(err.code)) return 2;
  return typeof err.exitCode === "number" ? err.exitCode : 1;
}

function errMessage(err: unknown): string {
  if (err instanceof Error && typeof err.message === "string") return err.message;
  return String(err);
}

const EMPTY_CREDS: Credentials = { token: null, saved_at: null, session_id: null, api_url: null };
const EMPTY_WORKSPACES: Workspaces = { current_workspace_id: null, items: [] };

export async function runCli(argv: readonly string[], io: RunCliIo = {}): Promise<number> {
  const stdout = (io.stdout ?? process.stdout) as WritableStreamLike;
  const stderr = (io.stderr ?? process.stderr) as WritableStreamLike;
  const env = io.env ?? process.env;
  const fetchImpl = io.fetchImpl ?? globalThis.fetch;

  const jsonMode = detectJsonMode(argv);
  const noColor = Boolean(env.NO_COLOR && env.NO_COLOR.length > 0);

  printPreReleaseWarning(stderr, { noColor, jsonMode, ttyOnly: !stderr.isTTY });

  // D25 — surface the --token leak warning before any command runs. The
  // check is pure (no env/process reads); fires for any argv shape, --version
  // included, because the value is in shell history either way.
  const tokenLeak = detectTokenInArgv(argv);
  if (tokenLeak) writeLine(stderr, tokenLeak);

  if (maybePrintVersion(argv, stdout, jsonMode, env)) return 0;

  // Bare `zombiectl` → --help so commander routes via stdout + exit 0 instead of stderr "missing command".
  const effectiveArgv = argv.length === 0 ? ["--help"] : [...argv];

  const [creds, workspaces] = await Promise.all([
    loadCredentials().catch(() => EMPTY_CREDS),
    loadWorkspaces().catch(() => EMPTY_WORKSPACES),
  ]);
  // Session identity (`device_id`, `session_id`, `session_last_active`)
  // is read+bumped inside `resolveIdentity` against `telemetry.json`
  // (mirrors supabase). No session file maintained here.
  // D26: TTY-priority — interactive shells let an env-var the operator
  // just exported beat a possibly-stale credentials.json; scripted runs
  // prefer the on-disk credential.
  const stdinSrc = io.stdin ?? process.stdin;
  const resolvedAuth = resolveAuthTokenForCli({
    fileToken: creds.token ?? null,
    env,
    isTty: Boolean((stdinSrc as { isTTY?: boolean }).isTTY),
  });
  const resolvedToken = resolvedAuth.token;
  const resolvedApiKey = env.API_KEY || env.ZOMBIE_API_KEY || null;
  const resolvedAuthRole = extractRoleFromToken(resolvedToken) || (resolvedApiKey ? ROLE_ADMIN : null);

  const explicitApi = resolveGlobalApiUrl(argv, env);
  const ctx: CommandCtx = {
    apiUrl: normalizeApiUrl(explicitApi || creds.api_url || DEFAULT_API_URL),
    token: resolvedToken,
    apiKey: resolvedApiKey,
    authRole: resolvedAuthRole,
    jsonMode,
    noOpen: false,
    noInput: false,
    // Tests inject partial WritableStreamLike mocks (just `.write` + `isTTY`);
    // CommandCtx declares the field as the richer NodeJS.WritableStream because
    // that matches the production runtime. Narrowing the field type would
    // ripple through every handler that reads `ctx.stdout`; the cast is the
    // smaller honest seam.
    stdout: stdout as unknown as NodeJS.WritableStream,
    stderr: stderr as unknown as NodeJS.WritableStream,
    env,
    fetchImpl,
  };

  const lifecycle: Lifecycle = {
    ctx,
    workspaces,
    deps: buildDeps(),
    lastCommand: null,
  };

  const handlers = buildHandlers(lifecycle);
  const state: ProgramState = { exitCode: 0 };
  const program = buildProgram({ handlers, version: VERSION, state });

  program.exitOverride();
  program.configureOutput({
    writeOut: (s: string) => { stdout.write(s); },
    writeErr: (s: string) => { stderr.write(s); },
  });

  installPreAction(program, ctx, state);

  // commander parse runs through the bridge. On success, the command
  // handler's own withCommandInstrumentation wrap (handlers-bind.ts) has
  // already emitted cli_command_executed. On parse-stage failure, the
  // bridge emits a single cli_command_executed with command "__parse__".
  const parseResult = await Effect.runPromise(runCommanderParse(program, effectiveArgv));

  if (!parseResult.ok) {
    const err = parseResult.commanderError ?? parseResult.otherError;
    if (err instanceof CommanderError) {
      return exitFromCommanderError(err, state);
    }
    if (err instanceof InvalidArgumentError) {
      writeLine(stderr, ui.err(`error: ${err.message}`));
      return 2;
    }
    const message = errMessage(err);
    if (ctx.jsonMode) {
      printJson(stderr, { error: { code: "UNEXPECTED", message } });
    } else {
      writeLine(stderr, ui.err(`error: ${message}`));
    }
    return 1;
  }

  return state.exitCode;
}
