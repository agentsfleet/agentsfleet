// `agentsfleet library` — the Fleet library gallery for the active workspace.
//
// It reads the workspace gallery (platform ∪ this workspace's tenant entries),
// which is the same list `install --library` resolves against. It used to read
// the platform-only catalogue, so a library onboarded into the workspace was
// installable by identifier and invisible here — and the identifier could then
// only come from the dashboard. One endpoint for both commands is what keeps
// "what I can see" and "what I can install" the same set.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetLibrariesPath } from "../lib/api-paths.ts";
import { collectPages } from "../lib/paged.ts";
import { ui, AGE_KEY, EMPTY_CELL } from "../output/index.ts";
import { LIBRARY_ID_PLACEHOLDER } from "../constants/cli-flags.ts";
import type { CliError } from "../errors/index.ts";
import type { FleetLibraryGalleryEntry } from "./fleet_install_source.ts";

const FIELD_ID = "id" as const;
const FIELD_NAME = "name" as const;
const FIELD_TIER = "tier" as const;
const FIELD_CREDENTIALS = "credentials" as const;


const EMPTY_GALLERY =
  "No Fleet libraries in this workspace." as const;
const EMPTY_HINT =
  "Add one with: agentsfleet library add --github <owner/repo>" as const;
const LIBRARIES_LISTED = "Fleet libraries" as const;
const INSTALL_HINT =
  `Install one with: agentsfleet install --library ${LIBRARY_ID_PLACEHOLDER}` as const;

const joinNames = (names: ReadonlyArray<string> | undefined): string =>
  names && names.length > 0 ? names.join(", ") : EMPTY_CELL;

export const libraryEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const output = yield* Output;
  const http = yield* HttpClient;

  const workspaceId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;

  const items = yield* collectPages<FleetLibraryGalleryEntry>(
    http,
    wsFleetLibrariesPath(workspaceId),
    token,
  );

  if (output.format !== OUTPUT_FORMAT.text) {
    yield* output.success(LIBRARIES_LISTED, { items });
    return;
  }

  if (items.length === 0) {
    yield* output.info(EMPTY_GALLERY);
    yield* output.info(ui.dim(EMPTY_HINT));
    return;
  }

  yield* output.printEntityTable(
    {
      name: { key: FIELD_NAME, label: "NAME" },
      id: { key: FIELD_ID, label: "LIBRARY" },
      domain: [
        { key: FIELD_TIER, label: "TIER" },
        { key: FIELD_CREDENTIALS, label: "SECRETS" },
      ],
    },
    items.map((entry) => ({
      id: String(entry.id ?? ""),
      name: String(entry.name ?? ""),
      // Two entries can share a name across tiers — the platform catalogue and
      // a workspace's own copy of the same bundle — so the tier is what tells
      // the two rows apart.
      tier: String(entry.visibility ?? EMPTY_CELL),
      credentials: joinNames(entry.requirements?.credentials),
      [AGE_KEY]: entry.created_at,
    })),
  );
  yield* output.info(ui.dim(INSTALL_HINT));
});
