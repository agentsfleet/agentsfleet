// The rate arithmetic the model-library forms and views share. Dependency-free
// beyond `lib/types` on purpose: client components read these without pulling
// the transport, whose retry policy is server-only. `NANOS_PER_USD` and
// `OPENAI_COMPATIBLE_PROVIDER` live in `lib/types`, their one home.

import { NANOS_PER_USD } from "../types";

// Rates are stored as integer nanos per million tokens (1 nano = 1e-9 USD) so the
// billing math stays in integers. The UI presents $/1M tokens — the conversion
// lives here, in one place, so every catalogue view and form agrees.
export function nanosToUsdPerMtok(nanos: number): number {
  return nanos / NANOS_PER_USD;
}
export function usdPerMtokToNanos(usd: number): number {
  return Math.round(usd * NANOS_PER_USD);
}
