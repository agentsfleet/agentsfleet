// Wire-format constants for billing endpoints. The enum spellings below are
// what the API accepts; the nanos denominator is what its numbers are quoted
// in. Rates are not here: the daemon charges them and serves the result.

export const CHARGE_TYPE = Object.freeze({
  receive: "receive",
  stage: "stage",
});

export const PROVIDER_MODE = Object.freeze({
  platform: "platform",
  self_managed: "self_managed",
});

export const NANOS_PER_USD = 1_000_000_000;
const NANOS_PER_DISPLAY_UNIT = 100_000n;
const DISPLAY_UNITS_PER_USD = 10_000n;
const HALF_DISPLAY_UNIT = NANOS_PER_DISPLAY_UNIT / 2n;
const FRACTION_DIGITS = 4;
const CENT_DIGITS = 2;
const DISPLAY_LOCALE = "en-US";
const INTEGER_FORMATTER = new Intl.NumberFormat(DISPLAY_LOCALE, {maximumFractionDigits: 0});

// Two-to-four decimal places — cents granularity, with sub-cent precision
// when the per-second run rate ($0.0001/sec) needs it.
const USD_FORMATTER = new Intl.NumberFormat(DISPLAY_LOCALE, {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});

export function formatDollars(nanos: string | number | null | undefined): string {
  if (typeof nanos === "string") {
    const units = (BigInt(nanos) + HALF_DISPLAY_UNIT) / NANOS_PER_DISPLAY_UNIT;
    const fraction = (units % DISPLAY_UNITS_PER_USD).toString().padStart(FRACTION_DIGITS, "0");
    const trimmed = fraction.replace(/0+$/u, "").padEnd(CENT_DIGITS, "0");
    return `$${INTEGER_FORMATTER.format(units / DISPLAY_UNITS_PER_USD)}.${trimmed}`;
  }
  return USD_FORMATTER.format((nanos ?? 0) / NANOS_PER_USD);
}
