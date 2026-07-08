import { cache } from "react";
import { listTenantModelEntries } from "@/lib/api/tenant_model_entries";
import { listSecrets } from "@/lib/api/secrets";

// Server-only read wrappers for the Models page. React's `cache()` is an
// RSC primitive that collapses repeat reads within ONE server render. Same
// convention as lib/workspace.ts / lib/api/tenant_billing.ts.
//
// MUST NOT be imported from a client component — `cache()` only exists on the
// server, and a client import would break the build.
//
// No `getTenantProviderCached` here (M121): the registry list carries
// `active` per entry and `platform_default_available` directly, so a second
// full tenant-provider round-trip is redundant — the secrets/lib/reads.ts
// sibling still needs its own copy for the Secrets page's delete guard.

export const listTenantModelEntriesCached = cache(listTenantModelEntries);
export const listSecretsCached = cache(listSecrets);
