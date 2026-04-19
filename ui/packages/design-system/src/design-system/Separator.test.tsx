import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Separator } from "./Separator";

describe("Separator", () => {
  it("renders with role=none when decorative (default)", () => {
    const { container } = render(<Separator data-testid="s" />);
    const el = container.querySelector('[data-testid="s"]') as HTMLElement;
    expect(el).not.toBeNull();
    // Radix Separator uses role=none when decorative=true
    expect(el.getAttribute("role")).toBe("none");
  });

  it("renders with role=separator when decorative=false", () => {
    const { container } = render(<Separator decorative={false} data-testid="s" />);
    const el = container.querySelector('[data-testid="s"]') as HTMLElement;
    expect(el.getAttribute("role")).toBe("separator");
  });

  it("applies horizontal sizing classes by default", () => {
    const { container } = render(<Separator data-testid="s" />);
    const cls = (container.querySelector('[data-testid="s"]') as HTMLElement).className;
    expect(cls).toContain("h-px");
    expect(cls).toContain("w-full");
    expect(cls).toContain("bg-border");
  });

  it("applies vertical sizing classes when orientation=vertical", () => {
    const { container } = render(<Separator orientation="vertical" data-testid="s" />);
    const cls = (container.querySelector('[data-testid="s"]') as HTMLElement).className;
    expect(cls).toContain("h-full");
    expect(cls).toContain("w-px");
  });

  it("sets aria-orientation on non-decorative vertical separators", () => {
    render(<Separator orientation="vertical" decorative={false} data-testid="s" />);
    expect(screen.getByTestId("s")).toHaveAttribute("aria-orientation", "vertical");
  });

  it("merges a custom className", () => {
    const { container } = render(<Separator className="my-sep" data-testid="s" />);
    const cls = (container.querySelector('[data-testid="s"]') as HTMLElement).className;
    expect(cls).toContain("my-sep");
    expect(cls).toContain("bg-border");
  });

  it("forwards ref to the underlying element", () => {
    const ref = { current: null as HTMLDivElement | null };
    render(<Separator ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLElement);
  });

  it("SSR renders a <div> with separator classes", () => {
    const html = renderToStaticMarkup(<Separator />);
    expect(html).toMatch(/^<div /);
    expect(html).toContain("bg-border");
    expect(html).toContain("h-px");
  });
});
