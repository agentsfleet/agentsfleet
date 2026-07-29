"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  listApiKeys,
  createApiKey,
  revokeApiKey,
  deleteApiKey,
  type ApiKeyListResponse,
  type ApiKeySort,
  type CreatedApiKey,
  type RevokedApiKey,
} from "@/lib/api/api_keys";

export async function listApiKeysAction(sort?: ApiKeySort): Promise<ActionResult<ApiKeyListResponse>> {
  return withToken((t) => listApiKeys(t, sort));
}

export async function createApiKeyAction(body: {
  key_name: string;
  description?: string;
}): Promise<ActionResult<CreatedApiKey>> {
  return withToken((t) => createApiKey(t, body));
}

export async function revokeApiKeyAction(id: string): Promise<ActionResult<RevokedApiKey>> {
  return withToken((t) => revokeApiKey(t, id));
}

export async function deleteApiKeyAction(id: string): Promise<ActionResult<void>> {
  return withToken((t) => deleteApiKey(t, id));
}
