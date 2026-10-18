import { MAX_NANOS } from "./nanos";
export { MAX_NANOS as MAX_RATE_NANOS } from "./nanos";

export const INFERENCE_API = {
  chat: "chat", responses: "responses", messages: "messages", gemini: "gemini", ollama: "ollama",
} as const;

export const NANOS_DECIMAL_PLACES = 9;
const DECIMAL_RATE = /^\d+(?:\.\d{1,9})?$/;
const INTEGER_NANOS = /^(?:0|[1-9]\d*)$/;
const INVALID_RATE = "Enter a non-negative decimal with at most nine fractional digits";
const RATE_OVERFLOW = "Price exceeds the maximum integer nanos amount";

export function usdPerMtokToNanos(usd: string): string {
  if (!DECIMAL_RATE.test(usd)) throw new RangeError(INVALID_RATE);
  const [whole, fraction = ""] = usd.split(".");
  const nanos = BigInt(`${whole}${fraction.padEnd(NANOS_DECIMAL_PLACES, "0")}`);
  if (nanos > MAX_NANOS) throw new RangeError(RATE_OVERFLOW);
  return String(nanos);
}

export function nanosToUsdPerMtok(nanos: string): string {
  if (!INTEGER_NANOS.test(nanos)) throw new RangeError(INVALID_RATE);
  if (BigInt(nanos) > MAX_NANOS) throw new RangeError(RATE_OVERFLOW);
  const digits = nanos.padStart(NANOS_DECIMAL_PLACES + 1, "0");
  const whole = digits.slice(0, -NANOS_DECIMAL_PLACES);
  const fraction = digits.slice(-NANOS_DECIMAL_PLACES).replace(/0+$/, "");
  return fraction ? `${whole}.${fraction}` : whole;
}
