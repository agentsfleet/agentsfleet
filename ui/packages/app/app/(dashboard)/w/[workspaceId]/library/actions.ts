"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { removeWorkspaceLibraryEntry } from "@/lib/api/fleet-library";

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
