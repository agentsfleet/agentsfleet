import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import FleetStatusLine from "./FleetStatusLine";
import { METRICS_UNAVAILABLE } from "./console-copy";

afterEach(cleanup);

it("reports unavailable metrics and announces the labels for each unknown figure", () => {
  render(<FleetStatusLine status="active" />);
  const line = screen.getByLabelText("Fleet summary");
  expect(line.textContent).toContain("Latest outcome " + METRICS_UNAVAILABLE);
  expect(line.textContent).toMatch(/Tokens\s*—Spend\s*—Duration\s*—/);
  expect(screen.getByText("active").closest("[data-tone]")?.getAttribute("data-tone")).toBe("pulse");
});

it("keeps an archived fleet identifiable when run data is unavailable", () => {
  render(<FleetStatusLine status="archived" />);
  expect(screen.getByText("Archived").closest("[data-tone]")?.getAttribute("data-tone")).toBe("neutral");
});
