import { Writable } from "node:stream";
import { ApiError } from "../src/lib/http.js";

import {
  commandInstall,
  commandUpdate,
  commandStatus,
  commandStop,
  commandResume,
  commandKill,
  commandDelete as commandZombieDeleteLeaf,
} from "../src/commands/zombie.js";
import { commandList as commandZombieList } from "../src/commands/zombie_list.js";
import { commandLogs as commandZombieLogs } from "../src/commands/zombie_logs.js";
import { commandEvents as commandZombieEvents } from "../src/commands/zombie_events.js";
import { commandSteer as commandZombieSteer } from "../src/commands/zombie_steer.js";
import {
  commandCredentialAdd,
  commandCredentialShow,
  commandCredentialList,
  commandCredentialDelete,
} from "../src/commands/zombie_credential.js";
import { commandLogin, commandLogout } from "../src/commands/core.js";
import { commandDoctor } from "../src/commands/core-ops.js";
import {
  workspaceAdd,
  workspaceList,
  workspaceUse,
  workspaceShow,
  workspaceCredentials,
  workspaceDelete,
} from "../src/commands/workspace.js";
import { commandBillingShow } from "../src/commands/billing.js";
import {
  commandTenantProviderShow,
  commandTenantProviderAdd,
  commandTenantProviderDelete,
} from "../src/commands/tenant.js";

export { ApiError };

/** Discard-all writable stream (use one per test to avoid state leaks). */
export function makeNoop() {
  return new Writable({ write(_c, _e, cb) { cb(); } });
}

/** Writable that buffers output; call .read() to inspect. */
export function makeBufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

/** Passthrough UI theme (no ANSI escapes). */
export const ui = { ok: (s) => s, err: (s) => s, info: (s) => s, dim: (s) => s, head: (s) => s };

// Test-only shim that re-creates the old createCoreHandlers return shape
// from the new top-level exports — direct-handler tests keep their
// `handlers.commandLogin(args)` / `handlers.commandWorkspace(args)`
// invocation pattern without rewriting every call site.
export function createCoreHandlers(ctx, workspaces, deps) {
  return {
    commandLogin:  (args = []) => commandLogin(ctx,  buildParsed(args), workspaces, deps),
    commandLogout: (args = []) => commandLogout(ctx, buildParsed(args), workspaces, deps),
    commandDoctor: (args = []) => commandDoctor(ctx, buildParsed(args), workspaces, deps),
    commandWorkspace: (args = []) => commandWorkspaceDispatch(ctx, args, workspaces, deps),
  };
}

function commandWorkspaceDispatch(ctx, args, workspaces, deps) {
  const action = args[0];
  const rest = args.slice(1);
  switch (action) {
    case "add":         return workspaceAdd(ctx, buildParsed(rest), workspaces, deps);
    case "list":        return workspaceList(ctx, buildParsed(rest), workspaces, deps);
    case "use":         return workspaceUse(ctx, buildParsed(rest), workspaces, deps);
    case "show":        return workspaceShow(ctx, buildParsed(rest), workspaces, deps);
    case "credentials": return workspaceCredentials(ctx, buildParsed(rest), workspaces, deps);
    case "delete":      return workspaceDelete(ctx, buildParsed(rest), workspaces, deps);
    default:
      if (deps && deps.writeError) {
        deps.writeError(ctx, "UNKNOWN_COMMAND", "usage: workspace add|list|use|show|credentials|delete", deps);
      }
      return Promise.resolve(2);
  }
}

// Test-only shim mirroring the old `commandTenant(ctx, args, _ws, deps)`
// dispatcher — routes `provider {show|add|delete}` to the new leaf exports.
export function commandTenant(ctx, args, workspaces, deps) {
  const subgroup = args[0];
  const action = args[1];
  const rest = args.slice(2);
  if (subgroup === "provider") {
    // Honor deps.parseFlags / deps.parseFlagsImpl when injected — legacy
    // tenant provider tests stub it to control parsed.options without
    // routing through the dashed-CLI form.
    const parse = (deps && (deps.parseFlags || deps.parseFlagsImpl)) || buildParsed;
    if (action === "show")   return commandTenantProviderShow(ctx,   parse(rest), workspaces, deps);
    if (action === "add")    return commandTenantProviderAdd(ctx,    parse(rest), workspaces, deps);
    if (action === "delete") return commandTenantProviderDelete(ctx, parse(rest), workspaces, deps);
    const message = `unknown tenant provider action: ${action ?? "(none)"}`;
    if (ctx?.jsonMode) {
      const printJson = (deps && deps.printJson) || ((s, v) => s.write(JSON.stringify(v)));
      printJson(ctx.stderr, { error: { code: "UNKNOWN_COMMAND", message } });
    } else if (ctx && ctx.stderr) {
      const writeLine = (deps && deps.writeLine) || ((s, line = "") => s.write(`${line}\n`));
      const err = (deps && deps.ui && deps.ui.err) || ((s) => s);
      writeLine(ctx.stderr, err("usage: zombiectl tenant provider show"));
      writeLine(ctx.stderr, err("       zombiectl tenant provider add --credential <name> [--model <override>]"));
      writeLine(ctx.stderr, err("       zombiectl tenant provider delete"));
    }
    return Promise.resolve(2);
  }
  const message = `unknown tenant subgroup: ${subgroup ?? "(none)"}`;
  if (ctx?.jsonMode) {
    const printJson = (deps && deps.printJson) || ((s, v) => s.write(JSON.stringify(v)));
    printJson(ctx.stderr, { error: { code: "UNKNOWN_COMMAND", message } });
  } else if (ctx && ctx.stderr) {
    const writeLine = (deps && deps.writeLine) || ((s, line = "") => s.write(`${line}\n`));
    const err = (deps && deps.ui && deps.ui.err) || ((s) => s);
    writeLine(ctx.stderr, err("usage: zombiectl tenant provider {show|add|delete}"));
  }
  return Promise.resolve(2);
}

// Test-only shim mirroring the old `commandBilling(ctx, args, _ws, deps)`
// dispatcher — routes the `show` verb to the new commandBillingShow leaf.
export function commandBilling(ctx, args, workspaces, deps) {
  const action = args[0];
  const rest = args.slice(1);
  if (action === "show") return commandBillingShow(ctx, buildParsed(rest), workspaces, deps);
  const message = `unknown billing action: ${action ?? "(none)"}`;
  if (ctx?.jsonMode) {
    const printJson = (deps && deps.printJson) || ((stream, value) => stream.write(JSON.stringify(value)));
    printJson(ctx.stderr, { error: { code: "UNKNOWN_COMMAND", message } });
  } else if (ctx && ctx.stderr) {
    const writeLine = (deps && deps.writeLine) || ((stream, line = "") => stream.write(`${line}\n`));
    const err = (deps && deps.ui && deps.ui.err) || ((s) => s);
    writeLine(ctx.stderr, err("usage: zombiectl billing show [--limit <n>] [--cursor <token>] [--json]"));
  }
  return Promise.resolve(2);
}

// Test-only dispatcher that re-creates the old `commandZombie(ctx, args, ws, deps)`
// surface from the new leaf exports. Production routes through commander
// (cli-tree.js); this shim keeps the direct-handler tests focused on leaf
// behavior without rewriting every call site to a different function per action.
export function commandZombieDispatch(ctx, args, workspaces, deps) {
  const action = args[0];
  const rest = args.slice(1);
  switch (action) {
    case "install": return commandInstall(ctx, buildParsed(rest), workspaces, deps);
    case "update":  return commandUpdate(ctx, buildParsed(rest), workspaces, deps);
    case "status":  return commandStatus(ctx, buildParsed(rest), workspaces, deps);
    case "kill":    return commandKill(ctx, buildParsed(rest), workspaces, deps);
    case "stop":    return commandStop(ctx, buildParsed(rest), workspaces, deps);
    case "resume":  return commandResume(ctx, buildParsed(rest), workspaces, deps);
    case "delete":  return commandZombieDeleteLeaf(ctx, buildParsed(rest), workspaces, deps);
    case "list":    return commandZombieList(ctx, buildParsed(rest), workspaces, deps);
    case "logs":    return commandZombieLogs(ctx, buildParsed(rest), workspaces, deps);
    case "events":  return commandZombieEvents(ctx, buildParsed(rest), workspaces, deps);
    case "steer":   return commandZombieSteer(ctx, buildParsed(rest), workspaces, deps);
    case "credential": {
      const sub = rest[0];
      const subRest = rest.slice(1);
      if (sub === "add")    return commandCredentialAdd(ctx, buildParsed(subRest), workspaces, deps);
      if (sub === "show")   return commandCredentialShow(ctx, buildParsed(subRest), workspaces, deps);
      if (sub === "list")   return commandCredentialList(ctx, buildParsed(subRest), workspaces, deps);
      if (sub === "delete") return commandCredentialDelete(ctx, buildParsed(subRest), workspaces, deps);
      break;
    }
  }
  if (deps && deps.writeError) {
    deps.writeError(ctx, "UNKNOWN_COMMAND", `unknown zombie subcommand: ${action ?? "(none)"}`, deps);
  }
  return Promise.resolve(2);
}

// Build the parsed = { options, positionals } shape that leaf handlers
// expect from a flat token array. Test-only utility — production now
// flows through commander (cli-tree.js). Matches the legacy parseFlags
// surface byte-for-byte so direct handler tests can keep synthesising
// parsed objects from `["--limit", "20", "<positional>"]` token lists.
export function buildParsed(tokens = []) {
  const options = {};
  const positionals = [];
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (!token.startsWith("--")) { positionals.push(token); continue; }
    const eq = token.indexOf("=");
    if (eq !== -1) {
      options[token.slice(2, eq)] = token.slice(eq + 1);
      continue;
    }
    const key = token.slice(2);
    const next = tokens[i + 1];
    if (next && !next.startsWith("--")) {
      options[key] = next;
      i += 1;
    } else {
      options[key] = true;
    }
  }
  return { options, positionals };
}

// ── Stable test constants ─────────────────────────────────────────────────────
export const AGENT_ID   = "0195b4ba-8d3a-7f13-8abc-000000000001";
export const AGENT_NAME = "my-agent";
export const WS_ID      = "0195b4ba-8d3a-7f13-8abc-000000000010";
export const SCORE_ID_1 = "0195b4ba-8d3a-7f13-8abc-000000000021";
export const SCORE_ID_2 = "0195b4ba-8d3a-7f13-8abc-000000000022";
export const RUN_ID_1   = "0195b4ba-8d3a-7f13-8abc-000000000031";
export const RUN_ID_2   = "0195b4ba-8d3a-7f13-8abc-000000000032";
export const PVER_ID    = "0195b4ba-8d3a-7f13-8abc-000000000041";
