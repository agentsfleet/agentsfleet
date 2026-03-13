import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import posthog from "posthog-js";
import CTABlock from "../components/CTABlock";
import {
  EVENT_SIGNUP_STARTED,
  EVENT_TEAM_PILOT_BOOKING_STARTED,
  resetAnalyticsForTests,
  trackSignupStarted,
} from "./posthog";

vi.mock("posthog-js", () => ({
  default: {
    init: vi.fn(),
    capture: vi.fn(),
  },
}));

const mockedPosthog = vi.mocked(posthog, true);

describe("website analytics", () => {
  beforeEach(() => {
    resetAnalyticsForTests();
    mockedPosthog.init.mockReset();
    mockedPosthog.capture.mockReset();
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: true,
      key: "phc_test_key",
      host: "https://us.i.posthog.com",
    };
  });

  afterEach(() => {
    delete (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__;
  });

  it("emits signup_started with privacy-safe allowlisted properties", () => {
    trackSignupStarted({
      source: "hero_primary",
      surface: "hero",
      mode: "humans",
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      email: "should-not-leak@example.com" as any,
    });

    expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.capture).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_SIGNUP_STARTED,
      expect.objectContaining({
        source: "hero_primary",
        surface: "hero",
        mode: "humans",
      }),
    );
    const props = mockedPosthog.capture.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(props.email).toBeUndefined();
  });

  it("captures CTA click events in website flow", async () => {
    const user = userEvent.setup();
    render(<CTABlock />);

    await user.click(screen.getByRole("link", { name: /start free/i }));
    await user.click(screen.getByRole("link", { name: /book team pilot/i }));

    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "signup_completed",
      expect.objectContaining({ source: "cta_block_start_free" }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_TEAM_PILOT_BOOKING_STARTED,
      expect.objectContaining({ source: "cta_block_team_pilot" }),
    );
  });

  it("does not emit when analytics is disabled", async () => {
    const user = userEvent.setup();
    resetAnalyticsForTests();
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: false,
      key: "phc_test_key",
      host: "https://us.i.posthog.com",
    };

    render(<CTABlock />);
    await user.click(screen.getByRole("link", { name: /start free/i }));

    expect(mockedPosthog.init).not.toHaveBeenCalled();
    expect(mockedPosthog.capture).not.toHaveBeenCalled();
  });
});
