// Shared catalogue labels, context windows and per-million prices.
import { nanosToUsdPerMtok } from "@/lib/api/admin-model-library-types";
import type { LibraryModel } from "@/lib/api/model-library-types";

export type RateFields = Pick<
  LibraryModel,
  "input_nanos_per_mtok" | "cached_input_nanos_per_mtok" | "output_nanos_per_mtok"
>;

type NamedModel = { model_id: string; display_name?: string };

export const EMPTY_VALUE = "—";
export const PRICE_UNKNOWN = "Unknown";
export const CONTEXT_HEADER = "Context";
export const RATES_HEADER = "Rates $/1M (in / cached / out)";

const RATE_SEPARATOR = " / ";
const RATE_MIN_DECIMALS = 2;
const RATE_MAX_DECIMALS = 9;
const RATE_FORMAT = new Intl.NumberFormat("en-US", {
  minimumFractionDigits: RATE_MIN_DECIMALS,
  maximumFractionDigits: RATE_MAX_DECIMALS,
  useGrouping: false,
});
const MODEL_COLLATOR = new Intl.Collator("en-US", { numeric: true });

/** Catalogue names are authoritative. A custom model keeps its exact identifier. */
export function modelLabel(modelId: string, displayName?: string): string {
  return displayName ?? modelId;
}

/** Sort by the displayed name, then exact identifier for equal names. */
export function compareModels(a: NamedModel, b: NamedModel): number {
  return MODEL_COLLATOR.compare(modelLabel(a.model_id, a.display_name), modelLabel(b.model_id, b.display_name))
    || MODEL_COLLATOR.compare(a.model_id, b.model_id);
}

export function modelIdCopyLabel(modelId: string): string {
  return `Copy model id: ${modelId}`;
}

export function formatContextTokens(tokens: number | undefined): string {
  if (tokens == null) return EMPTY_VALUE;
  return String(Math.trunc(tokens)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** Keep every stored fractional dollar digit visible, including sub-cent rates. */
export function formatRatesPerMtok(rates: RateFields | null): string {
  if (!rates) return EMPTY_VALUE;
  return [rates.input_nanos_per_mtok, rates.cached_input_nanos_per_mtok, rates.output_nanos_per_mtok]
    .map((nanos) => RATE_FORMAT.format(nanosToUsdPerMtok(nanos))).join(RATE_SEPARATOR);
}
