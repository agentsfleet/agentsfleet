import type { LibraryModel } from "@/lib/api/model-library-types";
import type { AdminModel } from "@/lib/api/admin_model_library";
import type { CatalogueMetadata, CataloguePrice } from "@/lib/api/catalogue-types";
import { INFERENCE_API } from "@/lib/api/admin-model-library-types";

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
export const ADMIN_MODEL: AdminModel = {
  id: "u1", provider: "fireworks", model_id: "glm-5.2", metadata: METADATA, prices: [PRICE],
  inference: { api: INFERENCE_API.chat, base_url: "https://models.example.com/v1", max_output_tokens: 8192 },
};
export const ADMIN_CATALOGUE: AdminModel[] = [
  ADMIN_MODEL,
  { ...ADMIN_MODEL, id: "u2", provider: "anthropic", model_id: "claude-opus-4-8",
    metadata: { ...METADATA, display_name: "Opus 4.8", context_cap_tokens: 200000 },
    prices: [{ ...PRICE, input_nanos_per_mtok: "15000000000", cached_input_nanos_per_mtok: "1500000000", output_nanos_per_mtok: "75000000000" }],
    inference: { api: INFERENCE_API.messages, base_url: "https://anthropic.example.com/v1", max_output_tokens: 16384 } },
];

export const CATALOGUE_MODEL: LibraryModel = {
  ...METADATA, id: ADMIN_MODEL.model_id, provider: ADMIN_MODEL.provider,
  input_nanos_per_mtok: PRICE.input_nanos_per_mtok, cached_input_nanos_per_mtok: PRICE.cached_input_nanos_per_mtok,
  output_nanos_per_mtok: PRICE.output_nanos_per_mtok, prices: [PRICE],
};
