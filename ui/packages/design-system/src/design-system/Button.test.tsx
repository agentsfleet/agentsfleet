import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Button, buttonClassName, buttonVariants } from "./Button";

describe("Button", () => {
  it("renders as <button type=button> by default", () => {
    render(<Button>Click</Button>);
    const btn = screen.getByRole("button", { name: "Click" });
    expect(btn.tagName).toBe("BUTTON");
    expect(btn).toHaveAttribute("type", "button");
  });

  it("applies the default variant — flat pulse fill, no gradient", () => {
    render(<Button>Go</Button>);
    const btn = screen.getByRole("button");
    expect(btn.className).toContain("bg-primary");
    expect(btn.className).toContain("text-primary-foreground");
    expect(btn.className).not.toContain("linear-gradient");
    expect(btn.className).not.toContain("rounded-full");
  });

  it("applies the destructive variant", () => {
    render(<Button variant="destructive">Delete</Button>);
    expect(screen.getByRole("button").className).toContain("bg-destructive");
  });

  it("applies the outline variant", () => {
    render(<Button variant="outline">Outline</Button>);
    const cls = screen.getByRole("button").className;
    expect(cls).toContain("border-border-strong");
    expect(cls).toContain("bg-transparent");
  });

  it("applies the secondary variant", () => {
    render(<Button variant="secondary">Secondary</Button>);
    expect(screen.getByRole("button").className).toContain("bg-secondary");
  });

  it("applies the ghost variant — muted text, transparent fill", () => {
    render(<Button variant="ghost">Ghost</Button>);
    const cls = screen.getByRole("button").className;
    expect(cls).toContain("bg-transparent");
    expect(cls).toContain("text-muted-foreground");
  });

  it("applies the link variant — pulse text, underline-on-hover", () => {
    render(<Button variant="link">Link</Button>);
    const cls = screen.getByRole("button").className;
    expect(cls).toContain("underline-offset-4");
    expect(cls).toContain("text-pulse");
  });

  it("applies the double-border variant — 2px primary outline, no shadow", () => {
    render(<Button variant="double-border">Setup</Button>);
    const cls = screen.getByRole("button").className;
    expect(cls).toContain("border-2");
    expect(cls).toContain("border-primary");
    expect(cls).toContain("bg-transparent");
    expect(cls).not.toContain("shadow-");
  });

  it("uses --r-md radius (no pill-radius on chrome)", () => {
    render(<Button>X</Button>);
    expect(screen.getByRole("button").className).toContain("rounded-md");
  });

  it("uses font-mono on chrome", () => {
    render(<Button>X</Button>);
    expect(screen.getByRole("button").className).toContain("font-mono");
  });

  it("default size has h-10 (40px) per spec component principles", () => {
    render(<Button>X</Button>);
    expect(screen.getByRole("button").className).toContain("h-10");
  });

  it("sm size has h-8", () => {
    render(<Button size="sm">X</Button>);
    expect(screen.getByRole("button").className).toContain("h-8");
  });

  it("lg size has h-12", () => {
    render(<Button size="lg">X</Button>);
    expect(screen.getByRole("button").className).toContain("h-12");
  });

  it("icon size is 36px square (spec max for icon-only)", () => {
    render(<Button size="icon" aria-label="settings">⚙</Button>);
    const cls = screen.getByRole("button").className;
    expect(cls).toContain("h-9");
    expect(cls).toContain("w-9");
  });

  it("merges a custom className via cn", () => {
    render(<Button className="extra-class">X</Button>);
    expect(screen.getByRole("button").className).toContain("extra-class");
  });

  it("fires onClick handlers", () => {
    const handler = vi.fn();
    render(<Button onClick={handler}>Click</Button>);
    screen.getByRole("button").click();
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("respects disabled prop", () => {
    const handler = vi.fn();
    render(
      <Button onClick={handler} disabled>
        Disabled
      </Button>,
    );
    const btn = screen.getByRole("button");
    expect(btn).toBeDisabled();
    btn.click();
    expect(handler).not.toHaveBeenCalled();
  });

  it("forwards ref to the underlying <button>", () => {
    const ref = { current: null as HTMLButtonElement | null };
    render(<Button ref={ref}>X</Button>);
    expect(ref.current).toBeInstanceOf(HTMLButtonElement);
  });

  it("asChild renders the provided child (e.g. <a>) with merged button classes", () => {
    render(
      <Button asChild variant="ghost">
        <a href="/docs" target="_blank" rel="noopener noreferrer">
          Docs
        </a>
      </Button>,
    );
    const link = screen.getByRole("link", { name: "Docs" });
    expect(link.tagName).toBe("A");
    expect(link).toHaveAttribute("href", "/docs");
    expect(link.className).toContain("bg-transparent");
    expect(link.className).toContain("text-muted-foreground");
  });

  it("asChild does NOT apply type=button to the cloned child", () => {
    render(
      <Button asChild>
        <a href="/x">Go</a>
      </Button>,
    );
    expect(screen.getByRole("link")).not.toHaveAttribute("type");
  });

  it("SSR renders a <button> markup string with button classes", () => {
    const html = renderToStaticMarkup(<Button>Hello</Button>);
    expect(html).toMatch(/^<button /);
    expect(html).toContain("Hello");
    expect(html).toContain("text-primary-foreground");
  });

  it("SSR renders an <a> when asChild is used with a link child", () => {
    const html = renderToStaticMarkup(
      <Button asChild variant="default">
        <a href="/x">X</a>
      </Button>,
    );
    expect(html).toMatch(/^<a /);
    expect(html).toContain('href="/x"');
    expect(html).toContain("text-primary-foreground");
    expect(html).not.toContain("type=");
  });
});

describe("buttonVariants / buttonClassName", () => {
  it("buttonClassName returns the full class string for a variant", () => {
    const cls = buttonClassName("default");
    expect(cls).toContain("text-primary-foreground");
    expect(cls).toContain("rounded-md");
    expect(cls).toContain("inline-flex");
    expect(cls).toContain("font-mono");
  });

  it("buttonClassName defaults to the default variant + default size", () => {
    expect(buttonClassName()).toBe(buttonClassName("default", "default"));
  });

  it("buttonVariants is stable across calls for the same inputs", () => {
    expect(buttonVariants({ variant: "ghost" })).toBe(buttonVariants({ variant: "ghost" }));
  });

  it("every variant produces a distinct class string", () => {
    const variants = ["default", "destructive", "outline", "secondary", "ghost", "link", "double-border"] as const;
    const classes = variants.map((v) => buttonVariants({ variant: v }));
    expect(new Set(classes).size).toBe(variants.length);
  });

  it("every size produces a distinct class string", () => {
    const sizes = ["default", "sm", "lg", "icon"] as const;
    const classes = sizes.map((s) => buttonVariants({ size: s }));
    expect(new Set(classes).size).toBe(sizes.length);
  });
});
