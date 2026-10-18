// Wire-format constants for billing endpoints. The enum spellings below are
// what the API accepts; the nanos denominator is what its numbers are quoted
// in. Rates are not here: the daemon charges them and serves the result.

export const CHARGE_TYPE = Object.freeze({
  receive: "receive",
  stage: "stage",
  authoring: "authoring",
});

export const PROVIDER_MODE = Object.freeze({
  platform: "platform",
  self_managed: "self_managed",
});

export const NANOS_PER_USD = 1_000_000_000;
const NANOS_DIGITS = 9;
const NANOS_PER_DISPLAY_UNIT = 100_000n;
const DISPLAY_UNITS_PER_USD = 10_000n;
const HALF_DISPLAY_UNIT = NANOS_PER_DISPLAY_UNIT / 2n;
const FRACTION_DIGITS = 4;
const CENT_DIGITS = 2;
const DISPLAY_LOCALE = "en-US";
const INTEGER_FORMATTER = new Intl.NumberFormat(DISPLAY_LOCALE, {maximumFractionDigits: 0});

// Two-to-four decimal places: cents, with the per-second run fee still visible.
export function formatDollars(nanos: string): string {
  const units = (BigInt(nanos) + HALF_DISPLAY_UNIT) / NANOS_PER_DISPLAY_UNIT;
  const fraction = (units % DISPLAY_UNITS_PER_USD).toString().padStart(FRACTION_DIGITS, "0");
  const trimmed = fraction.replace(/0+$/u, "").padEnd(CENT_DIGITS, "0");
  return `$${INTEGER_FORMATTER.format(units / DISPLAY_UNITS_PER_USD)}.${trimmed}`;
}

/** Catalogue prices retain every served fractional digit, including a one-nano rate. */
export function formatRate(nanos: string): string {
  const amount = BigInt(nanos);
  const denominator = BigInt(NANOS_PER_USD);
  const fraction = (amount % denominator).toString().padStart(NANOS_DIGITS, "0")
    .replace(/0+$/, "").padEnd(CENT_DIGITS, "0");
  return `$${INTEGER_FORMATTER.format(amount / denominator)}.${fraction}`;
}
