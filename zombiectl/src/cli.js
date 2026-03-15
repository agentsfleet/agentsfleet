import { openUrl } from "./lib/browser.js";
import { createCliAnalytics, shutdownCliAnalytics, trackCliEvent } from "./lib/analytics.js";
import { findRoute } from "./program/routes.js";
import { registerProgramCommands } from "./program/command-registry.js";
import { commandHarness as commandHarnessModule } from "./commands/harness.js";
import { ui, printKeyValue, printTable } from "./ui-theme.js";
import { createSpinner } from "./ui-progress.js";
import {
  appendRun,
  clearCredentials,
  loadCredentials,
  loadRuns,
  loadWorkspaces,
  newIdempotencyKey,
  saveCredentials,
  saveWorkspaces,
} from "./lib/state.js";
import { ApiError, apiHeaders, printApiError, request } from "./program/http-client.js";
import { parseFlags, parseGlobalArgs, normalizeApiUrl, DEFAULT_API_URL } from "./program/args.js";
import { extractDistinctIdFromToken } from "./program/auth-token.js";
import { printHelp, printJson, writeLine } from "./program/io.js";
import { createCoreHandlers } from "./commands/core.js";

const VERSION = "0.1.0";

export { parseGlobalArgs };

export async function runCli(argv, io = {}) {
  const stdout = io.stdout || process.stdout;
  const stderr = io.stderr || process.stderr;
  const env = io.env || process.env;
  const fetchImpl = io.fetchImpl || globalThis.fetch;

  const { global, rest } = parseGlobalArgs(argv, env);
  if (global.version) {
    writeLine(stdout, VERSION);
    return 0;
  }

  if (global.help || rest.length === 0) {
    printHelp(stdout, ui);
    return 0;
  }

  const creds = await loadCredentials();
  const workspaces = await loadWorkspaces();

  const ctx = {
    apiUrl: normalizeApiUrl(global.apiUrl || creds.api_url || DEFAULT_API_URL),
    token: creds.token || env.ZOMBIE_TOKEN || null,
    apiKey: env.API_KEY || env.ZOMBIE_API_KEY || null,
    jsonMode: global.json,
    noOpen: global.noOpen,
    noInput: global.noInput,
    stdout,
    stderr,
    env,
    fetchImpl,
  };

  const core = createCoreHandlers(ctx, workspaces, {
    appendRun,
    clearCredentials,
    createSpinner,
    loadRuns,
    newIdempotencyKey,
    openUrl,
    parseFlags,
    printJson,
    printKeyValue,
    printTable,
    request,
    saveCredentials,
    saveWorkspaces,
    ui,
    writeLine,
    apiHeaders,
  });

  const analyticsClient = await createCliAnalytics(env);
  const distinctId = extractDistinctIdFromToken(ctx.token);

  const command = rest[0];
  const args = rest.slice(1);
  const route = findRoute(command, args);
  const handlers = registerProgramCommands({
    login: (routeArgs) => core.commandLogin(routeArgs),
    logout: () => core.commandLogout(),
    workspace: (routeArgs) => core.commandWorkspace(routeArgs),
    specsSync: (routeArgs) => core.commandSpecsSync(routeArgs.slice(1)),
    run: (routeArgs) => core.commandRun(routeArgs),
    runsList: (routeArgs) => core.commandRunsList(routeArgs.slice(1)),
    doctor: () => core.commandDoctor(),
    harness: (routeArgs) => commandHarnessModule(ctx, routeArgs, workspaces, {
      parseFlags,
      request,
      apiHeaders,
      ui,
      printJson,
      writeLine,
    }),
    skillSecret: (routeArgs) => core.commandSkillSecret(routeArgs),
  });

  try {
    if (route && handlers[route.key]) {
      trackCliEvent(analyticsClient, distinctId, "cli_command_started", {
        command: route.key,
        json_mode: String(ctx.jsonMode),
      });

      const exitCode = await handlers[route.key](args);
      let eventDistinctId = distinctId;
      if (exitCode === 0 && route.key === "login") {
        const latestCreds = await loadCredentials();
        eventDistinctId = extractDistinctIdFromToken(latestCreds.token) || distinctId;
      }
      trackCliEvent(analyticsClient, distinctId, "cli_command_finished", {
        command: route.key,
        exit_code: String(exitCode),
      });

      if (exitCode === 0 && route.key === "login") {
        trackCliEvent(analyticsClient, eventDistinctId, "user_authenticated", {
          command: route.key,
        });
      }
      if (exitCode === 0 && route.key === "workspace" && args[0] === "add") {
        trackCliEvent(analyticsClient, distinctId, "workspace_created", {
          command: route.key,
        });
      }
      if (exitCode === 0 && route.key === "run" && args[0] !== "status") {
        trackCliEvent(analyticsClient, distinctId, "run_triggered", {
          command: route.key,
        });
      }
      if (exitCode !== 0) {
        trackCliEvent(analyticsClient, distinctId, "cli_error", {
          command: route.key,
          exit_code: String(exitCode),
        });
      }
      return exitCode;
    }

    writeLine(stderr, ui.err(`unknown command: ${command}`));
    trackCliEvent(analyticsClient, distinctId, "cli_error", {
      command,
      error_code: "UNKNOWN_COMMAND",
      exit_code: "2",
    });
    return 2;
  } catch (err) {
    const errorCode = err instanceof ApiError ? err.code || "API_ERROR" : "UNEXPECTED";
    trackCliEvent(analyticsClient, distinctId, "cli_error", {
      command: route?.key || command || "unknown",
      error_code: errorCode,
      exit_code: "1",
    });
    try {
      printApiError(stderr, err, global.json, printJson, writeLine);
      return 1;
    } catch {
      if (global.json) {
        printJson(stderr, { error: { code: "UNEXPECTED", message: String(err) } });
      } else {
        writeLine(stderr, `error: ${String(err)}`);
      }
      return 1;
    }
  } finally {
    await shutdownCliAnalytics(analyticsClient);
  }
}
