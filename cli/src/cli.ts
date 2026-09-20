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
import { CliOutput, Command } from "effect/unstable/cli";
import { BunServices } from "@effect/platform-bun";

import { loadState } from "./lib/state-load.ts";
import { detectTokenInArgv } from "./lib/argv-redact.ts";
import { resolveApiKeyFromEnv } from "./services/config.ts";
import type { FetchImpl } from "./lib/http.ts";
import { printJson, writeError, writeLine } from "./program/io.ts";
import { SUGGESTION_PREFIX } from "./constants/rejection.ts";
import { guardCommand } from "./program/auth-guard.ts";
import { ui } from "./output/index.ts";
import { DEFAULT_API_URL, normalizeApiUrl, resolveDashboardUrl } from "./util/url.ts";
import { rootCommand } from "./program/tree/root.command.ts";
import { resolveCommandPath, type CommandNode } from "./program/tree/resolve-path.ts";
import { mainLayerFor } from "./runtime/main-layer.ts";
import { makeCommandExitCode } from "./runtime/exit-code.service.ts";
import { CommandGuard, GuardRefused } from "./runtime/guard.service.ts";
import { withCommandInstrumentation } from "./services/telemetry/command-instrumentation.ts";
import { detectJsonMode, maybePrintVersion, resolveGlobalApiUrl } from "./program/entry/argv-scan.ts";
import { consoleForStreams } from "./program/entry/console-bridge.ts";
import { deferredHelp, type DeferredHelp } from "./program/entry/deferred-help.ts";
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

// VERSION: package.json source of truth; `make sync-version` updates consumers.
const PKG_JSON_PATH = join(dirname(fileURLToPath(import.meta.url)), "..", "package.json");
const pkgJson = JSON.parse(readFileSync(PKG_JSON_PATH, "utf8")) as { version: string };
export const VERSION: string = pkgJson.version;

const HELP_FLAG = "--help" as const;
const HELP_COMMAND = "help" as const;
const GUARD_EXIT_CODE = 1;
const SUCCESS_EXIT_CODE = 0;

export interface RunCliIo {
  stdout?: WritableStreamLike;
  stderr?: WritableStreamLike;
  stdin?: NodeJS.ReadableStream;
  env?: NodeJS.ProcessEnv;
  fetchImpl?: typeof fetch;
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
  const tree = rootCommand as unknown as CommandNode;
  const commandPath = resolveCommandPath(tree, argv);

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
        { printJson, writeLine, ui },
      );
      return Effect.fail(new GuardRefused(GUARD_EXIT_CODE));
    }),
  });

  const layer = mainLayerFor(
    layerInputFor({
      apiUrl,
      dashboardUrl: resolveDashboardUrl(apiUrl, env.AGENTSFLEET_DASHBOARD_URL),
      apiKey,
      jsonMode,
      noOpen: false,
      commandPath,
      env,
      stdout,
      stderr,
      stdin: io.stdin,
      fetchImpl: io.fetchImpl as FetchImpl | undefined,
    }),
  );

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
  // The library's own parse failures are the exception — it has already
  // written them, and their code comes from the exit-code map instead.
  // In JSON mode the library's help document is held back until the outcome
  // says whether it belongs on stdout — see `deferred-help.ts`. Outside JSON
  // mode there is nothing to decide, so the real stream is used directly.
  const heldHelp: DeferredHelp | null = jsonMode ? deferredHelp(stdout) : null;
  const libraryStdout = heldHelp?.stream ?? stdout;

  const program = Command.runWith(rootCommand, { version: VERSION, renderErrors: false })(effectiveArgv).pipe(
    withCommandInstrumentation(),
    Effect.exit,
    Effect.flatMap((exit) => {
      if (Exit.isSuccess(exit)) return renderAndCount(exit as Exit.Exit<unknown, never>);
      // A refusal the gate already wrote carries its own code and no message;
      // handing it to the shared renderer would print `error: undefined`
      // under the sentence it already printed.
      if (isGuardRefusal(exit.cause)) return Effect.succeed(exitCodeForFailure(exit.cause));
      return isLibraryUsageError(exit.cause)
        ? Effect.succeed(renderLibraryFailure(exit.cause, tree, argv, stderr, jsonMode, heldHelp))
        : renderAndCount(exit as Exit.Exit<unknown, never>);
    }),
    // The library renders help and parse errors through Console. Left alone
    // that reaches the real process streams, which would strand every test
    // that injected its own and break runCli's promise to write only where it
    // was told.
    Effect.provideService(Console.Console, consoleForStreams(libraryStdout, stderr)),
    Effect.provide(CliOutput.layer(helpFormatter())),
    Effect.provide(layer),
    Effect.provide(managedExitCode.layer),
    Effect.provide(guardLayer),
    Effect.provide(BunServices.layer),
  );

  const exit = await Effect.runPromiseExit(program as Effect.Effect<number, never, never>);
  // Help that no rejection spoke over is a person's help: it prints. `flush`
  // is a no-op once `renderLibraryFailure` has discarded.
  heldHelp?.flush();
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
// access log. Same two-line shape as a rendered error: fact, then Suggestion.
function reportUnreadableState(
  stderr: WritableStreamLike,
  unreadable: ReadonlyArray<{ readonly file: string; readonly code: string }>,
  apiUrl: string,
): void {
  const files = unreadable.map((u) => `${u.file}: ${u.code}`).join(", ");
  writeLine(
    stderr,
    `warning: cannot read your saved sign-in (${files}) — continuing signed out, against ${apiUrl}`,
  );
  writeLine(
    stderr,
    "  Suggestion: check the file's permissions, or run `agentsfleet login` to sign in again",
  );
}

// A parse failure, answered in the house shape rather than the library's.
//
// The library has already written its own text and help document by the time
// this runs, but neither carries the ✕ glyph, the `error:` stem, the
// Suggestion line, or — under `--json` — the stable code a consumer switches
// on. So the house lines are added here, and the exit code comes from the
// shared map.
function renderLibraryFailure(
  cause: Parameters<typeof exitCodeForFailure>[0],
  tree: CommandNode,
  argv: readonly string[],
  stderr: WritableStreamLike,
  jsonMode: boolean,
  heldHelp: DeferredHelp | null,
): number {
  const rejection = houseRejection(Cause.squash(cause), tree, argv);
  if (rejection !== null) {
    if (jsonMode) {
      // The envelope is the whole answer for a machine. The help document the
      // library rendered beneath the parse failure is dropped rather than
      // printed, so stdout stays parseable.
      heldHelp?.discard();
      printJson(stderr, {
        error: { code: rejection.code, message: rejection.detail },
      });
    } else {
      writeLine(
        stderr,
        ui.err(`error: ${rejection.detail}${SUGGESTION_PREFIX}${rejection.suggestion}`),
      );
    }
  }
  return exitCodeForFailure(cause);
}

// `[]` → `--help`, and `help x y` → `x y --help`. Any other argv is its own.
function helpArgv(argv: readonly string[]): string[] {
  if (argv.length === 0) return [HELP_FLAG];
  const [first, ...rest] = argv;
  if (first !== HELP_COMMAND) return [...argv];
  return [...rest, HELP_FLAG];
}
