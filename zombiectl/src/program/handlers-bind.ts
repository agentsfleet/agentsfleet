// Wires the imported leaf handlers into the shape cli-tree.ts expects.
// Every command routes through the Effect dispatcher (runEffect) and
// consumes services declared on its R channel.

import { Option, Redacted, type Effect } from "effect";
import { runEffect, type MainLayerServices } from "../lib/run-effect.ts";
import type { FetchImpl } from "../lib/http.ts";
import { mainLayerFor } from "../runtime/main-layer.ts";
import { ZOMBIE_TOKEN_ENV } from "../services/config.ts";
import { withCommandInstrumentation } from "../services/telemetry/command-instrumentation.ts";

import { authStatusEffect, logoutEffect } from "../commands/auth.ts";
import { loginEffectFromFlags } from "../commands/login.ts";
import type { CliError } from "../errors/index.ts";
import { doctorEffect } from "../commands/core-ops.ts";
import {
  agentAddEffectFromArgs,
  agentListEffectFromArgs,
  agentDeleteEffectFromArgs,
} from "../commands/agent.ts";
import {
  grantListEffectFromArgs,
  grantDeleteEffectFromArgs,
} from "../commands/grant.ts";
import {
  tenantProviderShowEffect,
  tenantProviderAddEffectFromArgs,
  tenantProviderDeleteEffect,
} from "../commands/tenant.ts";
import { billingShowEffectFromArgs } from "../commands/billing.ts";
import { buildZombieHandlers } from "./handlers-bind-zombie.ts";
import { buildWorkspaceHandlers } from "./handlers-bind-workspace.ts";

import type { ActionFrame, CommandHandlerFn, Handlers } from "./cli-tree-types.ts";
import { readStringOpt as optString, type CommandCtx, type CommandDeps, type Workspaces } from "../commands/types.ts";

const CTX = "ctx" as const;
const TYPE_STRING = "string" as const;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

export interface Lifecycle {
  ctx: CommandCtx;
  workspaces: Workspaces;
  deps: CommandDeps;
  lastCommand: string | null;
}

type LifecycleCtx = Lifecycle[typeof CTX];

// Thread runCli's env-resolved values into Effect's CliConfig override.
// `ctx.token` is already a `creds.token || env.ZOMBIE_TOKEN` merge from
// cli.ts; mirror it as the override's `accessToken` so commands' Effects
// receive the merged value.
function configOverrideFromCtx(ctx: LifecycleCtx): {
  jsonMode: boolean;
  noOpen: boolean;
  apiUrl: string;
  accessToken: Option.Option<Redacted.Redacted<string>>;
  fetchImpl?: FetchImpl;
} {
  return {
    jsonMode: Boolean(ctx.jsonMode),
    noOpen: Boolean(ctx.noOpen),
    apiUrl: ctx.apiUrl,
    accessToken:
      isString(ctx.token) && ctx.token.length > 0
        ? Option.some(Redacted.make(ctx.token))
        : Option.none(),
    ...(ctx.fetchImpl !== undefined
      ? { fetchImpl: ctx.fetchImpl as FetchImpl }
      : {}),
  };
}

function streamsFromCtx(
  ctx: LifecycleCtx,
): { stdout: NodeJS.WritableStream; stderr: NodeJS.WritableStream } | undefined {
  if (!ctx.stdout || !ctx.stderr) return undefined;
  if (ctx.stdout === process.stdout && ctx.stderr === process.stderr) return undefined;
  return { stdout: ctx.stdout, stderr: ctx.stderr };
}

// Only thread a non-default stdin stream (a string/null ctx.stdin is a test
// convenience the Stdin layer doesn't model). process.stdin is the layer's
// own default, so passing it through would be a no-op.
function stdinFromCtx(ctx: LifecycleCtx): NodeJS.ReadableStream | undefined {
  const s = ctx.stdin;
  if (!s || isString(s)) return undefined;
  if (s === process.stdin) return undefined;
  return s;
}

// Compose the per-invocation MainLayer at the handler-bind site
// (mirrors Supabase's shared/cli/run.ts::cliProgramFor — compose at one
// site, Effect.provide at the dispatcher boundary). Reads ctx AFTER
// commander's preAction has fired, so --no-open / --json / --api
// global flags are captured.
//
// `name` is the wrap-site label ("agent.add", "workspace.list", ...).
// Split into commandPath so CommandRuntime carries it (the span name
// becomes `cli.agent.add`, the analytics command label becomes
// "agent add").
function mainLayerForCtx(lifecycle: Lifecycle, name: string): ReturnType<typeof mainLayerFor> {
  const streams = streamsFromCtx(lifecycle.ctx);
  const stdin = stdinFromCtx(lifecycle.ctx);
  return mainLayerFor({
    config: configOverrideFromCtx(lifecycle.ctx),
    commandPath: name.split("."),
    ...(streams !== undefined ? { streams } : {}),
    ...(stdin !== undefined ? { stdin } : {}),
  });
}

// withCommandInstrumentation is applied HERE — single seam. Every
// command Effect picks up the supabase-pattern tracing span + the
// cli_command_executed analytics emit transparently. The 30+ files
// under src/commands/*.ts are NOT edited.
function wrapEffect<A, E extends CliError, R extends MainLayerServices>(
  name: string,
  effect: Effect.Effect<A, E, R>,
  lifecycle: Lifecycle,
): CommandHandlerFn {
  return async (_frame: ActionFrame): Promise<number> => {
    const exitCode = await runEffect({
      name,
      effect: effect.pipe(withCommandInstrumentation()),
      layer: mainLayerForCtx(lifecycle, name),
    });
    lifecycle.lastCommand = name;
    return exitCode;
  };
}

// Variant for command Effects whose flags come from the parsed frame
// (login's --no-open / --token, billing's cursor). The factory receives
// the frame and returns the Effect; everything else is the same as
// wrapEffect.
function wrapEffectFn<A, E extends CliError, R extends MainLayerServices>(
  name: string,
  factory: (frame: ActionFrame) => Effect.Effect<A, E, R>,
  lifecycle: Lifecycle,
): CommandHandlerFn {
  return async (frame: ActionFrame): Promise<number> => {
    const exitCode = await runEffect({
      name,
      effect: factory(frame).pipe(withCommandInstrumentation()),
      layer: mainLayerForCtx(lifecycle, name),
    });
    lifecycle.lastCommand = name;
    return exitCode;
  };
}

export function buildHandlers(lifecycle: Lifecycle): Handlers {
  const wrapE = <A, E extends CliError, R extends MainLayerServices>(
    name: string,
    effect: Effect.Effect<A, E, R>,
  ): CommandHandlerFn => wrapEffect(name, effect, lifecycle);
  return {
    login: wrapEffectFn(
      "login",
      (frame) => {
        const opts = frame.parsed.options;
        const tokenNameOpt = opts["tokenName"] ?? opts["token-name"];
        const tokenOpt = opts["token"];
        // Raw env value (not the file-merged ctx.token) — an existing
        // credentials.json must not be re-read as a "direct token".
        const envToken = lifecycle.ctx.env?.[ZOMBIE_TOKEN_ENV];
        return loginEffectFromFlags({
          noOpen: opts["open"] === false || opts["noOpen"] === true || opts["no-open"] === true,
          noInput: opts["input"] === false || opts["noInput"] === true || opts["no-input"] === true,
          force: opts["force"] === true,
          tokenName: isString(tokenNameOpt) ? tokenNameOpt : undefined,
          tokenFlag: isString(tokenOpt) ? tokenOpt : undefined,
          envToken: isString(envToken) ? envToken : undefined,
        });
      },
      lifecycle,
    ),
    logout: wrapEffectFn(
      "logout",
      (frame) =>
        logoutEffect({
          all: frame.parsed.options["all"] === true,
        }),
      lifecycle,
    ),
    auth: {
      status: wrapE("auth.status", authStatusEffect),
    },
    doctor: wrapE("doctor", doctorEffect),
    workspace: buildWorkspaceHandlers(
      wrapE,
      <E extends CliError, R extends MainLayerServices>(
        name: string,
        factory: (frame: ActionFrame) => Effect.Effect<void, E, R>,
      ) => wrapEffectFn(name, factory, lifecycle),
    ),
    agent: {
      add: wrapEffectFn(
        "agent.add",
        (frame) => {
          const opts = frame.parsed.options;
          return agentAddEffectFromArgs({
            workspaceId:
              optString(opts, "workspace") ??
              optString(opts, "workspaceId") ??
              optString(opts, "workspace-id"),
            zombieId:
              optString(opts, "zombie") ??
              optString(opts, "zombieId") ??
              optString(opts, "zombie-id"),
            name: optString(opts, "name"),
            description: optString(opts, "description"),
          });
        },
        lifecycle,
      ),
      list: wrapEffectFn(
        "agent.list",
        (frame) =>
          agentListEffectFromArgs(
            optString(frame.parsed.options, "workspace") ??
              optString(frame.parsed.options, "workspaceId") ??
              optString(frame.parsed.options, "workspace-id"),
          ),
        lifecycle,
      ),
      delete: wrapEffectFn(
        "agent.delete",
        (frame) =>
          agentDeleteEffectFromArgs(
            optString(frame.parsed.options, "workspace") ??
              optString(frame.parsed.options, "workspaceId") ??
              optString(frame.parsed.options, "workspace-id"),
            frame.parsed.positionals[0],
            optString(frame.parsed.options, "agent-id") ??
              optString(frame.parsed.options, "agentId"),
          ),
        lifecycle,
      ),
    },
    grant: {
      list: wrapEffectFn(
        "grant.list",
        (frame) =>
          grantListEffectFromArgs(
            frame.parsed.positionals[0],
            optString(frame.parsed.options, "zombie") ??
              optString(frame.parsed.options, "zombieId") ??
              optString(frame.parsed.options, "zombie-id"),
          ),
        lifecycle,
      ),
      delete: wrapEffectFn(
        "grant.delete",
        (frame) =>
          grantDeleteEffectFromArgs(
            optString(frame.parsed.options, "zombie") ??
              optString(frame.parsed.options, "zombieId") ??
              optString(frame.parsed.options, "zombie-id"),
            frame.parsed.positionals[0],
          ),
        lifecycle,
      ),
    },
    tenant: {
      provider: {
        show: wrapE("tenant.provider.show", tenantProviderShowEffect),
        add: wrapEffectFn(
          "tenant.provider.add",
          (frame) =>
            tenantProviderAddEffectFromArgs(
              optString(frame.parsed.options, "credential"),
              optString(frame.parsed.options, "model"),
            ),
          lifecycle,
        ),
        delete: wrapE("tenant.provider.delete", tenantProviderDeleteEffect),
      },
    },
    billing: {
      show: wrapEffectFn(
        "billing.show",
        (frame) =>
          billingShowEffectFromArgs({
            limit: optString(frame.parsed.options, "limit"),
            cursor: optString(frame.parsed.options, "cursor"),
          }),
        lifecycle,
      ),
    },
    zombie: buildZombieHandlers(
      wrapE,
      <E extends CliError, R extends MainLayerServices>(
        name: string,
        factory: (frame: ActionFrame) => Effect.Effect<void, E, R>,
      ) => wrapEffectFn(name, factory, lifecycle),
    ),
  };
}
