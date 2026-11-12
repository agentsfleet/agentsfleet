import { redirect } from "next/navigation";
import { Alert } from "@agentsfleet/design-system";
import { RouteRetryButton } from "@/components/domain/RouteRetryButton";
import { SecurityLayout } from "./components/SecurityLayout";
import { SessionSettings } from "@/components/account/SessionSettings";
import { requireCredential } from "@/lib/auth/credential";
import { ApiError } from "@/lib/api/errors";
import { listApiKeys } from "@/lib/api/api_keys";
import ApiKeysView from "./components/ApiKeysView";

export const dynamic = "force-dynamic";

export default async function SecurityPage() {
  const token = await requireCredential();
  let initial = null;
  let operatorOnly = false;
  let unavailable = false;
  try { initial = await listApiKeys(token); }
  catch (error) {
    if (error instanceof ApiError && error.status === 403) operatorOnly = true;
    else if (error instanceof ApiError && error.status === 401) redirect("/sign-in");
    else unavailable = true;
  }
  return <SecurityLayout>
    {unavailable ? <Alert variant="warning">API keys could not be loaded.<RouteRetryButton label="Retry API keys" /></Alert> : <ApiKeysView initial={initial} operatorOnly={operatorOnly} />}
    <SessionSettings />
  </SecurityLayout>;
}
