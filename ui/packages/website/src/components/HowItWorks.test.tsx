import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import HowItWorks from "./HowItWorks";

describe("HowItWorks", () => {
  it("renders the section heading", () => {
    const { container } = render(<HowItWorks />);
    const heading = container.querySelector("h2");
    expect(heading).toBeInTheDocument();
    expect(heading?.textContent).toBe("From trigger to evidenced diagnosis, durably.");
  });

  it("renders the eyebrow", () => {
    render(<HowItWorks />);
    expect(screen.getByText("How it works")).toBeInTheDocument();
  });

  it("renders all three steps", () => {
    render(<HowItWorks />);
    expect(screen.getByText("A trigger arrives")).toBeInTheDocument();
    expect(screen.getByText("The agent gathers evidence")).toBeInTheDocument();
    expect(screen.getByText("Diagnosis posts; the run is auditable")).toBeInTheDocument();
  });

  it("renders step descriptions", () => {
    render(<HowItWorks />);
    expect(screen.getByText(/A GitHub Actions deploy fails, a cron fires/i)).toBeInTheDocument();
    expect(screen.getByText(/calls the tools TRIGGER\.md allow-lists/i)).toBeInTheDocument();
    expect(screen.getByText(/Slack receives the evidenced diagnosis/i)).toBeInTheDocument();
  });
});
