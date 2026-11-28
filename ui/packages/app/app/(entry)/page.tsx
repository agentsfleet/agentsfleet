import { requireCredential, tenantClaimMissing } from "@/lib/auth/credential";
import { redirect } from "next/navigation";
import { ApiError } from "@/lib/api/errors";
import { firstTenantWorkspace } from "@/lib/api/workspaces";
import { ERROR_CODE } from "@/lib/errors";
import { defaultFleetPath } from "@/lib/api/fleets";
import DashboardLayout from "../(dashboard)/layout";
import NoWorkspaceEmptyState from "@/components/layout/NoWorkspaceEmptyState";
import { AccountSetup } from "./AccountSetup";

export const dynamic = "force-dynamic";

/** The refusals a token minted before signup's writeback can meet. */
const OPENING_REFUSALS: ReadonlySet<string> = new Set([
  // The daemon refuses a session token with no tenant claim at authentication,
  // before any scope is read (`afd_auth/src/authenticate.rs`): the first
  // request of almost every new account.
  ERROR_CODE.AUTH_UNAUTHORIZED,
  // A token that does reach the scope gate without the owner scopes.
  ERROR_CODE.INSUFFICIENT_SCOPE,
]);

// Resolve the destination outside the dashboard layout. A returning visitor only
// mounts the shell at their final URL. The empty and setup states still need it.
export default async function DashboardIndexPage() {
  const token = await requireCredential();

  // Only a successful empty list offers workspace creation. Ordinary failures
  // reach the retry boundary, and signup can wait for its tenant claim.
  let first;
  try {
    first = await firstTenantWorkspace(token);
  } catch (failure) {
    if (await accountStillOpening(failure)) return <DashboardLayout><AccountSetup /></DashboardLayout>;
    throw failure;
  }
  if (first) redirect(await defaultFleetPath(first.id, token));

  return <DashboardLayout><NoWorkspaceEmptyState /></DashboardLayout>;
}

/**
 * Whether the list was refused because this account is still opening: the
 * daemon refused the token at authentication or at the scope gate, and the
 * token carries no tenant. Both must hold. A token WITH a tenant that is
 * refused belongs to a person an operator narrowed or a credential that is
 * genuinely bad, for whom no amount of waiting helps.
 */
async function accountStillOpening(failure: unknown): Promise<boolean> {
  if (!(failure instanceof ApiError)) return false;
  if (!OPENING_REFUSALS.has(failure.code)) return false;
  return tenantClaimMissing();
}
