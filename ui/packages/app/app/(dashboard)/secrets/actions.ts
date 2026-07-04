"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { createSecret, deleteSecret } from "@/lib/api/secrets";

export async function createSecretAction(
  workspaceId: string,
  body: { name: string; data: Record<string, unknown> },
): Promise<ActionResult<{ name: string }>> {
  return withToken((t) => createSecret(workspaceId, body, t));
}

export async function deleteSecretAction(
  workspaceId: string,
  name: string,
): Promise<ActionResult<void>> {
  return withToken((t) => deleteSecret(workspaceId, name, t));
}
