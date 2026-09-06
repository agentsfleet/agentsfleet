// The model library catalogue (core.model_library), read through the
// authenticated GET /v1/models — see
// rustd/crates/afd_api_tenant/src/handler/tenant/models.rs
// for the wire shape. The dashboard fetches it once per session through a
// token-minting Server Action (settings/models/actions.ts → getModelLibraryAction),
// so the token never reaches the browser. The only static client-side model
// data is the small known-models fallback list
// (settings/models/lib/known-models.ts), used when the catalogue has no rows
// for a provider.

import { request } from "./client";
import type { ModelLibrary } from "./model-library-types";

// Route path — mirrors the `ModelLibrary` row of the route table in
// rustd/crates/afd_http/src/route/tenant.rs (shared verbatim).
const MODEL_LIBRARY_PATH = "/v1/models";

/**
 * Fetch the model library. Bearer-authed (any authenticated tenant — the route
 * carries no capability scope). Called server-side by getModelLibraryAction;
 * throws (ApiError) on a non-2xx response so callers can fall back to a
 * catalogue-free path (a free-text model field).
 */
export async function getModelLibrary(token: string): Promise<ModelLibrary> {
  return request<ModelLibrary>(MODEL_LIBRARY_PATH, { method: "GET" }, token);
}
