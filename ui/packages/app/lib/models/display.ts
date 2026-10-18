// Shared catalogue labels, context windows and per-million prices.
import { nanosToUsdPerMtok } from "@/lib/api/admin-model-library-types";
import type { CataloguePrice } from "@/lib/api/catalogue-types";
import type { LibraryModel } from "@/lib/api/model-library-types";

export type RateFields = Pick<
  LibraryModel,
  "input_nanos_per_mtok" | "cached_input_nanos_per_mtok" | "output_nanos_per_mtok"
>;

type NamedModel = { model_id: string; display_name?: string | null };

export const EMPTY_VALUE = "—";
export const PRICE_UNKNOWN = "Unknown";
export const CONTEXT_HEADER = "Context";
export const RATES_HEADER = "Rates $/1M (in / cached / out)";

const RATE_SEPARATOR = " / ";
const RATE_MIN_DECIMALS = 2;
const MODEL_COLLATOR = new Intl.Collator("en-US", { numeric: true });

/** Catalogue names are authoritative. A custom model keeps its exact identifier. */
export function modelLabel(modelId: string, displayName?: string | null): string {
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
  if (!rates) return PRICE_UNKNOWN;
  return [rates.input_nanos_per_mtok, rates.cached_input_nanos_per_mtok, rates.output_nanos_per_mtok]
    .map(formatRatePerMtok).join(RATE_SEPARATOR);
}

export function formatRatePerMtok(nanos: string | null): string {
  if (nanos === null) return PRICE_UNKNOWN;
  const [whole, fraction = ""] = nanosToUsdPerMtok(nanos).split(".");
  return `${whole}.${fraction.padEnd(RATE_MIN_DECIMALS, "0")}`;
}

export function compareNanos(a: string | null, b: string | null): number {
  const left = a === null ? -1n : BigInt(a);
  const right = b === null ? -1n : BigInt(b);
  return left < right ? -1 : left > right ? 1 : 0;
}

export function currentBasePrice(prices: CataloguePrice[], now = Date.now()): CataloguePrice | null {
  return prices.find((price) => price.prompt_tokens_above === 0 && Date.parse(price.effective_from) <= now
    && (price.effective_until === null || now < Date.parse(price.effective_until))) ?? null;
}
