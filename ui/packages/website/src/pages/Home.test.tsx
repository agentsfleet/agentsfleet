import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Home from "./Home";
import { DOCS_QUICKSTART_URL } from "../config";

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

  it("renders primary install CTA pointing at the docs quickstart", () => {
    renderHome();
    const cta = screen.getByTestId("hero-cta-primary");
    expect(cta).toHaveAttribute("href", DOCS_QUICKSTART_URL);
    expect(cta.textContent).toMatch(/install in claude code/i);
  });

  it("does not render Talk to us CTA", () => {
    renderHome();
    expect(screen.queryByRole("link", { name: /talk to us/i })).not.toBeInTheDocument();
  });

  it("renders the hero install transcript Terminal", () => {
    renderHome();
    expect(screen.getByLabelText(/install platform-ops via claude code/i)).toBeInTheDocument();
  });

  it("renders feature flow rows including Mission Control", () => {
    renderHome();
    expect(screen.getByRole("heading", { level: 3, name: "Install once." })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Every event on the record." })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Mission Control" })).toBeInTheDocument();
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
    expect(screen.getByText("BYOK")).toBeInTheDocument();
  });

  it("renders the install block", () => {
    renderHome();
    expect(screen.getByRole("heading", { level: 2, name: "Install zombiectl, then run /usezombie-install-platform-ops" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /read the docs/i })).toBeInTheDocument();
  });

  it("renders View full pricing as a React Router link", () => {
    renderHome();
    expect(screen.getByRole("link", { name: /view full pricing/i })).toHaveAttribute("href", "/pricing");
  });
});
