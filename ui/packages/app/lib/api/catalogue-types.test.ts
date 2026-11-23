import { describe, expect, it } from "vitest";
import { METADATA, PRICE } from "@/tests/helpers/catalogue-fixtures";
import { metadataSchema,
  nanosSchema, priceSchema } from "./catalogue-types";

describe("catalogue validation", () => {
  it("accepts metadata, nullable cache writes and empty conditions", () => {
    expect(metadataSchema.parse(METADATA)).toEqual(METADATA);
    expect(priceSchema.parse(PRICE)).toEqual(PRICE);
  });
  it.each(["bad", "-1", "1.5", "01", "9223372036854775808"])("returns validation errors for invalid nanos %s", (value) => {
    expect(nanosSchema.safeParse(value).success).toBe(false);
  });
  it("takes evidence of any length, leaving its bound to the server, and rejects missing required metadata", () => {
    const longUrl = `https://example.com/${"é".repeat(4096)}`;
    expect(metadataSchema.parse({ ...METADATA, context_source_url: longUrl }).context_source_url).toBe(longUrl);
    expect(priceSchema.parse({ ...PRICE, conditions: "x".repeat(4096) }).conditions).toHaveLength(4096);
    expect(metadataSchema.safeParse({ ...METADATA, display_name: "" }).success).toBe(false);
    expect(metadataSchema.safeParse({ ...METADATA, context_verified_at: "2026-10-18T00:00:00.1234567890Z" }).success).toBe(false);
  });
});
