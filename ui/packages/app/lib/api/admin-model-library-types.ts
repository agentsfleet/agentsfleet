// The rate arithmetic and the one provider id the model-library forms and views share. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

// Rates are stored as integer nanos per million tokens (1 nano = 1e-9 USD) so the
// billing math stays in integers. The UI presents $/1M tokens — the conversion
// lives here, in one place, so every catalogue view and form agrees.
export const NANOS_PER_USD = 1_000_000_000;
export function nanosToUsdPerMtok(nanos: number): number {
  return nanos / NANOS_PER_USD;
}
export function usdPerMtokToNanos(usd: number): number {
  return Math.round(usd * NANOS_PER_USD);
}

// The provider id that opts a default into a custom OpenAI-compatible endpoint —
// mirrors OPENAI_COMPATIBLE_PROVIDER in tenant_provider_resolver.zig. Only this
// provider may carry a base_url; named providers must omit it.
export const OPENAI_COMPATIBLE_PROVIDER = "openai-compatible";
