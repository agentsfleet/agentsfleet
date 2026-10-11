import { afterEach, describe, expect, it, vi } from "vitest";
import type { OnboardingStatus } from "@/lib/api/onboarding";
import { PREFERENCE_KEY } from "@/lib/api/preferences-types";
import { ERROR_CODE } from "@/lib/errors";

// Only the edges are stubbed: Clerk's session token and the two network
// clients. The real withToken resolves the token and wraps the result, and the
// real statusToInputs derives the checklist inputs.
const { getTokenMock, putPreference, getOnboardingRequired } = vi.hoisted(() => ({
  getTokenMock: vi.fn(),
  putPreference: vi.fn(),
  getOnboardingRequired: vi.fn(),
}));
vi.mock("@clerk/nextjs/server", () => ({
  auth: vi.fn(async () => ({ getToken: getTokenMock })),
}));
vi.mock("@/lib/api/preferences", () => ({
  putPreference: (...a: unknown[]) => putPreference(...a),
}));
vi.mock("@/lib/api/onboarding", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api/onboarding")>()),
  getOnboardingRequired: (...a: unknown[]) => getOnboardingRequired(...a),
}));

import { getOnboardingProgressAction, putPreferenceAction } from "./preferences";

const TOKEN = "tok_session";
const WORKSPACE_ID = "ws_1";
const STATUS: OnboardingStatus = {
  model_configured: true, has_fleet: false, has_secret: true,
  has_processed_event: false, has_steer_event: true,
  cli_ticked: true, dismissed: true, collapsed: false,
};

afterEach(() => {
  getTokenMock.mockReset();
  putPreference.mockReset();
  getOnboardingRequired.mockReset();
});

describe("putPreferenceAction", () => {
  it("writes the key and value for the workspace under the session token and returns the stored bag", async () => {
    getTokenMock.mockResolvedValue(TOKEN);
    const bag = { [PREFERENCE_KEY.COLLAPSED]: false };
    putPreference.mockResolvedValue(bag);

    const r = await putPreferenceAction(WORKSPACE_ID, PREFERENCE_KEY.COLLAPSED, false);

    expect(r).toEqual({ ok: true, data: bag });
    expect(putPreference).toHaveBeenCalledWith(WORKSPACE_ID, PREFERENCE_KEY.COLLAPSED, false, TOKEN);
  });
});

describe("getOnboardingProgressAction", () => {
  it("derives the checklist inputs and carries the widget's dismissed and collapsed flags from one read", async () => {
    getTokenMock.mockResolvedValue(TOKEN);
    getOnboardingRequired.mockResolvedValue(STATUS);

    const r = await getOnboardingProgressAction(WORKSPACE_ID);

    expect(r).toEqual({
      ok: true,
      data: {
        inputs: {
          modelConfigured: true,
          fleetTotal: 0,
          secretCount: 1,
          hasProcessedEvent: false,
          hasSteerEvent: true,
          cliTicked: true,
        },
        dismissed: true,
        collapsed: false,
      },
    });
    expect(getOnboardingRequired).toHaveBeenCalledWith(WORKSPACE_ID, TOKEN);
  });

  it("follows the read when the flags flip, rather than holding a default", async () => {
    getTokenMock.mockResolvedValue(TOKEN);
    getOnboardingRequired.mockResolvedValue({ ...STATUS, dismissed: false, collapsed: true });

    const r = await getOnboardingProgressAction(WORKSPACE_ID);

    expect(r.ok && r.data.dismissed).toBe(false);
    expect(r.ok && r.data.collapsed).toBe(true);
  });

  it("returns a failed action carrying the read's error when the progress read fails", async () => {
    getTokenMock.mockResolvedValue(TOKEN);
    getOnboardingRequired.mockRejectedValue(new Error("down"));

    await expect(getOnboardingProgressAction(WORKSPACE_ID)).resolves.toEqual({ ok: false, error: "down" });
  });

  it("refuses without a session and never reads the workspace", async () => {
    getTokenMock.mockResolvedValue(null);

    const r = await getOnboardingProgressAction(WORKSPACE_ID);

    expect(r).toMatchObject({ ok: false, status: 401, errorCode: ERROR_CODE.AUTH_401 });
    expect(getOnboardingRequired).not.toHaveBeenCalled();
  });
});
