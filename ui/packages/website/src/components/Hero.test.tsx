import { render, screen } from "@testing-library/react";
import { BrowserRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import Hero from "./Hero";
import { APP_BASE_URL } from "../config";

function renderHero() {
  return render(
    <BrowserRouter>
      <Hero />
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

  it("shows the human badge text", () => {
    renderHero();
    expect(screen.getByText(/for engineering teams/i)).toBeInTheDocument();
  });

  it("renders the primary CTA linking to app", () => {
    renderHero();
    const cta = screen.getByRole("link", { name: /connect github, automate prs/i });
    expect(cta).toHaveAttribute("href", APP_BASE_URL);
  });

  it("renders the pricing CTA", () => {
    renderHero();
    expect(screen.getByRole("link", { name: /see pricing/i })).toHaveAttribute("href", "/pricing");
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

  it("does not render the removed proof cards", () => {
    renderHero();
    expect(screen.queryByText("Validated PRs")).not.toBeInTheDocument();
    expect(screen.queryByText("Direct model billing")).not.toBeInTheDocument();
    expect(screen.queryByText("Measurable run quality")).not.toBeInTheDocument();
  });
});
