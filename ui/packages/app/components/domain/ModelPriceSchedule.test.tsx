import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { CatalogueEvidence, ModelPriceSchedule } from "./ModelPriceSchedule";
import { PRICE, SOURCE_URL, VERIFIED_AT } from "@/tests/helpers/catalogue-fixtures";

afterEach(cleanup);

describe("complete price disclosure", () => {
  it("shows every tier, rate, date, condition and evidence after expanding", () => {
    render(<ModelPriceSchedule prices={[
      { ...PRICE, effective_until: "2026-12-01T00:00:00Z", conditions: "Promotion" },
      { ...PRICE, prompt_tokens_above: 272000, cache_write_nanos_per_mtok: "0",
        cache_write_5m_nanos_per_mtok: "62500000", cache_write_1h_nanos_per_mtok: "9223372036854775807" },
    ]} />);
    fireEvent.click(screen.getByRole("button", { name: "Price schedule (2)" }));
    expect(screen.getByText("Base tier")).toBeTruthy();
    expect(screen.getByText("Whole prompt above 272,000 tokens")).toBeTruthy();
    expect(screen.getAllByText("Unknown")).toHaveLength(3);
    expect(screen.getByText("0.00")).toBeTruthy();
    expect(screen.getByText("0.0625")).toBeTruthy();
    expect(screen.getByText("9223372036.854775807")).toBeTruthy();
    expect(screen.getByText("Promotion")).toBeTruthy();
    expect(screen.getByText("Open-ended")).toBeTruthy();
    expect(screen.getAllByRole("link", { name: "Source" })[0]?.getAttribute("href")).toBe(SOURCE_URL);
  });
  it("labels an absent schedule unknown", () => {
    render(<ModelPriceSchedule prices={[]} />);
    expect(screen.getByText("Unknown")).toBeTruthy();
  });
  it("renders malformed or unsafe source text without making it a link", () => {
    render(<CatalogueEvidence source="javascript:alert(1)" verifiedAt={VERIFIED_AT} />);
    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.getByText("javascript:alert(1)")).toBeTruthy();
  });
});

it("keeps the complete source timestamps visible, including sub-millisecond precision", () => {
  const instant = "2026-10-18T00:00:00.123456789Z";
  render(<ModelPriceSchedule prices={[{ ...PRICE, effective_from: instant, verified_at: instant }]} />);
  fireEvent.click(screen.getByRole("button", { name: "Price schedule (1)" }));
  expect(screen.getAllByText(instant)).toHaveLength(2);
});
