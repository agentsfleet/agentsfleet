import { afterEach, describe, expect, it, vi } from "vitest";

const requestMock = vi.fn();
vi.mock("./client", () => ({ request: (...a: unknown[]) => requestMock(...a) }));

import { putPreference, PREFERENCE_KEY } from "./preferences";

afterEach(() => requestMock.mockReset());

describe("putPreference", () => {
  it("PUTs the value as the raw body and returns the updated bag", async () => {
    requestMock.mockResolvedValue({ prefs: { [PREFERENCE_KEY.COLLAPSED]: true } });
    const bag = await putPreference("ws_1", PREFERENCE_KEY.COLLAPSED, true, "tok");
    expect(bag).toEqual({ [PREFERENCE_KEY.COLLAPSED]: true });
    expect(requestMock).toHaveBeenCalledWith(
      `/v1/workspaces/ws_1/preferences/${PREFERENCE_KEY.COLLAPSED}`,
      { method: "PUT", body: "true" },
      "tok",
    );
  });

  it("returns an empty bag when the response omits prefs", async () => {
    requestMock.mockResolvedValue({});
    const bag = await putPreference("ws_1", PREFERENCE_KEY.DISMISSED, true, "tok");
    expect(bag).toEqual({});
  });
});
