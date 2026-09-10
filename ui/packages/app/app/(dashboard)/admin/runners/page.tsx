import { redirect } from "next/navigation";
import { requireCredential } from "@/lib/auth/credential";
import { ApiError } from "@/lib/api/errors";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { listRunners } from "@/lib/api/runners";
import RunnersView from "./components/RunnersView";

export const dynamic = "force-dynamic";

const NOT_ADMIN = "/settings?notice=runners-platform-admin-only";

export default async function RunnersPage() {
  // Runner operators only — hide the surface for a token without `runner:read`.
  // The backend independently 403s a token missing the scope (UZ-AUTH-022);
  // this is the UI guard.
  if (!(await hasScope(SCOPE.RUNNER_READ))) redirect(NOT_ADMIN);

  const token = await requireCredential();

  let data;
  try {
    data = await listRunners(token);
  } catch (e) {
    if (e instanceof ApiError && e.status === 403) redirect(NOT_ADMIN);
    if (e instanceof ApiError && e.status === 401) redirect("/sign-in");
    throw e;
  }

  return <RunnersView initial={data} />;
}
