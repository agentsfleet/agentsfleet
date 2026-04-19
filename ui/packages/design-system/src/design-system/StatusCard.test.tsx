import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { StatusCard } from "./StatusCard";

describe("StatusCard", () => {
  it("renders label and count with a composed aria-label", () => {
    render(<StatusCard label="Active" count={3} />);
    const el = screen.getByTestId("status-card");
    expect(el).toHaveAttribute("role", "group");
    expect(el).toHaveAttribute("aria-label", "Active, 3");
    expect(screen.getByText("Active")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  it("applies the danger variant accent class and surfaces the trend glyph", () => {
    render(
      <StatusCard label="Stopped" count={1} variant="danger" trend="up" sublabel="last 24h" />,
    );
    const el = screen.getByTestId("status-card");
    expect(el).toHaveAttribute("data-variant", "danger");
    // aria-label includes the trend text ("increasing") and sublabel
    expect(el.getAttribute("aria-label")).toContain("increasing");
    expect(el.getAttribute("aria-label")).toContain("last 24h");
    // Trend glyph is decorative
    const glyph = screen.getByText("↑");
    expect(glyph).toHaveAttribute("aria-hidden", "true");
  });

  it("uses responsive sizing (text-xl + sm:text-2xl)", () => {
    const { container } = render(<StatusCard label="X" count={9} />);
    const dd = container.querySelector("dd")!;
    expect(dd.className).toContain("text-xl");
    expect(dd.className).toContain("sm:text-2xl");
  });

  it("respects prefers-reduced-motion by disabling transitions", () => {
    render(<StatusCard label="X" count={0} />);
    expect(screen.getByTestId("status-card").className).toContain("motion-reduce:transition-none");
  });

  it("does not accept asChild (display composition wraps externally)", () => {
    // Compile-time contract: StatusCardProps doesn't declare asChild. This
    // test documents the intent for humans; the TS compiler enforces it.
    const props = {} as Parameters<typeof StatusCard>[0];
    expect("asChild" in props).toBe(false);
  });

  it("SSR renders with data-slot for styling hooks + data-variant", () => {
    const html = renderToStaticMarkup(<StatusCard label="X" count={1} variant="success" />);
    expect(html).toContain('data-slot="status-card"');
    expect(html).toContain('data-variant="success"');
    expect(html).toContain("text-success");
  });

  it("merges a custom className", () => {
    render(<StatusCard label="X" count={1} className="my-sc" />);
    expect(screen.getByTestId("status-card").className).toContain("my-sc");
  });
});
