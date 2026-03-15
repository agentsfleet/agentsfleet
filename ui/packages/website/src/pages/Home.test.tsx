import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Home from "./Home";
import { APP_BASE_URL } from "../config";

function renderHome() {
  return render(
    <BrowserRouter>
      <Home />
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
    expect(screen.getByText(/turns queued engineering work into validated pull requests with replay, run quality scoring, and policy controls/i)).toBeInTheDocument();
  });

  it("renders primary app CTA with app link", () => {
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

  it("renders feature flow rows including Mission Control", () => {
    renderHome();
    expect(screen.getByRole("heading", { level: 3, name: "Install once. Start shipping PRs." })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Traceability and replay by default" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Mission Control" })).toBeInTheDocument();
  });

  it("renders Why UseZombie section", () => {
    renderHome();
    expect(screen.getByText("Why UseZombie")).toBeInTheDocument();
    expect(screen.getByText("Queue work")).toBeInTheDocument();
    expect(screen.getByText("Agents execute with guardrails")).toBeInTheDocument();
    expect(screen.getByText("Review a validated PR")).toBeInTheDocument();
  });

  it("renders scaling features on the homepage", () => {
    renderHome();
    expect(screen.getByText(/core capabilities/i)).toBeInTheDocument();
    expect(screen.getByText("Validation before review")).toBeInTheDocument();
    expect(screen.getByText("Run quality scoring")).toBeInTheDocument();
  });

  it("renders the install block", () => {
    renderHome();
    expect(screen.getByRole("heading", { level: 2, name: "Install zombiectl and connect GitHub" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Read the docs" })).toBeInTheDocument();
  });

  it("renders View full pricing as a React Router link", () => {
    renderHome();
    expect(screen.getByRole("link", { name: /view full pricing/i })).toHaveAttribute("href", "/pricing");
  });
});
