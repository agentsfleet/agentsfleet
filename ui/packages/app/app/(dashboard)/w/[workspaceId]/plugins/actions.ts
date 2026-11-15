"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  removeWorkspacePluginEntry,
} from "@/lib/api/fleet-plugin";

export async function removePluginEntryAction(
  workspaceId: string,
  entryId: string,
): Promise<ActionResult<void>> {
  return withToken((t) => removeWorkspacePluginEntry(workspaceId, entryId, t));
}
