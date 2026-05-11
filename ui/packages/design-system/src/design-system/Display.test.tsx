import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { DisplayXL, DisplayLG } from "./Display";

describe("DisplayXL", () => {
  it("renders as <h1> with children", () => {
    const { container } = render(<DisplayXL>Hero headline</DisplayXL>);
    expect(container.firstChild?.nodeName).toBe("H1");
    expect(screen.getByText("Hero headline")).toBeInTheDocument();
  });

  it("applies the display-xl tokens (mono, fluid hero, tracking + leading)", () => {
    const { container } = render(<DisplayXL>X</DisplayXL>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("font-mono");
    expect(cls).toContain("text-fluid-hero");
    expect(cls).toContain("tracking-display-xl");
    expect(cls).toContain("leading-display-xl");
  });

  it("merges consumer className without dropping base utilities", () => {
    const { container } = render(<DisplayXL className="max-w-narrow">X</DisplayXL>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("max-w-narrow");
    expect(cls).toContain("font-mono");
  });
});

describe("DisplayLG", () => {
  it("renders as <h2> with children", () => {
    const { container } = render(<DisplayLG>Section heading</DisplayLG>);
    expect(container.firstChild?.nodeName).toBe("H2");
    expect(screen.getByText("Section heading")).toBeInTheDocument();
  });

  it("applies the display-lg tokens (mono, fluid display-lg, tracking + leading)", () => {
    const { container } = render(<DisplayLG>X</DisplayLG>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("font-mono");
    expect(cls).toContain("text-fluid-display-lg");
    expect(cls).toContain("tracking-display-lg");
    expect(cls).toContain("leading-display-md");
  });

  it("merges consumer className without dropping base utilities", () => {
    const { container } = render(<DisplayLG className="max-w-narrow">X</DisplayLG>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("max-w-narrow");
    expect(cls).toContain("font-mono");
  });
});
