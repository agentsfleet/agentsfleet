import { render, screen, within } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import userEvent from "@testing-library/user-event";
import { describe, it, expect } from "vitest";
import Pricing from "./Pricing";

function renderPricing() {
  return render(
    <BrowserRouter>
      <Pricing />
    </BrowserRouter>
  );
}

describe("Pricing", () => {
  it("renders the heading", () => {
    renderPricing();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/start free\. upgrade when you need stronger control\./i);
  });

  it("renders roadmap proof points", () => {
    renderPricing();
    expect(screen.getByText(/upcoming firecracker resource governance/i)).toBeInTheDocument();
    expect(screen.getByText(/upcoming agent scoring, failure analysis, and learning loops/i)).toBeInTheDocument();
  });

  it("renders Hobby, Core, Pro, and Enterprise tiers", () => {
    renderPricing();
    expect(screen.getByText("Hobby")).toBeInTheDocument();
    expect(screen.getByText("Core")).toBeInTheDocument();
    expect(screen.getByText("Pro")).toBeInTheDocument();
    expect(screen.getByText("Enterprise")).toBeInTheDocument();
  });

  it("renders Start free CTA for Hobby", () => {
    renderPricing();
    expect(screen.getByRole("link", { name: /start free/i })).toHaveAttribute(
      "href",
      "https://app.dev.usezombie.com",
    );
  });

  it("opens on-page notify flow for Core", async () => {
    const user = userEvent.setup();
    renderPricing();

    await user.click(screen.getAllByRole("button", { name: /notify me/i })[0]);

    expect(screen.getByRole("heading", { level: 2, name: /get notified when core opens/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/work email/i)).toBeInTheDocument();
  });

  it("keeps the notify flow on-site for Enterprise", async () => {
    const user = userEvent.setup();
    renderPricing();

    await user.click(screen.getByRole("button", { name: /talk to sales/i }));

    expect(screen.getByRole("heading", { level: 2, name: /talk to sales about enterprise/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /request follow-up/i })).toBeInTheDocument();
  });

  it("shows a missing endpoint error until lead capture is configured", async () => {
    const user = userEvent.setup();
    renderPricing();

    await user.click(screen.getAllByRole("button", { name: /notify me/i })[0]);
    await user.type(screen.getByLabelText(/work email/i), "team@example.com");
    const form = screen.getByLabelText(/work email/i).closest("form");
    expect(form).not.toBeNull();
    await user.click(within(form!).getByRole("button", { name: /^notify me$/i }));

    expect(screen.getByRole("alert")).toHaveTextContent(/notify me is not configured yet/i);
  });

  it("renders launch posture copy", () => {
    renderPricing();
    expect(screen.getByText(/only pricing captures demand/i)).toBeInTheDocument();
  });

  it("renders FAQ section", () => {
    renderPricing();
    expect(screen.getByText("What does BYOK mean?")).toBeInTheDocument();
  });
});
