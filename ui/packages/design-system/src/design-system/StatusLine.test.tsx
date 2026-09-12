import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { StatusLine, StatusLineItem, type StatusLineTone } from "./StatusLine";

describe("StatusLine", () => {
  it("renders a named, monospace, tabular line with a hairline between cells", () => {
    render(
      <StatusLine aria-label="Fleet summary">
        <StatusLineItem>3,255 tok</StatusLineItem>
        <StatusLineItem>$0.03</StatusLineItem>
      </StatusLine>,
    );
    const line = screen.getByLabelText("Fleet summary");
    expect(line.nodeName).toBe("DIV");
    for (const cls of ["font-mono", "tabular-nums", "text-label", "divide-x", "flex-wrap"]) {
      expect(line.className).toContain(cls);
    }
    expect(screen.getByText("3,255 tok")).toBeInTheDocument();
    expect(screen.getByText("$0.03")).toBeInTheDocument();
  });

  it("merges a caller's className on the line and on an item", () => {
    render(
      <StatusLine aria-label="Runner metrics" className="border-t">
        <StatusLineItem className="uppercase">active</StatusLineItem>
      </StatusLine>,
    );
    expect(screen.getByLabelText("Runner metrics").className).toContain("border-t");
    expect(screen.getByText("active").className).toContain("uppercase");
  });

  it("defaults an item to the neutral tone", () => {
    render(
      <StatusLine aria-label="line">
        <StatusLineItem>—</StatusLineItem>
      </StatusLine>,
    );
    const item = screen.getByText("—");
    expect(item.dataset.tone).toBe("neutral");
    expect(item.className).toContain("text-muted-foreground");
  });

  it.each([
    ["foreground", "text-foreground"],
    ["pulse", "text-pulse"],
    ["success", "text-success"],
    ["warning", "text-warning"],
    ["danger", "text-destructive"],
  ] as const satisfies ReadonlyArray<readonly [StatusLineTone, string]>)(
    "colours an item by its %s tone",
    (tone, cls) => {
      render(
        <StatusLine aria-label="line">
          <StatusLineItem tone={tone}>{tone}</StatusLineItem>
        </StatusLine>,
      );
      const item = screen.getByText(tone);
      expect(item.dataset.tone).toBe(tone);
      expect(item.className).toContain(cls);
    },
  );

  it("lets an item carry an icon ahead of its figure without a label collision", () => {
    render(
      <StatusLine aria-label="line">
        <StatusLineItem>
          <svg aria-hidden="true" data-testid="glyph" />
          <span className="sr-only">Duration </span>
          30.2s
        </StatusLineItem>
      </StatusLine>,
    );
    expect(screen.getByTestId("glyph")).toBeInTheDocument();
    expect(screen.getByText("Duration").className).toContain("sr-only");
    expect(screen.getByLabelText("line").textContent).toContain("30.2s");
  });
});
