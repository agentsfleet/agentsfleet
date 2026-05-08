import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Badge, badgeVariants } from "./Badge";

describe("Badge", () => {
  it("renders as a <div> with children", () => {
    const { container } = render(<Badge>New</Badge>);
    expect(container.firstChild?.nodeName).toBe("DIV");
    expect(screen.getByText("New")).toBeInTheDocument();
  });

  it("applies the default variant classes", () => {
    const { container } = render(<Badge>Default</Badge>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("border-border");
    expect(cls).toContain("bg-muted");
    expect(cls).toContain("text-muted-foreground");
  });

  it("applies the orange variant (primary-tinted)", () => {
    const { container } = render(<Badge variant="orange">Active</Badge>);
    expect((container.firstChild as HTMLElement).className).toContain("text-primary");
  });

  it("applies the destructive variant", () => {
    const { container } = render(<Badge variant="destructive">Err</Badge>);
    expect((container.firstChild as HTMLElement).className).toContain("text-destructive");
  });

  it("applies the amber/green/cyan status variants", () => {
    const { container: a } = render(<Badge variant="amber">A</Badge>);
    const { container: g } = render(<Badge variant="green">G</Badge>);
    const { container: c } = render(<Badge variant="cyan">C</Badge>);
    expect((a.firstChild as HTMLElement).className).toContain("text-warning");
    expect((g.firstChild as HTMLElement).className).toContain("text-success");
    expect((c.firstChild as HTMLElement).className).toContain("text-info");
  });

  it("always applies base pill shape + typography utilities", () => {
    const { container } = render(<Badge>X</Badge>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("rounded-sm");
    expect(cls).toContain("font-mono");
    expect(cls).toContain("uppercase");
  });

  it("merges a custom className", () => {
    const { container } = render(<Badge className="my-badge">X</Badge>);
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("my-badge");
    expect(cls).toContain("rounded-sm");
  });

  it("forwards arbitrary props (data-testid)", () => {
    render(<Badge data-testid="b">X</Badge>);
    expect(screen.getByTestId("b")).toBeInTheDocument();
  });

  it("forwards ref to the underlying <div>", () => {
    const ref = { current: null as HTMLDivElement | null };
    render(<Badge ref={ref}>X</Badge>);
    expect(ref.current).toBeInstanceOf(HTMLDivElement);
  });

  it("SSR renders <div> markup with badge classes", () => {
    const html = renderToStaticMarkup(<Badge variant="orange">SSR</Badge>);
    expect(html).toMatch(/^<div /);
    expect(html).toContain("rounded-sm");
    expect(html).toContain("text-primary");
  });
});

describe("badgeVariants", () => {
  it("every variant produces a distinct class string", () => {
    const variants = ["default", "orange", "amber", "green", "cyan", "destructive"] as const;
    const classes = variants.map((v) => badgeVariants({ variant: v }));
    expect(new Set(classes).size).toBe(variants.length);
  });
});
