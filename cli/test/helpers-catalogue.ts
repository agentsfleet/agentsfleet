import type { LibraryModel } from "../src/lib/model-catalogue-schema.ts";

const VERIFIED_AT = "2026-10-18T12:00:00Z";
const SOURCE = "https://provider.example/pricing";

export const catalogueModel = (
  id: string, provider: string, overrides: Partial<LibraryModel> = {},
): LibraryModel => ({
  id, provider, display_name: id, context_cap_tokens: 200000,
  context_source_url: SOURCE, context_verified_at: VERIFIED_AT, advanced: false,
  input_nanos_per_mtok: "5000000000", cached_input_nanos_per_mtok: "500000000",
  output_nanos_per_mtok: "25000000000",
  prices: [{ effective_from: VERIFIED_AT, effective_until: null, prompt_tokens_above: 0,
    input_nanos_per_mtok: "5000000000", cached_input_nanos_per_mtok: "500000000",
    output_nanos_per_mtok: "25000000000", cache_write_nanos_per_mtok: null,
    cache_write_5m_nanos_per_mtok: null, cache_write_1h_nanos_per_mtok: null,
    source_url: SOURCE, verified_at: VERIFIED_AT, conditions: "" }],
  ...overrides,
});

export const cataloguePage = <T>(models: ReadonlyArray<T>, next_cursor: string | null = null) =>
  ({ version: VERIFIED_AT, models, next_cursor, total: null });
