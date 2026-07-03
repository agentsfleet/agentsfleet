import { cache } from "react";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { listCredentials } from "@/lib/api/credentials";

// Server-only read wrappers for the Models page. React's `cache()` is an
// RSC primitive that collapses repeat reads within ONE server render — the hero
// and the switch list both need the tenant provider + the credential list, and
// without this they would each trigger a separate backend round-trip. Same
// convention as lib/workspace.ts / lib/api/tenant_billing.ts.
//
// MUST NOT be imported from a client component — `cache()` only exists on the
// server, and a client import would break the build.

export const getTenantProviderCached = cache(getTenantProvider);
export const listCredentialsCached = cache(listCredentials);
