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
import { resolveCommandPath, type CommandNode } from "./program/tree/resolve-path.ts";
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

  // Real read failures (EACCES, EIO — not absence) are recorded by the loader
  // and reported below, once the endpoint they cost us is known.
  const { creds, unreadable } = await loadState(env);
  // Two credential slots: the stored login credential from disk, and the
  // service API key from the environment. The key wins at the wire.
  // `resolveApiKeyFromEnv` trims, so a whitespace-only export counts as absent
  // to the guard and the wire alike, rather than sending a blank Bearer.
  const apiKey = resolveApiKeyFromEnv(env);
  const explicitApi = resolveGlobalApiUrl(argv, env);
  const apiUrl = normalizeApiUrl(explicitApi || creds.api_url || DEFAULT_API_URL);

  if (unreadable.length > 0) reportUnreadableState(stderr, unreadable, apiUrl);

  // Walked off the tree, not parsed: the layer carries the command path and
  // has to exist before the parser does. An invalid invocation yields the
  // deepest command that did resolve, which is the one whose help is coming.
  // The path is resolved on the fail-closed tree. Visibility only flips
  // `unlisted`, never a name, so the path is the same in every tree.
  const unknownTree = buildCommandTree(UNKNOWN_VISIBILITY);
  const commandPath = resolveCommandPath(unknownTree as unknown as CommandNode, argv);

  // The refusal is DECIDED here, where the credential and target were
  // resolved, and ASKED by the tree after the parser has run — a mistyped flag
  // on a command you are not signed in for reports the flag, because that is
  // the failure you can fix without leaving the terminal.
  const guardLayer = Layer.succeed(CommandGuard, {
    check: Effect.suspend(() => {
      const refusal = guardCommand(commandPath[0] ?? "", {
        token: creds.token ?? null,
        apiKey,
        apiUrl,
        storedApiUrl: creds.api_url ?? null,
        targetIsExplicit: Boolean(explicitApi),
      });
      if (refusal === null) return Effect.void;
      writeError(
        { stderr: stderr as unknown as NodeJS.WritableStream, jsonMode },
        refusal.errorCode,
        refusal.message,
      );
      return Effect.fail(new GuardRefused(GUARD_EXIT_CODE));
    }),
  });

  const layerInput = layerInputFor({
    apiUrl,
    dashboardUrl: resolveDashboardUrl(apiUrl, env[DASHBOARD_URL_ENV]),
    apiKey,
    jsonMode,
    noOpen: false,
    commandPath,
    env,
    stdout,
    stderr,
    stdin: io.stdin,
    fetchImpl: io.fetchImpl as FetchImpl | undefined,
  });
  const layer = mainLayerFor(layerInput);

  // Help, typos at the root or under `platform`, and `--completions` read the
  // caller's scopes once; every other invocation keeps the fail-closed tree
  // and makes no extra request. One tree then feeds path-based rejection and
  // the parser, so they cannot disagree about what exists.
  const visibility = await Effect.runPromise(discoverVisibility({
    argv,
    tree: unknownTree as unknown as CommandNode,
    apiUrl,
    targetIsExplicit: Boolean(explicitApi),
    apiKey,
    config: layerInput.config ?? {},
    env,
    stdout,
    stderr,
    logLevel: scanLogLevel(argv),
    timeoutMs: io.discoveryTimeoutMs ?? DISCOVERY_TIMEOUT_MS,
  }));
  const rootCommand = visibility._tag === "Unknown" ? unknownTree : buildCommandTree(visibility);
  const tree = rootCommand as unknown as CommandNode;

  // One holder per invocation: two runs in one process must not read each
  // other's verdict, and every test file is two runs in one process.
  const managedExitCode = makeCommandExitCode();

  // Bare `agentsfleet` asks for help explicitly rather than relying on the
  // parser's own empty-argv behaviour, so the body lands on stdout at exit 0
  // instead of reading as a missing-command failure.
  //
  // `agentsfleet help [command]` is the other spelling of the same request.
  // The previous parser carried it as a built-in command; this one does not,
  // and losing it would turn a form people have in their muscle memory into
  // an unknown-command error. It is rewritten rather than declared as a
  // command so `help schedule add` and `schedule add --help` cannot drift
  // apart.
  const effectiveArgv = helpArgv(argv);

  // One span and one analytics row per invocation, wrapped around the whole
  // run rather than each command. CommandRuntime is already correct for this
  // invocation because the layer was built from the resolved command path.
  // Rendered INSIDE the run, through the same `renderAndCount` the Effect
  // dispatcher has always used, so a `ServerError` still reports its `UZ-*`
  // code, its suggestion and its request id. It also restores the
  // command-managed exit code: `doctor` succeeds with a number when its checks
  // fail, and reading that as a plain success would report a broken
  // deployment as healthy.
  //
  // The library's own parse failures are the exception: the presenter writes
  // nothing for them, `renderLibraryFailure` writes the one block or envelope,
  // and their code comes from the exit-code map.
  const program = Command.runWith(rootCommand, { version: VERSION, renderErrors: true })(effectiveArgv).pipe(
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
    // The library renders help and parse errors through Console. Left alone
    // that reaches the real process streams, which would strand every test
    // that injected its own and break runCli's promise to write only where it
    // was told.
    Effect.provideService(Console.Console, consoleForStreams(stdout, stderr)),
    Effect.provide(CliOutput.layer(helpFormatter())),
    Effect.provide(CliOutput.layerPresenter(housePresenter)),
    Effect.provide(CliConfig.layer({ builtIns: [GlobalFlag.Help, GlobalFlag.Version, completionsFlag, GlobalFlag.LogLevel] })),
    Effect.provide(layer),
    Effect.provide(managedExitCode.layer),
    Effect.provide(guardLayer),
    Effect.provide(BunServices.layer),
  );

  const exit = await Effect.runPromiseExit(program as Effect.Effect<number, never, never>);
  // The pipeline above turns every outcome into a number, so a failure here is
  // the runtime itself falling over rather than a command failing.
  if (Exit.isFailure(exit)) return exitCodeForFailure(exit.cause);
  // A command that reported its own verdict outranks the dispatcher's 0: the
  // run succeeded, and the news it carries is still bad.
  return exit.value !== SUCCESS_EXIT_CODE ? exit.value : managedExitCode.read();
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
