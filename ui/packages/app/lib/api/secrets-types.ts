// The secret kinds the vault classifies by. Dependency-free on purpose: client components read these
// without pulling the transport, whose retry policy is server-only.

// Secret kinds, keyed off the server's `kind` discriminator. The string
// values are verbatim with `Kind::as_str` in that projection.rs
// (RULE UFS — cross-runtime parity); changing one without the other
// silently breaks classification.
export const SECRET_KIND = {
  provider_key: "provider_key",
  custom_endpoint: "custom_endpoint",
  custom_secret: "custom_secret",
} as const;

export type SecretKind = (typeof SECRET_KIND)[keyof typeof SECRET_KIND];
