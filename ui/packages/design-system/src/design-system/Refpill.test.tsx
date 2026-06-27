import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Refpill } from "./Refpill";

describe("Refpill", () => {
  it("renders as a <span> naming the reference", () => {
    const { container } = render(<Refpill>billing-bot</Refpill>);
    expect(container.firstChild?.nodeName).toBe("SPAN");
    expect(screen.getByText("billing-bot")).toBeInTheDocument();
  });

  it("applies the rounded mono chip base classes", () => {
    const { container } = render(<Refpill>x</Refpill>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("rounded-full");
    expect(cls).toContain("border");
    expect(cls).toContain("font-mono");
    expect(cls).toContain("text-muted-foreground");
  });

  it("merges a custom className", () => {
    const { container } = render(<Refpill className="text-primary">x</Refpill>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("text-primary");
    expect(cls).toContain("rounded-full");
  });

  it("forwards arbitrary props (data-testid)", () => {
    render(<Refpill data-testid="r">x</Refpill>);
    expect(screen.getByTestId("r")).toBeInTheDocument();
  });

  it("forwards ref to the underlying <span>", () => {
    const ref = { current: null as HTMLSpanElement | null };
    render(<Refpill ref={ref}>x</Refpill>);
    expect(ref.current).toBeInstanceOf(HTMLSpanElement);
  });

  it("SSR renders <span> markup with refpill classes", () => {
    const html = renderToStaticMarkup(<Refpill>SSR</Refpill>);
    expect(html).toMatch(/^<span /);
    expect(html).toContain("rounded-full");
  });
});
