import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Skeleton } from "./Skeleton";

describe("Skeleton", () => {
  it("renders a <div> with the pulse animation", () => {
    const { container } = render(<Skeleton data-testid="s" />);
    const el = screen.getByTestId("s");
    expect(el.tagName).toBe("DIV");
    expect(el.className).toContain("animate-pulse");
    expect(container.firstChild?.nodeName).toBe("DIV");
  });

  it("applies base background + radius utilities", () => {
    render(<Skeleton data-testid="s" />);
    const cls = screen.getByTestId("s").className;
    expect(cls).toContain("rounded-md");
    expect(cls).toContain("bg-muted");
  });

  it("merges a custom className (sizing lives on consumer)", () => {
    render(<Skeleton className="h-4 w-24" data-testid="s" />);
    const cls = screen.getByTestId("s").className;
    expect(cls).toContain("h-4");
    expect(cls).toContain("w-24");
    expect(cls).toContain("animate-pulse");
  });

  it("forwards arbitrary props", () => {
    render(<Skeleton data-testid="sk" aria-hidden="true" />);
    const el = screen.getByTestId("sk");
    expect(el).toHaveAttribute("aria-hidden", "true");
  });

  it("forwards ref to the underlying <div>", () => {
    const ref = { current: null as HTMLDivElement | null };
    render(<Skeleton ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLDivElement);
  });

  it("SSR renders <div> markup with skeleton classes", () => {
    const html = renderToStaticMarkup(<Skeleton />);
    expect(html).toMatch(/^<div /);
    expect(html).toContain("animate-pulse");
    expect(html).toContain("bg-muted");
  });
});
