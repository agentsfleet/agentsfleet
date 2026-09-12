"use server";

import { revalidatePath } from "next/cache";
import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { requireScope } from "@/lib/actions/require-scope";
import { SCOPE } from "@/lib/auth/scopes";
import {
  deletePlatformFleetLibraryEntry,
  listPlatformFleetLibrary,
  onboardPlatformFleetLibrary,
  patchPlatformFleetLibraryEntry,
} from "@/lib/api/fleet-library";
import type {
  OnboardLibraryEntryRequest,
  OnboardedPlatformLibraryEntry,
  PlatformCatalogEntry,
  PlatformCatalogPatch,
  PlatformCatalogResponse,
} from "@/lib/types";
import { ADMIN_FLEET_LIBRARIES_PATH } from "./library-copy";

// Every write runs server-side so the api-audience token never reaches the
// browser, and fails fast when the session lacks the operator scope — the backend
// independently 403s (UZ-AUTH-022), so this is the defence-in-depth arm, not the
// security boundary.
//
// Every write also revalidates the page. The table IS the confirmation: an
// operator must never have to guess whether the thing they just did took.

// Adds a fleet from a repository, or refetches an existing one's bundle. Either
// way the row lands as a draft — publishing is a separate, deliberate act.
export async function onboardPlatformLibraryAction(
  body: OnboardLibraryEntryRequest,
): Promise<ActionResult<OnboardedPlatformLibraryEntry>> {
  return requireScope(SCOPE.PLATFORM_LIBRARY_WRITE, async () => {
    const result = await withToken((t) => onboardPlatformFleetLibrary(body, t));
    if (result.ok) revalidatePath(ADMIN_FLEET_LIBRARIES_PATH);
    return result;
  });
}

// Re-reads the catalog so a timed-out import can be settled by the row rather
// than by our own patience. Read-only and deliberately without `revalidatePath`:
// this runs while the dialog is still deciding what happened, and the caller
// revalidates once it knows. It carries the same write scope as its neighbours
// because this surface has no read rung — see `PLATFORM_FLEET_LIBRARIES_PATH`.
export async function readPlatformLibraryAction(): Promise<ActionResult<PlatformCatalogResponse>> {
  return requireScope(SCOPE.PLATFORM_LIBRARY_WRITE, async () =>
    withToken((t) => listPlatformFleetLibrary(t)),
  );
}

// Curates the description and the per-credential install-gate copy — the two
// fields the importer cannot derive — and publishes or withdraws the entry.
export async function patchPlatformLibraryAction(
  id: string,
  body: PlatformCatalogPatch,
  ifMatch: string,
): Promise<ActionResult<PlatformCatalogEntry>> {
  return requireScope(SCOPE.PLATFORM_LIBRARY_WRITE, async () => {
    const result = await withToken((t) => patchPlatformFleetLibraryEntry(id, body, ifMatch, t));
    if (result.ok) revalidatePath(ADMIN_FLEET_LIBRARIES_PATH);
    return result;
  });
}

// Deletes an entry. The backend refuses while it is published, so the UI never
// offers this on a live fleet — but the route, not the UI, is the guard.
export async function deletePlatformLibraryAction(id: string): Promise<ActionResult<void>> {
  return requireScope(SCOPE.PLATFORM_LIBRARY_WRITE, async () => {
    const result = await withToken((t) => deletePlatformFleetLibraryEntry(id, t));
    if (result.ok) revalidatePath(ADMIN_FLEET_LIBRARIES_PATH);
    return result;
  });
}
