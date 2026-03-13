import { ApiError, apiRequest, authHeaders } from "./lib/http.js";
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

const VERSION = "0.1.0";
const DEFAULT_API_URL = "http://localhost:3000";

function writeLine(stream, line = "") {
  stream.write(`${line}\n`);
}

function printJson(stream, value) {
  writeLine(stream, JSON.stringify(value, null, 2));
}

function normalizeApiUrl(url) {
  return String(url || DEFAULT_API_URL).replace(/\/+$/, "");
}

function extractDistinctIdFromToken(token) {
  if (!token || typeof token !== "string") return null;
  const parts = token.split(".");
  if (parts.length < 2 || !parts[1]) return null;
  try {
    const base64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const padded = base64 + "===".slice((base64.length + 3) % 4);
    const payload = JSON.parse(Buffer.from(padded, "base64").toString("utf8"));
    if (payload && typeof payload.sub === "string" && payload.sub.trim().length > 0) {
      return payload.sub.trim();
    }
  } catch {
    return null;
  }
  return null;
}

function splitOption(token) {
  const idx = token.indexOf("=");
  if (idx === -1) return { key: token, value: null };
  return { key: token.slice(0, idx), value: token.slice(idx + 1) };
}

function parseFlags(tokens) {
  const options = {};
  const positionals = [];

  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (!token.startsWith("--")) {
      positionals.push(token);
      continue;
    }

    const { key, value } = splitOption(token);
    const normalized = key.slice(2);

    if (value !== null) {
      options[normalized] = value;
      continue;
    }

    const next = tokens[i + 1];
    if (next && !next.startsWith("--")) {
      options[normalized] = next;
      i += 1;
      continue;
    }

    options[normalized] = true;
  }

  return { options, positionals };
}

export function parseGlobalArgs(argv, env = process.env) {
  const options = {
    json: false,
    noInput: false,
    noOpen: false,
    help: false,
    version: false,
    api: null,
  };

  const rest = [];
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token === "--json") {
      options.json = true;
    } else if (token === "--no-input") {
      options.noInput = true;
    } else if (token === "--no-open") {
      options.noOpen = true;
    } else if (token === "--help" || token === "-h") {
      options.help = true;
    } else if (token === "--version") {
      options.version = true;
    } else if (token === "--api") {
      options.api = argv[i + 1] || null;
      i += 1;
    } else if (token.startsWith("--api=")) {
      options.api = token.slice("--api=".length);
    } else {
      rest.push(token);
    }
  }

  const derived = {
    apiUrl: normalizeApiUrl(options.api || env.ZOMBIE_API_URL || env.API_URL || DEFAULT_API_URL),
    json: options.json,
    noInput: options.noInput,
    noOpen: options.noOpen,
    help: options.help,
    version: options.version,
  };

  return { global: derived, rest };
}

function printHelp(stdout) {
  writeLine(stdout, ui.head("zombiectl - UseZombie operator CLI"));
  writeLine(stdout);
  writeLine(stdout, "USAGE:");
  writeLine(stdout, "  zombiectl [--api URL] [--json] <command> [subcommand] [flags]");
  writeLine(stdout);
  writeLine(stdout, "COMMANDS:");
  writeLine(stdout, "  login [--timeout-sec N] [--no-open]");
  writeLine(stdout, "  logout");
  writeLine(stdout, "  workspace add <repo_url> [--default-branch BRANCH]");
  writeLine(stdout, "  workspace list");
  writeLine(stdout, "  workspace remove <workspace_id>");
  writeLine(stdout, "  specs sync [--workspace-id ID]");
  writeLine(stdout, "  run [--workspace-id ID] [--spec-id ID] [--mode MODE] [--requested-by USER]");
  writeLine(stdout, "  run status <run_id>");
  writeLine(stdout, "  runs list [--workspace-id ID]");
  writeLine(stdout, "  doctor");
  writeLine(stdout, "  harness source put --workspace-id ID --file PATH [--profile-id ID] [--name NAME]");
  writeLine(stdout, "  harness compile --workspace-id ID [--profile-id ID] [--profile-version-id ID]");
  writeLine(stdout, "  harness activate --workspace-id ID --profile-version-id ID [--activated-by USER]");
  writeLine(stdout, "  harness active --workspace-id ID");
  writeLine(stdout, "  skill-secret put --workspace-id ID --skill-ref REF --key KEY --value VALUE [--scope host|sandbox]");
  writeLine(stdout, "  skill-secret delete --workspace-id ID --skill-ref REF --key KEY");
  writeLine(stdout);
  writeLine(stdout, ui.dim("workspace add opens UseZombie GitHub App install and binds via callback."));
}

function apiHeaders(ctx) {
  return authHeaders({ token: ctx.token, apiKey: ctx.apiKey });
}

async function request(ctx, reqPath, options = {}) {
  const url = `${ctx.apiUrl}${reqPath}`;
  return apiRequest(url, {
    ...options,
    fetchImpl: ctx.fetchImpl,
  });
}

function printApiError(stderr, err, jsonMode) {
  if (!(err instanceof ApiError)) throw err;
  const payload = {
    error: {
      code: err.code || "API_ERROR",
      message: err.message,
      status: err.status || null,
      request_id: err.requestId || null,
    },
  };
  if (jsonMode) {
    printJson(stderr, payload);
  } else {
    writeLine(stderr, `error: ${payload.error.code} ${payload.error.message}`);
    if (payload.error.request_id) writeLine(stderr, `request_id: ${payload.error.request_id}`);
  }
}

async function ensureWorkspaceId(workspaces, explicit) {
  if (explicit) return explicit;
  return workspaces.current_workspace_id;
}

async function commandLogin(ctx, args) {
  const { options } = parseFlags(args);
  const timeoutSec = Number.parseInt(String(options["timeout-sec"] || "300"), 10);
  const pollMs = Number.parseInt(String(options["poll-ms"] || "2000"), 10);

  const created = await request(ctx, "/v1/auth/sessions", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: "{}",
  });

  const loginUrl = created.login_url;
  const sessionId = created.session_id;

  if (!ctx.jsonMode) {
    writeLine(ctx.stdout, `session_id: ${sessionId}`);
    writeLine(ctx.stdout, `login_url: ${loginUrl}`);
  }

  const shouldOpen = options["no-open"] ? false : !ctx.noOpen;
  const opened = shouldOpen ? await openUrl(loginUrl, { env: ctx.env }) : false;

  if (!ctx.jsonMode) {
    if (shouldOpen && opened) writeLine(ctx.stdout, "browser: opened");
    if (shouldOpen && !opened) writeLine(ctx.stdout, "browser: not opened (open URL manually)");
  }

  const deadline = Date.now() + Math.max(5, timeoutSec) * 1000;
  let last = { status: "pending", token: null };
  const spinner = createSpinner({
    enabled: !ctx.jsonMode && Boolean(ctx.stderr.isTTY),
    stream: ctx.stderr,
    label: "waiting for browser login",
  });
  spinner.start();

  try {
    while (Date.now() < deadline) {
      last = await request(ctx, `/v1/auth/sessions/${encodeURIComponent(sessionId)}`, {
        method: "GET",
        headers: { "Content-Type": "application/json" },
      });

      if (last.status === "complete" && last.token) {
        const saved = {
          token: last.token,
          saved_at: Date.now(),
          session_id: sessionId,
          api_url: ctx.apiUrl,
        };
        await saveCredentials(saved);

        const result = {
          status: "complete",
          session_id: sessionId,
          token_saved: true,
          api_url: ctx.apiUrl,
        };
        if (ctx.jsonMode) printJson(ctx.stdout, result);
        else writeLine(ctx.stdout, ui.ok("login complete"));
        spinner.succeed();
        return 0;
      }

      if (last.status === "expired") {
        const result = { status: "expired", session_id: sessionId };
        if (ctx.jsonMode) printJson(ctx.stdout, result);
        else writeLine(ctx.stderr, ui.err("login session expired"));
        spinner.fail();
        return 1;
      }

      await new Promise((resolve) => setTimeout(resolve, Math.max(500, pollMs)));
    }
  } catch (err) {
    spinner.fail();
    throw err;
  }

  spinner.fail();
  const timeoutResult = { status: "timeout", session_id: sessionId };
  if (ctx.jsonMode) printJson(ctx.stdout, timeoutResult);
  else writeLine(ctx.stderr, ui.err("login timed out"));
  return 1;
}

async function commandLogout(ctx) {
  await clearCredentials();
  if (ctx.jsonMode) printJson(ctx.stdout, { status: "ok", logged_out: true });
  else writeLine(ctx.stdout, ui.ok("logout complete"));
  return 0;
}

async function commandWorkspace(ctx, args, workspaces) {
  const action = args[0];
  const tail = args.slice(1);

  if (action === "add") {
    const parsed = parseFlags(tail);
    const repoUrl = parsed.positionals[0];
    if (!repoUrl) {
      writeLine(ctx.stderr, ui.err("workspace add requires <repo_url>"));
      return 2;
    }

    const branch = parsed.options["default-branch"] || "main";
    const created = await request(ctx, "/v1/workspaces", {
      method: "POST",
      headers: apiHeaders(ctx),
      body: JSON.stringify({
        repo_url: repoUrl,
        default_branch: branch,
      }),
    });
    const workspaceId = created.workspace_id;
    const installUrl = created.install_url;

    const existing = workspaces.items.find((x) => x.workspace_id === workspaceId);
    if (!existing) {
      workspaces.items.push({
        workspace_id: workspaceId,
        repo_url: repoUrl,
        default_branch: branch,
        created_at: Date.now(),
      });
    }
    workspaces.current_workspace_id = workspaceId;
    await saveWorkspaces(workspaces);

    const out = {
      workspace_id: workspaceId,
      repo_url: repoUrl,
      install_url: installUrl,
      next_step: "open install_url and complete GitHub App install to bind server-side",
    };
    if (ctx.jsonMode) {
      printJson(ctx.stdout, out);
    } else {
      writeLine(ctx.stdout, ui.ok(`workspace added: ${workspaceId}`));
      printKeyValue(ctx.stdout, {
        workspace_id: workspaceId,
        repo_url: repoUrl,
        branch,
      });
      const opened = ctx.noOpen ? false : await openUrl(installUrl, { env: ctx.env });
      writeLine(ctx.stdout, ui.info(`github_app_install_url: ${installUrl}`));
      if (opened) {
        writeLine(ctx.stdout, ui.ok("opened GitHub App install page in browser"));
      } else {
        writeLine(ctx.stdout, ui.warn("could not auto-open browser; open URL above manually"));
      }
      writeLine(ctx.stdout, ui.dim("After install, GitHub calls /v1/github/callback and binds workspace automatically."));
    }
    return 0;
  }

  if (action === "list") {
    if (ctx.jsonMode) {
      printJson(ctx.stdout, {
        current_workspace_id: workspaces.current_workspace_id,
        workspaces: workspaces.items,
      });
    } else {
      if (workspaces.items.length === 0) {
        writeLine(ctx.stdout, ui.info("no workspaces"));
      }
      printTable(
        ctx.stdout,
        [
          { key: "active", label: "ACTIVE" },
          { key: "workspace_id", label: "WORKSPACE" },
          { key: "repo_url", label: "REPO" },
        ],
        workspaces.items.map((item) => ({
          active: item.workspace_id === workspaces.current_workspace_id ? "*" : "",
          workspace_id: item.workspace_id,
          repo_url: item.repo_url,
        })),
      );
    }
    return 0;
  }

  if (action === "remove") {
    const parsed = parseFlags(tail);
    const workspaceId = parsed.positionals[0] || parsed.options["workspace-id"];
    if (!workspaceId) {
      writeLine(ctx.stderr, "workspace remove requires <workspace_id>");
      return 2;
    }

    workspaces.items = workspaces.items.filter((x) => x.workspace_id !== workspaceId);
    if (workspaces.current_workspace_id === workspaceId) {
      workspaces.current_workspace_id = workspaces.items[0]?.workspace_id || null;
    }
    await saveWorkspaces(workspaces);

    if (ctx.jsonMode) printJson(ctx.stdout, { removed: workspaceId });
    else writeLine(ctx.stdout, ui.ok(`workspace removed: ${workspaceId}`));
    return 0;
  }

  writeLine(ctx.stderr, ui.err("usage: workspace add|list|remove"));
  return 2;
}

async function commandSpecsSync(ctx, args, workspaces) {
  const parsed = parseFlags(args);
  const workspaceId = await ensureWorkspaceId(workspaces, parsed.options["workspace-id"]);
  if (!workspaceId) {
    writeLine(ctx.stderr, ui.err("workspace_id required (set one with workspace add or pass --workspace-id)"));
    return 2;
  }

  const res = await request(ctx, `/v1/workspaces/${encodeURIComponent(workspaceId)}:sync`, {
    method: "POST",
    headers: apiHeaders(ctx),
    body: "{}",
  });

  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else writeLine(ctx.stdout, ui.ok(`specs synced: synced_count=${res.synced_count ?? 0} total_pending=${res.total_pending ?? 0}`));
  return 0;
}

async function commandRun(ctx, args, workspaces) {
  if (args[0] === "status") {
    const runId = args[1];
    if (!runId) {
      writeLine(ctx.stderr, ui.err("run status requires <run_id>"));
      return 2;
    }
    const res = await request(ctx, `/v1/runs/${encodeURIComponent(runId)}`, {
      method: "GET",
      headers: apiHeaders(ctx),
    });
    if (ctx.jsonMode) printJson(ctx.stdout, res);
    else {
      const state = res.current_state ?? res.state ?? "unknown";
      const snapshot = res.run_snapshot_version ?? "default-v1";
      writeLine(ctx.stdout, ui.info(`run ${res.run_id} state=${state} attempt=${res.attempt} run_snapshot_version=${snapshot}`));
    }
    return 0;
  }

  const parsed = parseFlags(args);
  const workspaceId = await ensureWorkspaceId(workspaces, parsed.options["workspace-id"]);
  if (!workspaceId) {
    writeLine(ctx.stderr, ui.err("workspace_id required (set one with workspace add or pass --workspace-id)"));
    return 2;
  }

  let specId = parsed.options["spec-id"];
  if (!specId) {
    const listed = await request(
      ctx,
      `/v1/specs?workspace_id=${encodeURIComponent(workspaceId)}&limit=1`,
      {
        method: "GET",
        headers: apiHeaders(ctx),
      },
    );
    const first = Array.isArray(listed.specs) ? listed.specs[0] : null;
    specId = first?.spec_id;
  }

  if (!specId) {
    writeLine(ctx.stderr, ui.err("spec_id required (no specs found)"));
    return 1;
  }

  const payload = {
    workspace_id: workspaceId,
    spec_id: specId,
    mode: parsed.options.mode || "api",
    requested_by: parsed.options["requested-by"] || "zombiectl",
    idempotency_key: parsed.options["idempotency-key"] || newIdempotencyKey(),
  };

  const res = await request(ctx, "/v1/runs", {
    method: "POST",
    headers: apiHeaders(ctx),
    body: JSON.stringify(payload),
  });

  await appendRun({
    run_id: res.run_id,
    workspace_id: workspaceId,
    spec_id: specId,
    state: res.state,
    attempt: res.attempt,
    created_at: Date.now(),
  });

  if (ctx.jsonMode) printJson(ctx.stdout, res);
  else writeLine(ctx.stdout, ui.ok(`run queued: ${res.run_id} state=${res.state}`));
  return 0;
}

async function commandRunsList(ctx, args, workspaces) {
  const parsed = parseFlags(args);
  const workspaceId = parsed.options["workspace-id"] || workspaces.current_workspace_id;
  const state = await loadRuns();
  let items = Array.isArray(state.items) ? state.items : [];
  if (workspaceId) items = items.filter((x) => x.workspace_id === workspaceId);

  if (ctx.jsonMode) printJson(ctx.stdout, { runs: items, total: items.length });
  else {
    if (items.length === 0) writeLine(ctx.stdout, ui.info("no runs"));
    printTable(
      ctx.stdout,
      [
        { key: "run_id", label: "RUN" },
        { key: "workspace_id", label: "WORKSPACE" },
        { key: "state", label: "STATE" },
      ],
      items,
    );
  }
  return 0;
}

async function commandDoctor(ctx, workspaces) {
  const checks = [];

  try {
    const healthz = await request(ctx, "/healthz", { method: "GET" });
    checks.push({ name: "healthz", ok: healthz.status === "ok", detail: healthz });
  } catch (err) {
    checks.push({ name: "healthz", ok: false, detail: String(err) });
  }

  try {
    const readyz = await request(ctx, "/readyz", { method: "GET" });
    checks.push({ name: "readyz", ok: readyz.ready === true, detail: readyz });
  } catch (err) {
    checks.push({ name: "readyz", ok: false, detail: String(err) });
  }

  checks.push({ name: "credentials", ok: Boolean(ctx.token || ctx.apiKey), detail: ctx.token ? "token" : ctx.apiKey ? "api_key" : "missing" });
  checks.push({ name: "workspace", ok: Boolean(workspaces.current_workspace_id), detail: workspaces.current_workspace_id || "missing" });

  const ok = checks.every((c) => c.ok);
  const report = { ok, api_url: ctx.apiUrl, checks };

  if (ctx.jsonMode) {
    printJson(ctx.stdout, report);
  } else {
    writeLine(ctx.stdout, ui.head("doctor"));
    for (const c of checks) writeLine(ctx.stdout, `${c.ok ? ui.ok(c.name) : ui.err(c.name)}`);
  }
  return ok ? 0 : 1;
}

async function commandSkillSecret(ctx, args, workspaces) {
  const action = args[0];
  const parsed = parseFlags(args.slice(1));
  const workspaceId = parsed.options["workspace-id"] || workspaces.current_workspace_id;
  const skillRef = parsed.options["skill-ref"];
  const key = parsed.options.key;

  if (!workspaceId || !skillRef || !key) {
    writeLine(ctx.stderr, ui.err("skill-secret requires --workspace-id --skill-ref --key"));
    return 2;
  }

  const route = `/v1/workspaces/${encodeURIComponent(workspaceId)}/skills/${encodeURIComponent(skillRef)}/secrets/${encodeURIComponent(key)}`;

  if (action === "put") {
    if (!parsed.options.value) {
      writeLine(ctx.stderr, ui.err("skill-secret put requires --value"));
      return 2;
    }
    const body = {
      value: String(parsed.options.value),
      scope: parsed.options.scope || "sandbox",
      meta: {},
    };
    const res = await request(ctx, route, {
      method: "PUT",
      headers: apiHeaders(ctx),
      body: JSON.stringify(body),
    });
    if (ctx.jsonMode) printJson(ctx.stdout, res);
    else writeLine(ctx.stdout, ui.ok("skill secret stored"));
    return 0;
  }

  if (action === "delete") {
    const res = await request(ctx, route, {
      method: "DELETE",
      headers: apiHeaders(ctx),
    });
    if (ctx.jsonMode) printJson(ctx.stdout, res);
    else writeLine(ctx.stdout, ui.ok("skill secret deleted"));
    return 0;
  }

  writeLine(ctx.stderr, ui.err("usage: skill-secret put|delete ..."));
  return 2;
}

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
    printHelp(stdout);
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
  const analyticsClient = await createCliAnalytics(env);
  const distinctId = extractDistinctIdFromToken(ctx.token);

  const command = rest[0];
  const args = rest.slice(1);
  const route = findRoute(command, args);
  const handlers = registerProgramCommands({
    login: (routeArgs) => commandLogin(ctx, routeArgs),
    logout: () => commandLogout(ctx),
    workspace: (routeArgs) => commandWorkspace(ctx, routeArgs, workspaces),
    specsSync: (routeArgs) => commandSpecsSync(ctx, routeArgs.slice(1), workspaces),
    run: (routeArgs) => commandRun(ctx, routeArgs, workspaces),
    runsList: (routeArgs) => commandRunsList(ctx, routeArgs.slice(1), workspaces),
    doctor: () => commandDoctor(ctx, workspaces),
    harness: (routeArgs) => commandHarnessModule(ctx, routeArgs, workspaces, {
      parseFlags,
      request,
      apiHeaders,
      ui,
      printJson,
      writeLine,
    }),
    skillSecret: (routeArgs) => commandSkillSecret(ctx, routeArgs, workspaces),
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
      printApiError(stderr, err, global.json);
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
