import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import LogLine, { LogToken, type LogSeverity } from "./LogLine";

const cases: Array<{ severity: LogSeverity; token: string }> = [
  { severity: "info", token: "text-foreground" },
  { severity: "debug", token: "text-muted-foreground" },
  { severity: "done", token: "text-success" },
  { severity: "evidence", token: "text-evidence" },
  { severity: "warn", token: "text-warning" },
  { severity: "error", token: "text-destructive" },
  { severity: "pulse", token: "text-pulse" },
];

describe("LogLine", () => {
  it("renders a <div> in mono with the foreground token at the default severity", () => {
    const { container } = render(<LogLine>plain line</LogLine>);
    const el = container.firstElementChild as HTMLElement;
    expect(el.tagName).toBe("DIV");
    expect(el.className).toContain("font-mono");
    expect(el.className).toContain("text-foreground");
  });

  it("applies the token-driven colour for every severity", () => {
    for (const { severity, token } of cases) {
      const { container, unmount } = render(
        <LogLine severity={severity}>{severity}</LogLine>,
      );
      expect((container.firstElementChild as HTMLElement).className).toContain(
        token,
      );
      unmount();
    }
  });

  it("merges a caller className alongside the severity token", () => {
    const { container } = render(
      <LogLine severity="warn" className="indent-4">
        x
      </LogLine>,
    );
    const cls = (container.firstElementChild as HTMLElement).className;
    expect(cls).toContain("text-warning");
    expect(cls).toContain("indent-4");
  });

  it("forwards arbitrary HTML attributes to the underlying div", () => {
    const { container } = render(
      <LogLine severity="error" data-testid="ll" title="boom">
        x
      </LogLine>,
    );
    const el = container.firstElementChild as HTMLElement;
    expect(el.getAttribute("data-testid")).toBe("ll");
    expect(el.getAttribute("title")).toBe("boom");
  });

  it("renders its children content", () => {
    const { getByText } = render(<LogLine severity="done">build OK</LogLine>);
    expect(getByText("build OK")).toBeTruthy();
  });
});

describe("LogToken", () => {
  it("renders a <span> in mono with the foreground token at the default severity", () => {
    const { container } = render(<LogToken>INFO</LogToken>);
    const el = container.firstElementChild as HTMLElement;
    expect(el.tagName).toBe("SPAN");
    expect(el.className).toContain("font-mono");
    expect(el.className).toContain("text-foreground");
  });

  it("applies the token-driven colour for every severity", () => {
    for (const { severity, token } of cases) {
      const { container, unmount } = render(
        <LogToken severity={severity}>{severity.toUpperCase()}</LogToken>,
      );
      expect((container.firstElementChild as HTMLElement).className).toContain(
        token,
      );
      unmount();
    }
  });

  it("merges a caller className alongside the severity token", () => {
    const { container } = render(
      <LogToken severity="evidence" className="font-bold">
        EVIDENCE
      </LogToken>,
    );
    const cls = (container.firstElementChild as HTMLElement).className;
    expect(cls).toContain("text-evidence");
    expect(cls).toContain("font-bold");
  });

  it("forwards arbitrary HTML attributes to the underlying span", () => {
    const { container } = render(
      <LogToken severity="pulse" data-testid="tok">
        PULSE
      </LogToken>,
    );
    expect((container.firstElementChild as HTMLElement).getAttribute("data-testid")).toBe(
      "tok",
    );
  });

  it("composes inside a LogLine so a colored tag sits beside monochrome metadata", () => {
    const { getByTestId } = render(
      <LogLine severity="info" data-testid="line">
        2026-05-08T03:14:23Z{" "}
        <LogToken severity="evidence" data-testid="tag">
          EVIDENCE
        </LogToken>{" "}
        cd_logs:281-294
      </LogLine>,
    );
    expect(getByTestId("line").className).toContain("text-foreground");
    expect(getByTestId("tag").className).toContain("text-evidence");
  });
});
