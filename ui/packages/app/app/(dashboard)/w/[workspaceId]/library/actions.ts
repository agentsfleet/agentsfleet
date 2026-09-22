"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  listWorkspaceLibraryEntries,
  removeWorkspaceLibraryEntry,
} from "@/lib/api/fleet-library";
import type { WorkspaceLibraryEntriesResponse } from "@/lib/api/library-types";

// Removal is idempotent server-side: an entry already gone and one naming
// another workspace's entry both answer 204. So this action succeeds in cases
// where nothing was removed, and the page refreshes to whatever is true now
// rather than asserting what it thinks it did.
export async function removeLibraryEntryAction(
  workspaceId: string,
  entryId: string,
): Promise<ActionResult<void>> {
  return withToken((t) => removeWorkspaceLibraryEntry(workspaceId, entryId, t));
}

// A later page, for the list's Load more. The first page is server-rendered by
// the page itself; this exists because reading only that page drops every
// entry past it with nothing on screen to say so — the hazard the gallery
// beside it already refuses to ship.
export async function listLibraryEntriesAction(
  workspaceId: string,
  startingAfter: string,
): Promise<ActionResult<WorkspaceLibraryEntriesResponse>> {
  return withToken((t) => listWorkspaceLibraryEntries(workspaceId, t, startingAfter));
}
