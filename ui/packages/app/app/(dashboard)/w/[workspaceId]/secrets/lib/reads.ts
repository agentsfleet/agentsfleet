import { cache } from "react";
import { listSecrets } from "@/lib/api/secrets";
import { getTenantProvider } from "@/lib/api/tenant_provider";

// Server-only read wrappers for the Secrets page. Mirrors the same
// per-render cache() convention as settings/models/lib/reads.ts.
// getTenantProviderCached feeds the page's delete-protection guard: the
// secret backing the active self-managed provider must not be deletable
// from here (SecretsList's protectedSecretName prop).
//
// MUST NOT be imported from a client component — `cache()` only exists on the
// server, and a client import would break the build.

export const listSecretsCached = cache(listSecrets);
export const getTenantProviderCached = cache(getTenantProvider);
