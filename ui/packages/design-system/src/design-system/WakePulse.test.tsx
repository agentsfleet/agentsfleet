import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { WakePulse } from "./WakePulse";

describe("WakePulse", () => {
  it("sets data-live when live=true", () => {
    const { container } = render(<WakePulse live={true} data-testid="p" />);
    const el = container.firstElementChild as HTMLElement;
    expect(el.hasAttribute("data-live")).toBe(true);
  });

  it("omits data-live when live=false", () => {
    const { container } = render(<WakePulse live={false} data-testid="p" />);
    const el = container.firstElementChild as HTMLElement;
    expect(el.hasAttribute("data-live")).toBe(false);
  });

  it("renders a span by default", () => {
    const { container } = render(<WakePulse live={true}>x</WakePulse>);
    expect(container.firstElementChild?.tagName).toBe("SPAN");
  });

  it("composes onto the child when asChild=true", () => {
    const { container } = render(
      <WakePulse asChild live={true}>
        <button type="button">go</button>
      </WakePulse>,
    );
    const btn = container.querySelector("button");
    expect(btn).not.toBeNull();
    expect(btn!.hasAttribute("data-live")).toBe(true);
    expect(container.querySelectorAll("span")).toHaveLength(0);
  });

  it("passes className through", () => {
    const { container } = render(
      <WakePulse live={true} className="size-2 rounded-full bg-pulse" />,
    );
    expect((container.firstElementChild as HTMLElement).className).toContain(
      "rounded-full",
    );
  });

  it("forwards arbitrary HTML attributes", () => {
    const { container } = render(
      <WakePulse live={true} aria-label="running zombie" />,
    );
    expect(
      (container.firstElementChild as HTMLElement).getAttribute("aria-label"),
    ).toBe("running zombie");
  });

  it("toggles data-live across re-renders", () => {
    const { container, rerender } = render(<WakePulse live={true} />);
    expect(
      (container.firstElementChild as HTMLElement).hasAttribute("data-live"),
    ).toBe(true);
    rerender(<WakePulse live={false} />);
    expect(
      (container.firstElementChild as HTMLElement).hasAttribute("data-live"),
    ).toBe(false);
  });
});
