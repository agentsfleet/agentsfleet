import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Agents from "./Agents";

function renderAgents() {
  return render(
    <BrowserRouter>
      <Agents />
    </BrowserRouter>
  );
}

describe("Agents", () => {
  it("renders the agent-first heading", () => {
    renderAgents();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      /this page is for autonomous agents/i,
    );
  });

  it("renders the canonical surface note", () => {
    renderAgents();
    expect(screen.getByText(/canonical surface/i)).toBeInTheDocument();
  });

  it("renders the install block with npm command", () => {
    renderAgents();
    expect(screen.getByRole("heading", { name: /install agentsfleet/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/install agentsfleet command/i)).toHaveTextContent(
      /npm install -g @usezombie\/zombiectl/,
    );
  });

  it("renders install block action buttons", () => {
    renderAgents();
    expect(screen.getByRole("link", { name: /read the docs/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );
    expect(screen.getByRole("link", { name: /open dashboard/i })).toBeInTheDocument();
  });

  it("renders bootstrap commands", () => {
    renderAgents();
    const block = screen.getByLabelText(/bootstrap commands/i);
    expect(block).toBeInTheDocument();
    expect(block).toHaveTextContent(/npm install -g @usezombie\/zombiectl/);
    expect(block).toHaveTextContent(/agentsfleet login/);
    expect(block).toHaveTextContent(/npx skills add usezombie\/skills/);
    expect(block).toHaveTextContent(/usezombie-install-platform-ops/);
  });

  it("renders machine surface table", () => {
    renderAgents();
    expect(screen.getByRole("heading", { name: /machine surface/i })).toBeInTheDocument();
    expect(screen.getByTestId("agents-openapi-link")).toHaveAttribute("href", "/openapi.json");
  });

  it("renders API operations table", () => {
    renderAgents();
    expect(screen.getByRole("heading", { name: /api operations/i })).toBeInTheDocument();
    expect(screen.getByText("Create agent")).toBeInTheDocument();
    expect(screen.getByText("Update agent")).toBeInTheDocument();
    expect(screen.getByText("Stop agent")).toBeInTheDocument();
    expect(screen.getByText("Resume agent")).toBeInTheDocument();
    expect(screen.getByText("Kill agent")).toBeInTheDocument();
    expect(screen.getByText("Delete agent")).toBeInTheDocument();
    expect(screen.getByText("Steer / chat")).toBeInTheDocument();
    expect(screen.getByText("Stream events")).toBeInTheDocument();
    expect(screen.getByText("Ingest webhook")).toBeInTheDocument();
  });

  it("renders HTTP methods", () => {
    renderAgents();
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
    renderAgents();
    expect(screen.getByRole("heading", { name: /webhook ingest example/i })).toBeInTheDocument();
    expect(screen.getByText(/deploy\.failed/)).toBeInTheDocument();
  });

  it("renders safety limits cards", () => {
    renderAgents();
    expect(screen.getByText(/^idempotency$/i)).toBeInTheDocument();
    expect(screen.getByText(/^audit trail$/i)).toBeInTheDocument();
    expect(screen.getByText(/^secret management$/i)).toBeInTheDocument();
    expect(screen.getByText(/^policy enforcement$/i)).toBeInTheDocument();
  });

  it("renders JSON-LD script", () => {
    const { container } = renderAgents();
    const script = container.querySelector('script[type="application/ld+json"]');
    expect(script).not.toBeNull();
    const data = JSON.parse(script!.textContent!);
    expect(data["@type"]).toBe("SoftwareApplication");
    expect(data.name).toBe("usezombie");
  });

  it("does not render orange-era decorative chrome", () => {
    const { container } = renderAgents();
    expect(container.querySelector(".scanline")).toBeNull();
    expect(container.querySelector(".agent-surface")).toBeNull();
    expect(container.querySelector(".agent-table")).toBeNull();
  });
});
