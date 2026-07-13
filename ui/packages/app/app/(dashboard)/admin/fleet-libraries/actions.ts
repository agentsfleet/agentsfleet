"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { requireScope } from "@/lib/actions/require-scope";
import { SCOPE } from "@/lib/auth/scopes";
import { onboardPlatformFleetLibrary } from "@/lib/api/fleet-library";
import type { OnboardLibraryEntryRequest, OnboardedPlatformLibraryEntry } from "@/lib/types";

// Onboards a repository into the platform catalog. Runs server-side so the
// api-audience token never reaches the browser, and fails fast when the session
// lacks the operator scope — the backend independently 403s (UZ-AUTH-022), so
// this is the defence-in-depth arm, not the security boundary.
export async function onboardPlatformLibraryAction(
  body: OnboardLibraryEntryRequest,
): Promise<ActionResult<OnboardedPlatformLibraryEntry>> {
  return requireScope(SCOPE.PLATFORM_LIBRARY_WRITE, () =>
    withToken((t) => onboardPlatformFleetLibrary(body, t)),
  );
}
