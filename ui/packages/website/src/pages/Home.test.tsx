import { render, screen, within } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Home from "./Home";
import {
  FLEET_PILLARS,
  CAPABILITY_ITEMS,
  HERO_HEADLINE,
  HOW_IT_WORKS_HEADING,
  PREBUILT_FLEETS,
  PRICING_COPY,
  RUNTIME_GUARANTEES_LABEL,
} from "../lib/marketing-copy";

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
    expect(within(hero).getByText("AI incident teammate")).toBeInTheDocument();
    expect(within(hero).getByText("logs, metrics, and code")).toBeInTheDocument();
    expect(hero.textContent).toMatch(/you control access and decide what ships/i);
  });

  it("hides the unavailable installer", () => {
    renderHome();
    expect(screen.queryByTestId("hero-install-command")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /copy the install command/i })).not.toBeInTheDocument();
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

  it("leads with the fleet catalogue before technical capabilities", () => {
    renderHome();
    const hero = screen.getByTestId("hero");
    const capabilities = screen.getByTestId("core-capabilities");
    const fleet = screen.getByTestId("prebuilt-fleets");
    expectDocumentOrder(hero, fleet);
    expectDocumentOrder(fleet, capabilities);
    expectDocumentOrder(screen.getByTestId("audience-section"), capabilities);
  });

  it("explains How it works before presenting the Fleet catalogue", () => {
    renderHome();
    const fleet = screen.getByTestId("prebuilt-fleets");
    const howItWorks = screen.getByTestId("how-it-works");
    expectDocumentOrder(screen.getByTestId("hero"), howItWorks);
    expectDocumentOrder(howItWorks, fleet);
    for (const fleet of PREBUILT_FLEETS) {
      expect(screen.getByTestId(`fleet-card-${fleet.id}`)).toHaveTextContent(fleet.name);
    }
    expect(screen.queryByTestId("fleet-card-coming-soon")).not.toBeInTheDocument();
    expect(screen.getByTestId("fleet-coming-soon-security-reviewer")).toHaveTextContent(/coming soon/i);
  });

  it("renders the connected incident workflow", () => {
    renderHome();
    expect(screen.getByText(HOW_IT_WORKS_HEADING)).toBeInTheDocument();
    expect(screen.getByRole("figure", { name: /incident diagnosis/i })).toHaveTextContent("Elasticsearch");
  });

  it("omits repetitive setup and operational knowledge sections", () => {
    renderHome();
    expect(screen.queryByTestId("operational-knowledge")).not.toBeInTheDocument();
    expect(screen.queryByTestId("setup-section")).not.toBeInTheDocument();
    expect(screen.queryByText(/your first fleet/i)).not.toBeInTheDocument();
    expectDocumentOrder(screen.getByTestId("core-capabilities"), screen.getByTestId("pricing-block"));
  });

  it("renders core capabilities — the three pillars plus the trust primitives", () => {
    renderHome();
    const capabilities = screen.getByTestId("core-capabilities");
    expect(within(capabilities).getByText(/core capabilities/i)).toBeInTheDocument();
    // The runtime-guarantees label names its group without claiming heading
    // rank: it sat at 12px among 20px sibling <h3>s (FINDING-M03). The group is
    // a <section aria-labelledby> pointing at it, so the accessible name holds.
    const guarantees = within(capabilities).getByRole("region", {
      name: RUNTIME_GUARANTEES_LABEL,
    });
    expect(guarantees).toBeInTheDocument();
    expect(
      within(capabilities).queryByRole("heading", { name: RUNTIME_GUARANTEES_LABEL }),
    ).not.toBeInTheDocument();
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
    expect(screen.getByTestId("pricing-block")).toHaveTextContent(PRICING_COPY.note);
    expect(screen.queryByTestId("pricing-rate-run")).not.toBeInTheDocument();
    expectDocumentOrder(screen.getByTestId("how-it-works"), screen.getByTestId("pricing-block"));
  });

  it("does not render a view-full-pricing link (pricing is inline)", () => {
    renderHome();
    expect(screen.queryByRole("link", { name: /view full pricing/i })).not.toBeInTheDocument();
  });
});
