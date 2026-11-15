// Shared catalogue labels, context windows and per-million prices.
import { nanosToUsdPerMtok } from "@/lib/api/admin-model-library-types";
import type { LibraryModel } from "@/lib/api/model-library-types";

export type RateFields = Pick<
  LibraryModel,
  "input_nanos_per_mtok" | "cached_input_nanos_per_mtok" | "output_nanos_per_mtok"
>;

export const EMPTY_VALUE = "—";
export const PRICE_UNKNOWN = "Unknown";

const RATE_SEPARATOR = " / ";
const RATE_MIN_DECIMALS = 2;

/** Catalogue names are authoritative. A custom model keeps its exact identifier. */
export function modelLabel(modelId: string, displayName?: string | null): string {
  return displayName ?? modelId;
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
