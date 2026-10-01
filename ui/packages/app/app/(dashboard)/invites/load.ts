import { redirect } from "next/navigation";
import { requireCredential } from "@/lib/auth/credential";
import { SIGN_IN_PATH } from "@/lib/auth/sign-in-redirect";
import { ApiError, HTTP_STATUS_UNAUTHORIZED } from "@/lib/api/errors";
import type { WaitingInvite } from "@/lib/api/invites";
import { listWaitingInvitesCached } from "@/lib/invites";

/** The invites waiting for the signed-in person, for either Invites route.
 * Cached per request, so it shares the dashboard layout's read. */
export async function loadWaitingInvites(): Promise<WaitingInvite[]> {
  const token = await requireCredential();
  try {
    return await listWaitingInvitesCached(token);
  } catch (e) {
    if (e instanceof ApiError && e.status === HTTP_STATUS_UNAUTHORIZED) redirect(SIGN_IN_PATH);
    throw e;
  }
}
