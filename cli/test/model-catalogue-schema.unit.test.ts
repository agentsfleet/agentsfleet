import { expect, test } from "bun:test";
import { Effect } from "effect";
import { decodeCatalogue } from "../src/lib/model-catalogue-schema.ts";
import { catalogueModel, cataloguePage } from "./helpers-catalogue.ts";

const model = catalogueModel("provider/model", "provider", { input_nanos_per_mtok: "9223372036854775807" });

test("catalogue JSON retains labels, full schedules and evidence without rounding", async () => {
  const page = cataloguePage([model]);
  expect(await Effect.runPromise(decodeCatalogue(page))).toEqual(page);
});
test.each([7, null, "01", "-1", "9223372036854775808"])("refuses an invalid model rate %j", async input_nanos_per_mtok => {
  const error = await Effect.runPromise(decodeCatalogue(cataloguePage([{ ...model, input_nanos_per_mtok }])).pipe(Effect.flip));
  expect(error).toMatchObject({ code: "MODEL_CATALOGUE_RESPONSE_INVALID", status: 502 });
});
test.each([7, "9223372036854775808"])("validates nested cache-write quotes %j", async cache_write_1h_nanos_per_mtok => {
  const error = await Effect.runPromise(decodeCatalogue(cataloguePage([
    { ...model, prices: [{ ...model.prices[0], cache_write_1h_nanos_per_mtok }] },
  ])).pipe(Effect.flip));
  expect(error).toMatchObject({ code: "MODEL_CATALOGUE_RESPONSE_INVALID" });
});
test("missing catalogue metadata cannot silently become a blank table cell", async () => {
  const error = await Effect.runPromise(decodeCatalogue(cataloguePage([{ id: "m", provider: "p" }])).pipe(Effect.flip));
  expect(error).toMatchObject({ code: "MODEL_CATALOGUE_RESPONSE_INVALID" });
});
