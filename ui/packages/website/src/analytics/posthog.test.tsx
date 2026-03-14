import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import posthog from "posthog-js";
import CTABlock from "../components/CTABlock";
import {
  EVENT_LEAD_CAPTURE_CLICKED,
  EVENT_LEAD_CAPTURE_FAILED,
  EVENT_LEAD_CAPTURE_OPENED,
  EVENT_LEAD_CAPTURE_SUBMITTED,
  EVENT_NAVIGATION_CLICKED,
  EVENT_SIGNUP_STARTED,
  resetAnalyticsForTests,
  trackLeadCaptureClicked,
  trackLeadCaptureFailed,
  trackLeadCaptureOpened,
  trackLeadCaptureSubmitted,
  trackSignupStarted,
} from "./posthog";

function renderCtaBlock() {
  return render(
    <BrowserRouter>
      <CTABlock />
    </BrowserRouter>
  );
}

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

  it("captures agent-safe CTA navigation events", async () => {
    const user = userEvent.setup();
    renderCtaBlock();

    await user.click(screen.getByRole("link", { name: /read quickstart/i }));
    await user.click(screen.getByRole("link", { name: /view pricing/i }));

    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_NAVIGATION_CLICKED,
      expect.objectContaining({ source: "agents_cta_docs" }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_NAVIGATION_CLICKED,
      expect.objectContaining({ source: "agents_cta_pricing" }),
    );
  });

  it("captures pricing lead funnel events with allowlisted metadata only", () => {
    trackLeadCaptureClicked({
      page: "pricing",
      surface: "pricing_card",
      cta_id: "pricing_scale_notify",
      plan_interest: "Scale",
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      email: "should-not-leak@example.com" as any,
    });
    trackLeadCaptureOpened({
      page: "pricing",
      surface: "pricing_lead_capture",
      cta_id: "pricing_scale_notify",
      plan_interest: "Scale",
    });
    trackLeadCaptureSubmitted({
      page: "pricing",
      surface: "pricing_lead_capture",
      cta_id: "pricing_scale_notify",
      plan_interest: "Scale",
      status: "success",
    });
    trackLeadCaptureFailed({
      page: "pricing",
      surface: "pricing_lead_capture",
      cta_id: "pricing_scale_notify",
      plan_interest: "Scale",
      status: "submit_failed",
    });

    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_LEAD_CAPTURE_CLICKED,
      expect.objectContaining({ cta_id: "pricing_scale_notify", plan_interest: "Scale" }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_LEAD_CAPTURE_OPENED,
      expect.objectContaining({ cta_id: "pricing_scale_notify", plan_interest: "Scale" }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_LEAD_CAPTURE_SUBMITTED,
      expect.objectContaining({ status: "success" }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      EVENT_LEAD_CAPTURE_FAILED,
      expect.objectContaining({ status: "submit_failed" }),
    );

    const props = mockedPosthog.capture.mock.calls[0]?.[1] as Record<string, unknown>;
    expect(props.email).toBeUndefined();
  });

  it("does not emit when analytics is disabled", async () => {
    const user = userEvent.setup();
    resetAnalyticsForTests();
    (globalThis as { __UZ_ANALYTICS_CONFIG__?: unknown }).__UZ_ANALYTICS_CONFIG__ = {
      enabled: false,
      key: "phc_test_key",
      host: "https://us.i.posthog.com",
    };

    renderCtaBlock();
    await user.click(screen.getByRole("link", { name: /read quickstart/i }));

    expect(mockedPosthog.init).not.toHaveBeenCalled();
    expect(mockedPosthog.capture).not.toHaveBeenCalled();
  });
});
