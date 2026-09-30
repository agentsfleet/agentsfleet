import { redirect } from "next/navigation";
import { requireCredential } from "@/lib/auth/credential";
import { SIGN_IN_PATH } from "@/lib/auth/sign-in-redirect";
import { ApiError, HTTP_STATUS_UNAUTHORIZED } from "@/lib/api/errors";
import { listWaitingInvites, type WaitingInvite } from "@/lib/api/invites";

/** The invites waiting for the signed-in person, for either Invites route. */
export async function loadWaitingInvites(): Promise<WaitingInvite[]> {
  const token = await requireCredential();
  try {
    return await listWaitingInvites(token);
  } catch (e) {
    if (e instanceof ApiError && e.status === HTTP_STATUS_UNAUTHORIZED) redirect(SIGN_IN_PATH);
    throw e;
  }
}
