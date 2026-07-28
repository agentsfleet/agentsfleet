"use server";

import { listWorkspaceFleetLibrary, type FleetLibraryPageResult } from "@/lib/api/fleet-library";
import { withToken, type ActionResult } from "@/lib/actions/with-token";

// Server-only reads for the install screen. The gallery endpoint is
// bearer-authed and the token is minted here, so it never reaches the browser.
//
// The first page arrives with the server render; this exists for load-more,
// which is a client gesture and therefore needs an action rather than a direct
// call. `startingAfter` null means the first page.
export async function readFleetLibraryPageAction(
  workspaceId: string,
  startingAfter: string | null = null,
): Promise<ActionResult<FleetLibraryPageResult>> {
  return withToken((token) => listWorkspaceFleetLibrary(workspaceId, token, startingAfter));
}
