import { requireCredential, tenantClaimMissing } from "@/lib/auth/credential";
import { redirect } from "next/navigation";
import { ApiError } from "@/lib/api/errors";
import { firstTenantWorkspace } from "@/lib/api/workspaces";
import { ERROR_CODE } from "@/lib/errors";
import { DEFAULT_WORKSPACE_SUBPATH, workspacePath } from "@/lib/workspace-routes";
import NoWorkspaceEmptyState from "@/components/layout/NoWorkspaceEmptyState";
import { AccountSetup } from "./AccountSetup";

export const dynamic = "force-dynamic";

// Dashboard entry (`/`). Resolves the caller's own workspace — or, owning none,
// the first one they joined — and redirects once to New fleet; a caller
// with no workspace at all lands on the create-workspace empty state instead
// of a broken page. This is the ONLY place the "default workspace" is chosen —
// every deeper page reads the workspace from its route param.
//
// One page, deliberately: the redirect fires before the layout tree renders,
// so nothing here can share the layout's cached full walk — the complete list
// on this request is pure double-payment on every cold entry.
export default async function DashboardIndexPage() {
  const token = await requireCredential();

  // A transient list failure must NOT fall through to the create-first empty
  // state — an operator who already owns workspaces would be shown "create a
  // workspace" and could make a duplicate. The error propagates to
  // `(dashboard)/error.tsx` (a retry surface) instead; the empty state renders
  // only on a genuinely empty list (a successful 200 with no items). The one
  // refusal caught is an account still opening, which only a fresh token fixes.
  let first;
  try {
    first = await firstTenantWorkspace(token);
  } catch (failure) {
    if (await accountStillOpening(failure)) return <AccountSetup />;
    throw failure;
  }
  if (first) redirect(workspacePath(first.id, DEFAULT_WORKSPACE_SUBPATH));

  return <NoWorkspaceEmptyState />;
}

/** The refusals a token minted before signup's writeback can meet. */
const OPENING_REFUSALS: ReadonlySet<string> = new Set([
  // The daemon refuses a session token with no tenant claim at authentication,
  // before any scope is read (`afd_auth/src/authenticate.rs`): the first
  // request of almost every new account.
  ERROR_CODE.AUTH_UNAUTHORIZED,
  // A token that does reach the scope gate without the owner scopes.
  ERROR_CODE.INSUFFICIENT_SCOPE,
]);

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
