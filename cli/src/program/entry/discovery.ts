// Which platform commands this invocation may advertise.
//
// Only a discovery request pays for the identity read: help or a typo at the
// root or under `platform`, and `--completions`. Everything else builds its
// tree as `Unknown` and goes straight to the parser, so no command that runs
// makes an extra round trip or sends its credential an extra time.
//
// Help is a side effect nobody expects to touch the network, so the read is
// held to a stricter rule than a command: a stored login goes only to the
// deployment that minted it, and an environment key never to a host named
// only by `--api` on this line. A failed, skipped or malformed read is
// `Unknown`, which hides operator advertising and blocks nothing — every
// typed command still meets the guard and the server's own scope check.

import { Console, Effect, Layer, Option, Redacted, References, Schema } from "effect";
import type { LogLevel } from "effect/LogLevel";
import { Credentials, credentialsLayer } from "../../services/credentials.ts";
import { HttpClient, httpClientLayer } from "../../services/http-client.ts";
import { cliConfigFromValuesLayer, type CliConfigShape } from "../../services/config.ts";
import { USERS_ME_PATH } from "../../lib/api-paths.ts";
import { normalizeApiUrl } from "../../util/url.ts";
import { unboundTarget } from "../auth-guard.ts";
import { walkCommandPath, booleanFlagNames, childrenOf, type CommandNode } from "../tree/resolve-path.ts";
import { UNKNOWN_VISIBILITY, knownVisibility, type Visibility } from "../tree/visibility.ts";
import { flaggedApiUrl, helpArgv } from "./argv-scan.ts";
import { consoleForStreams } from "./console-bridge.ts";
import type { WritableStreamLike } from "../../output/capability.ts";

export const DISCOVERY_TIMEOUT_MS = 2_000;

const END_OF_FLAGS = "--" as const;
const FLAG_COMPLETIONS = "--completions" as const;
const FLAG_HELP = "--help" as const;
const FLAG_HELP_SHORT = "-h" as const;
const PLATFORM = "platform" as const;
// Built-ins the tree does not declare, so the walk would otherwise read them
// as flags that swallow the next token.
const BUILT_IN_BOOLEANS = ["help", "version"] as const;
const SINGLE_ATTEMPT = { maxAttempts: 1 } as const;

const beforeEndOfFlags = (argv: ReadonlyArray<string>): ReadonlyArray<string> => {
  const end = argv.indexOf(END_OF_FLAGS);
  return end === -1 ? argv : argv.slice(0, end);
};

const asksForCompletions = (flags: ReadonlyArray<string>): boolean =>
  flags.some((token) => token === FLAG_COMPLETIONS || token.startsWith(`${FLAG_COMPLETIONS}=`));

const asksForHelp = (flags: ReadonlyArray<string>): boolean =>
  flags.some((token) => token === FLAG_HELP || token === FLAG_HELP_SHORT);

/**
 * Whether advertising could change what this invocation prints.
 *
 * Visibility only touches root help, root did-you-mean, the `platform`
 * subtree and completion scripts. A reached platform leaf runs, so it does
 * not discover unless it was asked for its own help.
 */
export const isDiscoveryRequest = (argv: ReadonlyArray<string>, tree: CommandNode): boolean => {
  const effective = helpArgv(argv);
  const flags = beforeEndOfFlags(effective);
  if (asksForCompletions(flags)) return true;
  const valueless = new Set([...booleanFlagNames(tree), ...BUILT_IN_BOOLEANS]);
  const walk = walkCommandPath(tree, effective, valueless);
  const atRoot = walk.path.length === 0;
  const inPlatform = walk.path[0] === PLATFORM;
  if (!atRoot && !inPlatform) return false;
  const help = asksForHelp(flags);
  const isGroup = childrenOf(walk.node).length > 0;
  if (!isGroup) return help;
  return help || walk.stop === undefined || !childrenOf(walk.node).some((child) => child.name === walk.stop);
};

export interface DiscoveryInput {
  readonly argv: ReadonlyArray<string>;
  readonly tree: CommandNode;
  // The resolved target, normalized, and whether argv or the environment named it.
  readonly apiUrl: string;
  readonly targetIsExplicit: boolean;
  // The environment key, trimmed; null when absent.
  readonly apiKey: string | null;
  readonly config: Partial<CliConfigShape>;
  readonly env: NodeJS.ProcessEnv;
  readonly stdout: WritableStreamLike;
  readonly stderr: WritableStreamLike;
  readonly logLevel: LogLevel | undefined;
  readonly timeoutMs: number;
}

// The credential discovery may present, or none. Never the stored login when
// an environment key is set: that would read visibility for a principal the
// commands do not run as.
const eligibleToken = (
  input: DiscoveryInput,
): Effect.Effect<Option.Option<Redacted.Redacted<string>>, never, Credentials> =>
  Effect.gen(function* () {
    if (input.apiKey !== null) {
      return flaggedApiUrl(input.argv) === null ? Option.some(Redacted.make(input.apiKey)) : Option.none();
    }
    const snapshot = yield* Effect.orElseSucceed((yield* Credentials).snapshot, () => null);
    if (snapshot === null || Option.isNone(snapshot.accessToken)) return Option.none();
    const unbound = unboundTarget({
      token: Redacted.value(snapshot.accessToken.value),
      apiKey: null,
      apiUrl: input.apiUrl,
      storedApiUrl: snapshot.apiUrl,
      targetIsExplicit: input.targetIsExplicit,
    });
    if (unbound !== null || snapshot.apiUrl === null) return Option.none();
    return normalizeApiUrl(snapshot.apiUrl) === input.apiUrl ? snapshot.accessToken : Option.none();
  });

// Strict on purpose: a list with one non-string member is not a list this CLI
// can reason about, so it is Unknown rather than "the strings that were there".
const ScopesBody = Schema.Struct({ scopes: Schema.Array(Schema.String) });

const readScopes = (input: DiscoveryInput): Effect.Effect<Visibility, never, Credentials | HttpClient> =>
  Effect.gen(function* () {
    const token = yield* eligibleToken(input);
    if (Option.isNone(token)) return UNKNOWN_VISIBILITY;
    const http = yield* HttpClient;
    const body = yield* http.request<unknown>({
      path: USERS_ME_PATH,
      token: token.value,
      retry: SINGLE_ATTEMPT,
      timeoutMs: input.timeoutMs,
    });
    const decoded = yield* Schema.decodeUnknownEffect(ScopesBody)(body);
    return knownVisibility(decoded.scopes);
  }).pipe(Effect.orElseSucceed(() => UNKNOWN_VISIBILITY));

const discoveryLayer = (input: DiscoveryInput): Layer.Layer<Credentials | HttpClient> =>
  Layer.mergeAll(
    credentialsLayer(input.env),
    httpClientLayer.pipe(Layer.provide(cliConfigFromValuesLayer(input.config))),
  );

/** The invocation's visibility. Never fails, never writes outside its own streams. */
export const discoverVisibility = (input: DiscoveryInput): Effect.Effect<Visibility> => {
  if (!isDiscoveryRequest(input.argv, input.tree)) return Effect.succeed(UNKNOWN_VISIBILITY);
  const scoped = readScopes(input).pipe(
    Effect.provide(discoveryLayer(input)),
    Effect.provideService(Console.Console, consoleForStreams(input.stdout, input.stderr)),
  );
  return input.logLevel === undefined
    ? scoped
    : scoped.pipe(Effect.provideService(References.MinimumLogLevel, input.logLevel));
};
