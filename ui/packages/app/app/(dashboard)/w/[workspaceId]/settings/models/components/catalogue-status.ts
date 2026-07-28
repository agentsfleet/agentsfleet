// Load states for the global model catalogue. Housed apart from
// ModelCatalogueProvider on purpose: ProviderModelSelect keys its control
// shape off these values, and the test suites stub the provider module's
// hooks wholesale — a const living in the stubbed module would vanish with
// it, so it lives here where every import resolves to the real thing.

export const CATALOGUE_STATUS = {
  /** Never requested. Distinct from "loaded and empty". */
  idle: "idle",
  loading: "loading",
  ready: "ready",
  error: "error",
} as const;

export type CatalogueStatus = (typeof CATALOGUE_STATUS)[keyof typeof CATALOGUE_STATUS];
