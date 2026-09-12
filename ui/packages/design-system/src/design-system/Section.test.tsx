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

  // ARIA drops `aria-label` on a plain <div>: there is no role for the name to
  // attach to. Two call sites lost their names that way while nine remembered
  // an `asChild` wrapper, so the tag follows the label rather than the caller's
  // memory.
  it("renders a <section> when it carries an accessible name", () => {
    const { container } = render(<Section aria-label="Workspace events">body</Section>);
    const root = container.firstElementChild!;
    expect(root.tagName).toBe("SECTION");
    expect(root.getAttribute("aria-label")).toBe("Workspace events");
  });

  it("renders a <section> when the name comes from aria-labelledby", () => {
    const { container } = render(<Section aria-labelledby="heading-id">body</Section>);
    expect(container.firstElementChild!.tagName).toBe("SECTION");
  });

  it("stays a <div> when it carries no name, so nothing claims a landmark", () => {
    const { container } = render(<Section>body</Section>);
    expect(container.firstElementChild!.tagName).toBe("DIV");
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
