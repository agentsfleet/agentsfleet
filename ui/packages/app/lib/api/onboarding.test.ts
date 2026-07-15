import { afterEach, describe, expect, it, vi } from "vitest";

const requestMock = vi.fn();
vi.mock("./client", () => ({ request: (...a: unknown[]) => requestMock(...a) }));

import { getOnboarding, statusToInputs, type OnboardingStatus } from "./onboarding";

const FULL: OnboardingStatus = {
  model_configured: true,
  has_fleet: true,
  has_secret: true,
  has_processed_event: true,
  has_steer_event: true,
  cli_ticked: true,
  dismissed: false,
  collapsed: true,
};

afterEach(() => requestMock.mockReset());

describe("getOnboarding", () => {
  it("returns the server status on success", async () => {
    requestMock.mockResolvedValue(FULL);
    const s = await getOnboarding("ws_1", "tok");
    expect(s).toEqual(FULL);
    expect(requestMock).toHaveBeenCalledWith(
      "/v1/workspaces/ws_1/onboarding",
      { method: "GET" },
      "tok",
    );
  });

  it("fails open — a thrown request yields an undismissed, all-false status", async () => {
    requestMock.mockRejectedValue(new Error("down"));
    const s = await getOnboarding("ws_1", "tok");
    expect(s.dismissed).toBe(false);
    expect(s.model_configured).toBe(false);
    expect(s.has_fleet).toBe(false);
    expect(s.cli_ticked).toBe(false);
  });
});

describe("statusToInputs", () => {
  it("maps present booleans to counts and passes the rest through", () => {
    expect(statusToInputs(FULL)).toEqual({
      modelConfigured: true,
      fleetTotal: 1,
      secretCount: 1,
      hasProcessedEvent: true,
      hasSteerEvent: true,
      cliTicked: true,
    });
  });

  it("maps absent product state to zero counts / false", () => {
    const empty: OnboardingStatus = {
      ...FULL,
      has_fleet: false,
      has_secret: false,
      model_configured: false,
      hasProcessedEvent: false,
    } as OnboardingStatus;
    const inputs = statusToInputs(empty);
    expect(inputs.fleetTotal).toBe(0);
    expect(inputs.secretCount).toBe(0);
    expect(inputs.modelConfigured).toBe(false);
  });
});
