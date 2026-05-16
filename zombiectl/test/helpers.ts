import { Writable } from "node:stream";
import { ApiError, type FetchImpl } from "../src/lib/http.ts";

import {
  commandStatus,
  commandStop,
  commandResume,
  commandKill,
  commandDelete as commandZombieDeleteLeaf,
} from "../src/commands/zombie.ts";
import { commandInstall, commandUpdate } from "../src/commands/zombie_install.ts";
import { commandList as commandZombieList } from "../src/commands/zombie_list.ts";
import { commandLogs as commandZombieLogs } from "../src/commands/zombie_logs.ts";
import { commandEvents as commandZombieEvents } from "../src/commands/zombie_events.ts";
import { commandSteer as commandZombieSteer } from "../src/commands/zombie_steer.ts";
import {
  commandCredentialAdd,
  commandCredentialShow,
  commandCredentialList,
  commandCredentialDelete,
} from "../src/commands/zombie_credential.ts";
import { commandLogin, commandLogout } from "../src/commands/core.ts";
import { commandDoctor } from "../src/commands/core-ops.ts";
import {
  workspaceAdd,
  workspaceList,
  workspaceUse,
  workspaceShow,
  workspaceCredentials,
  workspaceDelete,
} from "../src/commands/workspace.ts";
import { commandBillingShow } from "../src/commands/billing.ts";
import {
  commandTenantProviderShow,
  commandTenantProviderAdd,
  commandTenantProviderDelete,
} from "../src/commands/tenant.ts";

import type {
  CommandCtx,
  CommandDeps,
  ParsedArgs,
  Workspaces,
} from "../src/commands/types.ts";

export { ApiError };
export type { FetchImpl };

// Structural Response mocks for tests that hit apiRequest / streamFetch
// only need ok/status/statusText/headers.get/text (+ optional body for SSE).
// Double-cast widens to FetchImpl at the test→prod boundary so production
// code paths still face full strict-mode pressure.
export interface ResponseLike {
  ok: boolean;
  status: number;
  statusText: string;
  headers: { get: (name: string) => string | null };
  text: () => Promise<string>;
  body?: unknown;
}

export const asFetchImpl = (
  impl: (url: string, init?: RequestInit) => Promise<ResponseLike>,
): FetchImpl => impl as unknown as FetchImpl;

// runCli's RunCliIo.fetchImpl expects the full `typeof fetch` shape
// (including `preconnect`). Structural test mocks only implement what
// production reads — widen at the boundary so internal code paths
// still face full strict-mode pressure.
export const asFetchOverride = (
  impl: (url: string, init?: RequestInit) => Promise<ResponseLike>,
): typeof fetch => impl as unknown as typeof fetch;

// `Map<string, string>.get` returns `string | undefined`, but ResponseLike's
// `headers.get` is `string | null`. Wrap a Map so the missing-key shape lines
// up with the production Headers contract.
export function makeHeaders(
  entries: ReadonlyArray<readonly [string, string]>,
): { get: (name: string) => string | null } {
  const map = new Map(entries);
  return { get: (name) => map.get(name) ?? null };
}

export interface UiTheme {
  ok: (s: string) => string;
  err: (s: string) => string;
  info: (s: string) => string;
  dim: (s: string) => string;
  head: (s: string) => string;
}

// Tests mutate `stream.isTTY = true` to flip color/spinner code paths
// (capability.ts reads it for !isTTY → NONE). The Node `Writable` class
// has no `isTTY` field; the intersection makes the test-set safe under
// strict types without an `as` cast at every assignment site.
export type TestStream = Writable & { isTTY?: boolean };

/** Discard-all writable stream (use one per test to avoid state leaks). */
export function makeNoop(): TestStream {
  return new Writable({ write(_c, _e, cb) { cb(); } });
}

/** Writable that buffers output; call .read() to inspect. */
export function makeBufferStream(): { stream: TestStream; read: () => string } {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

/** Passthrough UI theme (no ANSI escapes). */
export const ui: UiTheme = {
  ok: (s) => s,
  err: (s) => s,
  info: (s) => s,
  dim: (s) => s,
  head: (s) => s,
};

// Direct-handler tests pass partial deps shaped against the production
// CommandDeps contract. The mocks omit fields the handler under test
// doesn't read; the boundary cast widens partial → CommandDeps at the
// call site, preserving strict-mode pressure on the production code path.
type TestDeps = Partial<CommandDeps> & {
  parseFlags?: (tokens: readonly string[]) => ParsedArgs;
  parseFlagsImpl?: (tokens: readonly string[]) => ParsedArgs;
};

type CoreHandler = (args?: readonly string[]) => Promise<number>;

// Named-member return shape (vs `Record<string, CoreHandler>`) so call
// sites like `core.commandLogin()` typecheck under
// `noUncheckedIndexedAccess` without a `!` non-null assertion.
export interface CoreHandlers {
  commandLogin: CoreHandler;
  commandLogout: CoreHandler;
  commandDoctor: CoreHandler;
  commandWorkspace: CoreHandler;
}

// Test-only shim that re-creates the old createCoreHandlers return shape
// from the new top-level exports — direct-handler tests keep their
// `handlers.commandLogin(args)` / `handlers.commandWorkspace(args)`
// invocation pattern without rewriting every call site.
export function createCoreHandlers(
  ctx: CommandCtx,
  workspaces: Workspaces,
  deps: CommandDeps,
): CoreHandlers {
  return {
    commandLogin:  (args = []) => commandLogin(ctx,  buildParsed(args), workspaces, deps),
    commandLogout: (args = []) => commandLogout(ctx, buildParsed(args), workspaces, deps),
    commandDoctor: (args = []) => commandDoctor(ctx, buildParsed(args), workspaces, deps),
    commandWorkspace: (args = []) => commandWorkspaceDispatch(ctx, args, workspaces, deps),
  };
}

function commandWorkspaceDispatch(
  ctx: CommandCtx,
  args: readonly string[],
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const action = args[0];
  const rest = args.slice(1);
  switch (action) {
    case "add":         return Promise.resolve(workspaceAdd(ctx, buildParsed(rest), workspaces, deps));
    case "list":        return Promise.resolve(workspaceList(ctx, buildParsed(rest), workspaces, deps));
    case "use":         return Promise.resolve(workspaceUse(ctx, buildParsed(rest), workspaces, deps));
    case "show":        return Promise.resolve(workspaceShow(ctx, buildParsed(rest), workspaces, deps));
    case "credentials": return Promise.resolve(workspaceCredentials(ctx, buildParsed(rest), workspaces, deps));
    case "delete":      return Promise.resolve(workspaceDelete(ctx, buildParsed(rest), workspaces, deps));
    default:
      if (deps?.writeError) {
        deps.writeError(ctx, "UNKNOWN_COMMAND", "usage: workspace add|list|use|show|credentials|delete", deps);
      }
      return Promise.resolve(2);
  }
}

// Test-only shim mirroring the old `commandTenant(ctx, args, _ws, deps)`
// dispatcher — routes `provider {show|add|delete}` to the new leaf exports.
export function commandTenant(
  ctx: CommandCtx,
  args: readonly string[],
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const subgroup = args[0];
  const action = args[1];
  const rest = args.slice(2);
  const testDeps = deps as TestDeps;
  if (subgroup === "provider") {
    // Honor deps.parseFlags / deps.parseFlagsImpl when injected — legacy
    // tenant provider tests stub it to control parsed.options without
    // routing through the dashed-CLI form.
    const parse = testDeps.parseFlags ?? testDeps.parseFlagsImpl ?? buildParsed;
    if (action === "show")   return Promise.resolve(commandTenantProviderShow(ctx,   parse(rest), workspaces, deps));
    if (action === "add")    return Promise.resolve(commandTenantProviderAdd(ctx,    parse(rest), workspaces, deps));
    if (action === "delete") return Promise.resolve(commandTenantProviderDelete(ctx, parse(rest), workspaces, deps));
    emitUsage(ctx, deps, "UNKNOWN_COMMAND", `unknown tenant provider action: ${action ?? "(none)"}`, [
      "usage: zombiectl tenant provider show",
      "       zombiectl tenant provider add --credential <name> [--model <override>]",
      "       zombiectl tenant provider delete",
    ]);
    return Promise.resolve(2);
  }
  emitUsage(ctx, deps, "UNKNOWN_COMMAND", `unknown tenant subgroup: ${subgroup ?? "(none)"}`, [
    "usage: zombiectl tenant provider {show|add|delete}",
  ]);
  return Promise.resolve(2);
}

// Test-only shim mirroring the old `commandBilling(ctx, args, _ws, deps)`
// dispatcher — routes the `show` verb to the new commandBillingShow leaf.
export function commandBilling(
  ctx: CommandCtx,
  args: readonly string[],
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const action = args[0];
  const rest = args.slice(1);
  if (action === "show") return Promise.resolve(commandBillingShow(ctx, buildParsed(rest), workspaces, deps));
  emitUsage(ctx, deps, "UNKNOWN_COMMAND", `unknown billing action: ${action ?? "(none)"}`, [
    "usage: zombiectl billing show [--limit <n>] [--cursor <token>] [--json]",
  ]);
  return Promise.resolve(2);
}

// Test-only dispatcher that re-creates the old `commandZombie(ctx, args, ws, deps)`
// surface from the new leaf exports. Production routes through commander
// (cli-tree.ts); this shim keeps the direct-handler tests focused on leaf
// behavior without rewriting every call site to a different function per action.
export function commandZombieDispatch(
  ctx: CommandCtx,
  args: readonly string[],
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number> {
  const action = args[0];
  const rest = args.slice(1);
  switch (action) {
    case "install": return commandInstall(ctx, buildParsed(rest), workspaces, deps);
    case "update":  return commandUpdate(ctx, buildParsed(rest), workspaces, deps);
    case "status":  return Promise.resolve(commandStatus(ctx, buildParsed(rest), workspaces, deps));
    case "kill":    return Promise.resolve(commandKill(ctx, buildParsed(rest), workspaces, deps));
    case "stop":    return Promise.resolve(commandStop(ctx, buildParsed(rest), workspaces, deps));
    case "resume":  return Promise.resolve(commandResume(ctx, buildParsed(rest), workspaces, deps));
    case "delete":  return Promise.resolve(commandZombieDeleteLeaf(ctx, buildParsed(rest), workspaces, deps));
    case "list":    return Promise.resolve(commandZombieList(ctx, buildParsed(rest), workspaces, deps));
    case "logs":    return Promise.resolve(commandZombieLogs(ctx, buildParsed(rest), workspaces, deps));
    case "events":  return commandZombieEvents(ctx, buildParsed(rest), workspaces, deps);
    case "steer":   return commandZombieSteer(ctx, buildParsed(rest), workspaces, deps);
    case "credential": {
      const sub = rest[0];
      const subRest = rest.slice(1);
      if (sub === "add")    return commandCredentialAdd(ctx, buildParsed(subRest), workspaces, deps);
      if (sub === "show")   return Promise.resolve(commandCredentialShow(ctx, buildParsed(subRest), workspaces, deps));
      if (sub === "list")   return Promise.resolve(commandCredentialList(ctx, buildParsed(subRest), workspaces, deps));
      if (sub === "delete") return Promise.resolve(commandCredentialDelete(ctx, buildParsed(subRest), workspaces, deps));
      break;
    }
  }
  if (deps?.writeError) {
    deps.writeError(ctx, "UNKNOWN_COMMAND", `unknown zombie subcommand: ${action ?? "(none)"}`, deps);
  }
  return Promise.resolve(2);
}

// Build the parsed = { options, positionals } shape that leaf handlers
// expect from a flat token array. Test-only utility — production now
// flows through commander (cli-tree.ts). Matches the legacy parseFlags
// surface byte-for-byte so direct handler tests can keep synthesising
// parsed objects from `["--limit", "20", "<positional>"]` token lists.
export function buildParsed(tokens: readonly string[] = []): ParsedArgs {
  const options: ParsedArgs["options"] = {};
  const positionals: string[] = [];
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token === undefined) continue;
    if (!token.startsWith("--")) { positionals.push(token); continue; }
    const eq = token.indexOf("=");
    if (eq !== -1) {
      options[token.slice(2, eq)] = token.slice(eq + 1);
      continue;
    }
    const key = token.slice(2);
    const next = tokens[i + 1];
    if (next !== undefined && !next.startsWith("--")) {
      options[key] = next;
      i += 1;
    } else {
      options[key] = true;
    }
  }
  return { options, positionals };
}

// Shared usage-message emitter — five call sites in commandTenant + commandBilling
// were each carrying the same json-mode-vs-tty branching. Hoisting drops ~30
// lines and unifies the format. Honors deps.printJson / deps.writeLine /
// deps.ui.err when injected; falls back to direct stream writes when the
// test passes a barebones deps bag.
function emitUsage(
  ctx: CommandCtx,
  deps: CommandDeps,
  code: string,
  message: string,
  usageLines: readonly string[],
): void {
  const testDeps = deps as TestDeps;
  if (ctx.jsonMode) {
    const printJson = testDeps.printJson ?? ((s, v) => { s.write(JSON.stringify(v)); });
    if (ctx.stderr) printJson(ctx.stderr, { error: { code, message } });
    return;
  }
  if (!ctx.stderr) return;
  const writeLine = testDeps.writeLine ?? ((s, line = "") => { s.write(`${line}\n`); });
  const err = testDeps.ui?.err ?? ((s: string) => s);
  for (const line of usageLines) writeLine(ctx.stderr, err(line));
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
