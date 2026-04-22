"use server";

import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";
import { ACTIVE_WORKSPACE_COOKIE } from "@/lib/workspace";

// Writes the active-workspace cookie server-side, then revalidates the
// dashboard so the next render picks up the new selection. The cookie is
// a UX preference (not auth) — no HttpOnly, no Secure flag required
// beyond what the transport enforces.
export async function setActiveWorkspace(workspaceId: string): Promise<void> {
  const store = await cookies();
  store.set({
    name: ACTIVE_WORKSPACE_COOKIE,
    value: workspaceId,
    path: "/",
    sameSite: "lax",
    maxAge: 60 * 60 * 24 * 365,
  });
  revalidatePath("/", "layout");
}
