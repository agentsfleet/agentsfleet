"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import {
  createCredential as apiCreateCredential,
  deleteCredential as apiDeleteCredential,
} from "@/lib/api/credentials";

export async function createCredentialAction(
  workspaceId: string,
  body: { name: string; data: Record<string, unknown> },
): Promise<ActionResult<{ name: string }>> {
  return withToken((t) => apiCreateCredential(workspaceId, body, t));
}

export async function deleteCredentialAction(
  workspaceId: string,
  name: string,
): Promise<ActionResult<void>> {
  return withToken((t) => apiDeleteCredential(workspaceId, name, t));
}
