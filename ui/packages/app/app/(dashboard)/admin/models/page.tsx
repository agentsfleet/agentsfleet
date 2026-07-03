import { redirect } from "next/navigation";
import { auth } from "@clerk/nextjs/server";
import { ApiError } from "@/lib/api/errors";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { listAdminModels } from "@/lib/api/admin_models";
import ModelsView from "./components/ModelsView";

export const dynamic = "force-dynamic";

const NOT_ADMIN = "/settings?notice=models-platform-admin-only";

export default async function AdminModelsPage() {
  // Model operators only — hide the surface for a token without `model:read`.
  // The backend independently 403s a token missing the scope (UZ-AUTH-022);
  // this is the UI guard.
  if (!(await hasScope(SCOPE.MODEL_READ))) redirect(NOT_ADMIN);

  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  let initial;
  try {
    initial = await listAdminModels(token);
  } catch (e) {
    if (e instanceof ApiError && e.status === 403) redirect(NOT_ADMIN);
    if (e instanceof ApiError && e.status === 401) redirect("/sign-in");
    throw e;
  }

  return <ModelsView initial={initial} />;
}
