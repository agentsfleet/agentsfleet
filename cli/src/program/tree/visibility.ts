// What this invocation may be shown.
//
// `Unknown` is the fail-closed answer: no identity read happened, or it did
// not produce a list this CLI trusts. `Known` carries the scopes the server
// reported for the credential in play. Visibility decides only what help,
// suggestions and completions advertise; the server decides what runs.

import { Command } from "effect/cli";

export type Visibility =
  | { readonly _tag: "Unknown" }
  | { readonly _tag: "Known"; readonly scopes: ReadonlySet<string> };

export const UNKNOWN_VISIBILITY: Visibility = { _tag: "Unknown" };

export const knownVisibility = (scopes: Iterable<string>): Visibility => ({
  _tag: "Known",
  scopes: new Set(scopes),
});

// Membership only. The server already renders the downward closure of what
// it granted, so a `model:admin` holder's list also carries `model:read`; the
// CLI keeps no hierarchy of its own to drift from the server's.
export const grants = (visibility: Visibility, scope: string): boolean =>
  visibility._tag === "Known" && visibility.scopes.has(scope);

/**
 * Lists `command` when `listed`, otherwise keeps it parseable but unlisted.
 *
 * `Command.unlisted` drops a child from its parent's help, from completion
 * scripts and from did-you-mean, and the exact name still parses and reaches
 * its handler — which is why hiding is safe: the handler's server call is
 * still where authorization happens.
 */
export const listedWhen =
  (listed: boolean) =>
  <const Name extends string, Input, E, R, ContextInput>(
    command: Command.Command<Name, Input, ContextInput, E, R>,
  ): Command.Command<Name, Input, ContextInput, E, R> =>
    listed ? command : Command.unlisted(command);
