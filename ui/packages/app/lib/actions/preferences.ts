"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  putPreference as apiPutPreference,
  type PreferenceBag,
  type PreferenceKey,
} from "@/lib/api/preferences";
import { getOnboardingRequired, statusToInputs } from "@/lib/api/onboarding";
import type { OnboardingInputs } from "@/lib/onboarding";

export type OnboardingProgress = {
  inputs: OnboardingInputs;
  dismissed: boolean;
  collapsed: boolean;
};

// The client shell has no token, so the sidebar widget reads the workspace's
// onboarding progress through this server action. The endpoint returns every
// derived signal and the widget preferences in one request. A failed read does
// not invent a zeroed checklist; the client keeps the last successful result.
export async function getOnboardingProgressAction(
  workspaceId: string,
): Promise<ActionResult<OnboardingProgress>> {
  return withToken(async (t) => {
    const status = await getOnboardingRequired(workspaceId, t);
    return {
      inputs: statusToInputs(status),
      dismissed: status.dismissed,
      collapsed: status.collapsed,
    };
  });
}

// The browser holds no token, so dismiss, collapse, and command-line preference
// writes route through this server action. On failure, the caller keeps its
// pre-action state and surfaces a retry.
export async function putPreferenceAction(
  workspaceId: string,
  key: PreferenceKey,
  value: unknown,
): Promise<ActionResult<PreferenceBag>> {
  return withToken((t) => apiPutPreference(workspaceId, key, value, t));
}
