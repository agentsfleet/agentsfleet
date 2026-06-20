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
      /this page is for autonomous Fleets/i,
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
    expect(block).toHaveTextContent(/agentsfleet-install-platform-ops/);
  });

  it("renders machine surface table", () => {
    renderFleets();
    expect(screen.getByRole("heading", { name: /machine surface/i })).toBeInTheDocument();
    expect(screen.getByTestId("fleets-openapi-link")).toHaveAttribute("href", "/openapi.json");
  });

  it("renders API operations table", () => {
    renderFleets();
    expect(screen.getByRole("heading", { name: /api operations/i })).toBeInTheDocument();
    expect(screen.getByText("Create Fleet")).toBeInTheDocument();
    expect(screen.getByText("Update Fleet")).toBeInTheDocument();
    expect(screen.getByText("Stop Fleet")).toBeInTheDocument();
    expect(screen.getByText("Resume Fleet")).toBeInTheDocument();
    expect(screen.getByText("Kill Fleet")).toBeInTheDocument();
    expect(screen.getByText("Delete Fleet")).toBeInTheDocument();
    expect(screen.getByText("Steer / chat")).toBeInTheDocument();
    expect(screen.getByText("Stream events")).toBeInTheDocument();
    expect(screen.getByText("Ingest webhook")).toBeInTheDocument();
  });

  it("renders HTTP methods", () => {
    renderFleets();
    const posts = screen.getAllByText("POST");
    const gets = screen.getAllByText("GET");
    const patches = screen.getAllByText("PATCH");
    const deletes = screen.getAllByText("DELETE");
    expect(posts.length).toBeGreaterThanOrEqual(2);
    expect(gets.length).toBeGreaterThanOrEqual(1);
    expect(patches.length).toBeGreaterThanOrEqual(4);
    expect(deletes.length).toBeGreaterThanOrEqual(1);
  });

  it("renders webhook example", () => {
    renderFleets();
    expect(screen.getByRole("heading", { name: /webhook ingest example/i })).toBeInTheDocument();
    expect(screen.getByText(/deploy\.failed/)).toBeInTheDocument();
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
