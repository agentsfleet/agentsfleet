// Which fleet `fleet steer` talks to. An id is used as given; `@name` is the one
// fleet this workspace saved under that name, read before the event tail opens,
// so the tail and the message both go to the fleet the person named.

import type { Redacted } from "effect/Redacted";
import { Effect } from "effect";
import { ValidationError, type CliError } from "../../errors/index.ts";
import { wsFleetsPath } from "../../lib/api-paths.ts";
import { readFleetAddress } from "../../lib/fleet-address.ts";
import { HttpClient } from "../../services/http-client.ts";
import { decodeFleetPage } from "./read.ts";

// The fleet list's exact-name filter, which answers zero or one fleet.
const QUERY_NAME = "name" as const;
const SUGGESTION_LIST_FLEETS = "run `agentsfleet fleet list` to see each fleet's name" as const;

/**
 * The fleet id a `fleet_id` argument stands for: the argument itself, or the id
 * of the fleet saved under its `@name`.
 *
 * The parser admits `@` only with a whole name and nothing after it, so an
 * argument that is not an address is an id the parser already checked.
 *
 * The answer's own name is compared as well as filtered for. A daemon that
 * predates the filter ignores `?name=` and answers its newest fleets, and the
 * first of those is not the fleet the person named.
 */
export const resolveSteerTarget = (
  wsId: string,
  fleetArgument: string,
  token: Redacted<string>,
): Effect.Effect<string, CliError, HttpClient> =>
  Effect.gen(function* () {
    const address = readFleetAddress(fleetArgument);
    if (address === undefined) return fleetArgument;
    const http = yield* HttpClient;
    const query = new URLSearchParams({ [QUERY_NAME]: address.name });
    const body = yield* http.request<unknown>({ path: `${wsFleetsPath(wsId)}?${query}`, token });
    const page = yield* decodeFleetPage(body);
    const named = page.items.find((fleet) => fleet.name === address.name);
    if (named !== undefined) return named.id;
    return yield* Effect.fail(
      new ValidationError({
        detail: `No fleet named \`${address.name}\` in this workspace`,
        suggestion: SUGGESTION_LIST_FLEETS,
      }),
    );
  });
