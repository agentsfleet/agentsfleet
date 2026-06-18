import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import HowItWorks from "./HowItWorks";
import { HOW_IT_WORKS_HEADING, LOOP_STEPS } from "../lib/marketing-copy";

describe("HowItWorks", () => {
  it("renders the section heading", () => {
    const { container } = render(<HowItWorks />);
    const heading = container.querySelector("h2");
    expect(heading).toBeInTheDocument();
    expect(heading?.textContent).toBe(HOW_IT_WORKS_HEADING);
  });

  it("renders the eyebrow", () => {
    render(<HowItWorks />);
    expect(screen.getByText(/how it works/i)).toBeInTheDocument();
  });

  it("renders all eight loop steps", () => {
    render(<HowItWorks />);
    for (const step of LOOP_STEPS) {
      expect(screen.getByText(step.title)).toBeInTheDocument();
    }
  });

  it("renders mono numbered eyebrows", () => {
    render(<HowItWorks />);
    for (const step of LOOP_STEPS) {
      expect(screen.getByText(step.number)).toBeInTheDocument();
    }
  });

  it("renders step descriptions in loop order", () => {
    render(<HowItWorks />);
    let previous: HTMLElement | null = null;
    for (const step of LOOP_STEPS) {
      const heading = screen.getByText(step.title);
      expect(screen.getByText(step.description)).toBeInTheDocument();
      if (previous) {
        expect(
          previous.compareDocumentPosition(heading) & Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      }
      previous = heading;
    }
  });
});
