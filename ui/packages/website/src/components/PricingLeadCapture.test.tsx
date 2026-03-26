import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const config = vi.hoisted(() => ({
  MARKETING_LEAD_CAPTURE_URL: "",
  TEAM_EMAIL: "team@usezombie.com",
}));

const analytics = vi.hoisted(() => ({
  trackLeadCaptureFailed: vi.fn(),
  trackLeadCaptureSubmitted: vi.fn(),
}));

vi.mock("../config", () => config);
vi.mock("../analytics/posthog", () => analytics);

import PricingLeadCapture from "./PricingLeadCapture";

const SCALE_INTENT = {
  ctaId: "pricing_scale_upgrade",
  planInterest: "Scale",
  title: "Get notified about Scale",
  description: "We will share rollout details for the Scale plan.",
  actionLabel: "Notify me",
};

describe("PricingLeadCapture", () => {
  beforeEach(() => {
    analytics.trackLeadCaptureFailed.mockReset();
    analytics.trackLeadCaptureSubmitted.mockReset();
    config.MARKETING_LEAD_CAPTURE_URL = "";
    vi.unstubAllGlobals();
    window.history.pushState({}, "", "/pricing");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders a placeholder card when there is no active intent", () => {
    render(<PricingLeadCapture intent={null} />);
    expect(screen.getByText(/choose a paid plan/i)).toBeInTheDocument();
  });

  it("resets the form when the pricing intent changes", () => {
    const { rerender } = render(<PricingLeadCapture intent={SCALE_INTENT} />);
    const input = screen.getByLabelText(/work email/i);

    fireEvent.change(input, { target: { value: "person@example.com" } });
    expect(input).toHaveValue("person@example.com");

    rerender(
      <PricingLeadCapture
        intent={{
          ...SCALE_INTENT,
          ctaId: "pricing_scale_followup",
          title: "Tell me when the next rollout opens",
        }}
      />,
    );

    expect(screen.getByLabelText(/work email/i)).toHaveValue("");
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent(/next rollout opens/i);
  });

  it("shows a configuration error and tracks missing endpoint failures", async () => {
    render(<PricingLeadCapture intent={SCALE_INTENT} />);

    fireEvent.change(screen.getByLabelText(/work email/i), { target: { value: "person@example.com" } });
    fireEvent.submit(screen.getByRole("button", { name: /notify me/i }).closest("form")!);

    expect(await screen.findByRole("alert")).toHaveTextContent(/notify me is not configured yet/i);
    expect(analytics.trackLeadCaptureFailed).toHaveBeenCalledWith({
      page: "pricing",
      surface: "pricing_lead_capture",
      cta_id: "pricing_scale_upgrade",
      plan_interest: "Scale",
      status: "missing_endpoint",
    });
  });

  it("submits lead capture payloads with UTM attribution and shows success state", async () => {
    config.MARKETING_LEAD_CAPTURE_URL = "https://example.com/lead";
    const fetchMock = vi.fn().mockResolvedValue({ ok: true });
    vi.stubGlobal("fetch", fetchMock);
    window.history.pushState({}, "", "/pricing?utm_source=ad&utm_medium=email&utm_campaign=launch");

    render(<PricingLeadCapture intent={SCALE_INTENT} />);

    fireEvent.change(screen.getByLabelText(/work email/i), { target: { value: "person@example.com" } });
    fireEvent.submit(screen.getByRole("button", { name: /notify me/i }).closest("form")!);

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(1);
    });
    expect(fetchMock).toHaveBeenCalledWith("https://example.com/lead", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: expect.any(String),
    });

    const request = fetchMock.mock.calls[0]?.[1] as { body: string };
    expect(JSON.parse(request.body)).toEqual(
      expect.objectContaining({
        email: "person@example.com",
        page: "pricing",
        cta_id: "pricing_scale_upgrade",
        plan_interest: "Scale",
        utm_source: "ad",
        utm_medium: "email",
        utm_campaign: "launch",
      }),
    );

    expect(await screen.findByRole("heading", { level: 3, name: /saved your interest for/i })).toHaveTextContent("Scale");
    expect(analytics.trackLeadCaptureSubmitted).toHaveBeenCalledWith({
      page: "pricing",
      surface: "pricing_lead_capture",
      cta_id: "pricing_scale_upgrade",
      plan_interest: "Scale",
      status: "success",
      utm_source: "ad",
      utm_medium: "email",
      utm_campaign: "launch",
    });
  });

  it("shows a retry error when the submit request fails", async () => {
    config.MARKETING_LEAD_CAPTURE_URL = "https://example.com/lead";
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 500 }));

    render(<PricingLeadCapture intent={SCALE_INTENT} />);

    fireEvent.change(screen.getByLabelText(/work email/i), { target: { value: "person@example.com" } });
    fireEvent.submit(screen.getByRole("button", { name: /notify me/i }).closest("form")!);

    expect(await screen.findByRole("alert")).toHaveTextContent(/something went wrong/i);
    expect(analytics.trackLeadCaptureFailed).toHaveBeenCalledWith({
      page: "pricing",
      surface: "pricing_lead_capture",
      cta_id: "pricing_scale_upgrade",
      plan_interest: "Scale",
      status: "submit_failed",
      utm_source: "",
      utm_medium: "",
      utm_campaign: "",
    });
  });
});
