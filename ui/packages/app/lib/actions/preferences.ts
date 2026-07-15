"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  putPreference as apiPutPreference,
  type PreferenceBag,
  type PreferenceKey,
} from "@/lib/api/preferences";
import { getOnboarding, statusToInputs } from "@/lib/api/onboarding";
import type { OnboardingInputs } from "@/lib/onboarding";

export type OnboardingSnapshot = {
  inputs: OnboardingInputs;
  dismissed: boolean;
  collapsed: boolean;
};

// The sidebar widget's own data pull — it lives in the client Shell, which has
// no token, so it fetches its snapshot through this action after it knows the
// route's workspace. Now a SINGLE onboarding call: the endpoint returns every
// derived signal AND the preferences in one request. Fail-open by construction —
// the client read defaults to an undismissed, zeroed status on error, so a read
// failure shows onboarding rather than hiding it.
export async function getOnboardingSnapshotAction(
  workspaceId: string,
): Promise<ActionResult<OnboardingSnapshot>> {
  return withToken(async (t) => {
    const status = await getOnboarding(workspaceId, t);
    return {
      inputs: statusToInputs(status),
      dismissed: status.dismissed,
      collapsed: status.collapsed,
    };
  });
}

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
