import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { DisplayXL, DisplayLG } from "./Display";

describe("DisplayXL", () => {
  it("renders as <h1> with children", () => {
    const { container } = render(<DisplayXL>Hero headline</DisplayXL>);
    expect(container.firstChild?.nodeName).toBe("H1");
    expect(screen.getByText("Hero headline")).toBeInTheDocument();
  });

  it("applies the display-xl token (mono, fluid clamp, tight tracking)", () => {
    const { container } = render(<DisplayXL>X</DisplayXL>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("font-mono");
    expect(cls).toContain("text-[clamp(40px,6vw,64px)]");
    expect(cls).toContain("tracking-[-0.025em]");
    expect(cls).toContain("leading-[1.05]");
  });

  it("merges consumer className without dropping base utilities", () => {
    const { container } = render(<DisplayXL className="max-w-[900px]">X</DisplayXL>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("max-w-[900px]");
    expect(cls).toContain("font-mono");
  });
});

describe("DisplayLG", () => {
  it("renders as <h2> with children", () => {
    const { container } = render(<DisplayLG>Section heading</DisplayLG>);
    expect(container.firstChild?.nodeName).toBe("H2");
    expect(screen.getByText("Section heading")).toBeInTheDocument();
  });

  it("applies the display-lg token (mono, fluid clamp, tight tracking)", () => {
    const { container } = render(<DisplayLG>X</DisplayLG>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("font-mono");
    expect(cls).toContain("text-[clamp(28px,4vw,40px)]");
    expect(cls).toContain("tracking-[-0.02em]");
    expect(cls).toContain("leading-[1.15]");
  });

  it("merges consumer className without dropping base utilities", () => {
    const { container } = render(<DisplayLG className="max-w-[640px]">X</DisplayLG>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("max-w-[640px]");
    expect(cls).toContain("font-mono");
  });
});
