import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { Textarea } from "./Textarea";

describe("Textarea", () => {
  it("renders a <textarea> element", () => {
    render(<Textarea placeholder="Body" />);
    const el = screen.getByPlaceholderText("Body");
    expect(el.tagName).toBe("TEXTAREA");
  });

  it("merges a custom className", () => {
    render(<Textarea className="extra" data-testid="t" />);
    const cls = screen.getByTestId("t").className;
    expect(cls).toContain("extra");
    expect(cls).toContain("rounded-md");
  });

  it("forwards rows attribute", () => {
    render(<Textarea rows={6} data-testid="t" />);
    expect(screen.getByTestId("t")).toHaveAttribute("rows", "6");
  });

  it("fires onChange in controlled usage", () => {
    const handler = vi.fn();
    render(<Textarea value="" onChange={handler} data-testid="t" />);
    fireEvent.change(screen.getByTestId("t"), { target: { value: "hello" } });
    expect(handler).toHaveBeenCalled();
  });

  it("respects disabled prop", () => {
    render(<Textarea disabled data-testid="t" />);
    expect(screen.getByTestId("t")).toBeDisabled();
  });

  it("forwards ref to the underlying <textarea>", () => {
    const ref = { current: null as HTMLTextAreaElement | null };
    render(<Textarea ref={ref} />);
    expect(ref.current).toBeInstanceOf(HTMLTextAreaElement);
  });

  it("SSR renders a <textarea> markup string with textarea classes", () => {
    const html = renderToStaticMarkup(<Textarea placeholder="Body" />);
    expect(html).toMatch(/^<textarea /);
    expect(html).toContain('placeholder="Body"');
    expect(html).toContain("bg-secondary");
  });
});
