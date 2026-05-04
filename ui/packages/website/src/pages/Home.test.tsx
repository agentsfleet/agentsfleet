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
  it("renders the hero headline with two lines", () => {
    renderHome();
    const h1 = screen.getByRole("heading", { level: 1 });
    expect(h1).toHaveTextContent(/operational outcomes/i);
    expect(h1).toHaveTextContent(/don't fall into limbo/i);
  });

  it("renders the hero kicker description", () => {
    renderHome();
    expect(screen.getByText(/durable, markdown-defined agent runtime/i)).toBeInTheDocument();
  });

  it("renders primary docs CTA with quickstart link", () => {
    renderHome();
    const ctas = screen.getAllByRole("link", { name: /install platform-ops/i });
    expect(ctas.length).toBeGreaterThanOrEqual(1);
    expect(ctas[0]).toHaveAttribute("href", DOCS_QUICKSTART_URL);
  });

  it("does not render Talk to us CTA", () => {
    renderHome();
    expect(screen.queryByRole("link", { name: /talk to us/i })).not.toBeInTheDocument();
  });

  it("renders the hero terminal quickstart command", () => {
    renderHome();
    expect(screen.getByLabelText(/quick start command/i)).toHaveTextContent("npm install -g @usezombie/zombiectl");
  });

  it("renders feature flow rows including Mission Control", () => {
    renderHome();
    expect(screen.getByRole("heading", { level: 3, name: "Install once. Operate forever." })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 3, name: "Every event, every actor, on the record." })).toBeInTheDocument();
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
    expect(screen.getByText("Markdown-defined behaviour")).toBeInTheDocument();
    expect(screen.getByText("Bring Your Own Key")).toBeInTheDocument();
  });

  it("renders the install block", () => {
    renderHome();
    expect(screen.getByRole("heading", { level: 2, name: "Install zombiectl, then run /usezombie-install-platform-ops" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Read the docs" })).toBeInTheDocument();
  });

  it("renders View full pricing as a React Router link", () => {
    renderHome();
    expect(screen.getByRole("link", { name: /view full pricing/i })).toHaveAttribute("href", "/pricing");
  });
});
