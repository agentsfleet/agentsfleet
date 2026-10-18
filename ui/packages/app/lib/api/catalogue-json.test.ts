import { describe, expect, it } from "vitest";
import { ZodError } from "zod";
import { readCatalogueBody, stringifyCatalogueBody } from "./catalogue-json";

describe("catalogue JSON integer boundaries", () => {
  it("keeps int64 source digits exact through decode and re-encode", async () => {
    const source = '{"prices":[{"input_nanos_per_mtok":9223372036854775807,"cache_write_nanos_per_mtok":null}],"context_cap_tokens":128000}';
    const body = await readCatalogueBody(new Response(source));
    expect(body).toEqual({ prices: [{ input_nanos_per_mtok: "9223372036854775807", cache_write_nanos_per_mtok: null }], context_cap_tokens: 128000 });
    expect(stringifyCatalogueBody(body)).toBe(source);
  });
  it.each(['"1"', 'true', '{}', '[]'])("refuses a nonnumeric wire rate %s", async (value) => {
    await expect(readCatalogueBody(new Response(`{"input_nanos_per_mtok":${value}}`))).rejects.toThrow(TypeError);
  });
  it.each(["-1", "1.5", "1e3", "9223372036854775808"])("refuses an invalid wire integer %s", async (value) => {
    await expect(readCatalogueBody(new Response(`{"input_nanos_per_mtok":${value}}`))).rejects.toThrow(ZodError);
  });
  it("propagates malformed JSON and broken body reads", async () => {
    await expect(readCatalogueBody(new Response("{"))).rejects.toThrow(SyntaxError);
    const failure = new Error("body interrupted");
    const body = new ReadableStream({ start(controller) { controller.error(failure); } });
    await expect(readCatalogueBody(new Response(body))).rejects.toBe(failure);
  });
  it.each(["-1", "01", "1.5", "9223372036854775808", 1, true])("refuses invalid outgoing nanos %s", (value) => {
    expect(() => stringifyCatalogueBody({ output_nanos_per_mtok: value })).toThrow(ZodError);
  });
});
