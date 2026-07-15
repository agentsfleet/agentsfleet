import { request } from "./client";
import type { OnboardingInputs } from "@/lib/onboarding";

// The consolidated onboarding endpoint's response — every checklist signal in
// one shape. Five are derived server-side (in one query); three are the stored
// preferences. This is the single call that replaced the old six-read gather.
export type OnboardingStatus = {
  model_configured: boolean;
  has_fleet: boolean;
  has_secret: boolean;
  has_processed_event: boolean;
  has_steer_event: boolean;
  cli_ticked: boolean;
  dismissed: boolean;
  collapsed: boolean;
};

// Fail-open default: every signal false and NOT dismissed, so a failed read
// shows onboarding rather than hiding it (Invariant 3). "I couldn't read your
// onboarding state" must look like "you've done nothing yet", never "you're
// done" or "you dismissed it".
const FAILOPEN_STATUS: OnboardingStatus = {
  model_configured: false,
  has_fleet: false,
  has_secret: false,
  has_processed_event: false,
  has_steer_event: false,
  cli_ticked: false,
  dismissed: false,
  collapsed: false,
};

export async function getOnboarding(
  workspaceId: string,
  token: string,
): Promise<OnboardingStatus> {
  try {
    return await request<OnboardingStatus>(
      `/v1/workspaces/${workspaceId}/onboarding`,
      { method: "GET" },
      token,
    );
  } catch {
    return FAILOPEN_STATUS;
  }
}

// Maps the endpoint's booleans onto the pure derivation's inputs. The derivation
// still speaks counts (`fleetTotal >= 1`), so a present/absent boolean becomes
// 1/0 — the checklist only ever cares whether the count crossed one.
export function statusToInputs(s: OnboardingStatus): OnboardingInputs {
  return {
    modelConfigured: s.model_configured,
    fleetTotal: s.has_fleet ? 1 : 0,
    secretCount: s.has_secret ? 1 : 0,
    hasProcessedEvent: s.has_processed_event,
    hasSteerEvent: s.has_steer_event,
    cliTicked: s.cli_ticked,
  };
}
