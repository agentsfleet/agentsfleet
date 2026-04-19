import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import Section from "./Section";

describe("Section", () => {
  it("renders a grid with gap-xl by default", () => {
    const { container } = render(<Section>x</Section>);
    const el = container.firstChild as HTMLElement;
    expect(el.tagName).toBe("DIV");
    expect(el.className).toContain("grid");
    expect(el.getAttribute("data-section")).toBe("stack");
  });

  it("adds section padding when gap=true", () => {
    const { container } = render(<Section gap>x</Section>);
    const el = container.firstChild as HTMLElement;
    expect(el.getAttribute("data-section")).toBe("gap");
    expect(el.className).toContain("py-5xl");
  });

  it("merges custom className", () => {
    const { container } = render(<Section className="custom">x</Section>);
    expect((container.firstChild as HTMLElement).className).toContain("custom");
  });

  it("asChild renders the provided child as root", () => {
    const { container } = render(
      <Section asChild>
        <main>content</main>
      </Section>,
    );
    expect((container.firstChild as HTMLElement).tagName).toBe("MAIN");
  });

  it("consecutive gap sections collapse the top padding (via [&+[data-section=gap]]:pt-0)", () => {
    const { container } = render(
      <>
        <Section gap>a</Section>
        <Section gap>b</Section>
      </>,
    );
    const [, second] = container.children;
    expect((second as HTMLElement).className).toContain("[&+[data-section=gap]]:pt-0");
  });
});
