import { describe, expect, it } from "vitest";
import { METADATA, PRICE } from "@/tests/helpers/catalogue-fixtures";
import { boundedText, metadataSchema,
  nanosSchema, priceSchema } from "./catalogue-types";

describe("catalogue validation", () => {
  it("accepts metadata, nullable cache writes and empty conditions", () => {
    expect(metadataSchema.parse(METADATA)).toEqual(METADATA);
    expect(priceSchema.parse(PRICE)).toEqual(PRICE);
  });
  it.each(["bad", "-1", "1.5", "01", "9223372036854775808"])("returns validation errors for invalid nanos %s", (value) => {
    expect(nanosSchema.safeParse(value).success).toBe(false);
  });
  it("bounds evidence by UTF-8 bytes and rejects missing required metadata", () => {
    expect(boundedText(2).safeParse("é").success).toBe(true);
    expect(boundedText(1).safeParse("é").success).toBe(false);
    expect(metadataSchema.safeParse({ ...METADATA, display_name: "" }).success).toBe(false);
    expect(metadataSchema.safeParse({ ...METADATA, context_verified_at: "2026-10-18T00:00:00.1234567890Z" }).success).toBe(false);
  });
});
