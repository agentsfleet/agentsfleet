import { afterEach, describe, expect, it, vi } from "vitest";
import type { OnboardingStatus } from "@/lib/api/onboarding";

// withToken normally resolves the Clerk token; here it just hands the fn a stub
// token and wraps the result, so the real action bodies run under test.
vi.mock("@/lib/actions/with-token", () => ({
  withToken: async (fn: (t: string) => Promise<unknown>) => {
    try {
      return { ok: true, data: await fn("tok") };
    } catch (error) {
      return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
  },
}));
const putPreference = vi.fn();
vi.mock("@/lib/api/preferences", () => ({
  putPreference: (...a: unknown[]) => putPreference(...a),
  PREFERENCE_KEY: { DISMISSED: "getting_started_dismissed", COLLAPSED: "getting_started_collapsed", CLI_TICKED: "getting_started_cli_ticked" },
}));
const getOnboardingRequired = vi.fn();
vi.mock("@/lib/api/onboarding", () => ({
  getOnboardingRequired: (...a: unknown[]) => getOnboardingRequired(...a),
  statusToInputs: (s: OnboardingStatus) => ({
    modelConfigured: s.model_configured,
    fleetTotal: s.has_fleet ? 1 : 0,
    secretCount: s.has_secret ? 1 : 0,
    hasProcessedEvent: s.has_processed_event,
    hasSteerEvent: s.has_steer_event,
    cliTicked: s.cli_ticked,
  }),
}));

import { getOnboardingProgressAction, putPreferenceAction } from "./preferences";

const STATUS: OnboardingStatus = {
  model_configured: true, has_fleet: false, has_secret: true,
  has_processed_event: false, has_steer_event: false,
  cli_ticked: true, dismissed: true, collapsed: false,
};

afterEach(() => { putPreference.mockReset(); getOnboardingRequired.mockReset(); });

describe("putPreferenceAction", () => {
  it("writes through the preferences client and wraps the result", async () => {
    putPreference.mockResolvedValue({ getting_started_dismissed: true });
    const r = await putPreferenceAction("ws_1", "getting_started_dismissed", true);
    expect(r.ok).toBe(true);
    expect(putPreference).toHaveBeenCalledWith("ws_1", "getting_started_dismissed", true, "tok");
  });
});

describe("getOnboardingProgressAction", () => {
  it("returns inputs + dismissed + collapsed from the one onboarding call", async () => {
    getOnboardingRequired.mockResolvedValue(STATUS);
    const r = await getOnboardingProgressAction("ws_1");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.data.dismissed).toBe(true);
      expect(r.data.collapsed).toBe(false);
      expect(r.data.inputs.modelConfigured).toBe(true);
      expect(r.data.inputs.fleetTotal).toBe(0);
      expect(r.data.inputs.secretCount).toBe(1);
    }
    expect(getOnboardingRequired).toHaveBeenCalledWith("ws_1", "tok");
  });

  it("returns a failed action when the live progress read fails", async () => {
    getOnboardingRequired.mockRejectedValue(new Error("down"));
    await expect(getOnboardingProgressAction("ws_1")).resolves.toEqual({
      ok: false,
      error: "down",
    });
  });
});
