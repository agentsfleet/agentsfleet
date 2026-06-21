import { render, screen, within } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Home from "./Home";
import {
  FLEET_PILLARS,
  CAPABILITY_ITEMS,
  HERO_HEADLINE,
  HOW_IT_WORKS_HEADING,
  KNOWLEDGE_POINTS,
  LOOP_STEPS,
  PREBUILT_FLEETS,
  OPERATIONAL_KNOWLEDGE_HEADING,
  PRICING_COPY,
  RUNTIME_GUARANTEES_LABEL,
} from "../lib/marketing-copy";
import { RATES_DISPLAY } from "../lib/rates";

function renderHome() {
  return render(
    <BrowserRouter>
      <Home />
    </BrowserRouter>
  );
}

function expectDocumentOrder(first: HTMLElement, second: HTMLElement) {
  expect(
    first.compareDocumentPosition(second) & globalThis.Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
}

describe("Home", () => {
  it("renders the resident-engineer hero headline", () => {
    renderHome();
    const h1 = screen.getByRole("heading", { level: 1 });
    expect(h1).toHaveTextContent(HERO_HEADLINE);
  });

  it("renders the hero lede in the warm teammates voice", () => {
    renderHome();
    const hero = screen.getByTestId("hero");
    expect(within(hero).getByText("AI teammates")).toBeInTheDocument();
    expect(within(hero).getByText("recurring engineering work")).toBeInTheDocument();
    expect(hero.textContent).toMatch(/hand you the change to approve/i);
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

  it("does not render Talk to us in the hero", () => {
    renderHome();
    expect(
      within(screen.getByTestId("hero")).queryByRole("link", { name: /talk to us/i }),
    ).not.toBeInTheDocument();
  });

  it("no longer renders the removed install-via Terminal in the hero", () => {
    renderHome();
    expect(screen.queryByLabelText(/install via agentsfleet\.dev/i)).not.toBeInTheDocument();
  });

  it("does not mount the retired standalone onboarding section", () => {
    renderHome();
    expect(screen.queryByTestId("onboarding-flow")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { level: 3, name: "Install the command-line interface" }),
    ).not.toBeInTheDocument();
  });

  it("renders core capabilities in the first post-hero slot", () => {
    renderHome();
    const hero = screen.getByTestId("hero");
    const capabilities = screen.getByTestId("core-capabilities");
    const fleet = screen.getByTestId("prebuilt-fleets");
    expectDocumentOrder(hero, capabilities);
    expectDocumentOrder(capabilities, fleet);
  });

  it("renders the prebuilt Fleet catalogue before How it works", () => {
    renderHome();
    const fleet = screen.getByTestId("prebuilt-fleets");
    const howItWorks = screen.getByTestId("how-it-works");
    expectDocumentOrder(fleet, howItWorks);
    for (const fleet of PREBUILT_FLEETS) {
      expect(screen.getByTestId(`fleet-card-${fleet.id}`)).toHaveTextContent(fleet.name);
    }
    expect(screen.getByTestId("fleet-card-coming-soon")).toBeInTheDocument();
  });

  it("renders How it works with the three-beat flow", () => {
    renderHome();
    expect(screen.getByText(HOW_IT_WORKS_HEADING)).toBeInTheDocument();
    for (const step of LOOP_STEPS) {
      expect(screen.getByText(step.title)).toBeInTheDocument();
    }
  });

  it("moves operational knowledge below How it works", () => {
    renderHome();
    const howItWorks = screen.getByTestId("how-it-works");
    const operationalKnowledge = screen.getByTestId("operational-knowledge");
    expectDocumentOrder(howItWorks, operationalKnowledge);
    expect(screen.getByText(OPERATIONAL_KNOWLEDGE_HEADING)).toBeInTheDocument();
    for (const point of KNOWLEDGE_POINTS) {
      expect(screen.getByText(point.title)).toBeInTheDocument();
    }
  });

  it("renders core capabilities — the three pillars plus the trust primitives", () => {
    renderHome();
    const capabilities = screen.getByTestId("core-capabilities");
    expect(within(capabilities).getByText(/core capabilities/i)).toBeInTheDocument();
    expect(
      within(capabilities).getByRole("heading", {
        level: 3,
        name: RUNTIME_GUARANTEES_LABEL,
      }),
    ).toBeInTheDocument();
    for (const pillar of FLEET_PILLARS) {
      expect(screen.getByTestId(`capability-pillar-${pillar.id}`)).toHaveTextContent(
        pillar.title,
      );
    }
    for (const item of CAPABILITY_ITEMS) {
      expect(within(capabilities).getAllByText(item.title).length).toBeGreaterThan(0);
    }
  });

  it("does not render a duplicate install block below pricing", () => {
    renderHome();
    // The old standalone InstallBlock below pricing was redundant with the
    // loop section at the top of the page; it must be gone.
    expect(
      screen.queryByRole("heading", { level: 2, name: /install agentsfleet, then run/i }),
    ).not.toBeInTheDocument();
  });

  it("embeds the Pricing block below How it works", () => {
    renderHome();
    expect(screen.getByTestId("pricing-block")).toBeInTheDocument();
    expect(screen.getByText(PRICING_COPY.headline)).toBeInTheDocument();
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
