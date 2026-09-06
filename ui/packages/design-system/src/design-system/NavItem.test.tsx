import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Nav, NavItem } from "../index";

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

  it.each([
    { left: 344, right: 431, expected: 121 },
    { left: -20, right: 67, expected: 28 },
    { left: 40, right: 127, expected: 64 },
  ])("reveals a focused destination only when clipped ($left, $right)", ({ left, right, expected }) => {
    const onFocus = vi.fn();
    render(<Nav aria-label="Sections"><NavItem href="/trigger" onFocus={onFocus}>Trigger</NavItem></Nav>);
    const navigation = screen.getByRole("navigation");
    const link = screen.getByRole("link");
    navigation.scrollLeft = 64;
    Object.defineProperties(navigation, { scrollWidth: { value: 431 }, clientWidth: { value: 358 } });
    vi.spyOn(navigation, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: 16, width: 358 }));
    vi.spyOn(link, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: left, width: right - left }));
    fireEvent.focus(link);
    expect(navigation.scrollLeft).toBe(expected);
    expect(onFocus).toHaveBeenCalledOnce();
  });

  it("preserves child focus cancellation", () => {
    const onFocus = vi.fn();
    render(<Nav aria-label="Sections"><NavItem asChild onFocus={onFocus}><a href="/trigger" onFocus={event => event.preventDefault()}>Trigger</a></NavItem></Nav>);
    const navigation = screen.getByRole("navigation");
    Object.defineProperties(navigation, { scrollWidth: { value: 431 }, clientWidth: { value: 358 } });
    const bounds = vi.spyOn(navigation, "getBoundingClientRect");
    fireEvent.focus(screen.getByRole("link"));
    expect(onFocus).toHaveBeenCalledOnce();
    expect(navigation.scrollLeft).toBe(0);
    expect(bounds).not.toHaveBeenCalled();
  });

  it("leaves navigation still when its destinations fit", () => {
    render(<Nav aria-label="Sections"><NavItem href="/trigger">Trigger</NavItem></Nav>);
    const navigation = screen.getByRole("navigation");
    Object.defineProperties(navigation, { scrollWidth: { value: 300 }, clientWidth: { value: 358 } });
    const bounds = vi.spyOn(navigation, "getBoundingClientRect");
    fireEvent.focus(screen.getByRole("link"));
    expect(navigation.scrollLeft).toBe(0);
    expect(bounds).not.toHaveBeenCalled();
  });

  it("allows focus without an enclosing navigation", () => {
    render(<NavItem href="/trigger">Trigger</NavItem>);
    expect(() => fireEvent.focus(screen.getByRole("link"))).not.toThrow();
  });
});
