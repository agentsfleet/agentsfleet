// `agentsfleet library remove <entry_id>` — take an onboarded entry back out.
//
// The verb slot 460 withheld. Removal is permanent: there is no unlist, no
// marker and no visibility flip, because nothing depends on the row. A fleet
// installed from an entry copied the bundle at install time and carries its
// own, so removing the entry cannot disturb a fleet running from it, and
// re-onboarding the same bytes afterwards mints a new entry.
//
// Idempotent, and this command does not pretend otherwise. The daemon answers
// 204 for an entry already gone and for one belonging to another workspace,
// which is deliberate — separating the two would need an unscoped read whose
// only effect is to confirm the id exists somewhere. So the success line says
// what is true afterwards ("no longer in this workspace"), never "deleted 1
// row", which this command cannot know.

import { Effect } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsLibraryEntryPath } from "../lib/api-paths.ts";
import { HTTP_METHOD } from "../constants/http-method.ts";

export const REMOVE_USAGE = "usage: agentsfleet library remove <entry_id>" as const;

export const removedNotice = (entryId: string): string =>
  `${entryId} is no longer in this workspace's Fleet library. ` +
  `Fleets already installed from it keep running.`;

const REINSTATE_HINT =
  "Onboard it again with: agentsfleet library add --github <owner/repo>" as const;

export const libraryRemoveEffectFromArgs = Effect.fn("library.remove")(
  function* (entryId: string) {
    const output = yield* Output;
    const http = yield* HttpClient;
    const workspaceId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;

    yield* http.request<unknown>({
      path: wsLibraryEntryPath(workspaceId, entryId),
      method: HTTP_METHOD.delete,
      token,
    });

    yield* output.success(removedNotice(entryId), { removed: true, id: entryId });
    yield* output.info(REINSTATE_HINT);
  },
);
