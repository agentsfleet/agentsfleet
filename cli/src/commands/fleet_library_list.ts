// `agentsfleet library list` — the entries THIS workspace onboarded.
//
// Bare `agentsfleet library` prints the gallery: the platform catalogue unioned
// with this workspace's own rows, which is the list `install --library`
// resolves against. That is the right answer to "what can I install here" and
// the wrong one to "what did we onboard, and what can I take back out" — a
// platform row is neither yours nor removable. So this reads the owned
// collection, which answers only the second question.
//
// The two commands sit beside each other rather than one growing a flag: the
// daemon serves them as two collections with two shapes and two cursors, which
// is the split the models domain settled before this one existed.

import { Effect } from "effect";
import { CliConfig } from "../services/config.ts";
import { Credentials } from "../services/credentials.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { Workspaces } from "../services/workspaces.ts";
import { requireWorkspaceId, resolveAuthToken } from "./workspace-guards.ts";
import { wsLibraryEntriesPath } from "../lib/api-paths.ts";
import { collectPages } from "../lib/paged.ts";
import { ui, AGE_KEY, EMPTY_CELL } from "../output/index.ts";
import type { CliError } from "../errors/index.ts";

const FIELD_ID = "id" as const;
const FIELD_NAME = "name" as const;
const FIELD_SOURCE = "source" as const;

const ENTRIES_LISTED = "Onboarded Fleet libraries" as const;
const EMPTY_OWNED = "This workspace has onboarded no Fleet libraries." as const;
// Names the command that creates one, because an empty list with no next step
// reads as a broken screen rather than an empty one.
const EMPTY_HINT = "Add one with: agentsfleet library create --github <owner/repo>" as const;
const GALLERY_HINT =
  "Everything installable here, platform entries included: agentsfleet library" as const;
const REMOVE_HINT = "Remove one with: agentsfleet library delete <entry_id>" as const;

/** One row of the owned collection. Metadata only — the endpoint projects no
 *  document column, so there is nothing here to hold bundle content. */
export interface OwnedLibraryEntry {
  readonly id?: string | null;
  readonly name?: string | null;
  readonly description?: string | null;
  readonly source_kind?: string | null;
  readonly source_ref?: string | null;
  readonly content_hash?: string | null;
  readonly created_at?: number | null;
}

/** `github:acme/reviewer`, or just the reference when the kind is unknown to
 *  this build. Two onboardings of near-identical bundles differ by their
 *  source before they differ by anything else a row shows. */
const provenance = (entry: OwnedLibraryEntry): string => {
  const ref = entry.source_ref ?? "";
  if (ref.length === 0) return EMPTY_CELL;
  const kind = entry.source_kind ?? "";
  return kind.length > 0 ? `${kind}:${ref}` : ref;
};

export const libraryListEffect: Effect.Effect<
  void,
  CliError,
  CliConfig | Credentials | HttpClient | Output | Workspaces
> = Effect.gen(function* () {
  const output = yield* Output;
  const http = yield* HttpClient;

  const workspaceId = yield* requireWorkspaceId;
  const token = yield* resolveAuthToken;

  const items = yield* collectPages<OwnedLibraryEntry>(
    http,
    wsLibraryEntriesPath(workspaceId),
    token,
  );

  if (output.format !== OUTPUT_FORMAT.text) {
    yield* output.success(ENTRIES_LISTED, { items });
    return;
  }

  if (items.length === 0) {
    yield* output.info(EMPTY_OWNED);
    yield* output.info(ui.dim(EMPTY_HINT));
    yield* output.info(ui.dim(GALLERY_HINT));
    return;
  }

  yield* output.printEntityTable(
    {
      name: { key: FIELD_NAME, label: "NAME" },
      id: { key: FIELD_ID, label: "ENTRY" },
      domain: [{ key: FIELD_SOURCE, label: "SOURCE" }],
    },
    items.map((entry) => ({
      id: String(entry.id ?? ""),
      name: String(entry.name ?? ""),
      source: provenance(entry),
      [AGE_KEY]: entry.created_at,
    })),
  );
  yield* output.info(ui.dim(REMOVE_HINT));
});
