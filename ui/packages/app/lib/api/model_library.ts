// The model catalogue (model.platform_models), read through the
// authenticated GET /v1/models — see
// rustd/crates/afd_api_tenant/src/handler/tenant/models/mod.rs
// for the wire shape. The dashboard fetches it once per session through a
// token-minting Server Action (settings/models/actions.ts → getModelLibraryAction),
// so the token never reaches the browser. Model suggestions and display names
// come from the catalogue.

import { request } from "./client";
import { modelLibrarySchema, type ModelLibrary } from "./model-library-types";

// Route path — mirrors the `ModelLibrary` row of the route table in
// rustd/crates/afd_http/src/route/tenant.rs (shared verbatim).
const MODEL_LIBRARY_PATH = "/v1/models";

/**
 * Fetch the model library. Bearer-authed (any authenticated tenant — the route
 * carries no capability scope). Called server-side by getModelLibraryAction;
 * throws (ApiError) on a non-2xx response so callers can fall back to a
 * catalogue-free path (a free-text model field).
 */
export async function getModelLibrary(token: string, advanced = false, startingAfter: string | null = null): Promise<ModelLibrary> {
  const params = new URLSearchParams();
  if (advanced) params.set("advanced", "true");
  if (startingAfter !== null) params.set("starting_after", startingAfter);
  const query = params.toString();
  const path = query ? `${MODEL_LIBRARY_PATH}?${query}` : MODEL_LIBRARY_PATH;
  return modelLibrarySchema.parse(await request<unknown>(path, { method: "GET" }, token));
}
