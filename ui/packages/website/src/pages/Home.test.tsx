import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Home from "./Home";
import { RATES_DISPLAY } from "../lib/rates";

function renderHome() {
  return render(
    <BrowserRouter>
      <Home />
    </BrowserRouter>
  );
}

describe("Home", () => {
  it("renders the hero headline (two-line spec voice)", () => {
    renderHome();
    const h1 = screen.getByRole("heading", { level: 1 });
    expect(h1).toHaveTextContent(/your deploy failed/i);
    expect(h1).toHaveTextContent(/the agent already knows why/i);
  });

  it("renders the hero lede in spec voice", () => {
    renderHome();
    expect(screen.getByText(/long-lived runtime that owns one operational outcome/i)).toBeInTheDocument();
    expect(screen.getByText(/durable, replayable log/i)).toBeInTheDocument();
  });

  it("renders the install command copy-row in the hero", () => {
    renderHome();
    expect(screen.getByTestId("hero-install-command").textContent).toContain(
      "curl -fsSL https://agentsfleet.dev | bash",
    );
    const cta = screen.getByTestId("hero-cta-primary");
    expect(cta.tagName).toBe("BUTTON");
    expect(cta.textContent).toMatch(/copy/i);
  });

  it("does not render Talk to us CTA", () => {
    renderHome();
    expect(screen.queryByRole("link", { name: /talk to us/i })).not.toBeInTheDocument();
  });

  it("renders the hero install Terminal", () => {
    renderHome();
    expect(screen.getByLabelText(/install via agentsfleet\.dev/i)).toBeInTheDocument();
  });

  it("mounts the OnboardingFlow at the #onboarding-flow anchor with all four steps", () => {
    renderHome();
    expect(screen.getByTestId("onboarding-flow").getAttribute("id")).toBe(
      "onboarding-flow",
    );
    expect(screen.getByRole("heading", { level: 3, name: "Install the CLI" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Run the install skill" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Wire your trigger" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Steer your agent" })).toBeInTheDocument();
  });

  it("renders How it works section", () => {
    renderHome();
    expect(screen.getByText("How it works")).toBeInTheDocument();
    expect(screen.getByText("A trigger arrives")).toBeInTheDocument();
    expect(screen.getByText("The agent gathers evidence")).toBeInTheDocument();
    expect(screen.getByText("Diagnosis posts; the run is auditable")).toBeInTheDocument();
  });

  it("renders core capabilities on the homepage", () => {
    renderHome();
    expect(screen.getByText(/core capabilities/i)).toBeInTheDocument();
    expect(screen.getByText("Markdown-defined")).toBeInTheDocument();
    expect(screen.getByText("Self-managed key")).toBeInTheDocument();
  });

  it("does not render a duplicate install block (OnboardingFlow already covers install)", () => {
    renderHome();
    // The old standalone InstallBlock below pricing was redundant with the
    // OnboardingFlow steps at the top of the page; it must be gone.
    expect(
      screen.queryByRole("heading", { level: 2, name: /install agentsfleet, then run/i }),
    ).not.toBeInTheDocument();
  });

  it("embeds the Pricing block below How it works", () => {
    renderHome();
    expect(screen.getByTestId("pricing-block")).toBeInTheDocument();
    expect(screen.getByTestId("pricing-rate-event")).toHaveTextContent(RATES_DISPLAY.EVENT_RATE);
    expect(screen.getByTestId("pricing-rate-run")).toHaveTextContent(
      RATES_DISPLAY.RUN_RATE_PER_SEC,
    );
    expect(screen.getByTestId("pricing-rate-run-hourly")).toHaveTextContent(
      RATES_DISPLAY.RUN_RATE_PER_HOUR,
    );
  });

  it("does not render a view-full-pricing link (pricing is inline)", () => {
    renderHome();
    expect(screen.queryByRole("link", { name: /view full pricing/i })).not.toBeInTheDocument();
  });
});
