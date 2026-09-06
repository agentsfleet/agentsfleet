import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Fleets from "./Fleets";

function renderFleets() {
  return render(
    <BrowserRouter>
      <Fleets />
    </BrowserRouter>
  );
}

describe("Fleets", () => {
  it("renders the Fleet-first heading", () => {
    renderFleets();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      /this page is for agents/i,
    );
  });

  it("renders the canonical surface note", () => {
    renderFleets();
    expect(screen.getByText(/canonical surface/i)).toBeInTheDocument();
  });

  it("renders the merged install heading and npm command", () => {
    renderFleets();
    // One install story: the "Install agentsfleet" heading sits over the
    // bootstrap terminal (CLI + skills + slash command) — the old standalone
    // InstallBlock was folded in.
    expect(screen.getByRole("heading", { name: /install agentsfleet/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/bootstrap commands/i)).toHaveTextContent(
      /npm install -g @agentsfleet\/cli/,
    );
  });

  it("renders install action links and no dashboard link", () => {
    renderFleets();
    expect(screen.getByRole("link", { name: /start a Fleet/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net/quickstart",
    );
    expect(screen.getByRole("link", { name: /read the docs/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );
    // "open dashboard" was removed — no dashboard link on the Fleet surface.
    expect(screen.queryByRole("link", { name: /open dashboard/i })).toBeNull();
  });

  it("renders bootstrap commands", () => {
    renderFleets();
    const block = screen.getByLabelText(/bootstrap commands/i);
    expect(block).toBeInTheDocument();
    expect(block).toHaveTextContent(/npm install -g @agentsfleet\/cli/);
    expect(block).toHaveTextContent(/agentsfleet login/);
    expect(block).toHaveTextContent(/npx skills add agentsfleet\/skills/);
    expect(block).toHaveTextContent("Create a fleet for incident response in my workspace.");
    expect(block).not.toHaveTextContent("/agentsfleet-install-platform-ops");
    expect(block).toHaveTextContent("curl -fsSL https://agentsfleet.dev | bash");
  });

  it("renders machine surface table", () => {
    renderFleets();
    expect(screen.getByRole("heading", { name: /machine surface/i })).toBeInTheDocument();
    expect(screen.getByTestId("fleets-openapi-link")).toHaveAttribute("href", "/openapi.json");
  });







  it("renders safety limits cards", () => {
    renderFleets();
    expect(screen.getByText(/^idempotency$/i)).toBeInTheDocument();
    expect(screen.getByText(/^audit trail$/i)).toBeInTheDocument();
    expect(screen.getByText(/^secret management$/i)).toBeInTheDocument();
    expect(screen.getByText(/^policy enforcement$/i)).toBeInTheDocument();
  });

  it("renders JSON-LD script", () => {
    const { container } = renderFleets();
    const script = container.querySelector('script[type="application/ld+json"]');
    expect(script).not.toBeNull();
    const data = JSON.parse(script!.textContent!);
    expect(data["@type"]).toBe("SoftwareApplication");
    expect(data.name).toBe("agentsfleet");
  });

  it("does not render orange-era decorative chrome", () => {
    const { container } = renderFleets();
    expect(container.querySelector(".scanline")).toBeNull();
    expect(container.querySelector(".fleet-surface")).toBeNull();
    expect(container.querySelector(".fleet-table")).toBeNull();
  });
});

it("puts installation before API steps and removes stale sections", () => {
  renderFleets();
  const headings = screen.getAllByRole("heading").map(heading => heading.textContent);
  expect(headings.indexOf("Install agentsfleet")).toBeLessThan(headings.indexOf("Get started in four calls"));
  expect(headings).not.toContain("Webhook ingest example");
  expect(headings).not.toContain("Coming soon");
  expect(headings).not.toContain("API operations");
});
