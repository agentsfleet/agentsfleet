import type { LibraryModel } from "@/lib/api/model-library-types";
import type { CatalogueMetadata, CataloguePrice } from "@/lib/api/catalogue-types";

export const VERIFIED_AT = "2026-10-18T00:00:00Z";
export const SOURCE_URL = "https://models.example.com/pricing";
export const METADATA: CatalogueMetadata = {
  display_name: "GLM 5.2", context_cap_tokens: 128000, context_source_url: SOURCE_URL,
  context_verified_at: VERIFIED_AT, advanced: false,
};
export const PRICE: CataloguePrice = {
  effective_from: VERIFIED_AT, effective_until: null, prompt_tokens_above: 0,
  input_nanos_per_mtok: "550000000", cached_input_nanos_per_mtok: "140000000", output_nanos_per_mtok: "2190000000",
  cache_write_nanos_per_mtok: null, cache_write_5m_nanos_per_mtok: null, cache_write_1h_nanos_per_mtok: null,
  source_url: SOURCE_URL, verified_at: VERIFIED_AT, conditions: "",
};
export const CATALOGUE_MODEL: LibraryModel = {
  ...METADATA, id: "glm-5.2", provider: "fireworks",
  input_nanos_per_mtok: PRICE.input_nanos_per_mtok, cached_input_nanos_per_mtok: PRICE.cached_input_nanos_per_mtok,
  output_nanos_per_mtok: PRICE.output_nanos_per_mtok, prices: [PRICE],
};
