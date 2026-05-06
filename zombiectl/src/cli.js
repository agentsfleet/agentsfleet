import { openUrl } from "./lib/browser.js";
import {
  cliAnalytics,
  drainCliAnalyticsEvents,
  getCliAnalyticsContext,
} from "./lib/analytics.js";
import { findRoute } from "./program/routes.js";
import { registerProgramCommands } from "./program/command-registry.js";
import { commandAgent as commandAgentModule } from "./commands/agent.js";
import { commandGrant as commandGrantModule } from "./commands/grant.js";
import { commandTenant as commandTenantModule } from "./commands/tenant.js";
import { commandBilling as commandBillingModule } from "./commands/billing.js";
import { commandZombie as commandZombieModule } from "./commands/zombie.js";
import { ui, printKeyValue, printSection, printTable } from "./ui-theme.js";
import { createSpinner } from "./ui-progress.js";
import {
  clearCredentials,
  loadCredentials,
  loadWorkspaces,
  newIdempotencyKey,
  saveCredentials,
  saveWorkspaces,
} from "./lib/state.js";
import { ApiError, apiHeaders, printApiError, request } from "./program/http-client.js";
import { parseFlags, parseGlobalArgs, normalizeApiUrl, DEFAULT_API_URL } from "./program/args.js";
import { extractDistinctIdFromToken, extractRoleFromToken } from "./program/auth-token.js";
import { printHelp, printJson, writeError, writeLine } from "./program/io.js";
import { printBanner, printPreReleaseWarning } from "./program/banner.js";
import { suggestCommand } from "./program/suggest.js";
import { requireAuth, AUTH_FAIL_MESSAGE } from "./program/auth-guard.js";
import { createCoreHandlers } from "./commands/core.js";

export const VERSION = "0.33.1";

export { parseGlobalArgs };

const AUTH_EXEMPT_ROUTES = new Set(["login"]);

export async function runCli(argv, io = {}) {
  const stdout = io.stdout || process.stdout;
  const stderr = io.stderr || process.stderr;
  const env = io.env || process.env;
  const fetchImpl = io.fetchImpl || globalThis.fetch;

  const { global, rest } = parseGlobalArgs(argv, env);
  const noColor = Boolean(env.NO_COLOR === "1" || env.NO_COLOR === "true");

  printPreReleaseWarning(stderr, { noColor, jsonMode: global.json, ttyOnly: !stderr.isTTY });

  if (global.version) {
    if (global.json) {
      printJson(stdout, { version: VERSION });
    } else {
      printBanner(stdout, VERSION, { noColor, jsonMode: false });
    }
    return 0;
  }

  const creds = await loadCredentials().catch(() => ({}));
  const workspaces = await loadWorkspaces().catch(() => ({ items: [], current_workspace_id: null }));
  const resolvedToken = creds.token || env.ZOMBIE_TOKEN || null;
  const resolvedApiKey = env.API_KEY || env.ZOMBIE_API_KEY || null;
  const resolvedAuthRole = extractRoleFromToken(resolvedToken) || (resolvedApiKey ? "admin" : null);

  if (global.help || rest.length === 0) {
    printHelp(stdout, ui, {
      version: VERSION,
      env,
      jsonMode: global.json,
      authRole: resolvedAuthRole,
    });
    return 0;
  }

  const ctx = {
    apiUrl: normalizeApiUrl(global.apiUrl || creds.api_url || DEFAULT_API_URL),
    token: resolvedToken,
    apiKey: resolvedApiKey,
    authRole: resolvedAuthRole,
    jsonMode: global.json,
    noOpen: global.noOpen,
    noInput: global.noInput,
    stdout,
    stderr,
    env,
    fetchImpl,
  };

  const command = rest[0];
  const args = rest.slice(1);
  const route = findRoute(command, args);

  // Auth guard: only `login` is exempt. Doctor and install both interact with
  // workspace state, so they require credentials before any HTTP call.
  if (route && !AUTH_EXEMPT_ROUTES.has(route.key)) {
    const auth = requireAuth(ctx);
    if (!auth.ok) {
      writeError(ctx, "AUTH_REQUIRED", AUTH_FAIL_MESSAGE, { printJson, writeLine, ui });
      return 1;
    }
  }

  const core = createCoreHandlers(ctx, workspaces, {
    clearCredentials,
    createSpinner,
    newIdempotencyKey,
    openUrl,
    parseFlags,
    printJson,
    printKeyValue,
    printSection,
    printTable,
    request,
    saveCredentials,
    saveWorkspaces,
    ui,
    writeLine,
    apiHeaders,
  });

  const analyticsClient = await cliAnalytics.createCliAnalytics(env);
  const distinctId = extractDistinctIdFromToken(ctx.token);

  const handlers = registerProgramCommands({
    login: (routeArgs) => core.commandLogin(routeArgs),
    logout: () => core.commandLogout(),
    workspace: (routeArgs) => core.commandWorkspace(routeArgs),
    doctor: () => core.commandDoctor(),
    // External agent key management
    agent: (routeArgs) => commandAgentModule(ctx, routeArgs, workspaces, {
      parseFlags,
      request,
      apiHeaders,
      ui,
      printJson,
      printKeyValue,
      printSection,
      printTable,
      writeLine,
    }),
    // Integration grant management
    grant: (routeArgs) => commandGrantModule(ctx, routeArgs, workspaces, {
      parseFlags,
      request,
      apiHeaders,
      ui,
      printJson,
      printTable,
      writeLine,
    }),
    // Tenant-scoped: provider posture (get/set/reset), billing snapshot.
    tenant: (routeArgs) => commandTenantModule(ctx, routeArgs, workspaces, {
      parseFlags,
      request,
      apiHeaders,
      ui,
      printJson,
      printTable,
      writeLine,
    }),
    // Tenant billing dashboard: `zombiectl billing show [--limit N] [--json]`.
    billing: (routeArgs) => commandBillingModule(ctx, routeArgs, workspaces, {
      parseFlags,
      request,
      apiHeaders,
      ui,
      printJson,
      printTable,
      writeLine,
    }),
    // Zombie commands
    zombieInstall: (routeArgs) => commandZombieModule(ctx, ["install", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieList: (routeArgs) => commandZombieModule(ctx, ["list", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, printTable, writeLine, writeError,
    }),
    zombieStatus: (routeArgs) => commandZombieModule(ctx, ["status", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieKill: (routeArgs) => commandZombieModule(ctx, ["kill", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieStop: (routeArgs) => commandZombieModule(ctx, ["stop", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieResume: (routeArgs) => commandZombieModule(ctx, ["resume", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieDelete: (routeArgs) => commandZombieModule(ctx, ["delete", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieLogs: (routeArgs) => commandZombieModule(ctx, ["logs", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieSteer: (routeArgs) => commandZombieModule(ctx, ["steer", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieEvents: (routeArgs) => commandZombieModule(ctx, ["events", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
    zombieCredential: (routeArgs) => commandZombieModule(ctx, ["credential", ...routeArgs], workspaces, {
      parseFlags, request, apiHeaders, ui, printJson, printKeyValue, printSection, writeLine, writeError,
    }),
  });

  try {
    if (route && handlers[route.key]) {
      cliAnalytics.trackCliEvent(analyticsClient, distinctId, "cli_command_started", {
        command: route.key,
        json_mode: String(ctx.jsonMode),
      });

      const exitCode = await handlers[route.key](args);
      const analyticsContext = getCliAnalyticsContext(ctx);
      let eventDistinctId = distinctId;
      if (exitCode === 0 && route.key === "login") {
        const latestCreds = await loadCredentials();
        eventDistinctId = extractDistinctIdFromToken(latestCreds.token) || distinctId;
      }
      cliAnalytics.trackCliEvent(analyticsClient, distinctId, "cli_command_finished", {
        command: route.key,
        exit_code: String(exitCode),
        ...analyticsContext,
      });

      if (exitCode === 0 && route.key === "login") {
        cliAnalytics.trackCliEvent(analyticsClient, eventDistinctId, "user_authenticated", {
          command: route.key,
          ...analyticsContext,
        });
      }
      if (exitCode === 0 && route.key === "workspace" && args[0] === "add") {
        cliAnalytics.trackCliEvent(analyticsClient, distinctId, "workspace_created", {
          command: route.key,
          ...analyticsContext,
        });
      }
      for (const queuedEvent of drainCliAnalyticsEvents(ctx)) {
        cliAnalytics.trackCliEvent(analyticsClient, eventDistinctId, queuedEvent.event, {
          command: route.key,
          ...analyticsContext,
          ...queuedEvent.properties,
        });
      }
      if (exitCode !== 0) {
        cliAnalytics.trackCliEvent(analyticsClient, distinctId, "cli_error", {
          command: route.key,
          exit_code: String(exitCode),
          ...analyticsContext,
        });
      }
      return exitCode;
    }

    // "Did you mean?" suggestion for unknown commands
    const fullInput = [command, ...args].join(" ");
    const suggestions = suggestCommand(fullInput);
    if (ctx.jsonMode) {
      writeError(ctx, "UNKNOWN_COMMAND", `unknown command: ${command}`, { printJson, writeLine, ui });
    } else if (suggestions.length > 0) {
      writeLine(stderr, ui.err(`unknown command: ${command}`));
      writeLine(stderr);
      writeLine(stderr, "The most similar commands are");
      for (const s of suggestions) {
        writeLine(stderr, `    ${s}`);
      }
    } else {
      writeLine(stderr, ui.err(`unknown command: ${command}`));
      writeLine(stderr, `Run 'zombiectl --help' for usage.`);
    }

    cliAnalytics.trackCliEvent(analyticsClient, distinctId, "cli_error", {
      command,
      error_code: "UNKNOWN_COMMAND",
      exit_code: "2",
      ...getCliAnalyticsContext(ctx),
    });
    return 2;
  } catch (err) {
    const errorCode = err instanceof ApiError ? err.code || "API_ERROR" : "UNEXPECTED";
    cliAnalytics.trackCliEvent(analyticsClient, distinctId, "cli_error", {
      command: route?.key || command || "unknown",
      error_code: errorCode,
      exit_code: "1",
      ...getCliAnalyticsContext(ctx),
    });
    try {
      const isNetworkFailure =
        err instanceof TypeError &&
        typeof err.message === "string" &&
        err.message.toLowerCase().includes("fetch failed");
      if (isNetworkFailure) {
        const apiUrl = ctx?.apiUrl || global.apiUrl || env.ZOMBIE_API_URL || DEFAULT_API_URL;
        const message = `cannot reach usezombie API at ${apiUrl} — check that the service is running and ZOMBIE_API_URL is correct`;
        if (global.json) {
          printJson(stderr, { error: { code: "API_UNREACHABLE", message } });
        } else {
          writeLine(stderr, ui.err(message));
        }
        return 1;
      }
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
    await cliAnalytics.shutdownCliAnalytics(analyticsClient);
  }
}
