"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  getPreferences as apiGetPreferences,
  putPreference as apiPutPreference,
  prefIsTrue,
  PREFERENCE_KEY,
  type PreferenceBag,
  type PreferenceKey,
} from "@/lib/api/preferences";
import { gatherOnboardingInputs } from "@/lib/onboarding-data";
import type { OnboardingInputs } from "@/lib/onboarding";

export type OnboardingSnapshot = {
  inputs: OnboardingInputs;
  dismissed: boolean;
  collapsed: boolean;
};

// The sidebar widget's own data pull — it lives in the client Shell, which has
// no token, so it fetches its snapshot through this action after it knows the
// route's workspace. Fail-open: an error yields a shown, expanded, undismissed
// widget with a zeroed checklist rather than hiding onboarding.
//
// Optimized for the common returning-user case: read the (single-row) prefs
// first, and if onboarding is already dismissed, SKIP the five-call onboarding
// gather entirely — a dismissed widget renders nothing, so its inputs are never
// read. Only an undismissed widget pays for the gather, and even then the five
// reads run concurrently (one round trip).
export async function getOnboardingSnapshotAction(
  workspaceId: string,
): Promise<ActionResult<OnboardingSnapshot>> {
  return withToken(async (t) => {
    const bag = await apiGetPreferences(workspaceId, t);
    const dismissed = prefIsTrue(bag, PREFERENCE_KEY.DISMISSED);
    const collapsed = prefIsTrue(bag, PREFERENCE_KEY.COLLAPSED);
    if (dismissed) {
      return { inputs: DISMISSED_INPUTS, dismissed, collapsed };
    }
    const inputs = await gatherOnboardingInputs(workspaceId, t);
    return { inputs, dismissed, collapsed };
  });
}

// Placeholder inputs for a dismissed widget — never rendered (the widget returns
// null when dismissed), so the values only need to be well-formed. Marking every
// step done avoids a spurious "next step" ring if the value ever leaked into a
// render before the dismissed guard.
const DISMISSED_INPUTS: OnboardingInputs = {
  modelConfigured: true,
  fleetTotal: 1,
  secretCount: 1,
  hasProcessedEvent: true,
  hasSteerEvent: true,
  cliTicked: true,
};

// Server action wrapping the preference write — the browser holds no token, so
// the widget's dismiss/collapse/CLI-tick all route through here. Returns the
// full updated bag on success; on failure the caller keeps its pre-action state
// and surfaces a retry (the fail-open direction for onboarding is always SHOW).
export async function putPreferenceAction(
  workspaceId: string,
  key: PreferenceKey,
  value: unknown,
): Promise<ActionResult<PreferenceBag>> {
  return withToken((t) => apiPutPreference(workspaceId, key, value, t));
}
