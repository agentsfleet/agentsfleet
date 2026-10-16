"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  removeWorkspaceLibraryEntry,
} from "@/lib/api/fleet-library";

export async function removeLibraryEntryAction(
  workspaceId: string,
  entryId: string,
): Promise<ActionResult<void>> {
  return withToken((t) => removeWorkspaceLibraryEntry(workspaceId, entryId, t));
}
