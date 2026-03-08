import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Home from "./Home";
import { APP_BASE_URL } from "../config";

function renderHome(mode: "humans" | "agents" = "humans") {
  return render(
    <BrowserRouter>
      <Home mode={mode} />
    </BrowserRouter>
  );
}

describe("Home", () => {
  it("renders the hero headline with two lines", () => {
    renderHome();
    const h1 = screen.getByRole("heading", { level: 1 });
    expect(h1).toHaveTextContent(/ship ai-generated prs/i);
    expect(h1).toHaveTextContent(/without babysitting the run/i);
  });

  it("renders the hero kicker description", () => {
    renderHome();
    expect(screen.getByText(/turns queued engineering work into validated pull requests/i)).toBeInTheDocument();
  });

  it("shows humans badge in humans mode", () => {
    renderHome("humans");
    expect(screen.getByText(/for engineering teams/i)).toBeInTheDocument();
  });

  it("shows agent badge in agents mode", () => {
    renderHome("agents");
    expect(screen.getByText(/agent delivery control plane/i)).toBeInTheDocument();
  });

  it("renders Connect GitHub CTA with app link", () => {
    renderHome();
    const ctas = screen.getAllByRole("link", { name: /connect github, automate prs/i });
    expect(ctas.length).toBeGreaterThanOrEqual(1);
    expect(ctas[0]).toHaveAttribute("href", APP_BASE_URL);
  });

  it("does not render Talk to us CTA", () => {
    renderHome();
    expect(screen.queryByRole("link", { name: /talk to us/i })).not.toBeInTheDocument();
  });

  it("renders the hero terminal quickstart command", () => {
    renderHome();
    expect(screen.getByLabelText(/quick start command/i)).toHaveTextContent("curl -fsSL https://usezombie.sh/install.sh | bash");
  });

  it("does not render hero stats row", () => {
    renderHome();
    expect(screen.queryByText("agents on Hobby")).not.toBeInTheDocument();
  });

  it("renders feature flow rows including Mission Control", () => {
    renderHome();
    expect(screen.getByRole("heading", { level: 3, name: "Install once. Start shipping PRs." })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Traceability and replay by default" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Mission Control" })).toBeInTheDocument();
  });

  it("does not render the workflow surfaces strip in humans mode", () => {
    renderHome();
    expect(screen.queryByText("Where UseZombie works")).not.toBeInTheDocument();
  });

  it("renders Why UseZombie section", () => {
    renderHome();
    expect(screen.getByText("Why UseZombie")).toBeInTheDocument();
    expect(screen.getByText("Queue work")).toBeInTheDocument();
    expect(screen.getByText("Agents execute with guardrails")).toBeInTheDocument();
    expect(screen.getByText("Review a validated PR")).toBeInTheDocument();
  });

  it("renders install block after Why section", () => {
    renderHome();
    expect(screen.getByRole("heading", { level: 2, name: "Install zombiectl and connect GitHub" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Read the docs" })).toBeInTheDocument();
    expect(screen.getAllByRole("link", { name: /connect github, automate prs/i }).length).toBeGreaterThanOrEqual(1);
  });

  it("renders View full pricing as React Router Link to /pricing in agents mode", () => {
    renderHome("agents");
    const link = screen.getByRole("link", { name: /view full pricing/i });
    expect(link).toHaveAttribute("href", "/pricing");
  });

  it("renders classic feature cards in agents mode", () => {
    renderHome("agents");
    expect(screen.getByText("Automated PR delivery")).toBeInTheDocument();
    expect(screen.getByText("Bring your own models")).toBeInTheDocument();
  });

  it("renders CTA block in agents mode", () => {
    renderHome("agents");
    expect(screen.getByText(/queue work\. review prs\. sleep\./i)).toBeInTheDocument();
  });

  it("renders provider strip in agents mode", () => {
    renderHome("agents");
    expect(screen.getByText("Where UseZombie works")).toBeInTheDocument();
  });

  it("renders CTA block in humans mode", () => {
    renderHome();
    expect(screen.queryByText(/queue work\. review prs\. sleep\./i)).not.toBeInTheDocument();
  });
});
