import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { CommanderError, InvalidArgumentError } from "commander";

import { openUrl } from "./lib/browser.js";
import {
  cliAnalytics,
  drainCliAnalyticsEvents,
  getCliAnalyticsContext,
} from "./lib/analytics.js";
import {
  clearCredentials,
  loadCredentials,
  loadWorkspaces,
  newIdempotencyKey,
  saveCredentials,
  saveWorkspaces,
} from "./lib/state.ts";
import { apiHeaders, request } from "./program/http-client.ts";
import { extractDistinctIdFromToken, extractRoleFromToken } from "./program/auth-token.ts";
import { printJson, writeError, writeLine } from "./program/io.js";
import { printVersion, printPreReleaseWarning } from "./program/banner.js";
import { requireAuth, AUTH_FAIL_MESSAGE } from "./program/auth-guard.js";
import { ui, printKeyValue, printSection, printTable } from "./output/index.ts";
import { createSpinner } from "./ui-progress.js";
import { DEFAULT_API_URL, normalizeApiUrl } from "./util/url.ts";
import { buildProgram } from "./program/cli-tree.js";
import { buildHandlers } from "./program/handlers-bind.js";
import { ROLE_ADMIN } from "./constants/auth-roles.ts";
import { EVT_USER_AUTHENTICATED, EVT_WORKSPACE_CREATED } from "./constants/analytics-events.ts";

// VERSION is the source-of-truth `package.json` field, read once at module
// load. `make sync-version` writes package.json + build.zig.zon together;
// no manual edits to cli.js to bump.
const PKG_JSON_PATH = join(dirname(fileURLToPath(import.meta.url)), "..", "package.json");
export const VERSION = JSON.parse(readFileSync(PKG_JSON_PATH, "utf8")).version;

// Only `login` skips the preAction auth-guard. Subcommands of every
// other root inherit the requirement.
const AUTH_EXEMPT = new Set(["login"]);

// Commander's built-in --version prints plain text and exits, which
// can't satisfy `--version --json` or `--help --version → --version
// wins`. Pre-scan argv so we render version ourselves.
function maybePrintVersion(argv, stdout, jsonMode, env) {
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

function detectJsonMode(argv) {
  for (const token of argv) {
    if (token === "--") return false;
    if (token === "--json") return true;
  }
  return false;
}

function resolveGlobalApiUrl(argv, env) {
  let api = null;
  for (let i = 0; i < argv.length; i += 1) {
    const t = argv[i];
    if (t === "--") break;
    if (t === "--api") { api = argv[i + 1] || null; break; }
    if (t.startsWith("--api=")) { api = t.slice("--api=".length); break; }
  }
  return api || env.ZOMBIE_API_URL || env.API_URL || null;
}

function buildDeps() {
  return {
    apiHeaders,
    clearCredentials,
    createSpinner,
    loadCredentials,
    newIdempotencyKey,
    openUrl,
    printJson,
    printKeyValue,
    printSection,
    printTable,
    request,
    saveCredentials,
    saveWorkspaces,
    ui,
    writeLine,
    writeError,
  };
}

function installPreAction(program, ctx, state) {
  program.hook("preAction", (thisCommand, actionCommand) => {
    // Carry --no-open / --no-input / --json / --api from commander's
    // globals into ctx so handlers see the operator's intent. Commander
    // normalises --no-open to opts.open === false (similarly --no-input).
    const opts = thisCommand.optsWithGlobals();
    ctx.jsonMode = ctx.jsonMode || Boolean(opts.json);
    ctx.noOpen = opts.open === false || opts.noOpen === true;
    ctx.noInput = opts.input === false || opts.noInput === true;
    if (opts.api) ctx.apiUrl = normalizeApiUrl(opts.api);

    // Auth-guard: walk to the top-level command and exempt `login` only.
    let root = actionCommand;
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
// Commander itself uses 1 for these — the legacy CLI used 2, the
// did-you-mean / unknown-subcommand tests rely on that contract.
const COMMANDER_USAGE_CODES = new Set([
  "commander.unknownCommand",
  "commander.unknownOption",
  "commander.invalidArgument",
  "commander.missingArgument",
  "commander.missingMandatoryOptionValue",
  "commander.optionMissingArgument",
  "commander.excessArguments",
]);

function exitFromCommanderError(err, state) {
  if (err.code === "commander.help" || err.code === "commander.helpDisplayed") return 0;
  if (state.exitCode !== 0) return state.exitCode;
  if (COMMANDER_USAGE_CODES.has(err.code)) return 2;
  return typeof err.exitCode === "number" ? err.exitCode : 1;
}

async function runPostActionAnalytics(lifecycle, state) {
  const { ctx, analyticsClient, distinctId } = lifecycle;
  const analyticsContext = getCliAnalyticsContext(ctx);
  let eventDistinctId = distinctId;
  if (state.exitCode === 0 && lifecycle.lastCommand === "login") {
    const latestCreds = await loadCredentials().catch(() => ({}));
    eventDistinctId = extractDistinctIdFromToken(latestCreds.token) || distinctId;
    cliAnalytics.trackCliEvent(analyticsClient, eventDistinctId, EVT_USER_AUTHENTICATED, {
      command: lifecycle.lastCommand,
      ...analyticsContext,
    });
  }
  if (state.exitCode === 0 && lifecycle.lastCommand === "workspace.add") {
    cliAnalytics.trackCliEvent(analyticsClient, distinctId, EVT_WORKSPACE_CREATED, {
      command: lifecycle.lastCommand,
      ...analyticsContext,
    });
  }
  for (const queuedEvent of drainCliAnalyticsEvents(ctx)) {
    cliAnalytics.trackCliEvent(analyticsClient, eventDistinctId, queuedEvent.event, {
      command: lifecycle.lastCommand || "unknown",
      ...analyticsContext,
      ...queuedEvent.properties,
    });
  }
}

export async function runCli(argv, io = {}) {
  const stdout = io.stdout || process.stdout;
  const stderr = io.stderr || process.stderr;
  const env = io.env || process.env;
  const fetchImpl = io.fetchImpl || globalThis.fetch;

  const jsonMode = detectJsonMode(argv);
  const noColor = Boolean(env.NO_COLOR && env.NO_COLOR.length > 0);

  printPreReleaseWarning(stderr, { noColor, jsonMode, ttyOnly: !stderr.isTTY });

  if (maybePrintVersion(argv, stdout, jsonMode, env)) return 0;

  // Bare `zombiectl` (no args) — commander defaults to a "missing
  // command" error on stderr; tests + operators expect help on stdout
  // with exit 0. Promote empty argv to `--help` so it routes through
  // commander's normal help path.
  const effectiveArgv = argv.length === 0 ? ["--help"] : argv;

  const creds = await loadCredentials().catch(() => ({}));
  const workspaces = await loadWorkspaces().catch(() => ({ items: [], current_workspace_id: null }));
  const resolvedToken = creds.token || env.ZOMBIE_TOKEN || null;
  const resolvedApiKey = env.API_KEY || env.ZOMBIE_API_KEY || null;
  const resolvedAuthRole = extractRoleFromToken(resolvedToken) || (resolvedApiKey ? ROLE_ADMIN : null);

  const explicitApi = resolveGlobalApiUrl(argv, env);
  const ctx = {
    apiUrl: normalizeApiUrl(explicitApi || creds.api_url || DEFAULT_API_URL),
    token: resolvedToken,
    apiKey: resolvedApiKey,
    authRole: resolvedAuthRole,
    jsonMode,
    noOpen: false,
    noInput: false,
    stdout,
    stderr,
    env,
    fetchImpl,
  };

  const analyticsClient = await cliAnalytics.createCliAnalytics(env);
  const distinctId = extractDistinctIdFromToken(ctx.token);

  const lifecycle = {
    ctx, workspaces, deps: buildDeps(), analyticsClient, distinctId, lastCommand: null,
  };

  const handlers = buildHandlers(lifecycle);
  const state = { exitCode: 0 };
  const program = buildProgram({ handlers, version: VERSION, state });

  program.exitOverride();
  program.configureOutput({
    writeOut: (s) => stdout.write(s),
    writeErr: (s) => stderr.write(s),
  });

  installPreAction(program, ctx, state);

  try {
    await program.parseAsync(effectiveArgv, { from: "user" });
  } catch (err) {
    if (err instanceof CommanderError) {
      const exitCode = exitFromCommanderError(err, state);
      if (COMMANDER_USAGE_CODES.has(err.code)) {
        try {
          cliAnalytics.trackCliEvent(analyticsClient, distinctId, "cli_error", {
            command: lifecycle.lastCommand || "unknown",
            error_code: err.code === "commander.unknownCommand" ? "UNKNOWN_COMMAND" : "USAGE_ERROR",
            exit_code: String(exitCode),
            ...getCliAnalyticsContext(ctx),
          });
        } catch {
          // Analytics failure is swallowed; the unknown-command UX is the
          // headline. The finally block still shuts down the client.
        }
      }
      return exitCode;
    }
    if (err instanceof InvalidArgumentError) {
      writeLine(stderr, ui.err(`error: ${err.message}`));
      return 2;
    }
    cliAnalytics.trackCliEvent(analyticsClient, distinctId, "cli_error", {
      command: lifecycle.lastCommand || "unknown",
      error_code: "UNEXPECTED",
      exit_code: "1",
      ...getCliAnalyticsContext(ctx),
    });
    if (ctx.jsonMode) {
      printJson(stderr, { error: { code: "UNEXPECTED", message: String(err?.message ?? err) } });
    } else {
      writeLine(stderr, ui.err(`error: ${String(err?.message ?? err)}`));
    }
    return 1;
  } finally {
    try {
      await runPostActionAnalytics(lifecycle, state);
    } finally {
      await cliAnalytics.shutdownCliAnalytics(analyticsClient);
    }
  }

  return state.exitCode;
}
