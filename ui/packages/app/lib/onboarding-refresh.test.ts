import { describe, expect, it, vi } from "vitest";
import {
  requestOnboardingRefresh,
  subscribeOnboardingRefresh,
} from "./onboarding-refresh";

describe("onboarding refresh subscriptions", () => {
  it("keeps remaining listeners subscribed until the final listener leaves", () => {
    const first = vi.fn();
    const second = vi.fn();
    const unsubscribeFirst = subscribeOnboardingRefresh("ws_1", first);
    const unsubscribeSecond = subscribeOnboardingRefresh("ws_1", second);

    unsubscribeFirst();
    requestOnboardingRefresh("ws_1");
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);

    unsubscribeSecond();
    requestOnboardingRefresh("ws_1");
    expect(second).toHaveBeenCalledTimes(1);
  });
});
