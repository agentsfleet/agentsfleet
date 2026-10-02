import { cache } from "react";
import { listWaitingInvites } from "./api/invites";

/**
 * Per-request deduped wrapper around `listWaitingInvites`, as
 * `listTenantWorkspacesCached` is in `lib/workspace.ts`. The dashboard layout
 * reads the waiting invites for the shell's notice and the Invites page reads
 * them for its table; `cache()` makes one render share one
 * GET /v1/users/me/invites.
 */
export const listWaitingInvitesCached = cache(listWaitingInvites);
