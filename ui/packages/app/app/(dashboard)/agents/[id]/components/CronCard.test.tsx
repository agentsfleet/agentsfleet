import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { CronExpressionParser } from "cron-parser";
import CronCard from "./CronCard";

afterEach(() => cleanup());

describe("CronCard", () => {
  it("renders the raw cron expression in the header", () => {
    render(<CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} agentId="agt_ax" />);
    expect(screen.getByText(/Cron — \*\/15 \* \* \* \*/)).toBeTruthy();
  });

  it("computes a next-fire when the expression parses", () => {
    render(<CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} agentId="agt_ax" />);
    const node = screen.getByTestId("cron-next-fire");
    expect(node.textContent).toMatch(/Next fire/);
    expect(node.querySelector("time")).not.toBeNull();
  });

  it("shows the unparseable warning when the schedule is malformed", () => {
    render(<CronCard trigger={{ type: "cron", schedule: "not a cron" }} agentId="agt_ax" />);
    const node = screen.getByTestId("cron-next-fire-error");
    expect(node.textContent).toMatch(/Schedule unparseable/);
    expect(node.textContent).toMatch(/TRIGGER\.md/);
  });

  it("links to the actor-filtered deliveries view", () => {
    render(<CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} agentId="agt_ax" />);
    const link = screen.getByTestId("cron-deliveries-link");
    expect(link.getAttribute("href")).toBe("/agents/agt_ax?actor=cron:*");
  });

  it("uses the literal 'unparseable' fallback reason when the parser throws a non-Error", () => {
    // Branch coverage: the `err instanceof Error ? err.message : "unparseable"`
    // ternary's right side. Real cron-parser always throws Error subclasses;
    // we stub it to throw a plain string to force the fallback path.
    const spy = vi.spyOn(CronExpressionParser, "parse").mockImplementation(() => {
      throw "raw string, not an Error";
    });
    try {
      render(
        <CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} agentId="agt_ax" />,
      );
      expect(screen.getByTestId("cron-next-fire-error").textContent).toMatch(
        /Schedule unparseable — check/,
      );
    } finally {
      spy.mockRestore();
    }
  });

  it("falls back to UTC when the runtime cannot resolve a timezone", () => {
    const OriginalDTF = Intl.DateTimeFormat;
    const StubDTF = function (...args: ConstructorParameters<typeof Intl.DateTimeFormat>) {
      const inner = new OriginalDTF(...args);
      const origResolved = inner.resolvedOptions.bind(inner);
      inner.resolvedOptions = () => ({ ...origResolved(), timeZone: "" });
      return inner;
    } as unknown as typeof Intl.DateTimeFormat;
    Intl.DateTimeFormat = StubDTF;
    try {
      render(
        <CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} agentId="agt_ax" />,
      );
      expect(screen.getByTestId("cron-next-fire").textContent).toContain("(UTC)");
    } finally {
      Intl.DateTimeFormat = OriginalDTF;
    }
  });
});
