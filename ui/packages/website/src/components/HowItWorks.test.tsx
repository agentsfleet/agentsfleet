import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import HowItWorks from "./HowItWorks";

describe("HowItWorks", () => {
  it("renders the section heading", () => {
    const { container } = render(<HowItWorks />);
    const heading = container.querySelector("h2");
    expect(heading).toBeInTheDocument();
    expect(heading?.textContent).toBe("From queued intent to validated pull requests.");
  });

  it("renders the eyebrow", () => {
    render(<HowItWorks />);
    expect(screen.getByText("Why UseZombie")).toBeInTheDocument();
  });

  it("renders all three steps", () => {
    render(<HowItWorks />);
    expect(screen.getByText("Queue work")).toBeInTheDocument();
    expect(screen.getByText("Agents execute with guardrails")).toBeInTheDocument();
    expect(screen.getByText("Review a validated PR")).toBeInTheDocument();
  });

  it("renders step descriptions", () => {
    render(<HowItWorks />);
    expect(screen.getByText(/trigger a run from CLI or API/i)).toBeInTheDocument();
    expect(screen.getByText(/Agents plan, implement, and validate with policy controls/i)).toBeInTheDocument();
    expect(screen.getByText(/pull request includes run replay/i)).toBeInTheDocument();
  });
});
