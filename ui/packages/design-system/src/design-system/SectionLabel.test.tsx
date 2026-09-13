import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { SectionLabel } from "./SectionLabel";

describe("SectionLabel", () => {
  it("renders as a <h2> with children", () => {
    const { container } = render(<SectionLabel>Pipeline</SectionLabel>);
    expect(container.firstChild?.nodeName).toBe("H2");
    expect(screen.getByText("Pipeline")).toBeInTheDocument();
  });

  // The marketing case: a display heading follows the eyebrow, so the eyebrow
  // must not also claim heading rank. Five sections on the landing page paired
  // a 12px h2 with a 40px h2 before this existed.
  it("renders as a <p> when a heading follows it", () => {
    const { container } = render(<SectionLabel as="p">core capabilities</SectionLabel>);
    expect(container.firstChild?.nodeName).toBe("P");
    expect(screen.getByText("core capabilities")).toBeInTheDocument();
  });

  it("keeps the eyebrow style whichever element it renders", () => {
    const { container } = render(<SectionLabel as="p">legal</SectionLabel>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("font-sans");
    expect(cls).toContain("uppercase");
    expect(cls).toContain("text-eyebrow");
  });

  it("applies the eyebrow style (sans, uppercase, muted, eyebrow tokens)", () => {
    const { container } = render(<SectionLabel>Recent runs</SectionLabel>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("font-sans");
    expect(cls).toContain("uppercase");
    expect(cls).toContain("tracking-eyebrow");
    expect(cls).toContain("text-muted-foreground");
    expect(cls).toContain("text-eyebrow");
    expect(cls).toContain("leading-eyebrow");
  });

  it("merges consumer className without dropping base utilities", () => {
    const { container } = render(<SectionLabel className="mb-0">X</SectionLabel>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("mb-0");
    expect(cls).toContain("font-sans");
  });
});
