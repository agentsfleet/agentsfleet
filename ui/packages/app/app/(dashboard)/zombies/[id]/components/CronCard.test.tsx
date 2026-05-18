import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import CronCard from "./CronCard";

afterEach(() => cleanup());

describe("CronCard", () => {
  it("renders the raw cron expression in the header", () => {
    render(<CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} zombieId="zmb_x" />);
    expect(screen.getByText(/Cron — \*\/15 \* \* \* \*/)).toBeTruthy();
  });

  it("computes a next-fire when the expression parses", () => {
    render(<CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} zombieId="zmb_x" />);
    const node = screen.getByTestId("cron-next-fire");
    expect(node.textContent).toMatch(/Next fire/);
    expect(node.querySelector("time")).not.toBeNull();
  });

  it("shows the unparseable warning when the schedule is malformed", () => {
    render(<CronCard trigger={{ type: "cron", schedule: "not a cron" }} zombieId="zmb_x" />);
    const node = screen.getByTestId("cron-next-fire-error");
    expect(node.textContent).toMatch(/Schedule unparseable/);
    expect(node.textContent).toMatch(/TRIGGER\.md/);
  });

  it("links to the actor-filtered deliveries view", () => {
    render(<CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} zombieId="zmb_x" />);
    const link = screen.getByTestId("cron-deliveries-link");
    expect(link.getAttribute("href")).toBe("/zombies/zmb_x?actor=cron:*");
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
        <CronCard trigger={{ type: "cron", schedule: "*/15 * * * *" }} zombieId="zmb_x" />,
      );
      expect(screen.getByTestId("cron-next-fire").textContent).toContain("(UTC)");
    } finally {
      Intl.DateTimeFormat = OriginalDTF;
    }
  });
});
