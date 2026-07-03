import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { SectionLabel } from "./SectionLabel";

describe("SectionLabel", () => {
  it("renders as a <h2> with children", () => {
    const { container } = render(<SectionLabel>Pipeline</SectionLabel>);
    expect(container.firstChild?.nodeName).toBe("H2");
    expect(screen.getByText("Pipeline")).toBeInTheDocument();
  });

  it("applies the eyebrow style (mono, uppercase, muted, eyebrow tokens)", () => {
    const { container } = render(<SectionLabel>Recent runs</SectionLabel>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("font-mono");
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
    expect(cls).toContain("font-mono");
  });
});
