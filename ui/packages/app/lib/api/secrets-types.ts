import type {Secret, SecretFleet} from "./secrets";

// The secret kinds the vault classifies by. No runtime dependencies: client components read these
// without pulling the transport, whose retry policy is server-only.

// Secret kinds, keyed off the server's `kind` discriminator. The string
// values are verbatim with `Kind::as_str` in afd_vault/src/projection/mod.rs
// (RULE UFS — cross-runtime parity); changing one without the other
// silently breaks classification.
export const SECRET_KIND = {
  provider_key: "provider_key",
  custom_endpoint: "custom_endpoint",
  custom_secret: "custom_secret",
} as const;

export type SecretKind = (typeof SECRET_KIND)[keyof typeof SECRET_KIND];

/** The recorded fleets, or `null` when access was not served or is not the served shape. */
export function fleetAccessOf(secret: Secret): SecretFleet[] | null {
  const served: unknown = secret.available_to;
  if (!Array.isArray(served)) return null;
  const fleets = served.filter((fleet): fleet is SecretFleet => typeof fleet?.id === "string" && typeof fleet?.name === "string" && fleet.name !== "");
  return fleets.length === served.length ? fleets : null;
}
