// Published reusable setup; the revision column pins a new private conversation.

import { Effect } from "effect";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetLibrariesPath, wsOwnedFleetLibrariesPath } from "../lib/api-paths.ts";
import { readLibraryPage, type LibraryPageOptions } from "./fleet-library-read.ts";
import { ui, AGE_KEY } from "../output/index.ts";

const FIELD = { id: "id", name: "name", visibility: "visibility", revision: "revision" } as const;
const CHAT_HINT = "Start from a revision: agentsfleet chat --library <library_id> --revision <revision_id>";
const PUBLISH_HINT = "Publish a reviewed chat with /publish <label>, or chat --publish <label> --review-digest <digest>.";

export const libraryEffectFromOptions = Effect.fn("library.browse")(function* (
  options: LibraryPageOptions,
  owned = false,
) {
  const output = yield* Output;
  const http = yield* HttpClient;
  const workspace = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;
  const path = owned ? wsOwnedFleetLibrariesPath(workspace) : wsFleetLibrariesPath(workspace);
  const result = yield* readLibraryPage(http, path, token, options);
  if (output.format !== OUTPUT_FORMAT.text) {
    yield* output.success(owned ? "Owned Fleet libraries" : "Fleet libraries", { ...result });
    return;
  }
  if (result.items.length === 0) yield* output.info("No published Fleet libraries in this page.");
  else yield* output.printEntityTable({
    name: { key: FIELD.name, label: "NAME" },
    id: { key: FIELD.id, label: "LIBRARY" },
    domain: [{ key: FIELD.visibility, label: "TIER" }, { key: FIELD.revision, label: "REVISION" }],
  }, result.items.map((entry) => ({
    id: entry.id, name: entry.name, visibility: entry.visibility,
    revision: entry.head_revision_id, [AGE_KEY]: entry.created_at,
  })));
  yield* output.info(ui.dim(result.items.length === 0 ? PUBLISH_HINT : CHAT_HINT));
  if (result.next_cursor !== null) yield* output.info(ui.dim(
    `More available: agentsfleet library${owned ? " list" : ""} --starting-after ${result.next_cursor}`,
  ));
});
