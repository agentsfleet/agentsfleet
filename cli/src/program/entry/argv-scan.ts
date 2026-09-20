// What argv says before anything parses it.
//
// Three values have to be known before the command tree runs, because each
// configures the thing that would otherwise be asked to produce them:
//
//   - `--json` picks the register the Output service answers in, and Output is
//     built into the layer the tree executes in.
//   - `--api` names the server HttpClient talks to, same reason.
//   - `--version` is answered here and not by the tree at all, because the
//     library's own version flag prints plain text and exits, which cannot
//     satisfy `--version --json`.
//
// Each scan stops at `--`, so a value after the end-of-flags marker is an
// argument and never mistaken for the flag it is spelled like.

import type { WritableStreamLike } from "../../output/capability.ts";
import { printJson } from "../io.ts";
import { printVersion } from "../banner.ts";

const END_OF_FLAGS = "--" as const;
const FLAG_API = "--api" as const;
const FLAG_API_INLINE = "--api=" as const;
const FLAG_JSON = "--json" as const;
const FLAG_VERSION = "--version" as const;
const FLAG_VERSION_SHORT = "-v" as const;
const ENV_API_URL = "AGENTSFLEET_API_URL" as const;
const ENV_NO_COLOR = "NO_COLOR" as const;

export const detectJsonMode = (argv: ReadonlyArray<string>): boolean => {
  for (const token of argv) {
    if (token === END_OF_FLAGS) return false;
    if (token === FLAG_JSON) return true;
  }
  return false;
};

/**
 * Renders the version if argv asked for it, reporting whether it did.
 *
 * `--version --json` and `--help --version` both have to resolve to the
 * version, which is why this runs ahead of the parser rather than being a flag
 * the tree declares.
 */
export const maybePrintVersion = (
  argv: ReadonlyArray<string>,
  stdout: WritableStreamLike,
  version: string,
  jsonMode: boolean,
  env: NodeJS.ProcessEnv,
): boolean => {
  for (const token of argv) {
    if (token === END_OF_FLAGS) break;
    if (token !== FLAG_VERSION && token !== FLAG_VERSION_SHORT) continue;
    if (jsonMode) {
      printJson(stdout, { version });
      return true;
    }
    const noColor = env[ENV_NO_COLOR];
    printVersion(stdout, version, {
      noColor: Boolean(noColor && noColor.length > 0),
      jsonMode: false,
    });
    return true;
  }
  return false;
};

/**
 * The API URL this invocation named, or null if it named none.
 *
 * Null is not the default URL — it is the absence of an override, which the
 * caller needs in order to distinguish a named target from an inferred one.
 * The deployment guard refuses a stored credential only when nobody named a
 * target, so collapsing the two here would disarm it.
 */
export const resolveGlobalApiUrl = (
  argv: ReadonlyArray<string>,
  env: NodeJS.ProcessEnv,
): string | null => {
  // An empty value — `--api ""`, or a trailing `--api` with nothing after it —
  // is not a target. It falls through to the environment rather than
  // short-circuiting to null, so an operator who exports AGENTSFLEET_API_URL
  // and then passes a blank flag still reaches their own deployment.
  const flagged = flaggedApiUrl(argv);
  return flagged || env[ENV_API_URL] || null;
};

const flaggedApiUrl = (argv: ReadonlyArray<string>): string | null => {
  for (const [index, token] of argv.entries()) {
    if (token === END_OF_FLAGS) return null;
    if (token === FLAG_API) return argv[index + 1] ?? null;
    if (token.startsWith(FLAG_API_INLINE)) return token.slice(FLAG_API_INLINE.length);
  }
  return null;
};
