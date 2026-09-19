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
import { Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsFleetLibrariesPath } from "../lib/api-paths.ts";
import { ui } from "../output/index.ts";
import { LIBRARY_ID_PLACEHOLDER } from "../constants/cli-flags.ts";
import type { CliError } from "../errors/index.ts";
import type { FleetLibraryGalleryEntry } from "./fleet_install_source.ts";

interface FleetLibraryGalleryPage {
  readonly items?: ReadonlyArray<FleetLibraryGalleryEntry>;
  readonly next_cursor?: string | null;
}

const FIELD_ID = "id" as const;
const FIELD_NAME = "name" as const;
const FIELD_TIER = "tier" as const;
const FIELD_CREDENTIALS = "credentials" as const;
const EMPTY_REQUIREMENT = "—" as const;

// Same page size and ceiling `install` resolves against. They must page
// alike: a `library` that stopped at the first page while `install` walked to
// the end would put the two commands back into the disagreement this whole
// change exists to remove, just at a different boundary.
const GALLERY_PAGE_LIMIT = 100;
const GALLERY_MAX_PAGES = 50;
const QUERY_LIMIT = "limit" as const;
const QUERY_STARTING_AFTER = "starting_after" as const;

const EMPTY_GALLERY =
  "No Fleet libraries in this workspace." as const;
const EMPTY_HINT =
  "Add one with: agentsfleet library add --github <owner/repo>" as const;
const INSTALL_HINT =
  `Install one with: agentsfleet install --library ${LIBRARY_ID_PLACEHOLDER}` as const;

const joinNames = (names: ReadonlyArray<string> | undefined): string =>
  names && names.length > 0 ? names.join(", ") : EMPTY_REQUIREMENT;

export const libraryEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const config = yield* CliConfig;
  const output = yield* Output;
  const http = yield* HttpClient;

  const workspaceId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;

  const items: FleetLibraryGalleryEntry[] = [];
  let cursor: string | null = null;
  for (let page = 0; page < GALLERY_MAX_PAGES; page += 1) {
    const params = new URLSearchParams({ [QUERY_LIMIT]: String(GALLERY_PAGE_LIMIT) });
    if (cursor !== null) params.set(QUERY_STARTING_AFTER, cursor);
    const res: FleetLibraryGalleryPage = yield* http.request<FleetLibraryGalleryPage>({
      path: `${wsFleetLibrariesPath(workspaceId)}?${params.toString()}`,
      token,
    });
    items.push(...(res.items ?? []));
    if (!res.next_cursor) break;
    cursor = res.next_cursor;
  }

  if (config.jsonMode) {
    yield* output.printJson({ items });
    return;
  }

  if (items.length === 0) {
    yield* output.info(EMPTY_GALLERY);
    yield* output.info(ui.dim(EMPTY_HINT));
    return;
  }

  yield* output.printTable(
    [
      { key: FIELD_ID, label: "LIBRARY" },
      { key: FIELD_NAME, label: "NAME" },
      { key: FIELD_TIER, label: "TIER" },
      { key: FIELD_CREDENTIALS, label: "SECRETS" },
    ],
    items.map((entry) => ({
      id: String(entry.id ?? ""),
      name: String(entry.name ?? ""),
      // Two entries can share a name across tiers — the platform catalogue and
      // a workspace's own copy of the same bundle — so the tier is what tells
      // the two rows apart.
      tier: String(entry.visibility ?? EMPTY_REQUIREMENT),
      credentials: joinNames(entry.requirements?.credentials),
    })),
  );
  yield* output.info(ui.dim(INSTALL_HINT));
});
