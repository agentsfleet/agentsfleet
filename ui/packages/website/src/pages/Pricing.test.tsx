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

  it("renders Hobby and Scale tiers", () => {
    renderPricing();
    expect(screen.getByRole("heading", { level: 2, name: "Hobby" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 2, name: "Scale" })).toBeInTheDocument();
  });

  it("renders Start free CTA for Hobby", () => {
    renderPricing();
    expect(screen.getByRole("link", { name: /start free/i })).toHaveAttribute(
      "href",
      "https://app.dev.usezombie.com",
    );
  });

  it("shows Free and Unlimited users in the card chrome", () => {
    renderPricing();
    expect(screen.getByText("Free")).toBeInTheDocument();
    expect(screen.getByText("Unlimited users")).toBeInTheDocument();
  });

  it("opens on-page notify flow for Scale", async () => {
    const user = userEvent.setup();
    renderPricing();

    await user.click(screen.getByRole("button", { name: /notify me/i }));

    expect(screen.getByRole("heading", { level: 2, name: /get notified when scale opens/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/work email/i)).toBeInTheDocument();
  });

  it("shows a missing endpoint error until lead capture is configured", async () => {
    const user = userEvent.setup();
    renderPricing();

    await user.click(screen.getByRole("button", { name: /notify me/i }));
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
