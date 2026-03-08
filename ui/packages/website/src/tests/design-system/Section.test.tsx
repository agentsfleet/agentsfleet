import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { Section } from "@usezombie/design-system";

describe("Section", () => {
  it("uses z-stack class by default", () => {
    const { container } = render(<Section>Content</Section>);
    expect(container.firstChild).toHaveClass("z-stack");
  });

  it("uses z-section-gap when gap=true", () => {
    const { container } = render(<Section gap>Content</Section>);
    expect(container.firstChild).toHaveClass("z-section-gap");
    expect(container.firstChild).not.toHaveClass("z-stack");
  });

  it("merges custom className and forwards props", () => {
    render(
      <Section className="custom" data-testid="section">
        Content
      </Section>,
    );
    const section = screen.getByTestId("section");
    expect(section).toHaveClass("z-stack", "custom");
  });
});
