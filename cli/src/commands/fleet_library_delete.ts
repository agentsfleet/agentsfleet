// Removing reusable setup leaves installed fleets pinned to their revisions.

import { Effect } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { Output } from "../services/output.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetLibraryPath } from "../lib/api-paths.ts";
import { HTTP_METHOD } from "../constants/http-method.ts";

export const removedNotice = (entryId: string): string =>
  `${entryId} is no longer in this workspace's Fleet library. ` +
  `Fleets already installed from it keep running.`;

const PUBLISH_HINT =
  "Publish reviewed chat setup with: agentsfleet chat --help" as const;

export const libraryDeleteEffectFromArgs = Effect.fn("library.delete")(
  function* (entryId: string) {
    const output = yield* Output;
    const http = yield* HttpClient;
    const workspaceId = yield* requireWorkspaceId;
    const token = yield* resolveAuthToken;

    yield* http.request<unknown>({
      path: wsFleetLibraryPath(workspaceId, entryId),
      method: HTTP_METHOD.delete,
      token,
    });

    yield* output.success(removedNotice(entryId), { removed: true, id: entryId });
    yield* output.info(PUBLISH_HINT);
  },
);
