import type { AdminModel, ModelInference } from "@/lib/api/admin_model_library";
import { z } from "zod";
import { INFERENCE_API, nanosToUsdPerMtok, usdPerMtokToNanos } from "@/lib/api/admin-model-library-types";

const positiveInteger = z.string().trim().refine((value) => value !== "" && Number.isSafeInteger(Number(value)) && Number(value) > 0, "Enter a positive integer");
const rate = z.string().trim().refine((value) => value !== "" && Number.isFinite(Number(value)) && Number(value) >= 0, "Enter a number of zero or more");
const endpoint = z.url().pipe(z.string().refine((value) => {
  const url = new URL(value);
  return url.protocol === "https:" && !url.username && !url.password && !url.search && !url.hash;
}, "Use an HTTPS base URL without credentials, query or fragment"));
export const settingsSchema = z.object({
  context_cap_tokens: positiveInteger,
  input_usd: rate, cached_usd: rate, output_usd: rate,
  api: z.enum(INFERENCE_API), base_url: endpoint,
  dialect: z.string().trim().max(64), max_output_tokens: positiveInteger,
}).refine((value) => Number(value.max_output_tokens) <= Number(value.context_cap_tokens), {
  path: ["max_output_tokens"], message: "Output limit must fit in the model context window",
});
export type ModelSettingsValues = z.infer<typeof settingsSchema>;
export const SETTINGS_DEFAULTS: ModelSettingsValues = {
  context_cap_tokens: "", input_usd: "0", cached_usd: "0", output_usd: "0",
  api: INFERENCE_API.chat, base_url: "", dialect: "", max_output_tokens: "",
};

export function valuesFromModel(model: AdminModel): ModelSettingsValues {
  return {
    context_cap_tokens: String(model.context_cap_tokens),
    input_usd: String(nanosToUsdPerMtok(model.input_nanos_per_mtok)),
    cached_usd: String(nanosToUsdPerMtok(model.cached_input_nanos_per_mtok)),
    output_usd: String(nanosToUsdPerMtok(model.output_nanos_per_mtok)),
    api: model.inference?.api ?? INFERENCE_API.chat,
    base_url: model.inference?.base_url ?? "", dialect: model.inference?.dialect ?? "",
    max_output_tokens: model.inference ? String(model.inference.max_output_tokens) : "",
  };
}

export function modelSettings(values: ModelSettingsValues) {
  const inference: ModelInference = {
    api: values.api, base_url: values.base_url,
    max_output_tokens: Number(values.max_output_tokens),
    ...(values.dialect ? { dialect: values.dialect } : {}),
  };
  return {
    context_cap_tokens: Number(values.context_cap_tokens),
    input_nanos_per_mtok: usdPerMtokToNanos(Number(values.input_usd)),
    cached_input_nanos_per_mtok: usdPerMtokToNanos(Number(values.cached_usd)),
    output_nanos_per_mtok: usdPerMtokToNanos(Number(values.output_usd)), inference,
  };
}
