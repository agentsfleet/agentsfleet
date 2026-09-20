// The preamble every workspace-scoped command runs, as one `yield*`.
//
// Fifty-four handler sites opened with the same five lines — pull CliConfig,
// Output and HttpClient out of context, then require a workspace and resolve a
// token — and eighteen of them then wrote the same twenty-word Effect
// signature by hand. Neither is a decision any single command makes; both are
// what "a command that talks to the daemon on a workspace's behalf" means.
//
// The order matters and is the reason this is one Effect rather than five
// imports: the workspace and the token are both REQUIRED before a handler may
// touch the network, so a command that forgets one cannot issue a request that
// a command that remembered would have refused.

import { Effect, type Redacted } from "effect";

import { CliConfig, type CliConfigShape } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient, type HttpClientShape } from "../services/http-client.ts";
import { Output, type OutputShape } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import type { CliError } from "../errors/index.ts";
import { resolveAuthToken, resolveWorkspaceId, requireWorkspaceId } from "./workspace-guards.ts";

/**
 * What every command that reaches the daemon needs, minus the workspace.
 *
 * Tenant- and account-scoped commands — `billing`, `api-key`, `whoami` — stop
 * here, because requiring a workspace they never use would refuse them for a
 * reason that is not theirs.
 */
export interface CommandContext {
  readonly config: CliConfigShape;
  readonly output: OutputShape;
  readonly http: HttpClientShape;
  readonly token: Redacted.Redacted<string>;
}

/** The same, for a command that acts inside one workspace. */
export interface WorkspaceContext extends CommandContext {
  readonly workspaceId: string;
}

/**
 * The signature eighteen handlers spelled out in full.
 *
 * Every service this module can yield is named, so a handler widening its own
 * requirements does not have to re-derive the union — and a reader sees one
 * name instead of twenty words that were identical at every site.
 */
export type CommandEffect<A = void> = Effect.Effect<
  A,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
>;

/** Services and a token, with no workspace required. */
export const commandContext: Effect.Effect<
  CommandContext,
  CliError,
  CliConfig | Credentials | HttpClient | Output
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;
  const token = yield* resolveAuthToken;
  return { config, output, http, token };
});

/** Services, a token, and the selected workspace. */
export const workspaceContext: CommandEffect<WorkspaceContext> = Effect.gen(
  function* () {
    const base = yield* commandContext;
    const workspaceId = yield* requireWorkspaceId;
    return { ...base, workspaceId };
  },
);

/**
 * Services, a token, and the workspace the caller named — validated — or the
 * selected one.
 *
 * Separate from [`workspaceContext`] because a command without a `--workspace`
 * flag should not be able to accept one by accident.
 */
export const workspaceContextFor = (
  override: string | undefined,
): CommandEffect<WorkspaceContext> =>
  Effect.gen(function* () {
    const base = yield* commandContext;
    const workspaceId = yield* resolveWorkspaceId(override);
    return { ...base, workspaceId };
  });
