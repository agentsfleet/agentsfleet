// The entry point: argv in, exit code out.
//
// Everything here happens in a fixed order, and the order is the design. The
// command tree executes inside a layer, and that layer has to be built before
// the tree runs — so the handful of things the layer is built FROM are read
// off argv first, by hand, rather than being parsed:
//
//   1. the register (`--json`) and the target (`--api`), because Output and
//      HttpClient are configured with them
//   2. the saved credential, because the auth guard answers before any command
//      is allowed to run
//   3. the command path, walked off the tree, because CommandRuntime carries
//      it and it names the span and the analytics row
//
// Only then is the layer composed and the tree handed the same argv to parse
// properly. The scans are deliberately narrow — they answer three questions
// and leave every other token to the parser.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { Cause, Console, Effect, Exit, Layer } from "effect";
import { CliConfig, CliOutput, Command, GlobalFlag } from "effect/cli";
import { BunServices } from "@effect/platform-bun";

import { loadState } from "./lib/state-load.ts";
import { detectTokenInArgv } from "./lib/argv-redact.ts";
import { resolveApiKeyFromEnv } from "./services/config.ts";
import type { FetchImpl } from "./lib/http.ts";
import { printJson, writeError, writeLine } from "./program/io.ts";
import { guardCommand } from "./program/auth-guard.ts";
import { DEFAULT_API_URL, normalizeApiUrl, resolveDashboardUrl } from "./util/url.ts";
import { buildCommandTree } from "./program/tree/index.ts";
import { asCommandNode, resolveCommandPath, type CommandNode } from "./program/tree/resolve-path.ts";
import { mainLayerFor } from "./runtime/main-layer.ts";
import { makeCommandExitCode } from "./runtime/exit-code.service.ts";
import { CommandGuard, GuardRefused } from "./runtime/guard.service.ts";
import { withCommandInstrumentation } from "./services/telemetry/command-instrumentation.ts";
import { detectJsonMode, helpArgv, maybePrintVersion, resolveGlobalApiUrl, scanLogLevel } from "./program/entry/argv-scan.ts";
import { DISCOVERY_TIMEOUT_MS, discoverVisibility } from "./program/entry/discovery.ts";
import { completionsFlag } from "./program/tree/global-flags.ts";
import { UNKNOWN_VISIBILITY } from "./program/tree/visibility.ts";
import { consoleForStreams } from "./program/entry/console-bridge.ts";
import { housePresenter } from "./program/entry/presenter.ts";
import { BLOCK_LABEL, BLOCK_SEVERITY, writeErrorBlock } from "./output/error-block.ts";
import { helpFormatter } from "./program/entry/help-formatter.ts";
import {
  exitCodeForFailure,
  isGuardRefusal,
  isLibraryUsageError,
} from "./program/entry/exit-code.ts";
import { houseRejection } from "./program/entry/rejection.ts";
import { renderAndCount } from "./lib/run-effect.ts";
import { layerInputFor } from "./program/entry/layer-input.ts";
import type { WritableStreamLike } from "./output/capability.ts";
import { DASHBOARD_URL_ENV } from "./constants/env.ts";

// VERSION: package.json source of truth; `make sync-version` updates consumers.
const PKG_JSON_PATH = join(dirname(fileURLToPath(import.meta.url)), "..", "package.json");
const pkgJson = JSON.parse(readFileSync(PKG_JSON_PATH, "utf8")) as { version: string };
export const VERSION: string = pkgJson.version;

const GUARD_EXIT_CODE = 1;
const SUCCESS_EXIT_CODE = 0;

export interface RunCliIo {
  stdout?: WritableStreamLike;
  stderr?: WritableStreamLike;
  stdin?: NodeJS.ReadableStream;
  env?: NodeJS.ProcessEnv;
  fetchImpl?: typeof fetch;
  // Bounds the identity read behind scoped help; tests shorten it.
  discoveryTimeoutMs?: number;
}

// What the invocation resolved before anything parsed: its streams, register,
// saved state and target.
interface Invocation {
  readonly argv: readonly string[];
  readonly io: RunCliIo;
  readonly stdout: WritableStreamLike;
  readonly stderr: WritableStreamLike;
  readonly env: NodeJS.ProcessEnv;
  readonly jsonMode: boolean;
  readonly creds: Awaited<ReturnType<typeof loadState>>["creds"];
  readonly apiKey: string | null;
  readonly explicitApi: string | null;
  readonly apiUrl: string;
}

export async function runCli(
  argv: readonly string[],
  io: RunCliIo = {},
): Promise<number> {
  const stdout = (io.stdout ?? process.stdout) as WritableStreamLike;
  const stderr = (io.stderr ?? process.stderr) as WritableStreamLike;
  const env = io.env ?? process.env;
  const jsonMode = detectJsonMode(argv);

  // Surfaced before any command runs, and for any argv shape — `--version`
  // included, because a token passed on the command line is in the shell
  // history either way. The check reads argv and nothing else.
  const tokenLeak = detectTokenInArgv(argv);
  if (tokenLeak) writeLine(stderr, tokenLeak);

  if (maybePrintVersion(argv, stdout, VERSION, jsonMode, env)) return SUCCESS_EXIT_CODE;

  const invocation = await resolveInvocation({ argv, io, stdout, stderr, env, jsonMode });

  // Walked off the tree, not parsed: the layer carries the command path and
  // has to exist before the parser does. An invalid invocation yields the
  // deepest command that did resolve, which names the span and the block's
  // suggestion. The path is resolved on the fail-closed tree: visibility only
  // flips `unlisted`, never a name, so the path is the same in every tree.
  const unknownTree = buildCommandTree(UNKNOWN_VISIBILITY);
  const commandPath = resolveCommandPath(asCommandNode(unknownTree), argv);
  const layerInput = layerInputOf(invocation, commandPath);
  const rootCommand = await scopedTree(invocation, unknownTree, layerInput.config ?? {});

  // One holder per invocation: two runs in one process must not read each
  // other's verdict, and every test file is two runs in one process.
  const managedExitCode = makeCommandExitCode();
  const program = programFor(invocation, rootCommand, Layer.mergeAll(
    mainLayerFor(layerInput),
    managedExitCode.layer,
    guardLayerFor(invocation, commandPath),
  ));

  const exit = await Effect.runPromiseExit(program as Effect.Effect<number, never, never>);
  // The pipeline turns every outcome into a number, so a failure here is the
  // runtime itself falling over rather than a command failing.
  if (Exit.isFailure(exit)) return exitCodeForFailure(exit.cause);
  // A command that reported its own verdict outranks the dispatcher's 0: the
  // run succeeded, and the news it carries is still bad.
  return exit.value !== SUCCESS_EXIT_CODE ? exit.value : managedExitCode.read();
}

function layerInputOf(invocation: Invocation, commandPath: ReadonlyArray<string>) {
  return layerInputFor({
    apiUrl: invocation.apiUrl,
    dashboardUrl: resolveDashboardUrl(invocation.apiUrl, invocation.env[DASHBOARD_URL_ENV]),
    apiKey: invocation.apiKey,
    jsonMode: invocation.jsonMode,
    noOpen: false,
    commandPath,
    env: invocation.env,
    stdout: invocation.stdout,
    stderr: invocation.stderr,
    stdin: invocation.io.stdin,
    fetchImpl: invocation.io.fetchImpl as FetchImpl | undefined,
  });
}

async function resolveInvocation(
  base: Pick<Invocation, "argv" | "io" | "stdout" | "stderr" | "env" | "jsonMode">,
): Promise<Invocation> {
  // Real read failures (EACCES, EIO — not absence) are recorded by the loader
  // and reported below, once the endpoint they cost us is known.
  const { creds, unreadable } = await loadState(base.env);
  // Two credential slots: the stored login credential from disk, and the
  // service API key from the environment. The key wins at the wire.
  // `resolveApiKeyFromEnv` trims, so a whitespace-only export counts as absent
  // to the guard and the wire alike, rather than sending a blank Bearer.
  const apiKey = resolveApiKeyFromEnv(base.env);
  const explicitApi = resolveGlobalApiUrl(base.argv, base.env);
  const apiUrl = normalizeApiUrl(explicitApi || creds.api_url || DEFAULT_API_URL);
  if (unreadable.length > 0) reportUnreadableState(base.stderr, unreadable, apiUrl);
  return { ...base, creds, apiKey, explicitApi, apiUrl };
}

// The refusal is DECIDED here, where the credential and target were resolved,
// and ASKED by the tree after the parser has run — a mistyped flag on a
// command you are not signed in for reports the flag, because that is the
// failure you can fix without leaving the terminal.
function guardLayerFor(invocation: Invocation, commandPath: ReadonlyArray<string>) {
  return Layer.succeed(CommandGuard, {
    check: Effect.suspend(() => {
      const refusal = guardCommand(commandPath[0] ?? "", {
        token: invocation.creds.token ?? null,
        apiKey: invocation.apiKey,
        apiUrl: invocation.apiUrl,
        storedApiUrl: invocation.creds.api_url ?? null,
        targetIsExplicit: Boolean(invocation.explicitApi),
      });
      if (refusal === null) return Effect.void;
      writeError(
        { stderr: invocation.stderr as unknown as NodeJS.WritableStream, jsonMode: invocation.jsonMode },
        refusal.errorCode,
        refusal.message,
      );
      return Effect.fail(new GuardRefused(GUARD_EXIT_CODE));
    }),
  });
}

// Help, typos at the root or under `platform`, and `--completions` read the
// caller's scopes once; every other invocation keeps the fail-closed tree and
// makes no extra request. The tree returned here is the one value path-based
// rejection and the parser both read, so they cannot disagree.
async function scopedTree(
  invocation: Invocation,
  unknownTree: ReturnType<typeof buildCommandTree>,
  config: NonNullable<ReturnType<typeof layerInputFor>["config"]>,
): Promise<ReturnType<typeof buildCommandTree>> {
  const visibility = await Effect.runPromise(discoverVisibility({
    argv: invocation.argv,
    tree: asCommandNode(unknownTree),
    apiUrl: invocation.apiUrl,
    apiKey: invocation.apiKey,
    config,
    env: invocation.env,
    stdout: invocation.stdout,
    stderr: invocation.stderr,
    logLevel: scanLogLevel(invocation.argv),
    timeoutMs: invocation.io.discoveryTimeoutMs ?? DISCOVERY_TIMEOUT_MS,
  }));
  return visibility._tag === "Unknown" ? unknownTree : buildCommandTree(visibility);
}

// Bare `agentsfleet` asks for help explicitly, and `agentsfleet help
// [command]` is rewritten to `[command] --help`, so the two spellings cannot
// drift apart (see `helpArgv`).
//
// One span and one analytics row per invocation, wrapped around the whole run.
// Rendered INSIDE the run, through the same `renderAndCount` the Effect
// dispatcher has always used, so a `ServerError` still reports its `UZ-*`
// code, its suggestion and its request id, and a command-managed exit code
// (`doctor`'s verdict) is restored rather than read as a plain success. The
// library's own parse failures are the exception: the presenter writes
// nothing for them, `renderLibraryFailure` writes the one block or envelope,
// and their code comes from the exit-code map.
function programFor<Services>(
  invocation: Invocation,
  rootCommand: ReturnType<typeof buildCommandTree>,
  services: Layer.Layer<Services>,
) {
  const { argv, stdout, stderr, jsonMode } = invocation;
  const tree = asCommandNode(rootCommand);
  return Command.runWith(rootCommand, { version: VERSION, renderErrors: true })(helpArgv(argv)).pipe(
    withCommandInstrumentation(),
    Effect.exit,
    Effect.flatMap((exit) => {
      if (Exit.isSuccess(exit)) return renderAndCount(exit as Exit.Exit<unknown, never>);
      // A refusal the gate already wrote carries its own code and no message;
      // handing it to the shared renderer would print `error: undefined`
      // under the sentence it already printed.
      if (isGuardRefusal(exit.cause)) return Effect.succeed(exitCodeForFailure(exit.cause));
      return isLibraryUsageError(exit.cause)
        ? Effect.succeed(renderLibraryFailure(exit.cause, tree, argv, stderr, jsonMode))
        : renderAndCount(exit as Exit.Exit<unknown, never>);
    }),
    // The library renders help through Console. Left alone that reaches the
    // real process streams, which would strand every test that injected its
    // own and break runCli's promise to write only where it was told.
    Effect.provideService(Console.Console, consoleForStreams(stdout, stderr)),
    Effect.provide(CliOutput.layer(helpFormatter())),
    Effect.provide(CliOutput.layerPresenter(housePresenter)),
    Effect.provide(CliConfig.layer({ builtIns: [GlobalFlag.Help, GlobalFlag.Version, completionsFlag, GlobalFlag.LogLevel] })),
    Effect.provide(services),
    Effect.provide(BunServices.layer),
  );
}

// One plain sentence when a saved file exists but cannot be read: what broke,
// what the CLI is doing instead, and how to put it right. The endpoint is
// named because a failed read took the recorded deployment down with it — an
// operator pinned to a self-hosted backend would otherwise find out from the
// access log. Same block as a rendered error, marked as a warning.
function reportUnreadableState(
  stderr: WritableStreamLike,
  unreadable: ReadonlyArray<{ readonly file: string; readonly code: string }>,
  apiUrl: string,
): void {
  const files = unreadable.map((u) => `${u.file}: ${u.code}`).join(", ");
  writeErrorBlock(stderr, {
    severity: BLOCK_SEVERITY.warning,
    message: `cannot read your saved sign-in (${files}); continuing signed out, against ${apiUrl}`,
    fields: { suggestion: "check the file's permissions, or run `agentsfleet login` to sign in again" },
  });
}

// A parse failure, answered in the house shape rather than the library's.
//
// The presenter wrote nothing for it, so this is the whole answer: one block
// in text mode, one envelope carrying the stable code under `--json`. The
// exit code comes from the shared map.
function renderLibraryFailure(
  cause: Parameters<typeof exitCodeForFailure>[0],
  tree: CommandNode,
  argv: readonly string[],
  stderr: WritableStreamLike,
  jsonMode: boolean,
): number {
  const rejection = houseRejection(Cause.squash(cause), tree, argv);
  if (rejection !== null) {
    if (jsonMode) {
      printJson(stderr, {
        error: { code: rejection.code, message: rejection.detail },
      });
    } else {
      writeErrorBlock(stderr, {
        severity: BLOCK_SEVERITY.error,
        message: rejection.headline,
        fields: {
          code: rejection.code,
          usage: rejection.usage,
          [BLOCK_LABEL.didYouMean]: rejection.didYouMean,
          suggestion: rejection.suggestion,
        },
      });
    }
  }
  return exitCodeForFailure(cause);
}
