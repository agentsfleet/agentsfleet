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
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(/autonomous agents/i);
  });

  it("renders the canonical surface note", () => {
    renderAgents();
    expect(screen.getByText(/canonical surface/i)).toBeInTheDocument();
  });

  it("renders the install block with npm command", () => {
    renderAgents();
    expect(screen.getByRole("heading", { name: "Install Zombiectl" })).toBeInTheDocument();
    expect(screen.getByLabelText(/install zombiectl command/i)).toHaveTextContent(
      /npm install -g @usezombie\/zombiectl/
    );
  });

  it("renders install block action buttons", () => {
    renderAgents();
    expect(screen.getByRole("link", { name: "Read the docs" })).toHaveAttribute(
      "href",
      "https://docs.usezombie.com"
    );
    expect(screen.getByRole("link", { name: "Setup your personal dashboard" })).toBeInTheDocument();
  });

  it("renders bootstrap commands", () => {
    renderAgents();
    const block = screen.getByLabelText(/bootstrap commands/i);
    expect(block).toBeInTheDocument();
    expect(block).toHaveTextContent(/npm install -g @usezombie\/zombiectl/);
    expect(block).toHaveTextContent(/zombiectl login/);
    expect(block).toHaveTextContent(/npx skills add usezombie\/usezombie/);
    expect(block).toHaveTextContent(/usezombie-install-platform-ops/);
  });

  it("renders machine surface table", () => {
    renderAgents();
    expect(screen.getByText("Machine Surface")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "/openapi.json" })).toHaveAttribute("href", "/openapi.json");
  });

  it("renders API operations table", () => {
    renderAgents();
    expect(screen.getByText("API Operations")).toBeInTheDocument();
    expect(screen.getByText("Create agent")).toBeInTheDocument();
    expect(screen.getByText("Update agent")).toBeInTheDocument();
    expect(screen.getByText("Stop agent")).toBeInTheDocument();
    expect(screen.getByText("Resume agent")).toBeInTheDocument();
    expect(screen.getByText("Kill agent")).toBeInTheDocument();
    expect(screen.getByText("Delete agent")).toBeInTheDocument();
    expect(screen.getByText("Steer / chat")).toBeInTheDocument();
    expect(screen.getByText("Stream events")).toBeInTheDocument();
    expect(screen.getByText("Ingest webhook")).toBeInTheDocument();
    expect(screen.queryByText("Execute tool")).not.toBeInTheDocument();
    expect(screen.queryByText("Pause workspace")).not.toBeInTheDocument();
  });

  it("renders HTTP methods", () => {
    renderAgents();
    const posts = screen.getAllByText("POST");
    const gets = screen.getAllByText("GET");
    const patches = screen.getAllByText("PATCH");
    const deletes = screen.getAllByText("DELETE");
    expect(posts.length).toBeGreaterThanOrEqual(2);
    expect(gets.length).toBeGreaterThanOrEqual(1);
    expect(patches.length).toBeGreaterThanOrEqual(4); // update, stop, resume, kill
    expect(deletes.length).toBeGreaterThanOrEqual(1); // delete
  });

  it("renders webhook example", () => {
    renderAgents();
    expect(screen.getByText("Webhook Ingest Example")).toBeInTheDocument();
    expect(screen.getByText(/deploy\.failed/)).toBeInTheDocument();
  });

  it("renders safety limits cards", () => {
    renderAgents();
    expect(screen.getByText("Idempotency")).toBeInTheDocument();
    expect(screen.getByText("Audit Trail")).toBeInTheDocument();
    expect(screen.getByText("Secret Management")).toBeInTheDocument();
    expect(screen.getByText("Policy Enforcement")).toBeInTheDocument();
  });

  it("renders JSON-LD script", () => {
    const { container } = renderAgents();
    const script = container.querySelector('script[type="application/ld+json"]');
    expect(script).not.toBeNull();
    const data = JSON.parse(script!.textContent!);
    expect(data["@type"]).toBe("SoftwareApplication");
    expect(data.name).toBe("UseZombie");
  });

  it("uses agent-surface class for terminal aesthetic", () => {
    const { container } = renderAgents();
    expect(container.querySelector(".agent-surface")).not.toBeNull();
  });

  it("renders scanline overlay", () => {
    const { container } = renderAgents();
    const scanline = container.querySelector(".scanline");
    expect(scanline).not.toBeNull();
    expect(scanline).toHaveAttribute("aria-hidden", "true");
  });
});
