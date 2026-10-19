import { describe, expect, it } from "vitest";
import { METADATA, PRICE } from "@/tests/helpers/catalogue-fixtures";
import { CATALOGUE_LIMITS, SCHEDULE_ERRORS, adminModelSchema, boundedText, metadataSchema,
  nanosSchema, priceScheduleSchema, priceSchema } from "./catalogue-types";

describe("catalogue validation", () => {
  it("accepts metadata, nullable cache writes, empty conditions and adjacent ranges", () => {
    expect(metadataSchema.parse(METADATA)).toEqual(METADATA);
    expect(priceSchema.parse(PRICE)).toEqual(PRICE);
    const first = { ...PRICE, effective_until: "2026-12-01T00:00:00Z" };
    const next = { ...PRICE, effective_from: first.effective_until };
    expect(priceScheduleSchema.parse([first, next])).toEqual([first, next]);
    expect(priceScheduleSchema.parse([next, first])).toEqual([next, first]);
  });
  it("treats different prompt tiers as distinct schedules", () => {
    expect(priceScheduleSchema.parse([PRICE, { ...PRICE, prompt_tokens_above: 272000 }])).toHaveLength(2);
  });
  it.each([{ prices: [] }, { prices: Array.from({ length: CATALOGUE_LIMITS.prices + 1 }, () => PRICE) }])("refuses an out-of-bounds schedule", ({ prices }) => {
    const result = priceScheduleSchema.safeParse(prices);
    expect(result.success).toBe(false);
    if (!result.success) expect(result.error.issues.some((issue) => issue.message === SCHEDULE_ERRORS.count)).toBe(true);
  });
  it.each([
    { prices: [{ ...PRICE, prompt_tokens_above: 1 }] },
    { prices: [{ ...PRICE, effective_until: PRICE.effective_from }] },
    { prices: [PRICE, PRICE] },
    { prices: [{ ...PRICE, effective_from: "bad" }] },
  ])("refuses missing bases, reversed dates, overlaps and malformed instants", ({ prices }) => {
    expect(priceScheduleSchema.safeParse(prices).success).toBe(false);
  });
  it("orders parsed instants across offsets and sub-millisecond boundaries", () => {
    const first = { ...PRICE, effective_from: "2026-10-18T01:00:00.000000001+01:00", effective_until: "2026-10-18T00:00:00.000000002Z" };
    const second = { ...PRICE, effective_from: first.effective_until };
    expect(priceScheduleSchema.safeParse([first, second]).success).toBe(true);
    expect(priceScheduleSchema.safeParse([{ ...first, effective_until: first.effective_from }]).success).toBe(false);
  });
  it.each(["bad", "-1", "1.5", "01", "9223372036854775808"])("returns validation errors for invalid nanos %s", (value) => {
    expect(nanosSchema.safeParse(value).success).toBe(false);
  });
  it("bounds evidence by UTF-8 bytes and rejects missing required metadata", () => {
    expect(boundedText(2).safeParse("é").success).toBe(true);
    expect(boundedText(1).safeParse("é").success).toBe(false);
    expect(metadataSchema.safeParse({ ...METADATA, display_name: "" }).success).toBe(false);
    expect(metadataSchema.safeParse({ ...METADATA, context_verified_at: "2026-10-18T00:00:00.1234567890Z" }).success).toBe(false);
    expect(adminModelSchema.safeParse({ id: "missing-metadata" }).success).toBe(false);
  });
});
