import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NavItem } from "../index";

describe("NavItem", () => {
  it("owns interface typography, theme tokens, and visible keyboard focus", () => {
    render(<NavItem href="/fleets">Fleets</NavItem>);
    const link = screen.getByRole("link", { name: "Fleets" });
    expect(link).toHaveClass("font-sans", "text-muted-foreground", "focus-visible:ring-ring");
    expect(link).not.toHaveAttribute("aria-current");
    expect(link).not.toHaveAttribute("data-active");
  });

  it("updates active semantics without changing the destination", () => {
    const { rerender } = render(<NavItem href="/fleets" active>Fleets</NavItem>);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("aria-current", "page");
    expect(link).toHaveAttribute("data-active", "true");
    rerender(<NavItem href="/fleets" active={false}>Fleets</NavItem>);
    expect(link).not.toHaveAttribute("aria-current");
    expect(link).toHaveAttribute("href", "/fleets");
  });

  it("composes router links without nested anchors or losing click handlers", () => {
    const onClick = vi.fn((event: { preventDefault(): void }) => event.preventDefault());
    render(<NavItem asChild active className="w-full"><a href="/events" onClick={onClick}>Events</a></NavItem>);
    expect(screen.getAllByRole("link")).toHaveLength(1);
    const link = screen.getByRole("link");
    expect(link).toHaveClass("w-full", "font-sans");
    fireEvent.click(link);
    expect(onClick).toHaveBeenCalledOnce();
  });
});
