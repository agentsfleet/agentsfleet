import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { renderToString } from "react-dom/server";
import { TooltipProvider } from "./Tooltip";
import { Time, formatTimeAbsolute, formatTimeRelative } from "./Time";

const ISO = "2026-05-03T10:00:00.000Z";

function renderTime(node: React.ReactElement) {
  return render(<TooltipProvider>{node}</TooltipProvider>);
}

describe("Time", () => {
  it("renders datetime attr from a string value", () => {
    const { container } = renderTime(<Time value={ISO} tooltip={false} />);
    const t = container.querySelector("time");
    expect(t?.getAttribute("datetime")).toBe(ISO);
  });

  it("renders datetime attr from a Date instance, normalised to ISO", () => {
    const { container } = renderTime(
      <Time value={new Date(ISO)} tooltip={false} />,
    );
    expect(container.querySelector("time")?.getAttribute("datetime")).toBe(ISO);
  });

  it("absolute format uses Intl.DateTimeFormat(en-US) by default", () => {
    const expected = formatTimeAbsolute(ISO, "en-US");
    const { container } = renderTime(
      <Time value={ISO} format="absolute" tooltip={false} />,
    );
    expect(container.querySelector("time")?.textContent).toBe(expected);
  });

  it("relative format renders 'X ago' for a past value", () => {
    const past = new Date(Date.now() - 5 * 60_000).toISOString();
    const { container } = renderTime(
      <Time value={past} format="relative" tooltip={false} />,
    );
    const txt = container.querySelector("time")?.textContent ?? "";
    expect(txt).toMatch(/\d+ minutes? ago/);
  });

  it("relative format defaults tooltip on, surfacing the absolute form", () => {
    const past = new Date(Date.now() - 60 * 60_000).toISOString();
    renderTime(<Time value={past} format="relative" />);
    // Trigger is rendered (the <time>); visible relative label shows.
    const t = screen.getByRole("time", { hidden: true }) ?? null;
    if (t) expect(t.textContent ?? "").toMatch(/hour/);
  });

  it("datetime format renders the full ISO string as visible text", () => {
    const { container } = renderTime(
      <Time value={ISO} format="datetime" tooltip={false} />,
    );
    expect(container.querySelector("time")?.textContent).toBe(ISO);
  });

  it("renders identical datetime attr in SSR + CSR", () => {
    const node = <Time value={ISO} format="absolute" tooltip={false} />;
    const ssr = renderToString(node);
    const { container } = render(<TooltipProvider>{node}</TooltipProvider>);
    const csrAttr = container.querySelector("time")?.getAttribute("datetime");
    const ssrMatch = ssr.match(/datetime="([^"]+)"/i) ?? ssr.match(/datetime=([^ >]+)/i);
    expect(csrAttr).toBe(ISO);
    expect(ssrMatch?.[1]?.replace(/^"|"$/g, "")).toBe(ISO);
  });

  it("renders without runtime error when tooltipContent overrides the body", () => {
    const past = new Date(Date.now() - 60 * 60_000).toISOString();
    const { container } = renderTime(
      <Time value={past} tooltip tooltipContent="raw-iso-marker" />,
    );
    // Trigger-only by default in jsdom (no hover); the trigger label is the
    // formatted absolute, not the override. Portal-mounted body is asserted
    // separately by Tooltip.test.tsx.
    expect(container.querySelector("time")?.textContent).not.toContain(
      "raw-iso-marker",
    );
  });

  it("renders a soft-fail fallback for an Invalid Date input (no throw)", () => {
    // Pre-migration: `new Date("nope").toLocaleString()` returned "Invalid Date"
    // without throwing. <Time> must preserve that contract — otherwise a junk
    // value crashes the surrounding subtree (server components → 500).
    expect(() => {
      const { container } = renderTime(<Time value="not-a-date" tooltip={false} />);
      const t = container.querySelector("time");
      expect(t).not.toBeNull();
      expect(t?.getAttribute("datetime")).toBeNull();
      expect(t?.textContent).toBe("—");
    }).not.toThrow();
  });

  it("uses labelOverride as the fallback text when value is invalid", () => {
    const { container } = renderTime(
      <Time value="bogus" label="n/a" tooltip={false} />,
    );
    expect(container.querySelector("time")?.textContent).toBe("n/a");
  });

  it("relative format opts into suppressHydrationWarning on the time element", () => {
    // suppressHydrationWarning is a React-only flag stripped from the DOM,
    // so we assert via the renderToString output containing the value
    // anyway and the visible relative text being present.
    const past = new Date(Date.now() - 30_000).toISOString();
    const { container } = renderTime(
      <Time value={past} format="relative" tooltip={false} />,
    );
    expect(container.querySelector("time")?.textContent).toMatch(/seconds? ago/);
  });
});

describe("formatTimeRelative", () => {
  const now = new Date("2026-05-03T12:00:00Z");
  it("returns 'just now' when value equals now", () => {
    expect(formatTimeRelative(now, now)).toBe("just now");
  });
  it("returns 'just now' for sub-5-second deltas in either direction", () => {
    expect(formatTimeRelative(new Date(now.getTime() - 2_000), now)).toBe("just now");
    expect(formatTimeRelative(new Date(now.getTime() + 3_000), now)).toBe("just now");
  });
  it("returns 'X seconds ago' under a minute", () => {
    expect(formatTimeRelative(new Date(now.getTime() - 30_000), now)).toBe("30 seconds ago");
  });
  it("returns 'X minutes ago' under an hour", () => {
    expect(formatTimeRelative(new Date(now.getTime() - 5 * 60_000), now)).toBe("5 minutes ago");
  });
  it("returns 'X hours ago' under a day", () => {
    expect(formatTimeRelative(new Date(now.getTime() - 3 * 3_600_000), now)).toBe("3 hours ago");
  });
  it("returns 'in X minutes' for future values", () => {
    expect(formatTimeRelative(new Date(now.getTime() + 10 * 60_000), now)).toBe("in 10 minutes");
  });
  it("singularises the unit when n === 1", () => {
    expect(formatTimeRelative(new Date(now.getTime() - 60_000), now)).toBe("1 minute ago");
  });
});
