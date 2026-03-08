import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Hero from "./Hero";
import { APP_BASE_URL } from "../config";

function renderHero(mode: "humans" | "agents" = "humans") {
  return render(
    <BrowserRouter>
      <Hero mode={mode} />
    </BrowserRouter>
  );
}

describe("Hero", () => {
  it("renders h1 with both headline lines", () => {
    renderHero();
    const h1 = screen.getByRole("heading", { level: 1 });
    expect(h1).toHaveTextContent(/ship ai-generated prs/i);
    expect(h1).toHaveTextContent(/without babysitting the run/i);
  });

  it("shows humans badge text in humans mode", () => {
    renderHero("humans");
    expect(screen.getByText(/for engineering teams/i)).toBeInTheDocument();
  });

  it("shows agents badge text in agents mode", () => {
    renderHero("agents");
    expect(screen.getByText(/agent delivery control plane/i)).toBeInTheDocument();
  });

  it("renders the primary CTA linking to app for humans mode", () => {
    renderHero();
    const cta = screen.getByRole("link", { name: /connect github, automate prs/i });
    expect(cta).toHaveAttribute("href", APP_BASE_URL);
  });

  it("renders the primary CTA linking to quickstart for agents mode", () => {
    renderHero("agents");
    const cta = screen.getByRole("link", { name: /connect github, automate prs/i });
    expect(cta).toHaveAttribute("href", "https://docs.usezombie.com/quickstart");
  });

  it("does not render secondary talk CTA", () => {
    renderHero();
    expect(screen.queryByRole("link", { name: /talk to us/i })).not.toBeInTheDocument();
  });

  it("renders the terminal with quick start label", () => {
    renderHero();
    expect(screen.getByLabelText(/quick start command/i)).toBeInTheDocument();
  });

  it("renders copy button on terminal", () => {
    renderHero();
    expect(screen.getByTestId("copy-btn")).toBeInTheDocument();
  });

  it("renders the data-command attribute on the terminal", () => {
    renderHero();
    const terminal = screen.getByLabelText(/quick start command/i);
    expect(terminal).toHaveAttribute("data-command", "curl -fsSL https://usezombie.sh/install.sh | bash");
  });

  it("does not render hero stats section", () => {
    renderHero();
    expect(screen.queryByText("agents on Hobby")).not.toBeInTheDocument();
  });
});
