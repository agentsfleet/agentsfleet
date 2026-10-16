"use server";

import type { PlatformPatch, StagePlatform } from "@/lib/api/platform-library-types";
import { revalidatePath } from "next/cache";
import { withToken } from "@/lib/actions/with-token";
import { requireScope } from "@/lib/actions/require-scope";
import { SCOPE } from "@/lib/auth/scopes";
import { deletePlatformFleetLibraryEntry, patchPlatformFleetLibraryEntry, stagePlatformFleetLibrary } from "@/lib/api/fleet-library";
import { ADMIN_FLEET_LIBRARIES_PATH } from "./library-copy";

// The daemon also checks the operator scope and current private candidate owner.
export async function stagePlatformLibraryAction(body: StagePlatform, operation: string) {
  return requireScope(SCOPE.PLATFORM_LIBRARY_WRITE, async () => {
    const result = await withToken((token) => stagePlatformFleetLibrary(body, operation, token));
    if (result.ok) revalidatePath(ADMIN_FLEET_LIBRARIES_PATH);
    return result;
  });
}

export async function patchPlatformLibraryAction(id: string, body: PlatformPatch, ifMatch: string) {
  return requireScope(SCOPE.PLATFORM_LIBRARY_WRITE, async () => {
    const result = await withToken((token) => patchPlatformFleetLibraryEntry(id, body, ifMatch, token));
    if (result.ok) revalidatePath(ADMIN_FLEET_LIBRARIES_PATH);
    return result;
  });
}

export async function deletePlatformLibraryAction(id: string, ifMatch: string) {
  return requireScope(SCOPE.PLATFORM_LIBRARY_WRITE, async () => {
    const result = await withToken((token) => deletePlatformFleetLibraryEntry(id, ifMatch, token));
    if (result.ok) revalidatePath(ADMIN_FLEET_LIBRARIES_PATH);
    return result;
  });
}
