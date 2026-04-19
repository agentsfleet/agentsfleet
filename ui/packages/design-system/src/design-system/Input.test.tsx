import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Input } from "./Input";

describe("Input", () => {
  it("renders an <input> element", () => {
    render(<Input placeholder="Email" />);
    const el = screen.getByPlaceholderText("Email");
    expect(el.tagName).toBe("INPUT");
  });

  it("defaults to no explicit type (browser default=text)", () => {
    render(<Input placeholder="X" />);
    const el = screen.getByPlaceholderText("X");
    expect(el.getAttribute("type")).toBeNull();
  });

  it("forwards type prop", () => {
    render(<Input type="email" placeholder="Email" />);
    expect(screen.getByPlaceholderText("Email")).toHaveAttribute("type", "email");
  });

  it("applies base semantic utilities", () => {
    render(<Input data-testid="i" />);
    const cls = screen.getByTestId("i").className;
    expect(cls).toContain("border-input");
    expect(cls).toContain("bg-muted");
    expect(cls).toContain("text-foreground");
    expect(cls).toContain("rounded-lg");
  });

  it("applies focus ring and disabled styles", () => {
    render(<Input data-testid="i" />);
    const cls = screen.getByTestId("i").className;
    expect(cls).toContain("focus:ring-ring");
    expect(cls).toContain("focus:border-primary");
    expect(cls).toContain("disabled:opacity-50");
  });

  it("uses muted-foreground for placeholder color", () => {
    render(<Input data-testid="i" />);
    expect(screen.getByTestId("i").className).toContain("placeholder:text-muted-foreground");
  });

  it("merges a custom className", () => {
    render(<Input className="extra" data-testid="i" />);
    const cls = screen.getByTestId("i").className;
    expect(cls).toContain("extra");
    expect(cls).toContain("rounded-lg");
  });

  it("fires onChange handlers and supports controlled usage", () => {
    const handler = vi.fn();
    render(<Input value="" onChange={handler} data-testid="i" />);
    const el = screen.getByTestId("i") as HTMLInputElement;
    fireEvent.change(el, { target: { value: "hello" } });
    expect(handler).toHaveBeenCalled();
  });

  it("respects disabled prop", () => {
    render(<Input disabled data-testid="i" />);
    expect(screen.getByTestId("i")).toBeDisabled();
  });

  it("forwards ref to the underlying <input>", () => {
    const ref = { current: null as HTMLInputElement | null };
    render(<Input ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLInputElement);
  });

  it("SSR renders an <input> markup string with input classes", () => {
    const html = renderToStaticMarkup(<Input placeholder="Email" />);
    expect(html).toMatch(/^<input /);
    expect(html).toContain('placeholder="Email"');
    expect(html).toContain("bg-muted");
  });
});
