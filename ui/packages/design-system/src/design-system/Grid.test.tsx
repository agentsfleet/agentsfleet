import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import Grid from "./Grid";

describe("Grid", () => {
  it("renders children in a grid container", () => {
    const { container } = render(
      <Grid columns="two">
        <div>A</div>
        <div>B</div>
      </Grid>,
    );
    const el = container.firstChild as HTMLElement;
    expect(el.tagName).toBe("DIV");
    expect(el.className).toContain("grid");
    expect(el.children).toHaveLength(2);
  });

  it("merges extra className", () => {
    const { container } = render(
      <Grid columns="two" className="custom">
        x
      </Grid>,
    );
    const cls = (container.firstChild as HTMLElement).className;
    expect(cls).toContain("grid");
    expect(cls).toContain("custom");
  });

  it("forwards ref to the underlying div", () => {
    const ref = { current: null as HTMLDivElement | null };
    render(
      <Grid ref={ref} columns="two">
        x
      </Grid>,
    );
    expect(ref.current).toBeInstanceOf(HTMLDivElement);
  });
});
