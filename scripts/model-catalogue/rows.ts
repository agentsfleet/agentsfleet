import { z } from "zod";
import { nanosPerMtok, type Catalogue, type Price } from "./catalogue";

const INSTANT = z.iso.datetime({ offset: true }).transform((value) => new Date(value).toISOString());
const RATE = z.string().regex(/^\d+$/);
const priceRowSchema = z.strictObject({
  effective_from: INSTANT, effective_until: INSTANT.nullable(), prompt_tokens_above: z.int().nonnegative(),
  input_nanos_per_mtok: RATE, cached_input_nanos_per_mtok: RATE,
  cache_write_nanos_per_mtok: RATE.nullable(),
  cache_write_5m_nanos_per_mtok: RATE.nullable(), cache_write_1h_nanos_per_mtok: RATE.nullable(),
  output_nanos_per_mtok: RATE, source_url: z.url(), verified_at: INSTANT, conditions: z.string(),
});
const modelRowSchema = z.strictObject({
  provider: z.string(), model_name: z.string(), display_name: z.string(), advanced: z.boolean(),
  context_cap_tokens: z.int().positive(), context_source_url: z.url(), context_verified_at: INSTANT,
  prices: z.array(priceRowSchema).transform((prices) => prices.sort(comparePrices)),
});
export const modelRowsSchema = z.array(modelRowSchema);
export type ModelRow = z.infer<typeof modelRowSchema>;
export type PriceRow = z.infer<typeof priceRowSchema>;
export type CatalogueDiff = { added: string[]; changed: string[]; deleted: string[] };

/** Only contexts proved by the serving provider enter the database. */
export function desiredRows(catalogue: Catalogue): ModelRow[] {
  return catalogue.models.flatMap((model) => model.context === null ? [] : [modelRowSchema.parse({
    provider: model.provider, model_name: model.model_name, display_name: model.display_name, advanced: model.advanced,
    context_cap_tokens: model.context.tokens, context_source_url: model.context.source_url,
    context_verified_at: model.context.verified_at, prices: model.prices.map(priceRow),
  })]);
}

export function catalogueDiff(wanted: ModelRow[], live: ModelRow[]): CatalogueDiff {
  const current = new Map(live.map((model) => [modelKey(model), model]));
  const desired = new Map(wanted.map((model) => [modelKey(model), model]));
  return {
    added: wanted.filter((model) => !current.has(modelKey(model))).map(modelKey),
    changed: wanted.filter((model) => current.has(modelKey(model))
      && !Bun.deepEquals(model, current.get(modelKey(model)))).map(modelKey),
    deleted: live.filter((model) => !desired.has(modelKey(model))).map(modelKey),
  };
}

export function hasChanges(diff: CatalogueDiff): boolean {
  return diff.added.length + diff.changed.length + diff.deleted.length > 0;
}

function modelKey(model: Pick<ModelRow, "provider" | "model_name">): string {
  return `${model.provider}/${model.model_name}`;
}

function comparePrices(left: PriceRow, right: PriceRow): number {
  return left.prompt_tokens_above - right.prompt_tokens_above
    || Date.parse(left.effective_from) - Date.parse(right.effective_from);
}

function priceRow(price: Price): PriceRow {
  return priceRowSchema.parse({
    effective_from: price.effective_from, effective_until: price.effective_until, prompt_tokens_above: price.prompt_tokens_above,
    input_nanos_per_mtok: nanosPerMtok(price.input), cached_input_nanos_per_mtok: nanosPerMtok(price.cached_input),
    cache_write_nanos_per_mtok: price.cache_write === null ? null : nanosPerMtok(price.cache_write),
    cache_write_5m_nanos_per_mtok: price.cache_write_5m === null ? null : nanosPerMtok(price.cache_write_5m),
    cache_write_1h_nanos_per_mtok: price.cache_write_1h === null ? null : nanosPerMtok(price.cache_write_1h),
    output_nanos_per_mtok: nanosPerMtok(price.output), source_url: price.source_url,
    verified_at: price.verified_at, conditions: price.conditions,
  });
}
