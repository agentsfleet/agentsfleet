import { redirect } from "next/navigation";
import { requireCredential } from "@/lib/auth/credential";
import { ApiError } from "@/lib/api/errors";
import { listApiKeys } from "@/lib/api/api_keys";
import ApiKeysView from "./components/ApiKeysView";

export const dynamic = "force-dynamic";

export default async function ApiKeysPage() {
  const token = await requireCredential();

  // Role-based access control (RBAC) guard via defense-in-depth: the dashboard
  // session token carries no role claim (AUTH.md — role lives only in the
  // api-template token the backend verifies), so the backend arbitrates.
  // `GET /v1/api-keys` requires the `apikey:read` scope (`TenantRoute::ApiKeys`
  // in rustd/crates/afd_http/src/route/tenant.rs). A principal without it gets
  // 403 — render this same page with the operator-only notice inline rather
  // than redirecting to a route that no longer exists.
  let data = null;
  let operatorOnly = false;
  try {
    data = await listApiKeys(token);
  } catch (e) {
    if (e instanceof ApiError && e.status === 403) {
      operatorOnly = true;
    } else if (e instanceof ApiError && e.status === 401) {
      redirect("/sign-in");
    } else {
      throw e;
    }
  }

  return <ApiKeysView initial={data} operatorOnly={operatorOnly} />;
}
