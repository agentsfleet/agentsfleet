"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { createSecret, deleteSecret, replaceSecret } from "@/lib/api/secrets";

export async function createSecretAction(
  workspaceId: string,
  body: { name: string; data: Record<string, unknown> },
): Promise<ActionResult<{ name: string }>> {
  return withToken((t) => createSecret(workspaceId, body, t));
}

// Rotation path: creation claims a free name and 409s on an existing one, so
// replacing a held secret's body must go through PUT on the named secret.
export async function replaceSecretAction(
  workspaceId: string,
  name: string,
  data: Record<string, unknown>,
): Promise<ActionResult<{ name: string }>> {
  return withToken((t) => replaceSecret(workspaceId, name, data, t));
}

export async function deleteSecretAction(
  workspaceId: string,
  name: string,
): Promise<ActionResult<void>> {
  return withToken((t) => deleteSecret(workspaceId, name, t));
}
